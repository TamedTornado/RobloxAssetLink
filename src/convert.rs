//! First offline conversion profile: static, untextured GLB to native meshes.

use crate::{IDENTITY, Matrix, Result, dot, mesh, multiply, normal_matrix, point};
use base64::Engine;
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
    pub collision: Option<CollisionEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionEntry {
    pub file: String,
    pub sha256: String,
    pub hull_count: usize,
    pub format: &'static str,
    pub engine_verified: bool,
}

pub(crate) struct Output {
    /// Original source point per output vertex (needed after FBX triangulation).
    pub source_points: Vec<usize>,
    pub entry: Entry,
    pub bytes: Vec<u8>,
    pub geometry: mesh::Mesh,
}

#[derive(Clone, Copy)]
pub(crate) enum GeometryProfile {
    Static,
    Skin,
}

pub(crate) fn visit(
    node: gltf::Node<'_>,
    transform: Matrix,
    buffers: &[Vec<u8>],
    scale: f32,
    profile: GeometryProfile,
    outputs: &mut Vec<Output>,
) -> Result<()> {
    if (node.skin().is_some() && matches!(profile, GeometryProfile::Static))
        || node.camera().is_some()
    {
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
                if matches!(profile, GeometryProfile::Skin)
                    && matches!(
                        semantic,
                        gltf::Semantic::Joints(0) | gltf::Semantic::Weights(0)
                    )
                {
                    continue;
                }
                if !matches!(
                    semantic,
                    gltf::Semantic::Positions
                        | gltf::Semantic::Normals
                        | gltf::Semantic::TexCoords(0)
                        | gltf::Semantic::Tangents
                        | gltf::Semantic::Colors(0)
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
            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or("positions required")?
                .collect();
            let normals: Vec<_> = reader.read_normals().ok_or("normals required")?.collect();
            let uvs: Vec<_> = reader
                .read_tex_coords(0)
                .map(|values| values.into_f32().collect())
                // This profile already rejects every texture dependency. UVs
                // have no source meaning when absent on an untextured mesh.
                .unwrap_or_else(|| vec![[0.; 2]; positions.len()]);
            if positions.len() != normals.len() || positions.len() != uvs.len() {
                return Err("mesh attribute counts do not match".into());
            }
            let tangents: Option<Vec<_>> = reader.read_tangents().map(Iterator::collect);
            let colors: Option<Vec<_>> = reader.read_colors(0).map(|c| c.into_rgba_f32().collect());
            if tangents
                .as_ref()
                .is_some_and(|v| v.len() != positions.len())
                || colors.as_ref().is_some_and(|v| v.len() != positions.len())
            {
                return Err("tangent/color attribute counts do not match positions".into());
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
                .enumerate()
                .map(|(index, ((position, normal), uv))| -> Result<_> {
                    let normal: [f32; 3] = std::array::from_fn(|r| {
                        (0..3).map(|c| normal_transform[c][r] * normal[c]).sum()
                    });
                    let length = dot(normal, normal).sqrt();
                    let normal = normal.map(|v| v / length);
                    let tangent = tangents
                        .as_ref()
                        .map(|values| {
                            crate::vertex_attributes::tangent(
                                transform,
                                normal,
                                values[index],
                                mirrored,
                            )
                        })
                        .transpose()?
                        .unwrap_or([0; 4]);
                    let color = colors
                        .as_ref()
                        .map(|values| crate::vertex_attributes::color(values[index]))
                        .transpose()?
                        .unwrap_or([255; 4]);
                    Ok(mesh::Vertex {
                        position: point(transform, position, scale),
                        normal,
                        uv,
                        tangent,
                        color,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
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
                source_points: (0..geometry.vertices.len()).collect(),
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
                    collision: None,
                },
                bytes,
                geometry,
            });
        }
    }
    Ok(())
}

fn read_gltf(source: &Path, bytes: &[u8], config: &Config) -> Result<Vec<Output>> {
    let gltf = gltf::Gltf::from_slice(bytes)?;
    if gltf.extensions_used().next().is_some() || gltf.animations().next().is_some() {
        return Err("this static profile does not yet support extensions or animations".into());
    }
    let buffers = load_gltf_buffers(source, &gltf)?;
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
            &buffers,
            config.metres_per_stud,
            GeometryProfile::Static,
            &mut outputs,
        )?;
        pending.extend(node.children().map(|child| (child, transform)));
    }
    if outputs.is_empty() {
        return Err("GLB contains no mesh geometry".into());
    }
    Ok(outputs)
}

pub(crate) fn load_gltf_buffers(source: &Path, gltf: &gltf::Gltf) -> Result<Vec<Vec<u8>>> {
    let root = source
        .canonicalize()?
        .parent()
        .ok_or("source parent missing")?
        .to_owned();
    let mut buffers = Vec::new();
    for buffer in gltf.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => gltf.blob.clone().ok_or("GLB binary chunk required")?,
            gltf::buffer::Source::Uri(uri) => read_buffer(&root, uri)?,
        };
        if data.len() < buffer.length() {
            return Err("buffer is shorter than declared byteLength".into());
        }
        buffers.push(data);
    }
    Ok(buffers)
}

fn read_buffer(root: &Path, uri: &str) -> Result<Vec<u8>> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (metadata, content) = data.split_once(',').ok_or("invalid data URI")?;
        if !matches!(
            metadata,
            "application/octet-stream;base64" | "application/gltf-buffer;base64"
        ) {
            return Err("unsupported buffer data URI encoding".into());
        }
        return Ok(base64::engine::general_purpose::STANDARD.decode(content)?);
    }
    let path = percent_encoding::percent_decode_str(uri).decode_utf8()?;
    if path.contains([':', '\\', '?', '#', '\0']) || Path::new(path.as_ref()).is_absolute() {
        return Err("buffer URI must be a local relative path; network access is forbidden".into());
    }
    let path = root.join(path.as_ref()).canonicalize()?;
    if !path.starts_with(root) {
        return Err("buffer path escapes the source directory".into());
    }
    Ok(fs::read(path)?)
}

pub fn convert(source: &Path, output: &Path, config: &Config) -> Result<Manifest> {
    convert_with_collision(source, output, config, None)
}

pub fn convert_with_collision(
    source: &Path,
    output: &Path,
    config: &Config,
    recipe: Option<&crate::collision::Recipe>,
) -> Result<Manifest> {
    if !config.metres_per_stud.is_finite() || config.metres_per_stud <= 0. {
        return Err("metresPerStud must be positive and finite".into());
    }
    let bytes = fs::read(source)?;
    let mut outputs = if bytes.starts_with(b"glTF")
        || source
            .extension()
            .is_some_and(|v| v.eq_ignore_ascii_case("gltf"))
    {
        read_gltf(source, &bytes, config)?
    } else {
        crate::source_mesh::read(source, &bytes, config)?
    };

    let mut collision_files = Vec::new();
    if let Some(recipe) = recipe {
        for artifact in &mut outputs {
            let hulls = crate::collision::cook(&artifact.geometry, recipe)?;
            let data = crate::collision::encode(&hulls)?;
            let file = format!("{}.physics", artifact.entry.file);
            artifact.entry.collision = Some(CollisionEntry {
                file: file.clone(),
                sha256: format!("{:x}", Sha256::digest(&data)),
                hull_count: hulls.len(),
                format: "CSGPHS-v5",
                engine_verified: false,
            });
            collision_files.push((file, data));
        }
    }

    // Validate and encode before creating output. Existing paths are never replaced.
    fs::create_dir(output)?;
    let result = (|| -> Result<Manifest> {
        for (file, data) in collision_files {
            fs::write(output.join(file), data)?;
        }
        let mut meshes = Vec::new();
        for artifact in outputs {
            fs::write(output.join(&artifact.entry.file), artifact.bytes)?;
            meshes.push(artifact.entry);
        }
        let manifest = Manifest {
            format: "roblox-static-mesh-bundle",
            version: 1,
            metres_per_stud: config.metres_per_stud,
            collision_generated: recipe.is_some(),
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
