//! FBX linear skin import using in-process ufbx, sharing the native skin writer.
use crate::{
    Result, skin,
    skin_import::{self, Binding, Config, Entry, Manifest},
};
use glam::Mat4;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn matrix(value: &ufbx::Matrix) -> Mat4 {
    Mat4::from_cols_array(&[
        value.m00 as f32,
        value.m10 as f32,
        value.m20 as f32,
        0.,
        value.m01 as f32,
        value.m11 as f32,
        value.m21 as f32,
        0.,
        value.m02 as f32,
        value.m12 as f32,
        value.m22 as f32,
        0.,
        value.m03 as f32,
        value.m13 as f32,
        value.m23 as f32,
        1.,
    ])
}

fn influence(
    weights: &[ufbx::SkinWeight],
    remap: &BTreeMap<usize, u16>,
) -> Result<skin::Influences> {
    let mut result = skin::Influences {
        joints: [0; 4],
        weights: [0.; 4],
    };
    let mut slot = 0;
    for weight in weights {
        if !weight.weight.is_finite() || weight.weight < 0. {
            return Err("invalid FBX skin weight".into());
        }
        if weight.weight == 0. {
            continue;
        }
        if slot == result.joints.len() {
            return Err(
                "FBX skin exceeds native four-influence envelope; no weights were discarded".into(),
            );
        }
        result.joints[slot] = *remap
            .get(&(weight.cluster_index as usize))
            .ok_or("FBX cluster index invalid")?;
        result.weights[slot] = weight.weight as f32;
        slot += 1;
    }
    if slot == 0 {
        return Err("FBX skin vertex has no positive weights".into());
    }
    Ok(result)
}

pub(crate) fn convert(source: &Path, output: &Path, config: &Config) -> Result<Manifest> {
    let scene = ufbx::load_memory(
        &fs::read(source)?,
        ufbx::LoadOpts {
            file_format: ufbx::FileFormat::Fbx,
            target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
            target_unit_meters: 1.,
            space_conversion: ufbx::SpaceConversion::ModifyGeometry,
            load_external_files: false,
            use_blender_pbr_material: true,
            ..Default::default()
        },
    )
    .map_err(|error| format!("FBX skin parse failed: {}", error.description))?;
    let node = scene
        .nodes
        .iter()
        .find(|node| node.element.typed_id as usize == config.mesh_node)
        .ok_or("skin meshNode is absent")?;
    let mesh = node.mesh.as_ref().ok_or("skin meshNode has no mesh")?;
    if mesh.skin_deformers.len() != 1
        || !mesh.blend_deformers.is_empty()
        || !mesh.cache_deformers.is_empty()
    {
        return Err(
            "FBX skin requires one skin deformer without blend shapes or vertex caches".into(),
        );
    }
    let deformer = &mesh.skin_deformers[0];
    if deformer.skinning_method != ufbx::SkinningMethod::Linear {
        return Err(
            "FBX skin requires linear blend skinning; dual-quaternion blending is unsupported"
                .into(),
        );
    }
    let first = deformer
        .clusters
        .first()
        .ok_or("FBX skin has no bone clusters")?;
    let geometry_bind = ufbx::matrix_mul(&first.bind_to_world, &first.geometry_to_bone);
    let mut source_nodes = BTreeMap::new();
    let mut bind_world = Vec::new();
    for (index, cluster) in deformer.clusters.iter().enumerate() {
        let bone = cluster
            .bone_node
            .as_ref()
            .ok_or("FBX skin cluster has no bone")?;
        if source_nodes.insert(bone.element.typed_id, index).is_some() {
            return Err("FBX skin repeats a bone cluster".into());
        }
        let candidate = ufbx::matrix_mul(&cluster.bind_to_world, &cluster.geometry_to_bone);
        if !matrix(&candidate).abs_diff_eq(matrix(&geometry_bind), config.rigid_tolerance) {
            return Err("FBX bone clusters disagree on geometry bind space".into());
        }
        let bind = matrix(&cluster.bind_to_world);
        skin_import::cframe(bind, config)
            .map_err(|error| format!("FBX bone {}: {error}", bone.element.name))?;
        bind_world.push(bind);
    }
    let mut parents = Vec::new();
    for cluster in &deformer.clusters {
        let bone = cluster.bone_node.as_ref().ok_or("FBX bone missing")?;
        let direct = bone
            .parent
            .as_ref()
            .and_then(|parent| source_nodes.get(&parent.element.typed_id))
            .copied();
        let mut ancestor = bone.parent.as_deref();
        while let Some(parent) = ancestor {
            if direct.is_none() && source_nodes.contains_key(&parent.element.typed_id) {
                return Err(
                    "FBX non-cluster ancestor between bones requires explicit rig conversion"
                        .into(),
                );
            }
            ancestor = parent.parent.as_deref();
        }
        parents.push(direct);
    }
    let mut remap = BTreeMap::new();
    let mut bones = Vec::new();
    let mut rig = Vec::new();
    while bones.len() < deformer.clusters.len() {
        let previous = bones.len();
        for (index, cluster) in deformer.clusters.iter().enumerate() {
            if remap.contains_key(&index)
                || parents[index].is_some_and(|parent| !remap.contains_key(&parent))
            {
                continue;
            }
            let node = cluster.bone_node.as_ref().ok_or("FBX bone missing")?;
            let name = node.element.name.to_string();
            let parent = parents[index].map(|parent| remap[&parent]);
            let bind = skin_import::cframe(bind_world[index], config)?;
            let local = parents[index].map_or(bind_world[index], |parent| {
                bind_world[parent].inverse() * bind_world[index]
            });
            remap.insert(index, u16::try_from(bones.len())?);
            bones.push(skin::Bone {
                name: name.clone(),
                parent,
                bind_cframe: bind,
                cull_distance: config.cull_distance_metres / config.metres_per_stud,
            });
            rig.push(Binding {
                source_node: node.element.typed_id as usize,
                name,
                parent,
                world_bind_cframe: bind,
                local_bind_cframe: skin_import::cframe(local, config)?,
            });
        }
        if previous == bones.len() {
            return Err("FBX skin hierarchy cannot be ordered".into());
        }
    }
    let geometry_config = crate::convert::Config {
        materials: None,
        metres_per_stud: config.metres_per_stud,
        obj_metres_per_unit: None,
    };
    let geometries = crate::source_mesh::read_skin_geometry(
        &scene,
        &geometry_config,
        node.element.typed_id,
        &geometry_bind,
    )?;
    let mut entries = Vec::new();
    let mut files = Vec::new();
    for geometry in geometries {
        let mut influences = Vec::new();
        for point in &geometry.source_points {
            let vertex = deformer
                .vertices
                .get(*point)
                .ok_or("FBX skin weight vertex is absent")?;
            let begin = vertex.weight_begin as usize;
            let end = begin
                .checked_add(vertex.num_weights as usize)
                .ok_or("FBX skin weight range overflow")?;
            let weights = deformer
                .weights
                .get(begin..end)
                .ok_or("FBX skin weight range is invalid")?;
            influences.push(influence(weights, &remap)?);
        }
        let bytes = skin::encode(&geometry.geometry, &influences, &bones)?;
        let entry = geometry.entry;
        entries.push(Entry {
            file: entry.file.clone(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            source_vertices: geometry.geometry.vertices.len(),
            triangles: geometry.geometry.triangles.len(),
            base_color: entry.base_color,
            metallic: entry.metallic,
            roughness: entry.roughness,
            double_sided: entry.double_sided,
        });
        files.push((entry.file, bytes));
    }
    skin_import::write(output, config.metres_per_stud, rig, entries, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelopes_reject_invalid_or_excess_influences_without_truncation() {
        let remap = BTreeMap::from([(0, 2), (1, 0), (2, 1), (3, 3), (4, 4)]);
        let weights: Vec<_> = (0..5)
            .map(|cluster_index| ufbx::SkinWeight {
                cluster_index,
                weight: 0.2,
            })
            .collect();
        assert!(
            influence(&weights, &remap)
                .err()
                .unwrap()
                .to_string()
                .contains("four-influence")
        );
        let packed = influence(&weights[..4], &remap).unwrap();
        assert_eq!(packed.joints, [2, 0, 1, 3]);
        for weight in [-1., f64::NAN, f64::INFINITY, 0.] {
            assert!(
                influence(
                    &[ufbx::SkinWeight {
                        cluster_index: 0,
                        weight
                    }],
                    &remap
                )
                .is_err()
            );
        }
        assert!(
            influence(
                &[ufbx::SkinWeight {
                    cluster_index: 99,
                    weight: 1.
                }],
                &remap
            )
            .is_err()
        );
    }
}
