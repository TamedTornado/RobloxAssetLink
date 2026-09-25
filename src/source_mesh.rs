//! In-process FBX/OBJ input adapter; output encoding remains format independent.

use crate::{
    Result,
    convert::{Config, Entry, Output},
    mesh,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub(crate) fn read(path: &Path, bytes: &[u8], config: &Config) -> Result<Vec<Output>> {
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (format, obj_scale) = match extension.as_str() {
        "fbx" => (ufbx::FileFormat::Fbx, 1.),
        "obj" => {
            let scale = config
                .obj_metres_per_unit
                .ok_or("OBJ requires objMetresPerUnit in JSON configuration")?;
            if !scale.is_finite() || scale <= 0. {
                return Err("objMetresPerUnit must be positive and finite".into());
            }
            (ufbx::FileFormat::Obj, scale)
        }
        _ => return Err("unsupported source format; currently supported: GLB, FBX, OBJ".into()),
    };
    // Read from memory and do not load external files. No implicit filesystem
    // traversal or network access through material/texture references.
    let scene = ufbx::load_memory(
        bytes,
        ufbx::LoadOpts {
            file_format: format,
            target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
            target_unit_meters: 1.,
            obj_unit_meters: obj_scale,
            obj_axes: ufbx::CoordinateAxes::right_handed_y_up(),
            load_external_files: false,
            use_blender_pbr_material: true,
            ..Default::default()
        },
    )
    .map_err(|e| format!("source parse failed: {}", e.description))?;
    if !scene.anim_stacks.is_empty() {
        return Err("static conversion does not support animation".into());
    }
    if format == ufbx::FileFormat::Obj
        && std::str::from_utf8(bytes)?
            .lines()
            .any(|line| line.trim_start().starts_with("mtllib "))
    {
        return Err("OBJ external material libraries are not supported yet".into());
    }
    read_nodes(&scene, config, None)
}

pub(crate) fn read_skin_geometry(
    scene: &ufbx::Scene,
    config: &Config,
    node: u32,
    bind: &ufbx::Matrix,
) -> Result<Vec<Output>> {
    read_nodes(scene, config, Some((node, bind)))
}

fn read_nodes(
    scene: &ufbx::Scene,
    config: &Config,
    skin_bind: Option<(u32, &ufbx::Matrix)>,
) -> Result<Vec<Output>> {
    let mut outputs = Vec::new();
    for node in &scene.nodes {
        if skin_bind.is_some_and(|(selected, _)| selected != node.element.typed_id) {
            continue;
        }
        let Some(source) = &node.mesh else {
            continue;
        };
        if (skin_bind.is_none() && !source.all_deformers.is_empty())
            || source.uv_sets.len() > 1
            || source.color_sets.len() > 1
            || source.num_line_faces != 0
            || source.num_point_faces != 0
            || source.num_empty_faces != 0
            || source.face_hole.iter().any(|v| *v)
        {
            return Err("unsupported deformer, vertex channel or non-surface face".into());
        }
        if !source.vertex_normal.exists || !source.vertex_uv.exists {
            return Err("normals and UV0 required".into());
        }
        let transform = skin_bind.map_or(&node.geometry_to_world, |(_, bind)| bind);
        let determinant = ufbx::matrix_determinant(transform);
        if !determinant.is_finite() || determinant == 0. {
            return Err("singular source transform".into());
        }
        let normal_transform = ufbx::matrix_for_normals(transform);
        let mut groups: BTreeMap<u32, (mesh::Mesh, Vec<usize>)> = BTreeMap::new();
        let mut indices = Vec::new();
        for (face_index, face) in source.faces.iter().enumerate() {
            let count = ufbx::triangulate_face_vec(&mut indices, source, *face) as usize * 3;
            if count == 0 {
                return Err("face could not be triangulated".into());
            }
            let material = source.face_material.get(face_index).copied().unwrap_or(0);
            let (geometry, source_points) = groups.entry(material).or_insert_with(|| {
                (
                    mesh::Mesh {
                        vertices: Vec::new(),
                        triangles: Vec::new(),
                    },
                    Vec::new(),
                )
            });
            for triangle in indices[..count].chunks_exact(3) {
                let start = u32::try_from(geometry.vertices.len())?;
                for index in triangle {
                    source_points.push(source.vertex_indices[*index as usize] as usize);
                    let position = ufbx::transform_position(
                        transform,
                        source.vertex_position[*index as usize],
                    );
                    let normal = ufbx::transform_direction(
                        &normal_transform,
                        source.vertex_normal[*index as usize],
                    );
                    let length =
                        (normal.x * normal.x + normal.y * normal.y + normal.z * normal.z).sqrt();
                    let normal = [normal.x, normal.y, normal.z].map(|v| (v / length) as f32);
                    let color = if source.vertex_color.exists {
                        let color = source.vertex_color[*index as usize];
                        crate::vertex_attributes::color(
                            [color.x, color.y, color.z, color.w].map(|v| v as f32),
                        )?
                    } else {
                        [255; 4]
                    };
                    let tangent = if source.vertex_tangent.exists {
                        if !source.vertex_bitangent.exists {
                            return Err(
                                "tangent data requires source bitangents to preserve handedness"
                                    .into(),
                            );
                        }
                        let t = source.vertex_tangent[*index as usize];
                        let b = source.vertex_bitangent[*index as usize];
                        let n = source.vertex_normal[*index as usize];
                        let handedness = crate::dot(
                            crate::cross(
                                [n.x as f32, n.y as f32, n.z as f32],
                                [t.x as f32, t.y as f32, t.z as f32],
                            ),
                            [b.x as f32, b.y as f32, b.z as f32],
                        )
                        .signum();
                        let t = ufbx::transform_direction(transform, t);
                        // FBX/OBJ UV V is flipped at this boundary, reversing
                        // the bitangent sign in addition to any reflected transform.
                        crate::vertex_attributes::tangent(
                            crate::IDENTITY,
                            normal,
                            [t.x as f32, t.y as f32, t.z as f32, -handedness],
                            determinant < 0.,
                        )?
                    } else {
                        [0; 4]
                    };
                    let uv = source.vertex_uv[*index as usize];
                    geometry.vertices.push(mesh::Vertex {
                        position: [position.x, position.y, position.z]
                            .map(|v| (v / f64::from(config.metres_per_stud)) as f32),
                        normal,
                        uv: [uv.x as f32, (1. - uv.y) as f32],
                        tangent,
                        color,
                    });
                }
                geometry.triangles.push(if determinant < 0. {
                    [start, start + 2, start + 1]
                } else {
                    [start, start + 1, start + 2]
                });
            }
        }
        for (material_index, (geometry, source_points)) in groups {
            let mut color = [1.; 4];
            let mut metallic = 0.;
            let mut roughness = 1.;
            if let Some(material) = node.materials.get(material_index as usize) {
                let emission = material.pbr.emission_color.value_vec4;
                let emission_factor = material.pbr.emission_factor.value_vec4.x;
                let transmission_factor = material.pbr.transmission_factor.value_vec4.x;
                // Legacy FBX transparency is color * factor. An opaque Lambert
                // material normally has black transparency and factor one.
                let transmission = if matches!(
                    material.shader_type,
                    ufbx::ShaderType::FbxLambert | ufbx::ShaderType::FbxPhong
                ) {
                    let color = material.pbr.transmission_color.value_vec4;
                    [color.x, color.y, color.z]
                        .iter()
                        .any(|value| value * transmission_factor != 0.)
                } else {
                    transmission_factor != 0.
                };
                if !material.textures.is_empty()
                    || transmission
                    || [emission.x, emission.y, emission.z]
                        .iter()
                        .any(|value| value * emission_factor != 0.)
                {
                    return Err(
                        "textured, emissive and transmission materials require material conversion"
                            .into(),
                    );
                }
                let base = material.pbr.base_color.value_vec4;
                let opacity = if material.pbr.opacity.has_value {
                    material.pbr.opacity.value_vec4.x
                } else {
                    1.
                };
                color = [base.x as f32, base.y as f32, base.z as f32, opacity as f32];
                metallic = material.pbr.metalness.value_vec4.x as f32;
                roughness = material.pbr.roughness.value_vec4.x as f32;
            }
            let bytes = mesh::encode(&geometry)?;
            outputs.push(Output {
                source_points,
                entry: Entry {
                    file: format!(
                        "node-{}-primitive-{material_index}.mesh",
                        node.element.typed_id
                    ),
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    node_index: node.element.typed_id as usize,
                    primitive_index: material_index as usize,
                    name: Some(node.element.name.to_string()),
                    vertices: geometry.vertices.len(),
                    triangles: geometry.triangles.len(),
                    base_color: color,
                    metallic,
                    roughness,
                    double_sided: false,
                    collision: None,
                    material: None,
                },
                bytes,
                geometry,
            });
        }
    }
    if outputs.is_empty() {
        return Err("source contains no surface geometry".into());
    }
    Ok(outputs)
}
