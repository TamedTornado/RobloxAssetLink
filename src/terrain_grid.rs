//! SmoothGrid version-one wire codec, not legacy ClusterGridV3.
//! Raw material/occupancy bytes: no unproved public Enum.Material mapping.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Observed native format bound: the reader rejects chunk exponents above eight.
pub(crate) const MAX_CHUNK_EXPONENT: u8 = 8;
/// The run length is stored as a byte plus one.
const MAX_RUN: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub material: u8,
    pub occupancy: u8,
    /// Extra channel byte admitted for partially occupied solid cells.
    /// Its game-facing liquid semantics still require independent confirmation.
    pub auxiliary: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Chunk {
    pub coordinate: [i32; 3],
    /// Native ordering: X fastest, then Z, then Y.
    pub cells: Vec<Cell>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Grid {
    pub chunk_exponent: u8,
    pub chunks: Vec<Chunk>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_chunks: usize,
    pub max_cells: usize,
}

pub(crate) fn chunk_cells(exponent: u8, limits: &Limits) -> Result<usize> {
    if limits.max_chunks == 0 || limits.max_cells == 0 {
        return Err("terrain resource limits must be positive".into());
    }
    if exponent > MAX_CHUNK_EXPONENT {
        return Err("unsupported SmoothGrid chunk exponent".into());
    }
    let count = 1usize << (3 * u32::from(exponent));
    if count > limits.max_cells {
        return Err("terrain chunk exceeds configured cell budget".into());
    }
    Ok(count)
}

fn byte(bytes: &[u8], offset: &mut usize) -> Result<u8> {
    let value = *bytes.get(*offset).ok_or("truncated SmoothGrid payload")?;
    *offset += 1;
    Ok(value)
}

pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Grid> {
    let mut offset = 0;
    if byte(bytes, &mut offset)? != 1 {
        return Err("only SmoothGrid serialization version one is implemented".into());
    }
    let chunk_exponent = byte(bytes, &mut offset)?;
    let count = chunk_cells(chunk_exponent, limits)?;
    let mut chunks = Vec::new();
    let mut coordinate = [0i32; 3];
    let mut seen = HashSet::new();
    while offset < bytes.len() {
        if chunks.len() >= limits.max_chunks || chunks.len() >= limits.max_cells / count {
            return Err("terrain exceeds configured chunk/cell budget".into());
        }
        // Delta coordinates are interleaved by byte significance, not three
        // ordinary contiguous integers. Native additions wrap in 32 bits.
        for shift in [24, 16, 8, 0] {
            for axis in &mut coordinate {
                *axis = axis.wrapping_add(i32::from(byte(bytes, &mut offset)?) << shift);
            }
        }
        if !seen.insert(coordinate) {
            return Err("duplicate SmoothGrid chunk coordinate".into());
        }
        let mut cells = Vec::new();
        cells.try_reserve_exact(count)?;
        while cells.len() < count {
            let header = byte(bytes, &mut offset)?;
            let material = header & 0x3f;
            let occupancy = if header & 0x40 != 0 {
                byte(bytes, &mut offset)?
            } else {
                255
            };
            let length = if header & 0x80 != 0 {
                usize::from(byte(bytes, &mut offset)?) + 1
            } else {
                1
            };
            let auxiliary = if header & 0x80 != 0 && length == 1 {
                byte(bytes, &mut offset)?
            } else {
                0
            };
            if length > count - cells.len() {
                return Err("SmoothGrid run exceeds chunk boundary".into());
            }
            let cell = Cell {
                material,
                occupancy: if material == 0 { 0 } else { occupancy },
                auxiliary: if material > 1 && occupancy != 255 {
                    auxiliary
                } else {
                    0
                },
            };
            cells.resize(cells.len() + length, cell);
        }
        chunks.push(Chunk { coordinate, cells });
    }
    Ok(Grid {
        chunk_exponent,
        chunks,
    })
}

fn validate(cell: Cell) -> Result<()> {
    if cell.material > 63
        || (cell.material == 0 && cell.occupancy != 0)
        || (cell.auxiliary != 0 && (cell.material <= 1 || cell.occupancy == 255))
    {
        return Err("cell cannot be represented losslessly by SmoothGrid version one".into());
    }
    Ok(())
}

pub fn encode(grid: &Grid, limits: &Limits) -> Result<Vec<u8>> {
    let count = chunk_cells(grid.chunk_exponent, limits)?;
    if grid.chunks.len() > limits.max_chunks || grid.chunks.len() > limits.max_cells / count {
        return Err("terrain exceeds configured chunk/cell budget".into());
    }
    let mut output = vec![1, grid.chunk_exponent];
    let mut previous = [0i32; 3];
    let mut seen = HashSet::new();
    for chunk in &grid.chunks {
        if chunk.cells.len() != count || !seen.insert(chunk.coordinate) {
            return Err("terrain chunk has incorrect cell count or duplicate coordinate".into());
        }
        let delta: [u32; 3] =
            std::array::from_fn(|axis| chunk.coordinate[axis].wrapping_sub(previous[axis]) as u32);
        for shift in [24, 16, 8, 0] {
            for axis in delta {
                output.push((axis >> shift) as u8);
            }
        }
        previous = chunk.coordinate;
        let mut index = 0;
        while index < count {
            let cell = chunk.cells[index];
            validate(cell)?;
            let mut length = 1;
            if cell.auxiliary == 0 {
                while length < MAX_RUN
                    && index + length < count
                    && chunk.cells[index + length] == cell
                {
                    length += 1;
                }
            }
            let explicit_occupancy = cell.material != 0 && cell.occupancy != 255;
            let explicit_run = length != 1 || cell.auxiliary != 0;
            output.push(
                cell.material
                    | if explicit_occupancy { 0x40 } else { 0 }
                    | if explicit_run { 0x80 } else { 0 },
            );
            if explicit_occupancy {
                output.push(cell.occupancy);
            }
            if explicit_run {
                output.push((length - 1) as u8);
                if length == 1 {
                    output.push(cell.auxiliary);
                }
            }
            index += length;
        }
    }
    Ok(output)
}
