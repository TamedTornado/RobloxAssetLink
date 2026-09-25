//! In-process FBX motion baking into explicitly sampled native rigid poses.
use crate::{
    Result,
    animation::{self, Clip, Frame, Pose, Rig, RigBinding},
};
use glam::{Mat3, Mat4, Quat, Vec3};
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
    pub resample_rate: f64,
    pub minimum_sample_rate: f64,
    pub max_keyframe_segments: usize,
    pub max_output_frames: usize,
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
    pub source_time_begin: f64,
    pub sampled_approximation: bool,
    pub unchanged_properties: Vec<String>,
}

struct Joint {
    name: String,
    parent: Option<usize>,
    children: Vec<usize>,
    rest: Mat4,
}

fn matrix(value: &ufbx::Transform, config: &Config) -> Result<Mat4> {
    let scale = Vec3::new(
        value.scale.x as f32,
        value.scale.y as f32,
        value.scale.z as f32,
    );
    let rotation = Quat::from_xyzw(
        value.rotation.x as f32,
        value.rotation.y as f32,
        value.rotation.z as f32,
        value.rotation.w as f32,
    );
    if !rotation.is_finite() || (rotation.length_squared() - 1.).abs() > config.rigid_tolerance {
        return Err("FBX animation rotation must be a finite unit quaternion".into());
    }
    let translation = Vec3::new(
        value.translation.x as f32,
        value.translation.y as f32,
        value.translation.z as f32,
    ) / config.metres_per_stud;
    let result = Mat4::from_scale_rotation_translation(scale, rotation.normalize(), translation);
    crate::rigid::validate(result, config.rigid_tolerance)?;
    // Remove admitted numerical scale drift, never actual scale animation.
    Ok(Mat4::from_rotation_translation(
        rotation.normalize(),
        translation,
    ))
}

fn cframe(value: Mat4) -> [f32; 12] {
    let rows = Mat3::from_mat4(value).transpose().to_cols_array();
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
        value.w_axis.x,
        value.w_axis.y,
        value.w_axis.z,
    ]
}

fn skeleton(scene: &ufbx::Scene, config: &Config) -> Result<BTreeMap<usize, Joint>> {
    let root = scene
        .nodes
        .get(config.root_node)
        .ok_or("FBX rootNode is absent")?;
    let mut pending = vec![(&**root, None)];
    let mut joints = BTreeMap::new();
    let mut names = HashSet::new();
    while let Some((node, parent)) = pending.pop() {
        let id = node.element.typed_id as usize;
        let name = node.element.name.to_string();
        if joints.contains_key(&id)
            || name.is_empty()
            || !names.insert(name.clone())
            || node.mesh.is_some()
            || node.camera.is_some()
        {
            return Err(
                "FBX animation requires a unique named joint hierarchy without mesh/camera nodes"
                    .into(),
            );
        }
        let children = node
            .children
            .iter()
            .map(|child| child.element.typed_id as usize)
            .collect();
        let rest = matrix(&node.local_transform, config)?;
        joints.insert(
            id,
            Joint {
                name,
                parent,
                children,
                rest,
            },
        );
        pending.extend(node.children.iter().map(|child| (&**child, Some(id))));
    }
    Ok(joints)
}

fn constant_keys(keys: &[ufbx::Keyframe], expected: f64) -> bool {
    expected.is_finite()
        && keys
            .iter()
            .all(|key| key.value == expected && key.left.dy == 0. && key.right.dy == 0.)
}

fn unchanged(property: &ufbx::AnimProp, stack: &ufbx::AnimStack) -> bool {
    let Some(original) = ufbx::find_prop(&property.element.props, property.prop_name.as_ref())
    else {
        return false;
    };
    if !matches!(
        original.type_,
        ufbx::PropType::Boolean
            | ufbx::PropType::Integer
            | ufbx::PropType::Number
            | ufbx::PropType::Vector
            | ufbx::PropType::Color
            | ufbx::PropType::ColorWithAlpha
            | ufbx::PropType::Translation
            | ufbx::PropType::Rotation
            | ufbx::PropType::Scaling
            | ufbx::PropType::Distance
    ) {
        return false;
    }
    // Constant per-layer values are not sufficient: additive/composited layers
    // must also evaluate to the original property value.
    let evaluated = ufbx::evaluate_prop(
        &stack.anim,
        &property.element,
        property.prop_name.as_ref(),
        stack.time_begin,
    );
    if evaluated.value_int != original.value_int
        || [
            evaluated.value_vec4.x,
            evaluated.value_vec4.y,
            evaluated.value_vec4.z,
            evaluated.value_vec4.w,
        ] != [
            original.value_vec4.x,
            original.value_vec4.y,
            original.value_vec4.z,
            original.value_vec4.w,
        ]
    {
        return false;
    }
    let values = [
        original.value_vec4.x,
        original.value_vec4.y,
        original.value_vec4.z,
    ];
    let default = property.anim_value.default_value;
    if [default.x, default.y, default.z] != values {
        return false;
    }
    property
        .anim_value
        .curves
        .iter()
        .zip(values)
        .all(|(curve, value)| {
            curve
                .as_ref()
                .is_none_or(|curve| constant_keys(&curve.keyframes, value))
        })
}

fn validate_channels(
    stack: &ufbx::AnimStack,
    joints: &BTreeMap<usize, Joint>,
) -> Result<Vec<String>> {
    let mut unchanged_properties = Vec::new();
    for layer in &stack.layers {
        if layer.weight_is_animated {
            return Err("animated FBX layer weights are not supported by this profile".into());
        }
        for property in &layer.anim_props {
            if property.element.type_ != ufbx::ElementType::Node
                || !joints.contains_key(&(property.element.typed_id as usize))
                || !matches!(
                    property.prop_name.as_ref(),
                    "Lcl Translation" | "Lcl Rotation" | "Lcl Scaling"
                )
            {
                if unchanged(property, stack) {
                    unchanged_properties
                        .push(format!("{}.{}", property.element.name, property.prop_name));
                    continue;
                }
                return Err(format!(
                    "unsupported or out-of-rig FBX animation property: {}",
                    property.prop_name
                )
                .into());
            }
            for curve in property.anim_value.curves.iter().flatten() {
                if curve.keyframes.windows(2).any(|keys| {
                    keys[0].value != keys[1].value
                        && matches!(
                            keys[0].interpolation,
                            ufbx::Interpolation::ConstantPrev | ufbx::Interpolation::ConstantNext
                        )
                }) {
                    return Err(
                        "stepped FBX channels require a discrete interpolation profile".into(),
                    );
                }
            }
        }
    }
    Ok(unchanged_properties)
}

fn pose(
    index: usize,
    joints: &BTreeMap<usize, Joint>,
    baked: &BTreeMap<usize, &ufbx::BakedNode>,
    time: f64,
    config: &Config,
) -> Result<Pose> {
    let joint = &joints[&index];
    let animated = if let Some(node) = baked.get(&index) {
        // Missing baked components retain the actual source rest component.
        let (scale, rotation, translation) = joint.rest.to_scale_rotation_translation();
        let mut value = ufbx::Transform {
            translation: ufbx::Vec3 {
                x: f64::from(translation.x * config.metres_per_stud),
                y: f64::from(translation.y * config.metres_per_stud),
                z: f64::from(translation.z * config.metres_per_stud),
            },
            rotation: ufbx::Quat {
                x: f64::from(rotation.x),
                y: f64::from(rotation.y),
                z: f64::from(rotation.z),
                w: f64::from(rotation.w),
            },
            scale: ufbx::Vec3 {
                x: f64::from(scale.x),
                y: f64::from(scale.y),
                z: f64::from(scale.z),
            },
        };
        if !node.translation_keys.is_empty() {
            value.translation = ufbx::evaluate_baked_vec3(&node.translation_keys, time);
        }
        if !node.rotation_keys.is_empty() {
            value.rotation = ufbx::evaluate_baked_quat(&node.rotation_keys, time);
        }
        if !node.scale_keys.is_empty() {
            value.scale = ufbx::evaluate_baked_vec3(&node.scale_keys, time);
        }
        matrix(&value, config)?
    } else {
        joint.rest
    };
    Ok(Pose {
        name: joint.name.clone(),
        cframe: cframe(joint.rest.inverse() * animated),
        weight: 1.,
        easing_style: "Linear".into(),
        easing_direction: "In".into(),
        children: joint
            .children
            .iter()
            .map(|child| pose(*child, joints, baked, time, config))
            .collect::<Result<_>>()?,
    })
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
    crate::rigid::validate(Mat4::IDENTITY, config.rigid_tolerance)?;
    if !config.metres_per_stud.is_finite()
        || config.metres_per_stud <= 0.
        || !config.resample_rate.is_finite()
        || config.resample_rate <= 0.
        || !config.minimum_sample_rate.is_finite()
        || config.minimum_sample_rate <= 0.
        || config.max_keyframe_segments == 0
        || config.max_output_frames == 0
    {
        return Err(
            "FBX animation needs positive explicit units, sampling rates and frame budgets".into(),
        );
    }
    let bytes = fs::read(source)?;
    let scene = ufbx::load_memory(
        &bytes,
        ufbx::LoadOpts {
            file_format: ufbx::FileFormat::Fbx,
            target_axes: ufbx::CoordinateAxes::right_handed_y_up(),
            target_unit_meters: 1.,
            space_conversion: ufbx::SpaceConversion::ModifyGeometry,
            load_external_files: false,
            ..Default::default()
        },
    )
    .map_err(|error| format!("FBX animation parse failed: {}", error.description))?;
    let stack = scene
        .anim_stacks
        .get(config.animation_index)
        .ok_or("FBX animationIndex is absent")?;
    if !scene.constraints.is_empty() {
        return Err(
            "FBX constraints require baking in the source before rigid animation conversion".into(),
        );
    }
    let joints = skeleton(&scene, &config)?;
    let root = &scene.nodes[config.root_node];
    let mut root_parent = Mat4::IDENTITY;
    if let Some(parent) = &root.parent {
        let m = &parent.node_to_world;
        root_parent = Mat4::from_cols_array(&[
            m.m00 as f32,
            m.m10 as f32,
            m.m20 as f32,
            0.,
            m.m01 as f32,
            m.m11 as f32,
            m.m21 as f32,
            0.,
            m.m02 as f32,
            m.m12 as f32,
            m.m22 as f32,
            0.,
            m.m03 as f32 / config.metres_per_stud,
            m.m13 as f32 / config.metres_per_stud,
            m.m23 as f32 / config.metres_per_stud,
            1.,
        ]);
    }
    crate::rigid::validate(root_parent, config.rigid_tolerance)?;
    let unchanged_properties = validate_channels(stack, &joints)?;
    let baked = ufbx::bake_anim(
        &scene,
        &stack.anim,
        ufbx::BakeOpts {
            // Keep keys and playback bounds in the same source-time domain.
            // The library trims keys but not playback bounds when this is true.
            trim_start_time: false,
            resample_rate: config.resample_rate,
            minimum_sample_rate: config.minimum_sample_rate,
            max_keyframe_segments: config.max_keyframe_segments,
            no_resample_rotation: false,
            key_reduction_enabled: false,
            ..Default::default()
        },
    )
    .map_err(|error| format!("FBX animation bake failed: {}", error.description))?;
    let mut times = vec![baked.playback_time_begin, baked.playback_time_end];
    let mut nodes = BTreeMap::new();
    for node in &baked.nodes {
        if !joints.contains_key(&(node.typed_id as usize)) {
            return Err("baked FBX motion targets a node outside the selected rig".into());
        }
        nodes.insert(node.typed_id as usize, node);
        times.extend(node.translation_keys.iter().map(|key| key.time));
        times.extend(node.rotation_keys.iter().map(|key| key.time));
        times.extend(node.scale_keys.iter().map(|key| key.time));
    }
    if times.iter().any(|time| !time.is_finite()) {
        return Err("FBX bake produced non-finite times".into());
    }
    times.retain(|time| *time >= baked.playback_time_begin && *time <= baked.playback_time_end);
    times.sort_by(f64::total_cmp);
    times.dedup();
    if times.is_empty() || times.len() > config.max_output_frames {
        return Err("FBX animation exceeds configured maxOutputFrames or has no samples".into());
    }
    let mut frames = Vec::new();
    for time in times {
        frames.push(Frame {
            name: String::new(),
            time: (time - baked.playback_time_begin) as f32,
            poses: vec![pose(config.root_node, &joints, &nodes, time, &config)?],
            markers: vec![],
        });
    }
    let rig: Vec<_> = joints
        .into_iter()
        .map(|(node_index, joint)| RigBinding {
            node_index,
            name: joint.name,
            parent_node: joint.parent,
            rest_cframe: cframe(joint.rest),
        })
        .collect();
    let rig = Rig {
        root_parent_cframe: cframe(root_parent),
        joints: rig,
    };
    let mut rig = rig;
    let mut clip = Clip {
        name: config.name.clone(),
        looped: config.looped,
        priority: config.priority.clone(),
        frames,
    };
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
    let artifact = animation::write_clip(&bytes, &clip, output)?;
    Ok(Manifest {
        artifact,
        config,
        rig,
        rig_sha256,
        bind_rig_sha256: target.map(|skin| skin.rig_sha256.clone()),
        source_time_begin: baked.playback_time_begin,
        sampled_approximation: true,
        unchanged_properties,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redundant_property_proof_rejects_hidden_cubic_or_extrapolated_motion() {
        let mut keys = [
            ufbx::Keyframe {
                time: 0.,
                value: 1.,
                interpolation: ufbx::Interpolation::Cubic,
                ..Default::default()
            },
            ufbx::Keyframe {
                time: 1.,
                value: 1.,
                interpolation: ufbx::Interpolation::Cubic,
                ..Default::default()
            },
        ];
        assert!(constant_keys(&keys, 1.));
        keys[0].right.dy = 0.2;
        assert!(!constant_keys(&keys, 1.));
        keys[0].right.dy = 0.;
        keys[0].left.dy = 0.2;
        assert!(!constant_keys(&keys, 1.));
        keys[0].left.dy = 0.;
        keys[1].value = 0.;
        assert!(!constant_keys(&keys, 1.));
    }
}
