//! First offline conversion profile: static, untextured GLB to native meshes.

use crate::{IDENTITY, Matrix, Result, dot, mesh, multiply, normal_matrix, point};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::Path,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub metres_per_stud: f32,
    /// OBJ has no standard unit metadata. Required only for OBJ input.
    pub obj_metres_per_unit: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: &'static str,
    pub version: u32,
    pub metres_per_stud: f32,
    pub collision_generated: bool,
    pub meshes: Vec<Entry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub file: String,
    pub sha256: String,
    pub node_index: usize,
    pub primitive_index: usize,
    pub name: Option<String>,
    pub vertices: usize,
    pub triangles: usize,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub double_sided: bool,
}

pub(crate) struct Output {
    pub entry: Entry,
    pub bytes: Vec<u8>,
}

fn visit(
    node: gltf::Node<'_>,
    transform: Matrix,
    blob: &[u8],
    scale: f32,
    outputs: &mut Vec<Output>,
) -> Result<()> {
    if node.skin().is_some() || node.camera().is_some() {
        return Err("static mesh conversion does not support skins or cameras".into());
    }
    let (normal_transform, mirrored) = normal_matrix(transform)?;

    if let Some(source) = node.mesh() {
        for (primitive_index, primitive) in source.primitives().enumerate() {
            if primitive.mode() != gltf::mesh::Mode::Triangles
                || primitive.morph_targets().next().is_some()
            {
                return Err(
                    "static mesh conversion requires triangles without morph targets".into(),
                );
            }
            for (semantic, _) in primitive.attributes() {
                if !matches!(
                    semantic,
                    gltf::Semantic::Positions
                        | gltf::Semantic::Normals
                        | gltf::Semantic::TexCoords(0)
                ) {
                    return Err(format!("unsupported static mesh attribute: {semantic:?}").into());
                }
            }
            let material = primitive.material();
            let pbr = material.pbr_metallic_roughness();
            if pbr.base_color_texture().is_some()
                || pbr.metallic_roughness_texture().is_some()
                || material.normal_texture().is_some()
                || material.occlusion_texture().is_some()
                || material.emissive_texture().is_some()
                || material.emissive_factor() != [0.; 3]
                || material.alpha_mode() == gltf::material::AlphaMode::Mask
            {
                return Err(
                    "this profile does not support textures, emission or alpha masking".into(),
                );
            }
            let reader = primitive.reader(|_| Some(blob));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("positions required")?
                .collect();
            let normals: Vec<_> = reader.read_normals().ok_or("normals required")?.collect();
            let uvs: Vec<_> = reader
                .read_tex_coords(0)
                .ok_or("UV0 required")?
                .into_f32()
                .collect();
            if positions.len() != normals.len() || positions.len() != uvs.len() {
                return Err("mesh attribute counts do not match".into());
            }
            let vertex_count = u32::try_from(positions.len())?;
            let indices: Vec<_> = reader
                .read_indices()
                .map(|indices| indices.into_u32().collect())
                .unwrap_or_else(|| (0..vertex_count).collect());
            if indices.len() % 3 != 0 {
                return Err("triangle index count is not divisible by three".into());
            }
            let vertices = positions
                .into_iter()
                .zip(normals)
                .zip(uvs)
                .map(|((position, normal), uv)| {
                    let normal: [f32; 3] = std::array::from_fn(|r| {
                        (0..3).map(|c| normal_transform[c][r] * normal[c]).sum()
                    });
                    let length = dot(normal, normal).sqrt();
                    mesh::Vertex {
                        position: point(transform, position, scale),
                        normal: normal.map(|v| v / length),
                        uv,
                        tangent: [0; 4],
                        color: [255; 4],
                    }
                })
                .collect();
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
            let geometry = mesh::Mesh {
                vertices,
                triangles,
            };
            let bytes = mesh::encode(&geometry)?;
            let mut base_color = pbr.base_color_factor();
            if material.alpha_mode() == gltf::material::AlphaMode::Opaque {
                base_color[3] = 1.;
            }
            outputs.push(Output {
                entry: Entry {
                    file: format!("node-{}-primitive-{primitive_index}.mesh", node.index()),
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    node_index: node.index(),
                    primitive_index,
                    name: node.name().or(source.name()).map(str::to_owned),
                    vertices: geometry.vertices.len(),
                    triangles: geometry.triangles.len(),
                    base_color,
                    metallic: pbr.metallic_factor(),
                    roughness: pbr.roughness_factor(),
                    double_sided: material.double_sided(),
                },
                bytes,
            });
        }
    }
    Ok(())
}

fn read_glb(bytes: &[u8], config: &Config) -> Result<Vec<Output>> {
    let gltf = gltf::Gltf::from_slice(bytes)?;
    if gltf
        .buffers()
        .any(|buffer| !matches!(buffer.source(), gltf::buffer::Source::Bin))
        || gltf.extensions_used().next().is_some()
        || gltf.animations().next().is_some()
    {
        return Err(
            "this profile requires self-contained GLB without extensions or animations".into(),
        );
    }
    let blob = gltf.blob.as_deref().ok_or("GLB binary chunk required")?;
    let scene = gltf
        .default_scene()
        .or_else(|| {
            if gltf.scenes().len() == 1 {
                gltf.scenes().next()
            } else {
                None
            }
        })
        .ok_or("select a default scene in multi-scene GLB")?;
    let mut outputs = Vec::new();
    let mut pending: VecDeque<_> = scene.nodes().map(|node| (node, IDENTITY)).collect();
    let mut visited = HashSet::new();
    while let Some((node, parent)) = pending.pop_front() {
        if !visited.insert(node.index()) {
            return Err("scene repeats a node or contains a cycle".into());
        }
        let transform = multiply(parent, node.transform().matrix());
        visit(
            node.clone(),
            transform,
            blob,
            config.metres_per_stud,
            &mut outputs,
        )?;
        pending.extend(node.children().map(|child| (child, transform)));
    }
    if outputs.is_empty() {
        return Err("GLB contains no mesh geometry".into());
    }
    Ok(outputs)
}

pub fn convert(source: &Path, output: &Path, config: &Config) -> Result<Manifest> {
    if !config.metres_per_stud.is_finite() || config.metres_per_stud <= 0. {
        return Err("metresPerStud must be positive and finite".into());
    }
    let bytes = fs::read(source)?;
    let outputs = if bytes.starts_with(b"glTF") {
        read_glb(&bytes, config)?
    } else {
        crate::source_mesh::read(source, &bytes, config)?
    };

    // Validate and encode before creating output. Existing paths are never replaced.
    fs::create_dir(output)?;
    let result = (|| -> Result<Manifest> {
        let mut meshes = Vec::new();
        for artifact in outputs {
            fs::write(output.join(&artifact.entry.file), artifact.bytes)?;
            meshes.push(artifact.entry);
        }
        let manifest = Manifest {
            format: "roblox-static-mesh-bundle",
            version: 1,
            metres_per_stud: config.metres_per_stud,
            collision_generated: false,
            meshes,
        };
        fs::write(
            output.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(manifest)
    })();
    if let Err(error) = &result {
        fs::remove_dir_all(output)
            .map_err(|cleanup| format!("conversion failed: {error}; cleanup failed: {cleanup}"))?;
    }
    result
}
