//! Uncompressed RGBA DDS with explicit color/normal mip semantics.
use crate::Result;
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Filter {
    ColorStraightAlpha,
    ColorPremultipliedAlpha,
    Normal,
}

pub fn encode(image: &RgbaImage, mipmaps: bool, budget: u64, filter: Filter) -> Result<Vec<u8>> {
    let (mut w, mut h) = image.dimensions();
    if w == 0 || h == 0 || budget == 0 {
        return Err("DDS dimensions and output budget must be positive".into());
    }
    let mut size = 128_u64;
    let mut levels = 0;
    loop {
        size = size
            .checked_add(
                u64::from(w)
                    .checked_mul(u64::from(h))
                    .and_then(|n| n.checked_mul(4))
                    .ok_or("DDS size overflow")?,
            )
            .ok_or("DDS size overflow")?;
        levels += 1;
        if !mipmaps || (w == 1 && h == 1) {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    if size > budget {
        return Err("DDS output exceeds maxOutputBytes".into());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(usize::try_from(size)?)?;
    bytes.extend_from_slice(b"DDS ");
    // Standard legacy DDS RGBA8 layout: little-endian channel masks.
    let mut header = [0_u32; 31];
    header[0] = 124;
    header[1] = 0x100f | if levels > 1 { 0x20000 } else { 0 };
    header[2] = image.height();
    header[3] = image.width();
    header[4] = image
        .width()
        .checked_mul(4)
        .ok_or("DDS row pitch overflow")?;
    header[6] = levels;
    header[18] = 32;
    header[19] = 0x41;
    header[21] = 32;
    header[22] = 0xff;
    header[23] = 0xff00;
    header[24] = 0xff0000;
    header[25] = 0xff000000;
    header[26] = 0x1000 | if levels > 1 { 0x400008 } else { 0 };
    for word in header {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(image.as_raw());
    let mut previous = image.clone();
    for _ in 1..levels {
        previous = downsample(&previous, filter)?;
        bytes.extend_from_slice(previous.as_raw());
    }
    Ok(bytes)
}

fn linear(sample: u8) -> f64 {
    let x = f64::from(sample) / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(x: f64) -> u8 {
    let encoded = if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

fn downsample(source: &RgbaImage, filter: Filter) -> Result<RgbaImage> {
    let (w, h) = ((source.width() / 2).max(1), (source.height() / 2).max(1));
    let mut target = RgbaImage::new(w, h);
    for (x, y, pixel) in target.enumerate_pixels_mut() {
        let (left, right) = (
            u64::from(x) * u64::from(source.width()),
            u64::from(x + 1) * u64::from(source.width()),
        );
        let (top, bottom) = (
            u64::from(y) * u64::from(source.height()),
            u64::from(y + 1) * u64::from(source.height()),
        );
        let mut sum = [0.0; 4];
        let mut total = 0.0;
        for sy in top / u64::from(h)..bottom.div_ceil(u64::from(h)) {
            let wy = bottom.min((sy + 1) * u64::from(h)) - top.max(sy * u64::from(h));
            for sx in left / u64::from(w)..right.div_ceil(u64::from(w)) {
                let wx = right.min((sx + 1) * u64::from(w)) - left.max(sx * u64::from(w));
                let weight = wx as f64 * wy as f64;
                let sample = source.get_pixel(sx as u32, sy as u32);
                let alpha = f64::from(sample[3]) / 255.0;
                for channel in 0..3 {
                    let value = match filter {
                        Filter::Normal => f64::from(sample[channel]) / 127.5 - 1.0,
                        Filter::ColorStraightAlpha => linear(sample[channel]),
                        Filter::ColorPremultipliedAlpha => linear(sample[channel]) * alpha,
                    };
                    sum[channel] += value * weight;
                }
                sum[3] += alpha * weight;
                total += weight;
            }
        }
        let mut out = [0; 4];
        out[3] = (sum[3] / total * 255.0).round() as u8;
        if matches!(filter, Filter::Normal) {
            let length = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
            if length == 0.0 {
                return Err(
                    "normal mip contains cancelling vectors; no direction can be preserved".into(),
                );
            }
            for channel in 0..3 {
                out[channel] = ((sum[channel] / length + 1.0) * 127.5)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        } else {
            let divisor = if matches!(filter, Filter::ColorPremultipliedAlpha) {
                sum[3]
            } else {
                total
            };
            for channel in 0..3 {
                out[channel] = if divisor == 0.0 {
                    0
                } else {
                    srgb(sum[channel] / divisor)
                };
            }
        }
        *pixel = Rgba(out);
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_and_alpha_are_distinct_explicit_filter_choices() {
        let image = RgbaImage::from_raw(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 0]).unwrap();
        assert_eq!(
            downsample(&image, Filter::ColorStraightAlpha)
                .unwrap()
                .as_raw(),
            &[188, 188, 188, 128]
        );
        assert_eq!(
            downsample(&image, Filter::ColorPremultipliedAlpha)
                .unwrap()
                .as_raw(),
            &[255, 255, 255, 128]
        );
        assert!(encode(&image, true, 139, Filter::ColorStraightAlpha).is_err());
        assert_eq!(
            encode(&image, true, 140, Filter::ColorStraightAlpha)
                .unwrap()
                .len(),
            140
        );
    }

    #[test]
    fn normal_filter_renormalizes_and_rejects_undefined_direction() {
        let normal =
            RgbaImage::from_raw(2, 1, vec![255, 128, 128, 255, 128, 255, 128, 255]).unwrap();
        assert_eq!(
            downsample(&normal, Filter::Normal).unwrap().as_raw(),
            &[218, 218, 128, 255]
        );
        let cancelled = RgbaImage::from_raw(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]).unwrap();
        assert!(downsample(&cancelled, Filter::Normal).is_err());
    }
}
