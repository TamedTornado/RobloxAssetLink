//! Asset editing uses the real offline build plan, not a parallel GLB catalog.
use crate::{Result, bundle, scene};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

fn validate(value: &Value) -> Result<bundle::Plan> {
    let plan: bundle::Plan = serde_json::from_value(value.clone())?;
    let mut ids = HashSet::new();
    for asset in &plan.assets {
        if asset.id.trim().is_empty() || !ids.insert(&asset.id) {
            return Err("asset IDs must be nonempty and unique".into());
        }
    }
    let mut ids = HashSet::new();
    for scene in &plan.scenes {
        if scene.id.trim().is_empty() || !ids.insert(&scene.id) {
            return Err("scene IDs must be nonempty and unique".into());
        }
    }
    bundle::ordered_assets(&plan)?;
    Ok(plan)
}

pub fn read(path: &Path) -> Result<Value> {
    let value = serde_json::from_slice(&fs::read(path)?)?;
    validate(&value)?;
    Ok(value)
}

pub fn revision(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn target(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let target = parent
        .canonicalize()?
        .join(path.file_name().ok_or("plan filename required")?);
    if fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("build plan must not be a symlink".into());
    }
    Ok(target)
}

fn lock(path: &Path) -> Result<File> {
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(PathBuf::from(name))?;
    file.lock()?;
    Ok(file)
}

fn save(path: &Path, value: &Value, create: bool) -> Result<()> {
    validate(value)?;
    let parent = path.parent().ok_or("plan parent required")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&serde_json::to_vec_pretty(value)?)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    if create {
        temporary.persist_noclobber(path)?;
    } else {
        temporary.persist(path)?;
    }
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn initialize(path: &Path) -> Result<Value> {
    let path = target(path)?;
    let _lock = lock(&path)?;
    let value = json!({"assets":[],"scenes":[]});
    save(&path, &value, true)?;
    Ok(value)
}

fn mutate(
    path: &Path,
    expected: Option<&str>,
    edit: impl FnOnce(&mut Value) -> Result<()>,
) -> Result<Value> {
    let path = target(path)?;
    let _lock = lock(&path)?;
    let mut value = read(&path)?;
    let current = revision(&value)?;
    if expected.is_some_and(|expected| expected != current) {
        return Err("build plan changed; reload before applying".into());
    }
    edit(&mut value)?;
    save(&path, &value, false)?;
    Ok(value)
}

/// The conversion uses source paths relative to the plan, just like `build bundle`.
pub fn put(
    path: &Path,
    id: &str,
    conversion: Value,
    replace: bool,
    expected: Option<&str>,
) -> Result<Value> {
    mutate(path, expected, |plan| {
        let assets = plan["assets"]
            .as_array_mut()
            .ok_or("plan assets must be an array")?;
        let position = assets.iter().position(|asset| asset["id"] == id);
        let entry = json!({"id":id,"conversion":conversion});
        match (replace, position) {
            (false, None) => assets.push(entry),
            (true, Some(index)) => assets[index] = entry,
            (false, Some(_)) => return Err(format!("asset already exists: {id}").into()),
            (true, None) => return Err(format!("unknown asset: {id}").into()),
        }
        Ok(())
    })
}

fn node_uses(node: &scene::Node, id: &str) -> bool {
    node.assets
        .values()
        .chain(node.material.iter())
        .chain(node.rig.iter())
        .any(|reference| reference.asset == id)
        || node.children.iter().any(|child| node_uses(child, id))
}

pub fn remove(path: &Path, id: &str, expected: Option<&str>) -> Result<Value> {
    let path = target(path)?;
    mutate(&path, expected, |value| {
        let plan = validate(value)?;
        let root = path.parent().ok_or("plan parent required")?;
        for entry in &plan.scenes {
            let source = bundle::local(root, &entry.source)?;
            let scene: scene::Specification = serde_json::from_slice(&fs::read(source)?)?;
            if scene.roots.iter().any(|node| node_uses(node, id))
                || scene
                    .animation_bindings
                    .iter()
                    .any(|binding| binding.animation.asset == id)
            {
                return Err(format!("scene {} still references asset {id}", entry.id).into());
            }
        }
        let assets = value["assets"]
            .as_array_mut()
            .ok_or("plan assets must be an array")?;
        let index = assets
            .iter()
            .position(|asset| asset["id"] == id)
            .ok_or_else(|| format!("unknown asset: {id}"))?;
        assets.remove(index);
        // Post-edit schema/dependency validation also rejects dangling bindTo.
        Ok(())
    })
}
