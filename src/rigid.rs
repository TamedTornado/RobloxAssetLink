//! Shared numerical admission for native CFrame-compatible source matrices.
use crate::Result;
use glam::{Mat4, Vec4};

pub(crate) fn validate(matrix: Mat4, tolerance: f32) -> Result<()> {
    if !tolerance.is_finite() || tolerance <= 0. || tolerance >= 1. {
        return Err("rigidTolerance must be finite and between zero and one".into());
    }
    let x = matrix.x_axis.truncate();
    let y = matrix.y_axis.truncate();
    let z = matrix.z_axis.truncate();
    let bottom = Vec4::new(
        matrix.x_axis.w,
        matrix.y_axis.w,
        matrix.z_axis.w,
        matrix.w_axis.w,
    );
    if !matrix.is_finite()
        || !bottom.abs_diff_eq(Vec4::W, tolerance)
        || (x.length_squared() - 1.).abs() > tolerance
        || (y.length_squared() - 1.).abs() > tolerance
        || (z.length_squared() - 1.).abs() > tolerance
        || x.dot(y).abs() > tolerance
        || x.dot(z).abs() > tolerance
        || y.dot(z).abs() > tolerance
        || (x.cross(y).dot(z) - 1.).abs() > tolerance
    {
        return Err(
            "matrices must be finite rigid transforms without scale, shear or reflection".into(),
        );
    }
    Ok(())
}
