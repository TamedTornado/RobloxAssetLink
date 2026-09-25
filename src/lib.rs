use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub mod animation;
pub mod animation_gltf;
pub mod audio;
pub mod bundle;
pub mod catalog;
pub mod collision;
mod collision_format;
pub mod config;
pub mod convert;
pub mod mesh;
pub mod scene;
pub mod scripts;
pub mod skin;
mod source_mesh;
pub mod texture;
mod vertex_attributes;
use config::{Config, PartSettings};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Mesh {
    pub name: String,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
    pub color: [f32; 4],
    pub settings: PartSettings,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub revision: String,
    pub meshes: Vec<Mesh>,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub protocol: u32,
    pub config: Config,
    pub assets: Vec<Asset>,
}

type Matrix = [[f32; 4]; 4];
const IDENTITY: Matrix = [
    [1., 0., 0., 0.],
    [0., 1., 0., 0.],
    [0., 0., 1., 0.],
    [0., 0., 0., 1.],
];

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}

fn point(m: Matrix, p: [f32; 3], metres_per_stud: f32) -> [f32; 3] {
    std::array::from_fn(|r| {
        (m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r]) / metres_per_stud
    })
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn normal_matrix(m: Matrix) -> Result<([[f32; 3]; 3], bool)> {
    let columns: [[f32; 3]; 3] = std::array::from_fn(|c| [m[c][0], m[c][1], m[c][2]]);
    let cofactors = [
        cross(columns[1], columns[2]),
        cross(columns[2], columns[0]),
        cross(columns[0], columns[1]),
    ];
    let determinant = dot(columns[0], cofactors[0]);
    if !determinant.is_finite() || determinant == 0. {
        return Err("singular node transform".into());
    }
    Ok((
        cofactors.map(|column| column.map(|value| value / determinant)),
        determinant < 0.,
    ))
}

fn visit(
    node: gltf::Node<'_>,
    parent: Matrix,
    blob: &[u8],
    config: &Config,
    output: &mut Vec<Mesh>,
) -> Result<()> {
    if node.skin().is_some() {
        return Err("skinned assets are not supported".into());
    }
    let transform = multiply(parent, node.transform().matrix());
    let (normal_transform, mirrored) = normal_matrix(transform)?;

    if let Some(mesh) = node.mesh() {
        for (index, primitive) in mesh.primitives().enumerate() {
            if primitive.mode() != gltf::mesh::Mode::Triangles
                || primitive.morph_targets().next().is_some()
            {
                return Err("only static triangle meshes are supported".into());
            }
            let material = primitive.material();
            let pbr = material.pbr_metallic_roughness();
            if pbr.base_color_texture().is_some()
                || pbr.metallic_roughness_texture().is_some()
                || material.normal_texture().is_some()
                || material.occlusion_texture().is_some()
                || material.emissive_texture().is_some()
                || material.emissive_factor() != [0.; 3]
            {
                return Err(
                    "textured/emissive materials are not supported by this first version".into(),
                );
            }
            if material.alpha_mode() == gltf::material::AlphaMode::Mask {
                return Err("alpha-mask materials are not supported".into());
            }
            let reader = primitive.reader(|_| Some(blob));
            if reader.read_colors(0).is_some() {
                return Err("vertex colors are not supported".into());
            }
            let vertices: Vec<_> = reader
                .read_positions()
                .ok_or("mesh has no positions")?
                .map(|p| point(transform, p, config.metres_per_stud))
                .collect();
            let normals: Vec<_> = reader
                .read_normals()
                .ok_or("export normals with the mesh")?
                .map(|n| {
                    let transformed: [f32; 3] = std::array::from_fn(|r| {
                        (0..3).map(|c| normal_transform[c][r] * n[c]).sum()
                    });
                    let length = dot(transformed, transformed).sqrt();
                    transformed.map(|v| v / length)
                })
                .collect();
            let uvs: Vec<_> = reader
                .read_tex_coords(0)
                .ok_or("export UV0 with the mesh")?
                .into_f32()
                .collect();
            let indices: Vec<_> = reader
                .read_indices()
                .map(|i| i.into_u32().collect())
                .unwrap_or_else(|| (0..vertices.len() as u32).collect());
            if vertices.is_empty()
                || normals.len() != vertices.len()
                || uvs.len() != vertices.len()
                || indices.is_empty()
                || indices.len() % 3 != 0
                || indices.iter().any(|i| *i as usize >= vertices.len())
                || vertices
                    .iter()
                    .flatten()
                    .chain(normals.iter().flatten())
                    .chain(uvs.iter().flatten())
                    .any(|n| !n.is_finite())
            {
                return Err("invalid mesh attributes or indices".into());
            }
            let triangles = indices
                .chunks_exact(3)
                .map(|t| {
                    if mirrored {
                        [t[0], t[2], t[1]]
                    } else {
                        [t[0], t[1], t[2]]
                    }
                })
                .collect();
            let name = node
                .name()
                .or(mesh.name())
                .ok_or("mesh nodes must be named")?;
            let mut color = pbr.base_color_factor();
            if material.alpha_mode() == gltf::material::AlphaMode::Opaque {
                color[3] = 1.;
            }
            output.push(Mesh {
                name: format!("{name}:{index}"),
                vertices,
                normals,
                uvs,
                triangles,
                color,
                settings: config.settings_for(name),
            });
        }
    }
    for child in node.children() {
        visit(child, transform, blob, config, output)?;
    }
    Ok(())
}

pub fn read_asset(path: &Path, config: &Config) -> Result<Asset> {
    config.validate()?;
    let bytes = fs::read(path)?;
    let gltf = gltf::Gltf::from_slice(&bytes)?;
    if gltf
        .buffers()
        .any(|b| !matches!(b.source(), gltf::buffer::Source::Bin))
    {
        return Err("external buffers are forbidden; use self-contained GLB".into());
    }
    if gltf.animations().next().is_some() || gltf.extensions_required().next().is_some() {
        return Err("animations and required glTF extensions are not supported".into());
    }
    let blob = gltf.blob.as_deref().ok_or("GLB has no binary chunk")?;
    let scene = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .ok_or("GLB has no scene")?;
    let mut meshes = Vec::new();
    for node in scene.nodes() {
        visit(node, IDENTITY, blob, config, &mut meshes)?;
    }
    if meshes.is_empty() {
        return Err("GLB has no meshes".into());
    }
    let mut hash = Sha256::new();
    hash.update(&bytes);
    hash.update(serde_json::to_vec(config)?);
    Ok(Asset {
        id: String::new(),
        name: path
            .file_stem()
            .and_then(|n| n.to_str())
            .ok_or("invalid filename")?
            .to_owned(),
        revision: format!("{:x}", hash.finalize()),
        meshes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_use_metres_and_compose_parent_translation() {
        let mut parent = IDENTITY;
        parent[3][0] = 2.8;
        let mut child = IDENTITY;
        child[3][1] = 1.4;
        let p = point(multiply(parent, child), [0., 0., 0.], 0.28);
        assert!((p[0] - 10.).abs() < 0.00001);
        assert!((p[1] - 5.).abs() < 0.00001);
    }

    #[test]
    fn inverse_transpose_and_reflections() {
        let mut matrix = IDENTITY;
        matrix[0][0] = -2.;
        matrix[1][1] = 4.;
        let (normal, mirrored) = normal_matrix(matrix).unwrap();
        assert!(mirrored);
        assert_eq!(normal[0][0], -0.5);
        assert_eq!(normal[1][1], 0.25);
        matrix[0][0] = 0.;
        assert!(normal_matrix(matrix).is_err());
    }

    #[test]
    fn invalid_scale_is_rejected_before_reading() {
        let mut config: Config =
            serde_json::from_str(include_str!("../examples/building-kit.json")).unwrap();
        for scale in [0., -1., f32::NAN, f32::INFINITY] {
            config.metres_per_stud = scale;
            assert!(
                read_asset(Path::new("absent.glb"), &config)
                    .unwrap_err()
                    .to_string()
                    .contains("metresPerStud")
            );
        }
    }

    #[test]
    fn unreadable_assets_fail_validation() {
        let config: Config =
            serde_json::from_str(include_str!("../examples/building-kit.json")).unwrap();
        assert!(read_asset(Path::new("absent.glb"), &config).is_err());
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("broken.glb"), b"bad glb").unwrap();
        assert!(read_asset(&directory.path().join("broken.glb"), &config).is_err());
    }
}
