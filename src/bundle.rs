//! Offline build orchestration. Every artifact remains local until deployment.
use crate::{Result, scene};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub assets: Vec<Asset>,
    pub scenes: Vec<Scene>,
    pub cache: Option<crate::build_cache::Config>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CachedAsset {
    generated: Vec<String>,
    rig: Option<crate::animation::Rig>,
    skin: Option<crate::skin_import::Manifest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub conversion: Conversion,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Conversion {
    MediaSource {
        source: PathBuf,
        config: crate::media_transcode::Config,
    },
    Media {
        source: PathBuf,
    },
    Video {
        source: PathBuf,
        config: crate::video::Config,
    },
    MaterialGltf {
        source: PathBuf,
        config: crate::material_gltf::Config,
    },
    Material {
        source: PathBuf,
    },
    AnimationFbx {
        source: PathBuf,
        config: crate::animation_fbx::Config,
        bind_to: Option<String>,
    },
    Skin {
        source: PathBuf,
        config: crate::skin_import::Config,
    },
    AnimationGltf {
        source: PathBuf,
        config: crate::animation_gltf::Config,
        bind_to: Option<String>,
    },
    Animation {
        source: PathBuf,
    },
    Mesh {
        source: PathBuf,
        config: crate::convert::Config,
        collision: Option<crate::collision::Recipe>,
    },
    Texture {
        source: PathBuf,
        config: crate::texture::Config,
    },
    Audio {
        source: PathBuf,
        config: crate::audio::Config,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub source: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub files: Vec<Artifact>,
    pub scenes: Vec<SceneArtifact>,
    pub published: bool,
    pub engine_verified: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Artifact {
    pub asset: String,
    pub file: String,
    pub path: String,
    pub local_uri: String,
    pub sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneArtifact {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub format: String,
}

fn local(root: &Path, source: &Path) -> Result<PathBuf> {
    if source.is_absolute() {
        return Err("bundle source paths must be relative".into());
    }
    let path = root.join(source).canonicalize()?;
    if !path.starts_with(root) {
        return Err("bundle source path escapes its directory".into());
    }
    Ok(path)
}

fn key(id: &str) -> String {
    format!("{:x}", Sha256::digest(id.as_bytes()))
}

fn bind_target(asset: &Asset) -> Option<&str> {
    match &asset.conversion {
        Conversion::AnimationGltf { bind_to, .. } | Conversion::AnimationFbx { bind_to, .. } => {
            bind_to.as_deref()
        }
        _ => None,
    }
}

fn ordered_assets(plan: &Plan) -> Result<Vec<&Asset>> {
    let indices: HashMap<_, _> = plan
        .assets
        .iter()
        .enumerate()
        .map(|(index, asset)| (asset.id.as_str(), index))
        .collect();
    let mut dependents = vec![Vec::new(); plan.assets.len()];
    let mut waiting = vec![0; plan.assets.len()];
    for (index, asset) in plan.assets.iter().enumerate() {
        if let Some(target) = bind_target(asset) {
            let parent = *indices
                .get(target)
                .ok_or_else(|| format!("unknown animation bindTo asset: {target}"))?;
            if !matches!(plan.assets[parent].conversion, Conversion::Skin { .. }) {
                return Err("animation bindTo requires a skin conversion asset".into());
            }
            dependents[parent].push(index);
            waiting[index] += 1;
        }
    }
    let mut ready: BTreeSet<_> = waiting
        .iter()
        .enumerate()
        .filter_map(|(index, n)| (*n == 0).then_some(index))
        .collect();
    let mut result = Vec::new();
    while let Some(index) = ready.pop_first() {
        result.push(&plan.assets[index]);
        for child in &dependents[index] {
            waiting[*child] -= 1;
            if waiting[*child] == 0 {
                ready.insert(*child);
            }
        }
    }
    if result.len() != plan.assets.len() {
        return Err("asset conversion dependency cycle".into());
    }
    Ok(result)
}

fn build_asset(
    root: &Path,
    output: &Path,
    asset: &Asset,
    uri_prefix: &str,
    rig: &mut Option<crate::animation::Rig>,
    target: Option<&crate::skin_import::Manifest>,
    skin: &mut Option<crate::skin_import::Manifest>,
) -> Result<Vec<String>> {
    match &asset.conversion {
        Conversion::MediaSource { source, config } => {
            fs::create_dir(output)?;
            let manifest = crate::media_transcode::convert(
                &local(root, source)?,
                &output.join("video.webm"),
                config,
            )?;
            fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            Ok(vec!["video.webm".into()])
        }
        Conversion::Media { source } => {
            fs::create_dir(output)?;
            let manifest =
                crate::media_mux::build(&local(root, source)?, &output.join("video.webm"))?;
            fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            Ok(vec!["video.webm".into()])
        }
        Conversion::Video { source, config } => {
            fs::create_dir(output)?;
            let manifest =
                crate::video::convert(&local(root, source)?, &output.join("video.webm"), config)?;
            fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            Ok(vec!["video.webm".into()])
        }
        Conversion::MaterialGltf { source, config } => {
            let manifest = crate::material_gltf::convert_linked(
                &local(root, source)?,
                output,
                config,
                Some(uri_prefix),
            )?;
            let mut files = vec!["material.rbxm".to_owned()];
            files.extend(manifest.maps.into_values().map(|map| map.file));
            Ok(files)
        }
        Conversion::Material { source } => {
            let manifest =
                crate::material::convert_linked(&local(root, source)?, output, Some(uri_prefix))?;
            let mut files = vec!["material.rbxm".to_owned()];
            files.extend(manifest.maps.into_values().map(|map| map.file));
            Ok(files)
        }
        Conversion::AnimationFbx { source, config, .. } => {
            fs::create_dir(output)?;
            let manifest = crate::animation_fbx::convert_with_bind_pose(
                &local(root, source)?,
                &output.join("animation.rbxm"),
                config.clone(),
                target,
            )?;
            fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            *rig = Some(manifest.rig);
            Ok(vec!["animation.rbxm".into()])
        }
        Conversion::Skin { source, config } => {
            let manifest = crate::skin_import::convert(&local(root, source)?, output, config)?;
            let mut files: Vec<_> = manifest
                .meshes
                .iter()
                .map(|mesh| mesh.file.clone())
                .collect();
            files.push("rig.rbxm".into());
            *skin = Some(manifest);
            Ok(files)
        }
        Conversion::AnimationGltf { source, config, .. } => {
            fs::create_dir(output)?;
            let manifest = crate::animation_gltf::convert_with_bind_pose(
                &local(root, source)?,
                &output.join("animation.rbxm"),
                config.clone(),
                target,
            )?;
            fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            *rig = Some(manifest.rig);
            Ok(vec!["animation.rbxm".into()])
        }
        Conversion::Animation { source } => {
            fs::create_dir(output)?;
            let manifest =
                crate::animation::convert(&local(root, source)?, &output.join("animation.rbxm"))?;
            fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            Ok(vec!["animation.rbxm".into()])
        }
        Conversion::Mesh {
            source,
            config,
            collision,
        } => {
            let manifest = crate::convert::convert_linked(
                &local(root, source)?,
                output,
                config,
                collision.as_ref(),
                Some(uri_prefix),
            )?;
            let mut files = Vec::new();
            for mesh in manifest.meshes {
                files.push(mesh.file);
                if let Some(collision) = mesh.collision {
                    files.push(collision.file);
                }
            }
            for material in manifest.materials {
                files.push(format!("{}/material.rbxm", material.directory));
                files.extend(
                    material
                        .artifact
                        .maps
                        .into_values()
                        .map(|map| format!("{}/{}", material.directory, map.file)),
                );
            }
            Ok(files)
        }
        Conversion::Texture { source, config } => {
            let manifest = crate::texture::convert(&local(root, source)?, output, config)?;
            Ok(manifest.textures.into_iter().map(|t| t.file).collect())
        }
        Conversion::Audio { source, config } => {
            fs::create_dir(output)?;
            let manifest =
                crate::audio::convert(&local(root, source)?, &output.join("audio.ogg"), config)?;
            fs::write(
                output.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            Ok(vec!["audio.ogg".into()])
        }
    }
}

pub fn build(source: &Path, output: &Path) -> Result<Manifest> {
    let source = source.canonicalize()?;
    let root = source.parent().ok_or("bundle source directory missing")?;
    let plan_bytes = fs::read(&source)?;
    let plan: Plan = serde_json::from_slice(&plan_bytes)?;
    let raw_plan: serde_json::Value = serde_json::from_slice(&plan_bytes)?;
    let mut ids = HashSet::new();
    for id in plan.assets.iter().map(|v| &v.id) {
        if id.is_empty() || !ids.insert(id) {
            return Err("asset ids must be nonempty and unique".into());
        }
    }
    ids.clear();
    for id in plan.scenes.iter().map(|v| &v.id) {
        if id.is_empty() || !ids.insert(id) {
            return Err("scene ids must be nonempty and unique".into());
        }
    }
    if plan.assets.is_empty() && plan.scenes.is_empty() {
        return Err("empty build plan".into());
    }
    let ordered = ordered_assets(&plan)?;
    let inputs = crate::bundle_inputs::collect(&source, &plan)?;
    if inputs.plan_sha256 != format!("{:x}", Sha256::digest(&plan_bytes)) {
        return Err("bundle plan changed during source inventory".into());
    }
    fs::create_dir(output)?;
    let result = (|| -> Result<Manifest> {
        let cache = plan
            .cache
            .as_ref()
            .map(|config| crate::build_cache::Cache::open(&root.join(&config.directory), output))
            .transpose()?;
        fs::create_dir(output.join("assets"))?;
        fs::create_dir(output.join("scenes"))?;
        let mut bindings = scene::AssetMap::new();
        let mut files = Vec::new();
        let mut skins = HashMap::new();
        let mut cache_keys = HashMap::new();
        let mut cache_hits = Vec::new();
        let mut cache_misses = Vec::new();
        for asset in ordered {
            let directory = format!("assets/{}", key(&asset.id));
            let mut rig = None;
            let mut skin = None;
            let target = bind_target(asset)
                .map(|id| skins.get(id).ok_or("skin dependency was not converted"))
                .transpose()?;
            let cache_key = if let Some(cache) = &cache {
                let definition = raw_plan["assets"]
                    .as_array()
                    .ok_or("missing asset definitions")?
                    .iter()
                    .find(|value| value["id"].as_str() == Some(&asset.id))
                    .ok_or("missing asset definition")?;
                let dependency = bind_target(asset)
                    .map(|id| cache_keys.get(id).ok_or("missing dependency cache key"))
                    .transpose()?;
                Some(crate::build_cache::digest(&serde_json::json!({
                    "protocol":"roblox-asset-cache-v1", "toolchain":cache.toolchain,
                    "asset":definition, "inputs":inputs.assets[&asset.id], "dependency":dependency
                }))?)
            } else {
                None
            };
            let restored = match (&cache, &cache_key) {
                (Some(cache), Some(key)) => cache.restore(key, &output.join(&directory))?,
                _ => None,
            };
            let generated = if let Some(metadata) = restored {
                let restored: CachedAsset = serde_json::from_value(metadata)?;
                rig = restored.rig;
                skin = restored.skin;
                cache_hits.push(asset.id.clone());
                restored.generated
            } else {
                let generated = build_asset(
                    root,
                    &output.join(&directory),
                    asset,
                    &format!("rbxasset://{directory}/"),
                    &mut rig,
                    target,
                    &mut skin,
                )?;
                if let (Some(cache), Some(key)) = (&cache, &cache_key) {
                    // Recheck source closure before publishing this immutable entry.
                    if crate::bundle_inputs::collect(&source, &plan)? != inputs {
                        return Err("bundle source inputs changed during conversion".into());
                    }
                    cache.store(
                        key,
                        &output.join(&directory),
                        serde_json::json!({
                            "generated":generated,"rig":rig,"skin":skin
                        }),
                    )?;
                    cache_misses.push(asset.id.clone());
                }
                generated
            };
            if let Some(key) = cache_key {
                cache_keys.insert(asset.id.clone(), key);
            }
            if let Some(skin) = skin {
                skins.insert(asset.id.clone(), skin);
            }
            for file in generated {
                let path = format!("{directory}/{file}");
                let local_uri = format!("rbxasset://{path}");
                let bytes = fs::read(output.join(&path))?;
                bindings.insert(
                    scene::AssetReference {
                        asset: asset.id.clone(),
                        file: file.clone(),
                    },
                    scene::ResolvedAsset {
                        uri: local_uri.clone(),
                        path: output.join(&path),
                        animation_rig: if file == "animation.rbxm" {
                            rig.take()
                        } else {
                            None
                        },
                    },
                );
                files.push(Artifact {
                    asset: asset.id.clone(),
                    file,
                    path,
                    local_uri,
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                });
            }
        }
        let mut scenes = Vec::new();
        for input in &plan.scenes {
            let source = local(root, &input.source)?;
            let spec: scene::Specification = serde_json::from_slice(&fs::read(&source)?)?;
            let extension = match spec.kind {
                scene::Kind::Model => "rbxm",
                scene::Kind::Place => "rbxl",
            };
            let path = format!("scenes/{}.{}", key(&input.id), extension);
            let scene = scene::build_with_assets(&source, &output.join(&path), &bindings)?;
            scenes.push(SceneArtifact {
                id: input.id.clone(),
                path,
                sha256: scene.sha256,
                format: scene.format.into(),
            });
        }
        let manifest = Manifest {
            format: "roblox-offline-bundle".into(),
            version: 1,
            files,
            scenes,
            published: false,
            engine_verified: false,
        };
        if crate::bundle_inputs::collect(&source, &plan)? != inputs {
            return Err("bundle source inputs changed during conversion".into());
        }
        fs::write(
            output.join("build-inputs.json"),
            serde_json::to_vec_pretty(&inputs)?,
        )?;
        if cache.is_some() {
            fs::write(
                output.join("cache-report.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "hits":cache_hits,"misses":cache_misses
                }))?,
            )?;
        }
        fs::write(
            output.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(manifest)
    })();
    if let Err(error) = &result {
        fs::remove_dir_all(output).map_err(|cleanup| {
            format!("bundle build failed: {error}; cleanup failed: {cleanup}")
        })?;
    }
    result
}
