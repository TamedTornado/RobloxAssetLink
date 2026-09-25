//! Native DDS luminance profile. Scalar samples are linear, not sRGB colors.
use crate::Result;
use image::{GrayImage, Luma};

const DDS_HEADER_BYTES: u64 = 128;

pub fn encode(image: &GrayImage, mipmaps: bool, max_output_bytes: u64) -> Result<Vec<u8>> {
    let (mut width, mut height) = image.dimensions();
    if width == 0 || height == 0 || max_output_bytes == 0 {
        return Err("DDS dimensions and output budget must be positive".into());
    }
    let mut size = DDS_HEADER_BYTES;
    let mut levels = 0_u32;
    loop {
        size = size
            .checked_add(u64::from(width) * u64::from(height))
            .ok_or("DDS output size overflow")?;
        levels += 1;
        if !mipmaps || (width == 1 && height == 1) {
            break;
        }
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    if size > max_output_bytes {
        return Err("DDS output exceeds maxOutputBytes".into());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(usize::try_from(size)?)?;
    bytes.extend_from_slice(b"DDS ");
    // DDS_HEADER, DDS_PIXELFORMAT and DDSCAPS: Microsoft DDS wire constants.
    let mut header = [0_u32; 31];
    header[0] = 124;
    header[1] = 0x100f | if levels > 1 { 0x20000 } else { 0 };
    header[2] = image.height();
    header[3] = image.width();
    header[4] = image.width(); // tightly packed bytes per row
    header[6] = levels;
    header[18] = 32;
    header[19] = 0x20000; // DDPF_LUMINANCE
    header[21] = 8;
    header[22] = 0xff;
    header[26] = 0x1000 | if levels > 1 { 0x400008 } else { 0 };
    for word in header {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(image.as_raw());
    let mut previous = image.clone();
    for _ in 1..levels {
        previous = downsample(&previous);
        bytes.extend_from_slice(previous.as_raw());
    }
    Ok(bytes)
}

// Exact area coverage includes every source texel for odd dimensions. Integer
// weights keep repeatability independent of floating-point contraction choices.
fn downsample(source: &GrayImage) -> GrayImage {
    let (width, height) = ((source.width() / 2).max(1), (source.height() / 2).max(1));
    GrayImage::from_fn(width, height, |x, y| {
        let (left, right) = (
            u64::from(x) * u64::from(source.width()),
            u64::from(x + 1) * u64::from(source.width()),
        );
        let (top, bottom) = (
            u64::from(y) * u64::from(source.height()),
            u64::from(y + 1) * u64::from(source.height()),
        );
        let mut sum = 0_u128;
        let mut total = 0_u128;
        for sy in top / u64::from(height)..bottom.div_ceil(u64::from(height)) {
            let wy = bottom.min((sy + 1) * u64::from(height)) - top.max(sy * u64::from(height));
            for sx in left / u64::from(width)..right.div_ceil(u64::from(width)) {
                let wx = right.min((sx + 1) * u64::from(width)) - left.max(sx * u64::from(width));
                let weight = u128::from(wx) * u128::from(wy);
                sum += u128::from(source.get_pixel(sx as u32, sy as u32)[0]) * weight;
                total += weight;
            }
        }
        Luma([((sum + total / 2) / total) as u8])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_scalar_chain_includes_edges_and_obeys_exact_budget() {
        let image = GrayImage::from_raw(3, 1, vec![0, 0, 255]).unwrap();
        let bytes = encode(&image, true, 132).unwrap();
        assert_eq!(&bytes[..4], b"DDS ");
        assert_eq!(&bytes[128..], &[0, 0, 255, 85]);
        assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 2);
        assert_eq!(encode(&image, true, 132).unwrap(), bytes);
        assert!(encode(&image, true, 131).is_err());
        assert_eq!(encode(&image, false, 131).unwrap().len(), 131);
    }

    #[test]
    fn scalar_average_is_linear_and_two_dimensional() {
        let image = GrayImage::from_raw(2, 2, vec![0, 255, 0, 255]).unwrap();
        assert_eq!(downsample(&image).as_raw(), &[128]);
        assert!(encode(&GrayImage::new(0, 0), true, 1024).is_err());
        assert!(encode(&image, true, 0).is_err());
    }
}
