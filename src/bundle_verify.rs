//! Read-only runtime-artifact integrity checks, not engine or publishing attestation.
use crate::{Result, bundle::Manifest};
use rbx_dom_weak::types::Variant;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    pub artifacts_verified: usize,
    pub native_models_verified: usize,
    pub owned_references_verified: usize,
    pub external_references: BTreeSet<String>,
    pub manifest_sha256: String,
    pub engine_verified: bool,
}

fn relative(path: &str) -> Result<()> {
    if path.is_empty()
        || path.contains(['\\', ':', '?', '#', '%', '\0'])
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("invalid contained bundle path: {path}").into());
    }
    Ok(())
}

fn contained(root: &Path, path: &str) -> Result<PathBuf> {
    relative(path)?;
    let file = root.join(path).canonicalize()?;
    if !file.starts_with(root) || !file.is_file() {
        return Err(format!(
            "bundle artifact escapes its directory or is not a regular file: {path}"
        )
        .into());
    }
    Ok(file)
}

fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    // Streaming I/O chunk size, not a file-size/resource-policy limit.
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        digest.update(&chunk[..count]);
    }
    Ok(format!("{digest:x}", digest = digest.finalize()))
}

fn inspect_native(path: &Path, files: &HashSet<String>, result: &mut Verification) -> Result<()> {
    let dom = rbx_binary::from_reader(BufReader::new(fs::File::open(path)?))?;
    for node in dom.descendants() {
        for value in node.properties.values() {
            let uri = match value {
                Variant::Content(value) => {
                    if let Some(reference) = value.as_object()
                        && reference.is_some()
                        && dom.get_by_ref(reference).is_none()
                    {
                        return Err("native model has a dangling content object reference".into());
                    }
                    value.as_uri()
                }
                Variant::ContentId(value) => Some(value.as_str()),
                Variant::Ref(reference) if reference.is_some() => {
                    if dom.get_by_ref(*reference).is_none() {
                        return Err("native model has a dangling instance reference".into());
                    }
                    None
                }
                _ => None,
            };
            let Some(uri) = uri.filter(|uri| !uri.is_empty()) else {
                continue;
            };
            if let Some(path) = uri.strip_prefix("rbxasset://")
                && (path.starts_with("assets/") || path.starts_with("scenes/"))
            {
                relative(path)?;
                if !files.contains(path) {
                    return Err(format!(
                        "native model references an unlisted local bundle artifact: {uri}"
                    )
                    .into());
                }
                result.owned_references_verified += 1;
                continue;
            }
            result.external_references.insert(uri.to_owned());
        }
    }
    result.native_models_verified += 1;
    Ok(())
}

pub fn verify(directory: &Path) -> Result<Verification> {
    let root = directory.canonicalize()?;
    if !root.is_dir() {
        return Err("bundle verification requires a directory".into());
    }
    let manifest_path = contained(&root, "manifest.json")?;
    let bytes = fs::read(&manifest_path)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    if manifest.format != "roblox-offline-bundle" || manifest.version != 1 {
        return Err("unsupported offline bundle manifest format/version".into());
    }
    let mut paths = HashSet::new();
    let mut identities = HashSet::new();
    let mut scenes = HashSet::new();
    let mut entries = Vec::new();
    for file in &manifest.files {
        if file.asset.is_empty()
            || file.file.is_empty()
            || !identities.insert((&file.asset, &file.file))
        {
            return Err("duplicate/empty bundle asset identity".into());
        }
        if file.local_uri != format!("rbxasset://{}", file.path) {
            return Err("bundle local URI differs from its artifact path".into());
        }
        entries.push((&file.path, &file.sha256));
    }
    for scene in &manifest.scenes {
        if scene.id.is_empty() || !scenes.insert(&scene.id) {
            return Err("duplicate/empty bundle scene identity".into());
        }
        if !matches!(scene.format.as_str(), "rbxm" | "rbxl")
            || Path::new(&scene.path).extension().and_then(|s| s.to_str())
                != Some(scene.format.as_str())
        {
            return Err("bundle scene format/extension mismatch".into());
        }
        entries.push((&scene.path, &scene.sha256));
    }
    if entries.is_empty() {
        return Err("bundle has no runtime artifacts".into());
    }
    let mut native = Vec::new();
    let mut packs = Vec::new();
    for (path, expected) in entries {
        if !paths.insert(path.clone()) {
            return Err(format!("duplicate bundle artifact path: {path}").into());
        }
        let file = contained(&root, path)?;
        if hash(&file)? != *expected {
            return Err(format!("bundle artifact hash mismatch: {path}").into());
        }
        if matches!(
            Path::new(path).extension().and_then(|s| s.to_str()),
            Some("rbxm" | "rbxl")
        ) {
            native.push(file.clone());
        }
        if Path::new(path).file_name().and_then(|s| s.to_str()) == Some("texturepack.xml") {
            packs.push(file);
        }
    }
    let mut result = Verification {
        artifacts_verified: paths.len(),
        native_models_verified: 0,
        owned_references_verified: 0,
        external_references: BTreeSet::new(),
        manifest_sha256: format!("{:x}", Sha256::digest(&bytes)),
        engine_verified: false,
    };
    for file in native {
        inspect_native(&file, &paths, &mut result)?;
    }
    for file in packs {
        for uri in crate::texture_pack::references(&fs::read(file)?)?.into_values() {
            let path = uri
                .strip_prefix("rbxasset://")
                .ok_or("TexturePack local reference expected")?;
            relative(path)?;
            if path.starts_with("assets/") || path.starts_with("scenes/") {
                if !paths.contains(path) {
                    return Err(format!(
                        "TexturePack references an unlisted local bundle artifact: {uri}"
                    )
                    .into());
                }
                result.owned_references_verified += 1;
            } else {
                result.external_references.insert(uri);
            }
        }
    }
    Ok(result)
}
