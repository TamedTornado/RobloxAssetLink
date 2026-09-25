//! Rigid skeletal glTF animation: local TR -> rest-relative Roblox poses.
use crate::{
    Result,
    animation::{self, Clip, Frame, Pose, Rig, RigBinding},
    convert::load_gltf_buffers,
};
use glam::{Mat3, Mat4, Quat, Vec3};
use gltf::animation::{Interpolation, util::ReadOutputs};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub animation_index: usize,
    pub root_node: usize,
    pub metres_per_stud: f32,
    pub rigid_tolerance: f32,
    pub name: String,
    pub looped: bool,
    pub priority: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub artifact: animation::Manifest,
    pub config: Config,
    pub rig: Rig,
    pub rig_sha256: String,
    pub bind_rig_sha256: Option<String>,
}

struct Joint {
    name: String,
    parent: Option<usize>,
    children: Vec<usize>,
    translation: Vec3,
    rotation: Quat,
}

enum Values {
    IdentityScale,
    Translation(Vec<Vec3>),
    Rotation(Vec<Quat>),
}

struct Track {
    node: usize,
    times: Vec<f32>,
    values: Values,
}

fn quaternion(value: [f32; 4], tolerance: f32) -> Result<Quat> {
    let value = Quat::from_array(value);
    if !value.is_finite() || (value.length_squared() - 1.).abs() > tolerance {
        return Err("animation rotations must be finite unit quaternions".into());
    }
    Ok(value.normalize())
}

fn cframe(rotation: Quat, translation: Vec3) -> [f32; 12] {
    let rows = Mat3::from_quat(rotation).transpose().to_cols_array();
    [
        rows[0],
        rows[1],
        rows[2],
        rows[3],
        rows[4],
        rows[5],
        rows[6],
        rows[7],
        rows[8],
        translation.x,
        translation.y,
        translation.z,
    ]
}

fn skeleton(
    document: &gltf::Document,
    root: usize,
    scale: f32,
    tolerance: f32,
) -> Result<BTreeMap<usize, Joint>> {
    let root = document
        .nodes()
        .nth(root)
        .ok_or("animation rootNode is absent")?;
    let mut pending = vec![(root, None)];
    let mut joints = BTreeMap::new();
    let mut names = HashSet::new();
    while let Some((node, parent)) = pending.pop() {
        if joints.contains_key(&node.index()) {
            return Err("animation hierarchy repeats a node or contains a cycle".into());
        }
        let name = node
            .name()
            .filter(|s| !s.is_empty())
            .ok_or("animation joints require names")?;
        if !names.insert(name.to_owned()) || node.mesh().is_some() || node.camera().is_some() {
            return Err(
                "animation subtree requires uniquely named joints without meshes/cameras".into(),
            );
        }
        if let gltf::scene::Transform::Decomposed { rotation, .. } = node.transform() {
            quaternion(rotation, tolerance)?;
        }
        let matrix = Mat4::from_cols_array_2d(&node.transform().matrix());
        crate::rigid::validate(matrix, tolerance)?;
        let joint = Joint {
            name: name.to_owned(),
            parent,
            children: node.children().map(|child| child.index()).collect(),
            translation: matrix.w_axis.truncate() / scale,
            rotation: Quat::from_mat4(&matrix).normalize(),
        };
        pending.extend(node.children().map(|child| (child, Some(node.index()))));
        joints.insert(node.index(), joint);
    }
    Ok(joints)
}

fn tracks(
    animation: gltf::Animation<'_>,
    buffers: &[Vec<u8>],
    joints: &BTreeMap<usize, Joint>,
    scale: f32,
    tolerance: f32,
) -> Result<Vec<Track>> {
    let mut tracks = Vec::new();
    let mut targets = HashSet::new();
    for channel in animation.channels() {
        let node = channel.target().node().index();
        if !joints.contains_key(&node) {
            return Err("animation channel targets a node outside the selected rig".into());
        }
        if channel.sampler().interpolation() != Interpolation::Linear {
            return Err("this animation profile supports LINEAR interpolation only".into());
        }
        let reader = channel.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
        let times: Vec<_> = reader
            .read_inputs()
            .ok_or("animation input is unreadable")?
            .collect();
        if times.is_empty()
            || times.iter().any(|v| !v.is_finite() || *v < 0.)
            || times.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(
                "animation times must be finite, nonnegative and strictly increasing".into(),
            );
        }
        let (kind, values) = match reader
            .read_outputs()
            .ok_or("animation output is unreadable")?
        {
            ReadOutputs::Translations(values) => {
                let values: Vec<_> = values.map(|v| Vec3::from_array(v) / scale).collect();
                if values.len() != times.len() || values.iter().any(|v| !v.is_finite()) {
                    return Err("invalid animation translation samples".into());
                }
                ("translation", Values::Translation(values))
            }
            ReadOutputs::Rotations(values) => {
                let values: Vec<_> = values
                    .into_f32()
                    .map(|value| quaternion(value, tolerance))
                    .collect::<Result<_>>()?;
                if values.len() != times.len() {
                    return Err("animation rotation sample count mismatch".into());
                }
                ("rotation", Values::Rotation(values))
            }
            ReadOutputs::Scales(values) => {
                let values: Vec<_> = values.collect();
                if values.len() != times.len()
                    || values.iter().any(|value| {
                        !Vec3::from_array(*value).is_finite()
                            || !Vec3::from_array(*value).abs_diff_eq(Vec3::ONE, tolerance)
                    })
                {
                    return Err("scale and morph animation cannot be represented by this rigid pose profile".into());
                }
                ("scale", Values::IdentityScale)
            }
            ReadOutputs::MorphTargetWeights(_) => {
                return Err(
                    "scale and morph animation cannot be represented by this rigid pose profile"
                        .into(),
                );
            }
        };
        if !targets.insert((node, kind)) {
            return Err("duplicate animation channel for a joint property".into());
        }
        tracks.push(Track {
            node,
            times,
            values,
        });
    }
    if tracks.is_empty() {
        return Err("animation has no channels".into());
    }
    Ok(tracks)
}

fn interval(times: &[f32], time: f32) -> (usize, usize, f32) {
    let upper = times.partition_point(|value| *value <= time);
    if upper == 0 {
        return (0, 0, 0.);
    }
    if upper == times.len() {
        return (upper - 1, upper - 1, 0.);
    }
    let lower = upper - 1;
    (
        lower,
        upper,
        (time - times[lower]) / (times[upper] - times[lower]),
    )
}

fn pose(index: usize, joints: &BTreeMap<usize, Joint>, tracks: &[Track], time: f32) -> Pose {
    let joint = &joints[&index];
    let mut translation = joint.translation;
    let mut rotation = joint.rotation;
    for track in tracks.iter().filter(|track| track.node == index) {
        let (a, b, t) = interval(&track.times, time);
        match &track.values {
            Values::IdentityScale => {}
            Values::Translation(values) => translation = values[a].lerp(values[b], t),
            Values::Rotation(values) => rotation = values[a].slerp(values[b], t).normalize(),
        }
    }
    // Bone.Transform is an offset after the rest CFrame: rest^-1 * animated.
    let inverse_rest = joint.rotation.conjugate();
    Pose {
        name: joint.name.clone(),
        cframe: cframe(
            (inverse_rest * rotation).normalize(),
            inverse_rest * (translation - joint.translation),
        ),
        weight: 1.,
        easing_style: "Linear".into(),
        easing_direction: "In".into(),
        children: joint
            .children
            .iter()
            .map(|child| pose(*child, joints, tracks, time))
            .collect(),
    }
}

fn root_parent(document: &gltf::Document, config: &Config) -> Result<[f32; 12]> {
    let mut parents = BTreeMap::new();
    for node in document.nodes() {
        for child in node.children() {
            if parents.insert(child.index(), node.index()).is_some() {
                return Err("animation hierarchy has multiple parents".into());
            }
        }
    }
    let mut visited = HashSet::from([config.root_node]);
    let mut current = parents.get(&config.root_node).copied();
    let mut world = Mat4::IDENTITY;
    while let Some(index) = current {
        if !visited.insert(index) {
            return Err("animation ancestor hierarchy contains a cycle".into());
        }
        let node = document
            .nodes()
            .nth(index)
            .ok_or("animation ancestor is absent")?;
        if let gltf::scene::Transform::Decomposed { rotation, .. } = node.transform() {
            quaternion(rotation, config.rigid_tolerance)?;
        }
        world = Mat4::from_cols_array_2d(&node.transform().matrix()) * world;
        current = parents.get(&index).copied();
    }
    crate::rigid::validate(world, config.rigid_tolerance)?;
    let position = world.w_axis.truncate() / config.metres_per_stud;
    if !position.is_finite() {
        return Err("animation parent translation overflows selected units".into());
    }
    Ok(cframe(Quat::from_mat4(&world).normalize(), position))
}

pub fn read(source: &Path, config: &Config) -> Result<(Clip, Rig)> {
    crate::rigid::validate(Mat4::IDENTITY, config.rigid_tolerance)?;
    if !config.metres_per_stud.is_finite() || config.metres_per_stud <= 0. {
        return Err("metresPerStud must be finite and positive".into());
    }
    let gltf = gltf::Gltf::from_slice(&fs::read(source)?)?;
    if gltf.extensions_used().next().is_some() {
        return Err("animation glTF extensions are not supported".into());
    }
    let buffers = load_gltf_buffers(source, &gltf)?;
    let root_parent_cframe = root_parent(&gltf.document, config)?;
    let joints = skeleton(
        &gltf.document,
        config.root_node,
        config.metres_per_stud,
        config.rigid_tolerance,
    )?;
    let animation = gltf
        .animations()
        .nth(config.animation_index)
        .ok_or("animationIndex is absent")?;
    let tracks = tracks(
        animation,
        &buffers,
        &joints,
        config.metres_per_stud,
        config.rigid_tolerance,
    )?;
    let mut times: Vec<_> = tracks
        .iter()
        .flat_map(|track| track.times.iter().copied())
        .collect();
    times.sort_by(f32::total_cmp);
    times.dedup();
    let frames = times
        .into_iter()
        .map(|time| Frame {
            name: String::new(),
            time,
            poses: vec![pose(config.root_node, &joints, &tracks, time)],
            markers: vec![],
        })
        .collect();
    let rig = joints
        .into_iter()
        .map(|(node_index, joint)| RigBinding {
            node_index,
            name: joint.name,
            parent_node: joint.parent,
            rest_cframe: cframe(joint.rotation, joint.translation),
        })
        .collect();
    Ok((
        Clip {
            name: config.name.clone(),
            looped: config.looped,
            priority: config.priority.clone(),
            frames,
        },
        Rig {
            root_parent_cframe,
            joints: rig,
        },
    ))
}

pub fn convert(source: &Path, output: &Path, config: Config) -> Result<Manifest> {
    convert_with_bind_pose(source, output, config, None)
}

pub(crate) fn convert_with_bind_pose(
    source: &Path,
    output: &Path,
    config: Config,
    target: Option<&crate::skin_import::Manifest>,
) -> Result<Manifest> {
    let (mut clip, mut rig) = read(source, &config)?;
    if let Some(target) = target {
        crate::animation_rebase::apply(
            &mut clip,
            &mut rig,
            target,
            config.metres_per_stud,
            config.rigid_tolerance,
        )?;
    }
    let rig_sha256 = rig.sha256()?;
    let artifact = animation::write_clip(&fs::read(source)?, &clip, output)?;
    Ok(Manifest {
        artifact,
        config,
        rig,
        rig_sha256,
        bind_rig_sha256: target.map(|skin| skin.rig_sha256.clone()),
    })
}
