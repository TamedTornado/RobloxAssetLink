//! PhysicsGrid v2 wire preservation and native lazy spatial-index generation.
//! This does not precompute triangle/contact geometry.
use crate::{Result, terrain_grid};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Native decoder consumes exactly three independently delta-coded groups.
const GROUP_COUNT: usize = 3;
const COORDINATE_BYTES: usize = 12;
const MAX_EXPONENT: u8 = 8;

#[derive(Debug, PartialEq, Eq)]
pub struct Grid {
    pub exponent: u8,
    /// Preserve order and duplicates: the independent native fixture has both.
    /// Group meanings are not exposed as unverified solid/liquid classifications.
    pub coordinate_groups: [Vec<[i32; 3]>; GROUP_COUNT],
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_entries: usize,
}

fn validate(exponent: u8, limits: &Limits) -> Result<()> {
    if exponent > MAX_EXPONENT {
        return Err("unsupported PhysicsGrid exponent".into());
    }
    if limits.max_entries == 0 {
        return Err("PhysicsGrid entry budget must be positive".into());
    }
    Ok(())
}

pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Grid> {
    if bytes.first() != Some(&2) {
        return Err("only PhysicsGrid version two is implemented".into());
    }
    let exponent = *bytes.get(1).ok_or("truncated PhysicsGrid header")?;
    validate(exponent, limits)?;
    let mut offset = 2usize;
    let mut total = 0usize;
    let mut coordinate_groups = std::array::from_fn(|_| Vec::new());
    for group in &mut coordinate_groups {
        let count_bytes: [u8; 4] = bytes
            .get(offset..offset + 4)
            .ok_or("truncated PhysicsGrid count")?
            .try_into()?;
        offset += 4;
        let count = usize::try_from(u32::from_be_bytes(count_bytes))?;
        if count > limits.max_entries.saturating_sub(total) {
            return Err("PhysicsGrid exceeds configured entry budget".into());
        }
        total += count;
        let length = count
            .checked_mul(COORDINATE_BYTES)
            .ok_or("PhysicsGrid byte count overflow")?;
        let end = offset
            .checked_add(length)
            .ok_or("PhysicsGrid offset overflow")?;
        let data = bytes
            .get(offset..end)
            .ok_or("truncated PhysicsGrid coordinates")?;
        group.try_reserve_exact(count)?;
        let mut coordinate = [0i32; 3];
        for encoded in data.chunks_exact(COORDINATE_BYTES) {
            for (index, shift) in [24, 16, 8, 0].into_iter().enumerate() {
                for axis in 0..3 {
                    coordinate[axis] = coordinate[axis]
                        .wrapping_add(i32::from(encoded[index * 3 + axis]) << shift);
                }
            }
            group.push(coordinate);
        }
        offset = end;
    }
    if offset != bytes.len() {
        return Err("trailing PhysicsGrid bytes".into());
    }
    Ok(Grid {
        exponent,
        coordinate_groups,
    })
}

pub fn encode(grid: &Grid, limits: &Limits) -> Result<Vec<u8>> {
    validate(grid.exponent, limits)?;
    let mut total = 0usize;
    for group in &grid.coordinate_groups {
        if group.len() > limits.max_entries.saturating_sub(total) {
            return Err("PhysicsGrid exceeds configured entry budget".into());
        }
        u32::try_from(group.len())?;
        total += group.len();
    }
    let mut bytes = vec![2, grid.exponent];
    bytes.try_reserve_exact(
        total
            .checked_mul(COORDINATE_BYTES)
            .and_then(|n| n.checked_add(GROUP_COUNT * 4))
            .ok_or("PhysicsGrid output size overflow")?,
    )?;
    for group in &grid.coordinate_groups {
        bytes.extend_from_slice(&u32::try_from(group.len())?.to_be_bytes());
        let mut previous = [0i32; 3];
        for coordinate in group {
            let delta: [u32; 3] =
                std::array::from_fn(|axis| coordinate[axis].wrapping_sub(previous[axis]) as u32);
            for shift in [24, 16, 8, 0] {
                for axis in delta {
                    bytes.push((axis >> shift) as u8);
                }
            }
            previous = *coordinate;
        }
    }
    Ok(bytes)
}

/// Native group-zero entries request lazy mask recomputation from voxel data.
/// This produces a spatial candidate index, not triangle/contact geometry.
pub fn lazy_index(voxels: &terrain_grid::Grid, limits: &Limits) -> Result<Grid> {
    const REGION_EDGE: i32 = 8;
    const NATIVE_EXPONENT: u8 = 3;
    validate(NATIVE_EXPONENT, limits)?;
    if voxels.chunk_exponent > terrain_grid::MAX_CHUNK_EXPONENT {
        return Err("unsupported SmoothGrid chunk exponent".into());
    }
    let edge = 1i32 << voxels.chunk_exponent;
    let count = (edge as usize).pow(3);
    let mut candidates = BTreeSet::new();
    let mut chunks = BTreeSet::new();
    for chunk in &voxels.chunks {
        if chunk.cells.len() != count || !chunks.insert(chunk.coordinate) {
            return Err("invalid SmoothGrid chunk shape/coordinate".into());
        }
        let mut origin = [0i32; 3];
        for (axis, value) in origin.iter_mut().enumerate() {
            *value = chunk.coordinate[axis]
                .checked_mul(edge)
                .ok_or("terrain physics coordinate overflow")?;
        }
        for (index, cell) in chunk.cells.iter().enumerate() {
            if cell.material == 0 {
                continue;
            }
            let e = edge as usize;
            let local = [index % e, index / (e * e), (index / e) % e];
            let mut low = [0; 3];
            let mut high = [0; 3];
            for axis in 0..3 {
                let coordinate = origin[axis]
                    .checked_add(local[axis] as i32)
                    .ok_or("terrain physics coordinate overflow")?;
                low[axis] = coordinate
                    .checked_sub(1)
                    .ok_or("terrain physics halo overflow")?
                    .div_euclid(REGION_EDGE);
                high[axis] = coordinate
                    .checked_add(1)
                    .ok_or("terrain physics halo overflow")?
                    .div_euclid(REGION_EDGE);
            }
            // Native masks inspect an eight-cell region with a one-cell border.
            // Index all regions whose inspection volume can contain this cell.
            for x in low[0]..=high[0] {
                for y in low[1]..=high[1] {
                    for z in low[2]..=high[2] {
                        let coordinate = [x, y, z];
                        if !candidates.contains(&coordinate)
                            && candidates.len() >= limits.max_entries
                        {
                            return Err(
                                "terrain physics index exceeds configured entry budget".into()
                            );
                        }
                        candidates.insert(coordinate);
                    }
                }
            }
        }
    }
    Ok(Grid {
        exponent: NATIVE_EXPONENT,
        coordinate_groups: [candidates.into_iter().collect(), Vec::new(), Vec::new()],
    })
}
