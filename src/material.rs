//! Native SurfaceAppearance assembly with explicit local texture dependencies.
use crate::{Result, texture};
use rbx_dom_weak::{
    InstanceBuilder, WeakDom,
    types::{Color3, Content, ContentId, Enum},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Specification {
    pub name: String,
    pub alpha_mode: String,
    /// Native SurfaceAppearance.Color; not a glTF linear baseColorFactor.
    pub color: [f32; 3],
    /// Where the output directory will be mounted in the local content tree.
    pub local_uri_prefix: String,
    pub maps: Vec<Map>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Map {
    pub source: PathBuf,
    pub config: texture::Config,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub file: String,
    pub uri: String,
    pub sha256: String,
    pub color_space: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: &'static str,
    pub sha256: String,
    pub maps: BTreeMap<String, Dependency>,
    pub texture_pack: PackDependency,
    pub engine_verified: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackDependency {
    pub file: String,
    pub uri: String,
    pub sha256: String,
}

fn validate(spec: &Specification) -> Result<Enum> {
    if spec.name.is_empty()
        || spec
            .color
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(
            "material requires a nonempty name and finite color components in [0,1]".into(),
        );
    }
    let prefix = spec.local_uri_prefix.strip_prefix("rbxasset://")
        .and_then(|p| p.strip_suffix('/'))
        .ok_or("material localUriPrefix must be rbxasset:// followed by a local path and trailing slash")?;
    if prefix.is_empty()
        || prefix.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || !part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        return Err("material localUriPrefix must contain safe local path segments".into());
    }
    let value = rbx_reflection_database::get_bundled()
        .enums
        .get("AlphaMode")
        .and_then(|e| e.items.get(spec.alpha_mode.as_str()))
        .ok_or("material alphaMode is not supported by the pinned reflection database")?;
    Ok(Enum::from_u32(*value))
}

pub fn convert(source: &Path, output: &Path) -> Result<Manifest> {
    convert_linked(source, output, None)
}

/// Bundle assembly owns the final content location and relinks before encoding.
pub(crate) fn convert_linked(
    source: &Path,
    output: &Path,
    prefix: Option<&str>,
) -> Result<Manifest> {
    let spec: Specification = serde_json::from_slice(&fs::read(source)?)?;
    let alpha = validate(&spec)?;
    let spec = match prefix {
        Some(prefix) => Specification {
            local_uri_prefix: prefix.to_owned(),
            ..spec
        },
        None => spec,
    };
    validate(&spec)?;
    let root = source
        .canonicalize()?
        .parent()
        .ok_or("material source has no parent")?
        .to_owned();
    let mut inputs = Vec::new();
    for map in &spec.maps {
        if map.source.as_os_str().is_empty()
            || map
                .source
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("material map source must be a contained relative path".into());
        }
        let path = root.join(&map.source).canonicalize()?;
        if !path.starts_with(&root) {
            return Err("material map source escapes the source directory".into());
        }
        inputs.push(path);
    }

    fs::create_dir(output)?;
    let result = (|| -> Result<Manifest> {
        let mut maps = BTreeMap::new();
        let mut appearance = InstanceBuilder::new("SurfaceAppearance")
            .with_name(&spec.name)
            .with_property("AlphaMode", alpha)
            .with_property(
                "Color",
                Color3::new(spec.color[0], spec.color[1], spec.color[2]),
            );
        for (index, (map, input)) in spec.maps.iter().zip(inputs).enumerate() {
            let directory = format!("map-{index}");
            let converted = texture::convert(&input, &output.join(&directory), &map.config)?;
            for texture in converted.textures {
                let property = match texture.semantic {
                    "color" => "ColorMapContent",
                    "normal" => "NormalMapContent",
                    "roughness" => "RoughnessMapContent",
                    "metalness" => "MetalnessMapContent",
                    _ => return Err("unsupported material texture semantic".into()),
                };
                if maps.contains_key(texture.semantic) {
                    return Err(format!("duplicate material map: {}", texture.semantic).into());
                }
                let file = format!("{directory}/{}", texture.file);
                let uri = format!("{}{file}", spec.local_uri_prefix);
                appearance = appearance.with_property(property, Content::from_uri(uri.clone()));
                maps.insert(
                    texture.semantic.to_owned(),
                    Dependency {
                        file,
                        uri,
                        sha256: texture.sha256,
                        color_space: texture.color_space,
                    },
                );
            }
        }
        let pack = crate::texture_pack::encode(
            alpha.to_u32(),
            &maps
                .iter()
                .map(|(semantic, map)| (semantic.clone(), map.uri.clone()))
                .collect(),
        )?;
        let texture_pack = PackDependency {
            file: "texturepack.xml".into(),
            uri: format!("{}texturepack.xml", spec.local_uri_prefix),
            sha256: format!("{:x}", Sha256::digest(&pack)),
        };
        appearance =
            appearance.with_property("TexturePack", ContentId::from(texture_pack.uri.clone()));
        let dom = WeakDom::new(appearance);
        let mut bytes = Vec::new();
        rbx_binary::Serializer::new()
            .reflection_database(rbx_reflection_database::get_bundled())
            .serialize(&mut bytes, &dom, &[dom.root_ref()])?;
        let manifest = Manifest {
            format: "roblox-surface-appearance",
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            maps,
            texture_pack,
            engine_verified: false,
        };
        fs::write(output.join("material.rbxm"), bytes)?;
        fs::write(output.join("texturepack.xml"), pack)?;
        fs::write(
            output.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(manifest)
    })();
    if let Err(error) = &result {
        fs::remove_dir_all(output).map_err(|cleanup| {
            format!("material conversion failed: {error}; cleanup failed: {cleanup}")
        })?;
    }
    result
}
