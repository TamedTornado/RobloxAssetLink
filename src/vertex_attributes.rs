use crate::{Matrix, Result, dot};

pub(crate) fn color(value: [f32; 4]) -> Result<[u8; 4]> {
    if value
        .iter()
        .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
    {
        return Err("vertex colors must be finite normalized components".into());
    }
    Ok(value.map(|v| (v * 255.).round() as u8))
}

pub(crate) fn tangent(
    transform: Matrix,
    normal: [f32; 3],
    source: [f32; 4],
    mirrored: bool,
) -> Result<[u8; 4]> {
    if source.iter().any(|v| !v.is_finite()) || !matches!(source[3], -1. | 1.) {
        return Err("tangent must be finite with sign -1 or 1".into());
    }
    let direction: [f32; 3] =
        std::array::from_fn(|r| (0..3).map(|c| transform[c][r] * source[c]).sum());
    let projection = dot(direction, normal);
    let direction: [f32; 3] = std::array::from_fn(|i| direction[i] - projection * normal[i]);
    let length = dot(direction, direction).sqrt();
    if !length.is_finite() || length == 0. {
        return Err("tangent is degenerate after transformation".into());
    }
    let xyz = direction.map(|v| ((v / length).clamp(-1., 1.) * 127. + 127.).round() as u8);
    let sign = if mirrored { -source[3] } else { source[3] };
    Ok([xyz[0], xyz[1], xyz[2], if sign > 0. { 254 } else { 0 }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tangent_bias_matches_independent_format_example_and_reflections() {
        assert_eq!(
            tangent(crate::IDENTITY, [0., 1., 0.], [0., 0., -1., 1.], false).unwrap(),
            [127, 127, 0, 254]
        );
        let mut transform = crate::IDENTITY;
        transform[0][0] = -2.;
        assert_eq!(
            tangent(transform, [0., 1., 0.], [1., 0., 0., 1.], true).unwrap(),
            [0, 127, 127, 0]
        );
        assert!(tangent(transform, [0., 1., 0.], [0., 1., 0., 1.], true).is_err());
    }

    #[test]
    fn colors_quantize_without_losing_alpha_or_accepting_invalid_data() {
        assert_eq!(color([0., 0.5, 1., 0.25]).unwrap(), [0, 128, 255, 64]);
        assert!(color([f32::NAN, 0., 0., 1.]).is_err());
        assert!(color([2., 0., 0., 1.]).is_err());
    }
}
