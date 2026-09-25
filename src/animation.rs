//! Native KeyframeSequence serialization from explicit rest-relative pose data.
use crate::Result;
use rbx_dom_weak::{
    InstanceBuilder, WeakDom,
    types::{CFrame, Enum, Matrix3, Vector3},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Write, path::Path};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Clip {
    pub name: String,
    pub looped: bool,
    pub priority: String,
    pub frames: Vec<Frame>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Frame {
    pub name: String,
    pub time: f32,
    pub poses: Vec<Pose>,
    pub markers: Vec<Marker>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Marker {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pose {
    pub name: String,
    /// Joint-rest-relative rotation rows followed by translation, in studs.
    pub cframe: [f32; 12],
    pub weight: f32,
    pub easing_style: String,
    pub easing_direction: String,
    pub children: Vec<Pose>,
}

fn enumeration(kind: &str, name: &str) -> Result<Enum> {
    let value = rbx_reflection_database::get_bundled()
        .enums
        .get(kind)
        .and_then(|descriptor| descriptor.items.get(name))
        .ok_or_else(|| format!("unknown {kind} enum value: {name}"))?;
    Ok(Enum::from_u32(*value))
}

fn poses(values: &[Pose]) -> Result<Vec<InstanceBuilder>> {
    let mut names = HashSet::new();
    let mut result = Vec::new();
    for pose in values {
        if pose.name.is_empty()
            || !names.insert(&pose.name)
            || pose.cframe.iter().any(|v| !v.is_finite())
            || !pose.weight.is_finite()
            || !(0. ..=1.).contains(&pose.weight)
        {
            return Err("invalid/duplicate pose name, transform or blend weight".into());
        }
        let value = pose.cframe;
        let rotation = Matrix3::new(
            Vector3::new(value[0], value[1], value[2]),
            Vector3::new(value[3], value[4], value[5]),
            Vector3::new(value[6], value[7], value[8]),
        );
        let mut builder = InstanceBuilder::new("Pose")
            .with_name(&pose.name)
            .with_property(
                "CFrame",
                CFrame::new(Vector3::new(value[9], value[10], value[11]), rotation),
            )
            .with_property("Weight", pose.weight)
            .with_property(
                "EasingStyle",
                enumeration("PoseEasingStyle", &pose.easing_style)?,
            )
            .with_property(
                "EasingDirection",
                enumeration("PoseEasingDirection", &pose.easing_direction)?,
            );
        for child in poses(&pose.children)? {
            builder = builder.with_child(child);
        }
        result.push(builder);
    }
    Ok(result)
}

pub fn encode(clip: &Clip) -> Result<Vec<u8>> {
    if clip.name.is_empty() || clip.frames.is_empty() {
        return Err("animation requires a name and keyframes".into());
    }
    let mut sequence = InstanceBuilder::new("KeyframeSequence")
        .with_name(&clip.name)
        .with_property("Loop", clip.looped)
        .with_property(
            "Priority",
            enumeration("AnimationPriority", &clip.priority)?,
        );
    let mut previous = None;
    for frame in &clip.frames {
        if !frame.time.is_finite()
            || frame.time < 0.
            || previous.is_some_and(|time| frame.time <= time)
        {
            return Err(
                "keyframe times must be finite, nonnegative and strictly increasing".into(),
            );
        }
        previous = Some(frame.time);
        let mut keyframe = InstanceBuilder::new("Keyframe")
            .with_name(&frame.name)
            .with_property("Time", frame.time);
        for pose in poses(&frame.poses)? {
            keyframe = keyframe.with_child(pose);
        }
        for marker in &frame.markers {
            if marker.name.is_empty() {
                return Err("animation marker name must be nonempty".into());
            }
            keyframe = keyframe.with_child(
                InstanceBuilder::new("KeyframeMarker")
                    .with_name(&marker.name)
                    .with_property("Value", marker.value.clone()),
            );
        }
        sequence = sequence.with_child(keyframe);
    }
    let dom = WeakDom::new(sequence);
    let mut bytes = Vec::new();
    rbx_binary::Serializer::new()
        .reflection_database(rbx_reflection_database::get_bundled())
        .serialize(&mut bytes, &dom, &[dom.root_ref()])?;
    Ok(bytes)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: &'static str,
    pub source_sha256: String,
    pub sha256: String,
    pub keyframes: usize,
    pub bytes: usize,
    pub engine_verified: bool,
}

/// Convert canonical rest-relative JSON, not arbitrary source-node animation.
pub fn convert(source: &Path, output: &Path) -> Result<Manifest> {
    if output.extension().and_then(|s| s.to_str()) != Some("rbxm") {
        return Err("native animation output requires the .rbxm extension".into());
    }
    let source = fs::read(source)?;
    let clip: Clip = serde_json::from_slice(&source)?;
    let bytes = encode(&clip)?;
    let manifest = Manifest {
        format: "rbxm-keyframe-sequence",
        source_sha256: format!("{:x}", Sha256::digest(&source)),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        keyframes: clip.frames.len(),
        bytes: bytes.len(),
        engine_verified: false,
    };

    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(output)?;
    Ok(manifest)
}
