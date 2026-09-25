//! Source glTF metallic/roughness material import, entirely offline.
use crate::{Result, material, texture};
use base64::Engine;
use image::{Rgba, RgbaImage};
use serde::Deserialize;
use serde_json::json;
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub material_index: usize,
    pub name: String,
    pub local_uri_prefix: String,
    pub max_width: u32,
    pub max_height: u32,
    pub max_decoded_bytes: u64,
}

fn image_bytes(source: &Path, gltf: &gltf::Gltf, image: gltf::Image<'_>) -> Result<Vec<u8>> {
    match image.source() {
        gltf::image::Source::View { view, .. } => {
            let buffers = crate::convert::load_gltf_buffers(source, gltf)?;
            let buffer = buffers
                .get(view.buffer().index())
                .ok_or("image buffer missing")?;
            let end = view
                .offset()
                .checked_add(view.length())
                .ok_or("image buffer view overflow")?;
            Ok(buffer
                .get(view.offset()..end)
                .ok_or("image buffer view outside buffer")?
                .to_vec())
        }
        gltf::image::Source::Uri { uri, .. } => {
            if let Some(data) = uri.strip_prefix("data:") {
                let (kind, bytes) = data.split_once(',').ok_or("invalid image data URI")?;
                if !matches!(kind, "image/png;base64" | "image/jpeg;base64") {
                    return Err("unsupported glTF image data URI".into());
                }
                return Ok(base64::engine::general_purpose::STANDARD.decode(bytes)?);
            }
            crate::convert::read_local_uri(source.parent().ok_or("source parent missing")?, uri)
        }
    }
}

fn pixels(
    source: &Path,
    gltf: &gltf::Gltf,
    texture: Option<gltf::Texture<'_>>,
    config: &Config,
    temporary: &Path,
    slot: &str,
) -> Result<RgbaImage> {
    let Some(texture) = texture else {
        return Ok(RgbaImage::from_pixel(1, 1, Rgba([255; 4])));
    };
    let sampler = texture.sampler();
    if sampler.wrap_s() != gltf::texture::WrappingMode::Repeat
        || sampler.wrap_t() != gltf::texture::WrappingMode::Repeat
        || sampler
            .mag_filter()
            .is_some_and(|f| f != gltf::texture::MagFilter::Linear)
        || sampler.min_filter().is_some_and(|f| {
            !matches!(
                f,
                gltf::texture::MinFilter::Linear | gltf::texture::MinFilter::LinearMipmapLinear
            )
        })
    {
        return Err("material sampler requires repeat wrapping and linear filtering; unsupported settings cannot be preserved".into());
    }
    let input = temporary.join(format!("{slot}.image"));
    fs::write(&input, image_bytes(source, gltf, texture.source())?)?;
    let output = temporary.join(format!("decoded-{slot}"));
    texture::convert(
        &input,
        &output,
        &texture::Config {
            operation: texture::Operation::Color,
            max_width: config.max_width,
            max_height: config.max_height,
            max_decoded_bytes: config.max_decoded_bytes,
        },
    )?;
    Ok(image::open(output.join("color.png"))?.to_rgba8())
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn linear(value: u8) -> f32 {
    let value = value as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(value: f32) -> u8 {
    byte(if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    })
}

pub fn convert(source: &Path, output: &Path, config: &Config) -> Result<material::Manifest> {
    convert_linked(source, output, config, None)
}

pub(crate) fn convert_linked(
    source: &Path,
    output: &Path,
    config: &Config,
    prefix: Option<&str>,
) -> Result<material::Manifest> {
    if config.max_width == 0 || config.max_height == 0 || config.max_decoded_bytes == 0 {
        return Err("material image limits must be positive".into());
    }
    let source = source.canonicalize()?;
    let gltf = gltf::Gltf::open(&source)?;
    if gltf.extensions_used().next().is_some() {
        return Err(
            "glTF material extensions are not implemented; refusing to discard their semantics"
                .into(),
        );
    }
    let material = gltf
        .materials()
        .nth(config.material_index)
        .ok_or("glTF material index is out of range")?;
    let pbr = material.pbr_metallic_roughness();
    if pbr
        .base_color_factor()
        .into_iter()
        .chain([pbr.metallic_factor(), pbr.roughness_factor()])
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err("glTF PBR factors must be finite and in [0,1]".into());
    }
    if material.occlusion_texture().is_some()
        || material.emissive_texture().is_some()
        || material.emissive_factor() != [0.0; 3]
    {
        return Err("glTF occlusion and emissive material mapping is not implemented".into());
    }
    if material.alpha_mode() == gltf::material::AlphaMode::Mask {
        return Err(
            "glTF alpha-mask rendering cannot be represented by this material profile".into(),
        );
    }
    if material.double_sided() {
        return Err("double-sided glTF material requires MeshPart geometry integration, not a SurfaceAppearance alone".into());
    }
    let base = pbr.base_color_texture();
    let packed = pbr.metallic_roughness_texture();
    let normal = material.normal_texture();
    if base.as_ref().is_some_and(|t| t.tex_coord() != 0)
        || packed.as_ref().is_some_and(|t| t.tex_coord() != 0)
        || normal
            .as_ref()
            .is_some_and(|t| t.tex_coord() != 0 || t.scale() != 1.0)
    {
        return Err("material requires UV0 and unit normal scale; additional UV sets and normal scaling are not implemented".into());
    }
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let mut color = pixels(
        &source,
        &gltf,
        base.map(|t| t.texture()),
        config,
        root,
        "base",
    )?;
    let factor = pbr.base_color_factor();
    for pixel in color.pixels_mut() {
        for channel in 0..3 {
            pixel[channel] = srgb(linear(pixel[channel]) * factor[channel]);
        }
        pixel[3] = if material.alpha_mode() == gltf::material::AlphaMode::Opaque {
            255
        } else {
            byte(pixel[3] as f32 / 255.0 * factor[3])
        };
    }
    color.save(root.join("color.png"))?;
    let packed = pixels(
        &source,
        &gltf,
        packed.map(|t| t.texture()),
        config,
        root,
        "packed",
    )?;
    let roughness = image::GrayImage::from_fn(packed.width(), packed.height(), |x, y| {
        image::Luma([byte(
            packed.get_pixel(x, y)[1] as f32 / 255.0 * pbr.roughness_factor(),
        )])
    });
    let metalness = image::GrayImage::from_fn(packed.width(), packed.height(), |x, y| {
        image::Luma([byte(
            packed.get_pixel(x, y)[2] as f32 / 255.0 * pbr.metallic_factor(),
        )])
    });
    roughness.save(root.join("roughness.png"))?;
    metalness.save(root.join("metalness.png"))?;
    let mut operations = vec![
        ("color.png", "color"),
        ("roughness.png", "roughness"),
        ("metalness.png", "metalness"),
    ];
    if let Some(normal) = normal {
        pixels(
            &source,
            &gltf,
            Some(normal.texture()),
            config,
            root,
            "normal",
        )?
        .save(root.join("normal.png"))?;
        operations.push(("normal.png", "normalOpenGl"));
    }
    let maps: Vec<_> = operations.into_iter().map(|(file, operation)| json!({"source":file,"config":{
        "operation":operation,"maxWidth":config.max_width,"maxHeight":config.max_height,"maxDecodedBytes":config.max_decoded_bytes
    }})).collect();
    let specification = json!({"name":config.name,"alphaMode":"Transparency","color":[1,1,1],"localUriPrefix":config.local_uri_prefix,"maps":maps});
    let document = root.join("material.json");
    fs::write(&document, serde_json::to_vec(&specification)?)?;
    material::convert_linked(&document, output, prefix)
}
