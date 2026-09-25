//! Native rigid-bind skin import; glTF mesh-node transforms do not affect skinning.
use crate::{IDENTITY, Result, convert, skin};
use glam::{Mat4, Vec4};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub mesh_node: usize,
    pub metres_per_stud: f32,
    pub cull_distance_metres: f32,
    /// Absolute numerical tolerance for rigid matrix validation, not a mesh limit.
    pub rigid_tolerance: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub source_node: usize,
    pub name: String,
    pub parent: Option<u16>,
    pub world_bind_cframe: [f32; 12],
    pub local_bind_cframe: [f32; 12],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub file: String,
    pub sha256: String,
    pub source_vertices: usize,
    pub triangles: usize,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub double_sided: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: &'static str,
    pub metres_per_stud: f32,
    pub rig: Vec<Binding>,
    pub meshes: Vec<Entry>,
    pub engine_verified: bool,
}

fn validate_config(config: &Config) -> Result<()> {
    if !config.metres_per_stud.is_finite()
        || config.metres_per_stud <= 0.
        || !config.cull_distance_metres.is_finite()
        || config.cull_distance_metres < 0.
        || !config.rigid_tolerance.is_finite()
        || config.rigid_tolerance <= 0.
        || config.rigid_tolerance >= 1.
    {
        return Err("invalid skin unit scale, culling distance or rigid-matrix tolerance".into());
    }
    Ok(())
}

fn cframe(matrix: Mat4, config: &Config) -> Result<[f32; 12]> {
    let tolerance = config.rigid_tolerance;
    let x = matrix.x_axis.truncate();
    let y = matrix.y_axis.truncate();
    let z = matrix.z_axis.truncate();
    let bottom = Vec4::new(
        matrix.x_axis.w,
        matrix.y_axis.w,
        matrix.z_axis.w,
        matrix.w_axis.w,
    );
    if !matrix.is_finite()
        || !bottom.abs_diff_eq(Vec4::W, tolerance)
        || (x.length_squared() - 1.).abs() > tolerance
        || (y.length_squared() - 1.).abs() > tolerance
        || (z.length_squared() - 1.).abs() > tolerance
        || x.dot(y).abs() > tolerance
        || x.dot(z).abs() > tolerance
        || y.dot(z).abs() > tolerance
        || (x.cross(y).dot(z) - 1.).abs() > tolerance
    {
        return Err(
            "skin bind matrices must be finite rigid transforms without scale, shear or reflection"
                .into(),
        );
    }
    let position = matrix.w_axis.truncate() / config.metres_per_stud;
    if !position.is_finite() {
        return Err("skin bind translation overflows selected units".into());
    }
    Ok([
        x.x, y.x, z.x, x.y, y.y, z.y, x.z, y.z, z.z, position.x, position.y, position.z,
    ])
}

struct Skeleton {
    bones: Vec<skin::Bone>,
    bindings: Vec<Binding>,
    // glTF JOINTS_0 indexes the skin's joint list, not document nodes.
    remap: BTreeMap<usize, u16>,
}

fn skeleton(
    document: &gltf::Document,
    source: gltf::Skin<'_>,
    buffers: &[Vec<u8>],
    config: &Config,
) -> Result<Skeleton> {
    let joints: Vec<_> = source.joints().collect();
    let node_to_joint: BTreeMap<_, _> = joints
        .iter()
        .enumerate()
        .map(|(i, n)| (n.index(), i))
        .collect();
    if joints.is_empty() || node_to_joint.len() != joints.len() {
        return Err("skin needs distinct joint nodes".into());
    }
    let mut parents = BTreeMap::new();
    for node in document.nodes() {
        for child in node.children() {
            if parents.insert(child.index(), node.index()).is_some() {
                return Err("skin source hierarchy has multiple parents".into());
            }
        }
    }
    // Traverse ancestry to reject cycles and intermediate non-joint nodes that
    // would otherwise disappear from an animation binding.
    let mut joint_parents = Vec::new();
    for joint in &joints {
        let mut visited = HashSet::from([joint.index()]);
        let mut ancestor = parents.get(&joint.index()).copied();
        let direct = ancestor.and_then(|node| node_to_joint.get(&node).copied());
        while let Some(node) = ancestor {
            if !visited.insert(node) {
                return Err("skin source hierarchy contains a cycle".into());
            }
            if direct.is_none() && node_to_joint.contains_key(&node) {
                return Err(
                    "non-joint nodes between skin joints require an explicit rig conversion".into(),
                );
            }
            ancestor = parents.get(&node).copied();
        }
        joint_parents.push(direct);
    }
    let reader = source.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
    let inverse: Vec<_> = reader
        .read_inverse_bind_matrices()
        .map(|values| {
            values
                .map(|value| Mat4::from_cols_array_2d(&value))
                .collect()
        })
        .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
    if inverse.len() != joints.len() {
        return Err("skin inverse-bind matrix count mismatch".into());
    }
    let mut world = Vec::new();
    for matrix in inverse {
        cframe(matrix, config)?;
        let bind = matrix.inverse();
        cframe(bind, config)?;
        world.push(bind);
    }
    let mut result = Skeleton {
        bones: Vec::new(),
        bindings: Vec::new(),
        remap: BTreeMap::new(),
    };
    while result.bones.len() < joints.len() {
        let previous = result.bones.len();
        for (index, joint) in joints.iter().enumerate() {
            if result.remap.contains_key(&index)
                || joint_parents[index].is_some_and(|p| !result.remap.contains_key(&p))
            {
                continue;
            }
            let name = joint
                .name()
                .filter(|n| !n.is_empty())
                .ok_or("skin joints require names")?
                .to_owned();
            let parent = joint_parents[index].map(|p| result.remap[&p]);
            let bind = cframe(world[index], config)?;
            let local =
                joint_parents[index].map_or(world[index], |p| world[p].inverse() * world[index]);
            let native_index = u16::try_from(result.bones.len())?;
            result.remap.insert(index, native_index);
            result.bones.push(skin::Bone {
                name: name.clone(),
                parent,
                bind_cframe: bind,
                cull_distance: config.cull_distance_metres / config.metres_per_stud,
            });
            result.bindings.push(Binding {
                source_node: joint.index(),
                name,
                parent,
                world_bind_cframe: bind,
                local_bind_cframe: cframe(local, config)?,
            });
        }
        if previous == result.bones.len() {
            return Err("skin joint hierarchy cannot be ordered".into());
        }
    }
    Ok(result)
}

pub fn convert(source: &Path, output: &Path, config: &Config) -> Result<Manifest> {
    validate_config(config)?;
    let gltf = gltf::Gltf::from_slice(&fs::read(source)?)?;
    if gltf.extensions_used().next().is_some() {
        return Err("skin glTF extensions are not supported".into());
    }
    let buffers = convert::load_gltf_buffers(source, &gltf)?;
    let node = gltf
        .nodes()
        .nth(config.mesh_node)
        .ok_or("skin meshNode is absent")?;
    let source_skin = node.skin().ok_or("selected meshNode has no skin")?;
    let source_mesh = node.mesh().ok_or("selected meshNode has no mesh")?;
    let skeleton = skeleton(&gltf.document, source_skin, &buffers, config)?;
    let mut geometry = Vec::new();
    // The glTF specification explicitly ignores the skinned mesh-node transform.
    convert::visit(
        node,
        IDENTITY,
        &buffers,
        config.metres_per_stud,
        convert::GeometryProfile::Skin,
        &mut geometry,
    )?;
    let mut files = Vec::new();
    let mut entries = Vec::new();
    for (primitive, mesh) in source_mesh.primitives().zip(geometry) {
        let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
        let joints: Vec<_> = reader
            .read_joints(0)
            .ok_or("skin requires JOINTS_0")?
            .into_u16()
            .collect();
        let weights: Vec<_> = reader
            .read_weights(0)
            .ok_or("skin requires WEIGHTS_0")?
            .into_f32()
            .collect();
        if joints.len() != mesh.geometry.vertices.len() || weights.len() != joints.len() {
            return Err("skin requires one joint/weight envelope per source vertex".into());
        }
        let mut influences = Vec::new();
        for (joints, weights) in joints.into_iter().zip(weights) {
            let mut mapped = [0; 4];
            for (slot, index) in joints.into_iter().enumerate() {
                mapped[slot] = *skeleton
                    .remap
                    .get(&usize::from(index))
                    .ok_or("skin joint index outside joint list")?;
            }
            influences.push(skin::Influences {
                joints: mapped,
                weights,
            });
        }
        let bytes = skin::encode(&mesh.geometry, &influences, &skeleton.bones)?;
        let entry = Entry {
            file: mesh.entry.file.clone(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            source_vertices: mesh.geometry.vertices.len(),
            triangles: mesh.geometry.triangles.len(),
            base_color: mesh.entry.base_color,
            metallic: mesh.entry.metallic,
            roughness: mesh.entry.roughness,
            double_sided: mesh.entry.double_sided,
        };
        files.push((mesh.entry.file, bytes));
        entries.push(entry);
    }
    if entries.is_empty() {
        return Err("skin mesh has no primitives".into());
    }
    let manifest = Manifest {
        format: "roblox-skinned-mesh-v4.01",
        metres_per_stud: config.metres_per_stud,
        rig: skeleton.bindings,
        meshes: entries,
        engine_verified: false,
    };
    fs::create_dir(output)?;
    let result = (|| -> Result<()> {
        for (name, bytes) in files {
            fs::write(output.join(name), bytes)?;
        }
        fs::write(
            output.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        fs::remove_dir_all(output).map_err(|cleanup| {
            format!("skin conversion failed: {error}; cleanup failed: {cleanup}")
        })?;
        return Err(error);
    }
    Ok(manifest)
}
