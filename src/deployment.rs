//! Local deployment linking; supplied IDs are assertions, not upload receipts.
use crate::{Result, bundle::Manifest, bundle_verify};
use rbx_dom_weak::types::{Content, Variant};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mapping {
    pub bundle_manifest_sha256: String,
    pub bindings: Vec<Binding>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub asset: String,
    pub file: String,
    pub source_artifact_sha256: String,
    pub remote_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedScene {
    pub bundle_manifest_sha256: String,
    pub source_scene_sha256: String,
    pub output_sha256: String,
    pub references_linked: usize,
    pub published: bool,
    pub remote_ids_verified: bool,
    pub engine_verified: bool,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn remote_uri(id: &str) -> Result<String> {
    let number = id
        .parse::<u64>()
        .map_err(|_| "remote ID must be a positive decimal u64 string")?;
    if number == 0 || number.to_string() != id {
        return Err("remote ID must be canonical positive decimal without leading zeroes".into());
    }
    Ok(format!("rbxassetid://{id}"))
}

fn bindings(manifest: &Manifest, mapping: &Mapping) -> Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();
    for binding in &mapping.bindings {
        let artifact = manifest
            .files
            .iter()
            .find(|file| file.asset == binding.asset && file.file == binding.file)
            .ok_or_else(|| {
                format!(
                    "mapping references unknown artifact: {}/{}",
                    binding.asset, binding.file
                )
            })?;
        if artifact.sha256 != binding.source_artifact_sha256 {
            return Err(format!(
                "mapping has stale artifact hash: {}/{}",
                binding.asset, binding.file
            )
            .into());
        }
        if resolved
            .insert(artifact.local_uri.clone(), remote_uri(&binding.remote_id)?)
            .is_some()
        {
            return Err("duplicate deployment binding".into());
        }
    }
    Ok(resolved)
}

/// Rewrite only typed content properties in a verified native scene. Embedded
/// physics, terrain, scripts and instance references are not string-replaced.
pub fn link_scene(
    directory: &Path,
    scene_id: &str,
    mapping: &Mapping,
    output: &Path,
) -> Result<LinkedScene> {
    let root = directory.canonicalize()?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    if parent.starts_with(&root) {
        return Err("linked output must be outside the immutable source bundle".into());
    }
    let verification = bundle_verify::verify(&root)?;
    if verification.manifest_sha256 != mapping.bundle_manifest_sha256 {
        return Err("mapping has stale bundle manifest hash".into());
    }
    let bytes = fs::read(root.join("manifest.json"))?;
    if digest(&bytes) != verification.manifest_sha256 {
        return Err("bundle manifest changed during linking".into());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    let scene = manifest
        .scenes
        .iter()
        .find(|scene| scene.id == scene_id)
        .ok_or("unknown bundle scene ID")?;
    if output.extension().and_then(|extension| extension.to_str()) != Some(scene.format.as_str()) {
        return Err("linked output extension must match the native scene format".into());
    }
    let resolved = bindings(&manifest, mapping)?;
    let source = root.join(&scene.path).canonicalize()?;
    if !source.starts_with(&root) {
        return Err("scene escaped source bundle during linking".into());
    }
    let bytes = fs::read(source)?;
    if digest(&bytes) != scene.sha256 {
        return Err("scene changed during linking".into());
    }
    let (bytes, references_linked) = rewrite_native(&bytes, &resolved)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, &bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(output)?;
    Ok(LinkedScene {
        bundle_manifest_sha256: verification.manifest_sha256,
        source_scene_sha256: scene.sha256.clone(),
        output_sha256: digest(&bytes),
        references_linked,
        published: false,
        remote_ids_verified: false,
        engine_verified: false,
    })
}

pub(crate) fn rewrite_native(
    bytes: &[u8],
    resolved: &HashMap<String, String>,
) -> Result<(Vec<u8>, usize)> {
    let mut dom = rbx_binary::from_reader(bytes)?;
    let nodes: Vec<_> = dom.descendants().map(|node| node.referent()).collect();
    let mut references_linked = 0;
    for node in nodes {
        for value in dom
            .get_by_ref_mut(node)
            .ok_or("missing scene instance")?
            .properties
            .values_mut()
        {
            let uri = match value {
                Variant::Content(content) => content.as_uri(),
                Variant::ContentId(content) => Some(content.as_str()),
                _ => None,
            };
            let Some(uri) = uri else { continue };
            let Some(path) = uri.strip_prefix("rbxasset://") else {
                continue;
            };
            if !path.starts_with("assets/") && !path.starts_with("scenes/") {
                continue;
            }
            let remote = resolved
                .get(uri)
                .ok_or_else(|| format!("missing remote ID binding for {uri}"))?;
            *value = match value {
                Variant::Content(_) => Variant::Content(Content::from_uri(remote)),
                Variant::ContentId(_) => Variant::ContentId(remote.clone().into()),
                _ => unreachable!("only typed content has a URI"),
            };
            references_linked += 1;
        }
    }
    let mut bytes = Vec::new();
    rbx_binary::to_writer(&mut bytes, &dom, dom.root().children())?;
    Ok((bytes, references_linked))
}
