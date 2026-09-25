//! Offline scalar PNG heightmap -> metric sparse voxels -> native terrain grid.
use crate::{
    Result,
    terrain_grid::Grid,
    terrain_voxels::{self, Voxel},
};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZDirection {
    Positive,
    Negative,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub terrain: terrain_voxels::Config,
    pub material: String,
    pub pixel_size_metres: f64,
    pub floor_metres: f64,
    pub height_min_metres: f64,
    pub height_max_metres: f64,
    pub row_direction: ZDirection,
    pub max_width: u32,
    pub max_height: u32,
    pub max_decoded_bytes: u64,
}

pub fn read(source: &Path, config: &Config) -> Result<Grid> {
    // Validate the shared geometry/unit/material contract before allocating pixels.
    terrain_voxels::build(&[], &config.terrain)?;
    let material = config
        .terrain
        .materials
        .get(&config.material)
        .ok_or("heightmap material alias is not configured")?;
    if terrain_voxels::material_slot(material)? == 0 {
        return Err("heightmap columns require a non-Air material".into());
    }
    let size = config.terrain.metres_per_stud * terrain_voxels::STUDS_PER_VOXEL;
    if !config.pixel_size_metres.is_finite()
        || config.pixel_size_metres <= 0.
        || (config.pixel_size_metres - size).abs() > config.terrain.alignment_tolerance_metres
        || !config.floor_metres.is_finite()
        || !config.height_min_metres.is_finite()
        || !config.height_max_metres.is_finite()
        || config.height_min_metres < config.floor_metres
        || config.height_max_metres < config.height_min_metres
        || config.max_width == 0
        || config.max_height == 0
        || config.max_decoded_bytes == 0
    {
        return Err("heightmap requires finite ordered heights, native-sized pixels and positive image limits".into());
    }
    let floor = (config.floor_metres / size).round();
    let top = (config.height_max_metres / size).ceil();
    if floor < f64::from(i32::MIN)
        || floor > f64::from(i32::MAX)
        || !top.is_finite()
        || top > f64::from(i32::MAX)
        || (floor * size - config.floor_metres).abs() > config.terrain.alignment_tolerance_metres
    {
        return Err(
            "heightmap floor must be voxel aligned and height range must fit cell coordinates"
                .into(),
        );
    }
    let mut reader = ImageReader::open(source)?.with_guessed_format()?;
    if reader.format() != Some(ImageFormat::Png) {
        return Err("heightmap input must be a grayscale 8-bit or 16-bit PNG".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(config.max_width);
    limits.max_image_height = Some(config.max_height);
    limits.max_alloc = Some(config.max_decoded_bytes);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    if decoder.orientation()? != image::metadata::Orientation::NoTransforms {
        return Err("heightmap orientation must be baked into pixel order".into());
    }
    let image = DynamicImage::from_decoder(decoder)?;
    if !matches!(
        image,
        DynamicImage::ImageLuma8(_) | DynamicImage::ImageLuma16(_)
    ) {
        return Err(
            "heightmaps require scalar grayscale samples, not RGB, palette color or alpha".into(),
        );
    }
    let width = i32::try_from(image.width())?;
    let height = i32::try_from(image.height())?;
    let mut voxels = Vec::new();
    for row in 0..height {
        let z = match config.row_direction {
            ZDirection::Positive => row,
            ZDirection::Negative => -row,
        };
        for x in 0..width {
            let sample = match &image {
                DynamicImage::ImageLuma8(pixels) => {
                    f64::from(pixels.get_pixel(x as u32, row as u32)[0]) / f64::from(u8::MAX)
                }
                DynamicImage::ImageLuma16(pixels) => {
                    f64::from(pixels.get_pixel(x as u32, row as u32)[0]) / f64::from(u16::MAX)
                }
                _ => unreachable!("validated grayscale representation"),
            };
            // Samples are linear height data, not sRGB luminance. Interpolation
            // stays between finite endpoints without overflowing their difference.
            let surface =
                config.height_min_metres * (1. - sample) + config.height_max_metres * sample;
            let end = (surface / size).ceil() as i32;
            let cells = usize::try_from(i64::from(end) - floor as i64)?;
            if cells > config.terrain.limits.max_cells.saturating_sub(voxels.len()) {
                return Err("heightmap columns exceed configured cell budget".into());
            }
            voxels.try_reserve(cells)?;
            for y in floor as i32..end {
                let occupancy = ((surface - f64::from(y) * size) / size).clamp(0., 1.) as f32;
                voxels.push(Voxel {
                    position: [x, y, z],
                    material: config.material.clone(),
                    occupancy,
                });
            }
        }
    }
    terrain_voxels::build(&voxels, &config.terrain)
}
