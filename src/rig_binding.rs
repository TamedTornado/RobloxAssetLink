//! Validate source animation rest-space compatibility against actual scene Bones.
use crate::{
    Result,
    animation::Rig,
    scene::{AssetMap, AssetReference},
};
use glam::Mat4;
use rbx_dom_weak::{
    WeakDom,
    types::{CFrame, Ref, Variant},
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub animation: AssetReference,
    pub root: String,
    pub bone: Option<String>,
    pub rigid_tolerance: f32,
}

fn matrix(value: [f32; 12]) -> Mat4 {
    Mat4::from_cols_array(&[
        value[0], value[3], value[6], 0., value[1], value[4], value[7], 0., value[2], value[5],
        value[8], 0., value[9], value[10], value[11], 1.,
    ])
}

fn native(value: CFrame) -> Mat4 {
    let r = value.orientation;
    matrix([
        r.x.x,
        r.x.y,
        r.x.z,
        r.y.x,
        r.y.y,
        r.y.z,
        r.z.x,
        r.z.y,
        r.z.z,
        value.position.x,
        value.position.y,
        value.position.z,
    ])
}

struct TargetBone {
    parent: Option<String>,
    rest: Mat4,
}

fn target_bones(dom: &WeakDom, root: Ref) -> Result<HashMap<String, TargetBone>> {
    let mut pending = vec![(root, None)];
    let mut result = HashMap::new();
    while let Some((reference, parent)) = pending.pop() {
        let bone = dom.get_by_ref(reference).ok_or("missing target bone")?;
        if bone.class.as_str() != "Bone" {
            return Err("animation binding root and joint descendants must be Bones".into());
        }
        let rest = match bone.properties.get(&"CFrame".into()) {
            Some(Variant::CFrame(value)) => native(*value),
            None => Mat4::IDENTITY,
            _ => return Err("target Bone.CFrame has wrong type".into()),
        };
        if result
            .insert(bone.name.clone(), TargetBone { parent, rest })
            .is_some()
        {
            return Err("animation target has duplicate bone names".into());
        }
        for child in bone.children() {
            let instance = dom.get_by_ref(*child).ok_or("missing bone child")?;
            if instance.class.as_str() == "Bone" {
                pending.push((*child, Some(bone.name.clone())));
            } else if !instance.children().is_empty() {
                return Err(
                    "animation target has a non-Bone intermediary; unsupported hierarchy".into(),
                );
            }
        }
    }
    Ok(result)
}

pub(crate) fn validate(
    binding: &Binding,
    dom: &WeakDom,
    ids: &HashMap<String, Ref>,
    assets: &AssetMap,
) -> Result<()> {
    let reference = ids
        .get(&binding.root)
        .ok_or("animation binding root id missing")?;
    let asset = assets
        .get(&binding.animation)
        .ok_or("animation binding asset missing")?;
    let rig = asset.animation_rig.as_ref().ok_or(
        "animation binding needs source rig metadata from animationGltf/animationFbx conversion",
    )?;
    let reference = if let Some(name) = &binding.bone {
        let container = dom.get_by_ref(*reference).ok_or("missing rig container")?;
        let matches: Vec<_> = container
            .children()
            .iter()
            .filter(|reference| {
                dom.get_by_ref(**reference)
                    .is_some_and(|node| node.class.as_str() == "Bone" && node.name == *name)
            })
            .copied()
            .collect();
        if matches.len() != 1 {
            return Err("animation binding named root Bone missing or ambiguous".into());
        }
        matches[0]
    } else {
        *reference
    };
    validate_rig(rig, dom, reference, binding.rigid_tolerance)
}

fn validate_rig(rig: &Rig, dom: &WeakDom, root: Ref, tolerance: f32) -> Result<()> {
    let root_instance = dom.get_by_ref(root).ok_or("missing rig root")?;
    let parent_instance = dom
        .get_by_ref(root_instance.parent())
        .ok_or("missing rig container")?;
    if parent_instance.class.as_str() != "MeshPart" {
        return Err("animation binding root must be directly under its MeshPart; external Bone ancestry would change rest space".into());
    }
    let parent = matrix(rig.root_parent_cframe);
    crate::rigid::validate(parent, tolerance)?;
    if rig.joints.is_empty() {
        return Err("animation rig is empty".into());
    }
    let targets = target_bones(dom, root)?;
    if targets.len() != rig.joints.len() {
        return Err("animation target bone count differs from source rig".into());
    }
    let mut sources = HashMap::new();
    let mut names = HashSet::new();
    for joint in &rig.joints {
        if sources.insert(joint.node_index, joint).is_some() || !names.insert(&joint.name) {
            return Err("animation rig contains duplicate ids/names".into());
        }
    }
    let mut roots = 0;
    for joint in &rig.joints {
        let actual = targets
            .get(&joint.name)
            .ok_or_else(|| format!("animation target bone missing: {}", joint.name))?;
        let source_parent = joint
            .parent_node
            .map(|id| {
                sources
                    .get(&id)
                    .map(|p| p.name.clone())
                    .ok_or("animation rig parent missing")
            })
            .transpose()?;
        if actual.parent != source_parent {
            return Err(format!("animation hierarchy mismatch at {}", joint.name).into());
        }
        let rest = matrix(joint.rest_cframe);
        crate::rigid::validate(rest, tolerance)?;
        crate::rigid::validate(actual.rest, tolerance)?;
        let expected = if source_parent.is_none() {
            roots += 1;
            parent * rest
        } else {
            rest
        };
        if !expected.abs_diff_eq(actual.rest, tolerance) {
            return Err(format!(
                "animation rest-space mismatch at {}; no automatic retargeting is performed",
                joint.name
            )
            .into());
        }
    }
    if roots != 1 {
        return Err("animation binding requires exactly one source root".into());
    }
    Ok(())
}
