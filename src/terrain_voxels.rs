//! Sparse named-material voxels to the established SmoothGrid v1 profile.
use crate::{
    Result,
    terrain_grid::{self, Cell, Chunk, Grid, Limits},
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

/// Native material slots, not configurable project aliases or public enum IDs.
/// Established from WriteVoxels' enum-to-slot table; see terrain-format.md.
const MATERIALS: [&str; 23] = [
    "Air",
    "Water",
    "Grass",
    "Slate",
    "Concrete",
    "Brick",
    "Sand",
    "WoodPlanks",
    "Rock",
    "Glacier",
    "Snow",
    "Sandstone",
    "Mud",
    "Basalt",
    "Ground",
    "CrackedLava",
    "Asphalt",
    "Cobblestone",
    "Ice",
    "LeafyGrass",
    "Salt",
    "Limestone",
    "Pavement",
];
/// Fixed native terrain cell resolution, not a project unit convention.
const STUDS_PER_VOXEL: f64 = 4.;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub metres_per_stud: f64,
    pub voxel_size_metres: f64,
    pub origin_metres: [f64; 3],
    pub alignment_tolerance_metres: f64,
    pub chunk_exponent: u8,
    pub limits: Limits,
    /// Project material alias -> supported Roblox material name.
    pub materials: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Voxel {
    /// Integer cell offsets from the configured metric origin (X,Y,Z).
    pub position: [i32; 3],
    pub material: String,
    pub occupancy: f32,
}

pub fn material_slot(name: &str) -> Result<u8> {
    MATERIALS
        .iter()
        .position(|candidate| *candidate == name)
        .map(|slot| slot as u8)
        .ok_or_else(|| format!("unsupported native terrain material: {name}").into())
}

/// Match the inspected WriteVoxels float quantizer, not round(o * 255).
pub fn cell(material: u8, occupancy: f32) -> Result<Cell> {
    if usize::from(material) >= MATERIALS.len()
        || !occupancy.is_finite()
        || !(0. ..=1.).contains(&occupancy)
    {
        return Err("terrain requires a supported material and finite occupancy in [0,1]".into());
    }
    if material == 0 && occupancy != 0. {
        return Err("Air terrain voxels must have zero occupancy".into());
    }
    if occupancy == 0. {
        return Ok(Cell::default());
    }
    Ok(Cell {
        material,
        occupancy: (occupancy * 256. - 0.5).trunc().clamp(0., 255.) as u8,
        auxiliary: 0,
    })
}

pub fn build(voxels: &[Voxel], config: &Config) -> Result<Grid> {
    let count = terrain_grid::chunk_cells(config.chunk_exponent, &config.limits)?;
    let size = config.metres_per_stud * STUDS_PER_VOXEL;
    let tolerance = config.alignment_tolerance_metres;
    if !size.is_finite()
        || size <= 0.
        || !config.voxel_size_metres.is_finite()
        || config.voxel_size_metres <= 0.
        || !tolerance.is_finite()
        || tolerance <= 0.
        || tolerance >= size / 2.
        || (config.voxel_size_metres - size).abs() > tolerance
    {
        return Err("terrain metric voxel size must match four studs with a valid explicit alignment tolerance".into());
    }
    if voxels.len() > config.limits.max_cells {
        return Err("terrain source exceeds configured cell budget".into());
    }
    let mut origin = [0i32; 3];
    for (axis, value) in config.origin_metres.iter().copied().enumerate() {
        let index = (value / size).round();
        if !value.is_finite()
            || !index.is_finite()
            || index < f64::from(i32::MIN)
            || index > f64::from(i32::MAX)
            || (index * size - value).abs() > tolerance
        {
            return Err("terrain metric origin must align to the native voxel grid within configured tolerance".into());
        }
        origin[axis] = index as i32;
    }
    let mut materials = BTreeMap::new();
    for (alias, name) in &config.materials {
        if alias.is_empty() {
            return Err("terrain material aliases must be nonempty".into());
        }
        materials.insert(alias.as_str(), material_slot(name)?);
    }
    let edge = 1i32 << config.chunk_exponent;
    let mut chunks: BTreeMap<[i32; 3], Vec<Cell>> = BTreeMap::new();
    let mut seen = HashSet::new();
    for voxel in voxels {
        if !seen.insert(voxel.position) {
            return Err("duplicate terrain voxel position".into());
        }
        let material = *materials
            .get(voxel.material.as_str())
            .ok_or("unknown terrain project material alias")?;
        let value = cell(material, voxel.occupancy)?;
        let mut position = [0i32; 3];
        for axis in 0..3 {
            position[axis] = origin[axis]
                .checked_add(voxel.position[axis])
                .ok_or("terrain voxel coordinate overflow")?;
        }
        if value == Cell::default() {
            continue;
        }
        let coordinate = position.map(|v| v.div_euclid(edge));
        if !chunks.contains_key(&coordinate) {
            if chunks.len() >= config.limits.max_chunks
                || chunks.len() >= config.limits.max_cells / count
            {
                return Err("terrain chunks exceed configured chunk/cell budget".into());
            }
            let mut cells = Vec::new();
            cells.try_reserve_exact(count)?;
            cells.resize(count, Cell::default());
            chunks.insert(coordinate, cells);
        }
        let [x, y, z] = position.map(|v| v.rem_euclid(edge) as usize);
        let index = x + edge as usize * (z + edge as usize * y);
        chunks
            .get_mut(&coordinate)
            .ok_or("missing allocated terrain chunk")?[index] = value;
    }
    Ok(Grid {
        chunk_exponent: config.chunk_exponent,
        chunks: chunks
            .into_iter()
            .map(|(coordinate, cells)| Chunk { coordinate, cells })
            .collect(),
    })
}
