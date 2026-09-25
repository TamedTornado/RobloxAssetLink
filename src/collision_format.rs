use crate::{Result, collision::Hull};

/// CSGPHS v5 layout, independently described in rbx_mesh union_physics/v3.rs
/// and v5.rs. The preamble lengths and scalar width are fixed protocol fields.
pub fn encode(hulls: &[Hull]) -> Result<Vec<u8>> {
    if hulls.is_empty() {
        return Err("collision payload requires hulls".into());
    }
    let mut bytes = b"CSGPHS\x05\0\0\0".to_vec();
    for hull in hulls {
        if hull.positions.len() < 4
            || hull.triangles.len() < 4
            || hull.positions.iter().flatten().any(|v| !v.is_finite())
            || hull
                .triangles
                .iter()
                .flatten()
                .any(|i| *i as usize >= hull.positions.len())
        {
            return Err("invalid collision hull geometry".into());
        }
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 16]);
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 12]);
        bytes.extend_from_slice(&1_f32.to_le_bytes());
        let position_count = u32::try_from(hull.positions.len())?
            .checked_mul(3)
            .ok_or("collision position count overflow")?;
        bytes.extend_from_slice(&position_count.to_le_bytes());
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        for coordinate in hull.positions.iter().flatten() {
            bytes.extend_from_slice(&coordinate.to_le_bytes());
        }
        let index_count = u32::try_from(hull.triangles.len())?
            .checked_mul(3)
            .ok_or("collision index count overflow")?;
        bytes.extend_from_slice(&index_count.to_le_bytes());
        for index in hull.triangles.iter().flatten() {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
    }
    Ok(bytes)
}
