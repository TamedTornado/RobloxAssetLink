//! Offline texture normalization and PBR channel conversion.
use crate::Result;
use image::{DynamicImage, GrayImage, ImageDecoder, ImageFormat, ImageReader, Luma};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Cursor, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub operation: Operation,
    pub max_width: u32,
    pub max_height: u32,
    pub max_decoded_bytes: u64,
    #[serde(default)]
    pub output: Output,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(tag = "format", rename_all = "camelCase", deny_unknown_fields)]
pub enum Output {
    #[serde(rename_all = "camelCase")]
    DdsRgba8 {
        mipmaps: bool,
        max_output_bytes: u64,
        mip_filter: crate::texture_rgba::Filter,
    },
    #[default]
    Png,
    #[serde(rename_all = "camelCase")]
    DdsL8 {
        mipmaps: bool,
        max_output_bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    DdsBc4 {
        mipmaps: bool,
        max_output_bytes: u64,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Operation {
    Color,
    NormalOpenGl,
    NormalDirectX,
    GltfMetallicRoughness,
    Roughness,
    Metalness,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: &'static str,
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub textures: Vec<Texture>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Texture {
    pub file: String,
    pub sha256: String,
    pub semantic: &'static str,
    pub color_space: &'static str,
}

pub fn convert(source: &Path, output: &Path, config: &Config) -> Result<Manifest> {
    if let Output::DdsRgba8 { mip_filter, .. } = config.output {
        let supported = match mip_filter {
            crate::texture_rgba::Filter::Normal => matches!(
                config.operation,
                Operation::NormalOpenGl | Operation::NormalDirectX
            ),
            _ => matches!(config.operation, Operation::Color),
        };
        if !supported {
            return Err("RGBA DDS mipFilter does not match texture operation".into());
        }
    }
    if matches!(config.output, Output::DdsL8 { .. } | Output::DdsBc4 { .. })
        && !matches!(
            config.operation,
            Operation::Roughness | Operation::Metalness | Operation::GltfMetallicRoughness
        )
    {
        return Err("scalar DDS output requires linear roughness/metalness maps".into());
    }
    if config.max_width == 0 || config.max_height == 0 || config.max_decoded_bytes == 0 {
        return Err("texture decode limits must be positive".into());
    }
    let mut reader = ImageReader::open(source)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(config.max_width);
    limits.max_image_height = Some(config.max_height);
    limits.max_alloc = Some(config.max_decoded_bytes);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    if decoder.icc_profile()?.is_some() {
        return Err("ICC-managed texture input requires an explicit color-profile conversion, not implemented yet".into());
    }
    if decoder.orientation()? != image::metadata::Orientation::NoTransforms {
        return Err(
            "texture input has EXIF orientation; bake orientation before conversion".into(),
        );
    }
    let decoded = DynamicImage::from_decoder(decoder)?;
    if !matches!(
        decoded.color(),
        image::ColorType::L8
            | image::ColorType::La8
            | image::ColorType::Rgb8
            | image::ColorType::Rgba8
    ) {
        return Err("this texture profile requires 8-bit channels; explicit HDR/16-bit conversion is not implemented".into());
    }
    let (width, height) = (decoded.width(), decoded.height());
    let images = match config.operation {
        Operation::Roughness | Operation::Metalness => {
            let pixels = decoded.to_rgba8();
            if pixels
                .pixels()
                .any(|p| p[0] != p[1] || p[1] != p[2] || p[3] != 255)
            {
                return Err(
                    "standalone scalar maps require equal RGB channels and opaque alpha".into(),
                );
            }
            let semantic = if matches!(config.operation, Operation::Roughness) {
                "roughness"
            } else {
                "metalness"
            };
            let gray = GrayImage::from_fn(width, height, |x, y| Luma([pixels.get_pixel(x, y)[0]]));
            vec![(semantic, "linear", DynamicImage::ImageLuma8(gray))]
        }
        Operation::Color => vec![(
            "color",
            "sRGB",
            DynamicImage::ImageRgba8(decoded.to_rgba8()),
        )],
        Operation::NormalOpenGl | Operation::NormalDirectX => {
            let mut pixels = decoded.to_rgb8();
            if matches!(config.operation, Operation::NormalDirectX) {
                for pixel in pixels.pixels_mut() {
                    pixel[1] = 255 - pixel[1];
                }
            }
            vec![("normal", "linear", DynamicImage::ImageRgb8(pixels))]
        }
        Operation::GltfMetallicRoughness => {
            let pixels = decoded.to_rgb8();
            let roughness =
                GrayImage::from_fn(width, height, |x, y| Luma([pixels.get_pixel(x, y)[1]]));
            let metalness =
                GrayImage::from_fn(width, height, |x, y| Luma([pixels.get_pixel(x, y)[2]]));
            vec![
                ("roughness", "linear", DynamicImage::ImageLuma8(roughness)),
                ("metalness", "linear", DynamicImage::ImageLuma8(metalness)),
            ]
        }
    };
    let mut files = Vec::new();
    let mut textures = Vec::new();
    for (semantic, color_space, image) in images {
        let (extension, bytes) = match config.output {
            Output::DdsRgba8 {
                mipmaps,
                max_output_bytes,
                mip_filter,
            } => (
                "dds",
                crate::texture_rgba::encode(
                    &image.to_rgba8(),
                    mipmaps,
                    max_output_bytes,
                    mip_filter,
                )?,
            ),
            Output::DdsBc4 {
                mipmaps,
                max_output_bytes,
            } => (
                "dds",
                crate::texture_dds::encode_bc4(
                    image.as_luma8().ok_or("DDS scalar image expected")?,
                    mipmaps,
                    max_output_bytes,
                )?,
            ),
            Output::Png => {
                let mut bytes = Cursor::new(Vec::new());
                image.write_to(&mut bytes, ImageFormat::Png)?;
                ("png", bytes.into_inner())
            }
            Output::DdsL8 {
                mipmaps,
                max_output_bytes,
            } => (
                "dds",
                crate::texture_dds::encode(
                    image.as_luma8().ok_or("DDS scalar image expected")?,
                    mipmaps,
                    max_output_bytes,
                )?,
            ),
        };
        let file = format!("{semantic}.{extension}");
        textures.push(Texture {
            file: file.clone(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            semantic,
            color_space,
        });
        files.push((file, bytes));
    }
    let manifest = Manifest {
        format: "roblox-texture-bundle",
        version: 1,
        width,
        height,
        textures,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    fs::create_dir(output)?;
    let result = (|| -> Result<()> {
        for (file, bytes) in files {
            fs::write(output.join(file), bytes)?;
        }
        fs::write(output.join("manifest.json"), manifest_bytes)?;
        Ok(())
    })();
    if let Err(error) = &result {
        fs::remove_dir_all(output).map_err(|cleanup| {
            format!("texture conversion failed: {error}; cleanup failed: {cleanup}")
        })?;
    }
    result?;
    Ok(manifest)
}
