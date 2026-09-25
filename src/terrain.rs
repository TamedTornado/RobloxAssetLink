//! File conversion boundary for local terrain payloads. No physics-cache claims.
use crate::{Result, terrain_grid, terrain_heightmap, terrain_voxels};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Specification {
    Voxels {
        config: terrain_voxels::Config,
        voxels: Vec<terrain_voxels::Voxel>,
        physics: Option<crate::terrain_physics::Limits>,
    },
    Heightmap {
        source: PathBuf,
        config: terrain_heightmap::Config,
        physics: Option<crate::terrain_physics::Limits>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: &'static str,
    pub file: &'static str,
    pub sha256: String,
    pub source_sha256: String,
    pub heightmap_sha256: Option<String>,
    pub metres_per_stud: f64,
    pub chunk_exponent: u8,
    pub chunks: usize,
    pub cells: usize,
    pub physics_generated: bool,
    pub physics_file: Option<&'static str>,
    pub physics_sha256: Option<String>,
    pub physics_kind: Option<&'static str>,
    pub engine_verified: bool,
}

pub(crate) fn image_source(document: &Path, source: &Path) -> Result<PathBuf> {
    let root = document.parent().ok_or("terrain source parent missing")?;
    let path = root.join(source).canonicalize()?;
    if source.is_absolute() || !path.starts_with(root) || !path.is_file() {
        return Err("terrain heightmap must be a contained local regular file".into());
    }
    Ok(path)
}

pub fn convert(source: &Path, output: &Path) -> Result<Manifest> {
    let source = source.canonicalize()?;
    let bytes = fs::read(&source)?;
    let specification: Specification = serde_json::from_slice(&bytes)?;
    let (grid, config, heightmap_sha256, physics) = match &specification {
        Specification::Voxels {
            config,
            voxels,
            physics,
        } => (
            terrain_voxels::build(voxels, config)?,
            config,
            None,
            physics,
        ),
        Specification::Heightmap {
            source: image,
            config,
            physics,
        } => {
            let image = image_source(&source, image)?;
            let hash = format!("{:x}", Sha256::digest(fs::read(&image)?));
            (
                terrain_heightmap::read(&image, config)?,
                &config.terrain,
                Some(hash),
                physics,
            )
        }
    };
    let encoded = terrain_grid::encode(&grid, &config.limits)?;
    let physics_bytes = physics
        .as_ref()
        .map(|limits| {
            let index = crate::terrain_physics::lazy_index(&grid, limits)?;
            crate::terrain_physics::encode(&index, limits)
        })
        .transpose()?;
    let manifest = Manifest {
        format: "roblox-smooth-grid-v1",
        file: "terrain.smoothgrid",
        sha256: format!("{:x}", Sha256::digest(&encoded)),
        source_sha256: format!("{:x}", Sha256::digest(&bytes)),
        heightmap_sha256,
        metres_per_stud: config.metres_per_stud,
        chunk_exponent: grid.chunk_exponent,
        chunks: grid.chunks.len(),
        cells: grid.chunks.iter().map(|chunk| chunk.cells.len()).sum(),
        physics_generated: physics_bytes.is_some(),
        physics_file: physics_bytes.as_ref().map(|_| "terrain.physicsgrid"),
        physics_sha256: physics_bytes
            .as_ref()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes))),
        physics_kind: physics_bytes.as_ref().map(|_| "native-lazy-spatial-index"),
        engine_verified: false,
    };
    fs::create_dir(output)?;
    let result = (|| -> Result<()> {
        fs::write(output.join(manifest.file), encoded)?;
        if let Some(bytes) = physics_bytes {
            fs::write(output.join("terrain.physicsgrid"), bytes)?;
        }
        fs::write(
            output.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        fs::remove_dir_all(output).map_err(|cleanup| {
            format!("terrain conversion failed: {error}; cleanup failed: {cleanup}")
        })?;
        return Err(error);
    }
    Ok(manifest)
}
