//! Exact rigid rest-basis changes; not proportional/anatomical retargeting.
use crate::{
    Result,
    animation::{Clip, Pose, Rig},
    skin_import,
};
use glam::Mat4;
use std::collections::{HashMap, HashSet};

fn matrix(c: [f32; 12]) -> Mat4 {
    Mat4::from_cols_array(&[
        c[0], c[3], c[6], 0., c[1], c[4], c[7], 0., c[2], c[5], c[8], 0., c[9], c[10], c[11], 1.,
    ])
}

fn cframe(m: Mat4) -> [f32; 12] {
    [
        m.x_axis.x, m.y_axis.x, m.z_axis.x, m.x_axis.y, m.y_axis.y, m.z_axis.y, m.x_axis.z,
        m.y_axis.z, m.z_axis.z, m.w_axis.x, m.w_axis.y, m.w_axis.z,
    ]
}

struct Correction {
    parent: Option<String>,
    matrix: Mat4,
}

fn poses(
    values: &mut [Pose],
    parent: Option<&str>,
    corrections: &HashMap<String, Correction>,
    seen: &mut HashSet<String>,
    tolerance: f32,
) -> Result<()> {
    for pose in values {
        if !seen.insert(pose.name.clone()) || pose.weight != 1.0 {
            return Err("rest rebasing requires unique full-weight joint poses".into());
        }
        let correction = corrections
            .get(&pose.name)
            .ok_or("pose is absent from source rig")?;
        if correction.parent.as_deref() != parent {
            return Err("pose hierarchy differs from source rig".into());
        }
        let original = matrix(pose.cframe);
        crate::rigid::validate(original, tolerance)?;
        pose.cframe = cframe(correction.matrix * original);
        poses(
            &mut pose.children,
            Some(&pose.name),
            corrections,
            seen,
            tolerance,
        )?;
    }
    Ok(())
}

pub(crate) fn apply(
    clip: &mut Clip,
    rig: &mut Rig,
    target: &skin_import::Manifest,
    metres_per_stud: f32,
    tolerance: f32,
) -> Result<()> {
    crate::rigid::validate(Mat4::IDENTITY, tolerance)?;
    if metres_per_stud != target.metres_per_stud
        || !metres_per_stud.is_finite()
        || metres_per_stud <= 0.0
    {
        return Err("animation and skin must use the same positive metresPerStud".into());
    }
    if rig.joints.is_empty() || rig.joints.len() != target.rig.len() {
        return Err(format!(
            "animation and skin joint counts differ: animation {} ({:?}), skin {} ({:?})",
            rig.joints.len(),
            rig.joints.iter().map(|j| &j.name).collect::<Vec<_>>(),
            target.rig.len(),
            target.rig.iter().map(|j| &j.name).collect::<Vec<_>>()
        )
        .into());
    }
    let mut targets = HashMap::new();
    let mut worlds = Vec::new();
    for (index, bone) in target.rig.iter().enumerate() {
        let local = matrix(bone.local_bind_cframe);
        let world = matrix(bone.world_bind_cframe);
        crate::rigid::validate(local, tolerance)?;
        crate::rigid::validate(world, tolerance)?;
        let expected = match bone.parent {
            Some(parent) => {
                *worlds
                    .get(usize::from(parent))
                    .ok_or("skin parents must precede children")?
                    * local
            }
            None => local,
        };
        if !world.abs_diff_eq(expected, tolerance) {
            return Err("skin local/world bind matrices disagree".into());
        }
        if bone.name.is_empty() || targets.insert(bone.name.as_str(), index).is_some() {
            return Err("skin has duplicate/empty joint names".into());
        }
        worlds.push(world);
    }
    let mut source_names = HashMap::new();
    for joint in &rig.joints {
        if source_names
            .insert(joint.node_index, joint.name.clone())
            .is_some()
        {
            return Err("duplicate source joint id".into());
        }
    }
    let root_parent = matrix(rig.root_parent_cframe);
    crate::rigid::validate(root_parent, tolerance)?;
    let mut corrections = HashMap::new();
    let mut new_rests = Vec::new();
    let mut roots = 0;
    for joint in &rig.joints {
        let index = *targets
            .get(joint.name.as_str())
            .ok_or_else(|| format!("skin is missing animation bone {}", joint.name))?;
        let bone = &target.rig[index];
        let parent = joint
            .parent_node
            .map(|id| {
                source_names
                    .get(&id)
                    .cloned()
                    .ok_or("source rig parent missing")
            })
            .transpose()?;
        let target_parent = bone
            .parent
            .map(|index| target.rig[usize::from(index)].name.clone());
        if parent != target_parent {
            return Err(format!("skin/animation parent mismatch at {}", joint.name).into());
        }
        let source_rest = matrix(joint.rest_cframe);
        crate::rigid::validate(source_rest, tolerance)?;
        let source_rest = if parent.is_none() {
            roots += 1;
            root_parent * source_rest
        } else {
            source_rest
        };
        let bind = matrix(bone.local_bind_cframe);
        // B * (B^-1 * S * P) == S * P; root S already includes its parent context.
        let correction = bind.inverse() * source_rest;
        crate::rigid::validate(correction, tolerance)?;
        if corrections
            .insert(
                joint.name.clone(),
                Correction {
                    parent,
                    matrix: correction,
                },
            )
            .is_some()
        {
            return Err("duplicate source joint name".into());
        }
        new_rests.push(bone.local_bind_cframe);
    }
    if roots != 1 {
        return Err("rebasing requires a single matching joint hierarchy".into());
    }
    for frame in &mut clip.frames {
        let mut seen = HashSet::new();
        poses(&mut frame.poses, None, &corrections, &mut seen, tolerance)?;
        if seen.len() != corrections.len() {
            return Err("rest rebasing requires every joint in each sampled frame".into());
        }
    }
    for (joint, rest) in rig.joints.iter_mut().zip(new_rests) {
        joint.rest_cframe = rest;
    }
    rig.root_parent_cframe = cframe(Mat4::IDENTITY);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{Frame, RigBinding};
    use glam::{Quat, Vec3};

    fn inputs() -> (Clip, Rig, skin_import::Manifest) {
        let source = Mat4::from_rotation_translation(Quat::from_rotation_z(0.4), Vec3::X);
        let bind =
            Mat4::from_rotation_translation(Quat::from_rotation_y(0.3), Vec3::new(-2., 1., 0.));
        let pose =
            Mat4::from_rotation_translation(Quat::from_rotation_x(0.8), Vec3::new(2., -1., 0.));
        let clip = Clip {
            name: "Motion".into(),
            looped: false,
            priority: "Action".into(),
            frames: vec![Frame {
                name: "Frame".into(),
                time: 0.,
                markers: vec![],
                poses: vec![Pose {
                    name: "Root".into(),
                    cframe: cframe(pose),
                    weight: 1.,
                    easing_style: "Linear".into(),
                    easing_direction: "In".into(),
                    children: vec![],
                }],
            }],
        };
        let rig = Rig {
            root_parent_cframe: cframe(Mat4::from_translation(Vec3::new(3., 4., 5.))),
            joints: vec![RigBinding {
                node_index: 0,
                name: "Root".into(),
                parent_node: None,
                rest_cframe: cframe(source),
            }],
        };
        let skin = skin_import::Manifest {
            format: "roblox-skinned-mesh-v4.01",
            metres_per_stud: 0.5,
            rig: vec![skin_import::Binding {
                source_node: 10,
                name: "Root".into(),
                parent: None,
                world_bind_cframe: cframe(bind),
                local_bind_cframe: cframe(bind),
            }],
            meshes: vec![],
            engine_verified: false,
            rig_file: "rig.rbxm".into(),
            rig_sha256: String::new(),
        };
        (clip, rig, skin)
    }

    #[test]
    fn rigid_rebase_preserves_motion_with_noncommuting_rotations() {
        let (mut clip, mut rig, skin) = inputs();
        let expected = matrix(rig.root_parent_cframe)
            * matrix(rig.joints[0].rest_cframe)
            * matrix(clip.frames[0].poses[0].cframe);
        apply(&mut clip, &mut rig, &skin, 0.5, 0.00001).unwrap();
        let actual = matrix(skin.rig[0].world_bind_cframe) * matrix(clip.frames[0].poses[0].cframe);
        assert!(actual.abs_diff_eq(expected, 0.00001));
        assert_eq!(rig.root_parent_cframe, cframe(Mat4::IDENTITY));
        assert_eq!(rig.joints[0].rest_cframe, skin.rig[0].local_bind_cframe);
    }

    #[test]
    fn invalid_bind_data_or_partial_pose_semantics_are_not_guessed() {
        for case in 0..4 {
            let (mut clip, mut rig, mut skin) = inputs();
            match case {
                0 => skin.rig[0].world_bind_cframe[9] += 1.,
                1 => skin.rig[0].parent = Some(0),
                2 => clip.frames[0].poses[0].weight = 0.5,
                _ => clip.frames[0].poses.clear(),
            }
            assert!(apply(&mut clip, &mut rig, &skin, 0.5, 0.00001).is_err());
        }
    }
}
