//! Deployment configuration and complete, offline preflight before any writes.
use crate::{
    Result, bundle::Manifest, bundle_verify, cloud_http::Policy, deployment, texture_pack,
};
use rbx_dom_weak::types::Variant;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn id(value: &str) -> Result<()> {
    let number = value
        .parse::<u64>()
        .map_err(|_| "asset/destination ID must be a positive decimal integer string")?;
    if number == 0 || number.to_string() != value {
        return Err("ID must be canonical positive decimal".into());
    }
    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Creator {
    pub user_id: Option<String>,
    pub group_id: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub api_base_url: String,
    pub api_key_file: PathBuf,
    pub creator: Creator,
    pub universe_id: String,
    pub place_id: String,
    pub scene: String,
    pub policy: Policy,
    pub uploads: Vec<Upload>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum AssetType {
    Mesh,
    Image,
    Audio,
    Animation,
    Model,
    TexturePack,
}

impl AssetType {
    pub fn mime(self) -> &'static str {
        match self {
            Self::Mesh => "model/x-file-mesh-data",
            Self::Image => "image/png",
            Self::Audio => "audio/ogg",
            Self::Animation | Self::Model => "model/x-rbxm",
            Self::TexturePack => "application/xml",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Upload {
    pub asset: String,
    pub file: String,
    pub asset_type: AssetType,
    pub display_name: String,
    pub description: String,
}

pub struct Input {
    pub upload: Upload,
    pub uri: String,
    pub source_sha256: String,
    pub bytes: Vec<u8>,
    pub dependencies: Vec<String>,
}

pub struct Prepared {
    pub manifest_sha256: String,
    pub inputs: Vec<Input>,
    pub scene: Vec<u8>,
}

pub fn owned(uri: &str) -> bool {
    uri.starts_with("rbxasset://assets/") || uri.starts_with("rbxasset://scenes/")
}

fn references(bytes: &[u8]) -> Result<Vec<String>> {
    let dom = rbx_binary::from_reader(bytes)?;
    let mut refs = HashSet::new();
    for node in dom.descendants() {
        for value in node.properties.values() {
            let uri = match value {
                Variant::Content(v) => v.as_uri(),
                Variant::ContentId(v) => Some(v.as_str()),
                _ => None,
            };
            if let Some(uri) = uri.filter(|u| owned(u)) {
                refs.insert(uri.to_owned());
            }
        }
    }
    Ok(refs.into_iter().collect())
}

fn bytes(root: &Path, path: &str, expected: &str, limit: u64) -> Result<Vec<u8>> {
    let path = root.join(path).canonicalize()?;
    if !path.starts_with(root) {
        return Err("artifact escaped verified bundle".into());
    }
    let file = fs::File::open(path)?;
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(limit.checked_add(1).ok_or("maxUploadBytes overflow")?)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("artifact exceeds configured maxUploadBytes".into());
    }
    if digest(&bytes) != expected {
        return Err("artifact changed after bundle verification".into());
    }
    Ok(bytes)
}

pub fn prepare(directory: &Path, config: &Config) -> Result<Prepared> {
    match (&config.creator.user_id, &config.creator.group_id) {
        (Some(value), None) | (None, Some(value)) => id(value)?,
        _ => return Err("choose exactly one creator userId or groupId".into()),
    }
    id(&config.universe_id)?;
    id(&config.place_id)?;
    if config.policy.max_upload_bytes == 0 {
        return Err("maxUploadBytes must be positive".into());
    }
    let root = directory.canonicalize()?;
    let verification = bundle_verify::verify(&root)?;
    let manifest_bytes = fs::read(root.join("manifest.json"))?;
    if digest(&manifest_bytes) != verification.manifest_sha256 {
        return Err("manifest changed after verification".into());
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    let scene = manifest
        .scenes
        .iter()
        .find(|s| s.id == config.scene)
        .ok_or("unknown deployment scene")?;
    if scene.format != "rbxl" {
        return Err("place publishing requires a prebuilt rbxl scene".into());
    }
    let scene = bytes(
        &root,
        &scene.path,
        &scene.sha256,
        config.policy.max_upload_bytes,
    )?;
    let mut inputs = HashMap::new();
    for upload in &config.uploads {
        if upload.display_name.trim().is_empty() {
            return Err("upload displayName must not be empty".into());
        }
        let entry = manifest
            .files
            .iter()
            .find(|a| a.asset == upload.asset && a.file == upload.file)
            .ok_or("upload selects an unknown bundle artifact")?;
        let bytes = bytes(
            &root,
            &entry.path,
            &entry.sha256,
            config.policy.max_upload_bytes,
        )?;
        let dependencies = match upload.asset_type {
            AssetType::TexturePack => texture_pack::references(&bytes)?.into_values().collect(),
            AssetType::Model|AssetType::Animation => {
                let dom = rbx_binary::from_reader(bytes.as_slice())?;
                if upload.asset_type==AssetType::Animation && !dom.descendants().any(|n|n.class.as_str()=="KeyframeSequence") {
                    return Err("Animation upload requires a native KeyframeSequence".into());
                }
                references(&bytes)?
            }
            AssetType::Image if bytes.starts_with(b"\x89PNG\r\n\x1a\n")=>vec![],
            AssetType::Mesh if bytes.starts_with(b"version 2.00\n") || bytes.starts_with(b"version 4.01\n")=>vec![],
            AssetType::Audio if bytes.starts_with(b"OggS")=>vec![],
            _=>return Err("upload content does not match the selected native type; DDS/video/source-model uploads are not supported".into()),
        };
        let input = Input {
            upload: upload.clone(),
            uri: entry.local_uri.clone(),
            source_sha256: entry.sha256.clone(),
            bytes,
            dependencies,
        };
        if inputs.insert(entry.local_uri.clone(), input).is_some() {
            return Err("duplicate upload selection".into());
        }
    }
    for uri in references(&scene)? {
        if !inputs.contains_key(&uri) {
            return Err(format!("missing upload selection for scene dependency {uri}").into());
        }
    }
    let mut ordered = Vec::new();
    let mut ready = HashSet::new();
    while !inputs.is_empty() {
        let mut keys: Vec<_> = inputs
            .iter()
            .filter(|(_, i)| i.dependencies.iter().all(|d| ready.contains(d)))
            .map(|(k, _)| k.clone())
            .collect();
        keys.sort();
        if keys.is_empty() {
            return Err("upload dependencies contain a cycle or missing selection".into());
        }
        for key in keys {
            ready.insert(key.clone());
            ordered.push(inputs.remove(&key).unwrap());
        }
    }
    Ok(Prepared {
        manifest_sha256: verification.manifest_sha256,
        inputs: ordered,
        scene,
    })
}

pub fn request(config: &Config, input: &Input) -> Value {
    let creator = match (&config.creator.user_id, &config.creator.group_id) {
        (Some(id), None) => json!({"userId":id}),
        (None, Some(id)) => json!({"groupId":id}),
        _ => unreachable!("preflight validates creator"),
    };
    json!({"assetType":input.upload.asset_type,"displayName":input.upload.display_name,
        "description":input.upload.description,"creationContext":{"creator":creator,"expectedPrice":0}})
}

pub fn payload(input: &Input, ids: &HashMap<String, String>) -> Result<Vec<u8>> {
    match input.upload.asset_type {
        AssetType::TexturePack => {
            let refs = texture_pack::references(&input.bytes)?;
            let mut xml = String::from_utf8(input.bytes.clone())?;
            for uri in refs.values() {
                let remote = ids.get(uri).ok_or("unresolved pack dependency")?;
                let asset_id = remote
                    .strip_prefix("rbxassetid://")
                    .ok_or("invalid pack mapping")?;
                id(asset_id)?;
                xml = xml.replace(&format!(">{uri}<"), &format!(">{asset_id}<"));
            }
            Ok(xml.into_bytes())
        }
        AssetType::Model | AssetType::Animation => {
            Ok(deployment::rewrite_native(&input.bytes, ids)?.0)
        }
        _ => Ok(input.bytes.clone()),
    }
}
