use crate::{Asset, Result, Snapshot, config::Config, read_asset};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// Catalog registration is not evidence of a successful Studio import.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    pub source: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub format_version: u32,
    pub config: Config,
    pub assets: BTreeMap<String, Entry>,
}

impl Catalog {
    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1 {
            return Err("unsupported catalog format".into());
        }
        self.config.validate()?;
        for (id, entry) in &self.assets {
            if id.trim().is_empty()
                || entry.name.trim().is_empty()
                || entry.source.as_os_str().is_empty()
            {
                return Err("asset id, name and source must be nonempty".into());
            }
        }
        Ok(())
    }

    pub fn revision(&self) -> Result<String> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }

    pub fn asset(&self, path: &Path, id: &str) -> Result<Asset> {
        let entry = self
            .assets
            .get(id)
            .ok_or_else(|| format!("unknown asset: {id}"))?;
        let source = path.parent().unwrap_or(Path::new(".")).join(&entry.source);
        let mut asset =
            read_asset(&source, &self.config).map_err(|e| format!("asset {id}: {e}"))?;
        asset.id = id.to_owned();
        asset.name = entry.name.clone();
        // Display-name changes must reach Studio without changing stable identity.
        let revision = format!("{}:{}:{}", asset.revision, id, entry.name);
        asset.revision = format!("{:x}", Sha256::digest(revision));
        Ok(asset)
    }

    pub fn snapshot(&self, path: &Path) -> Result<Snapshot> {
        let assets = self
            .assets
            .keys()
            .map(|id| self.asset(path, id))
            .collect::<Result<Vec<_>>>()?;
        Ok(Snapshot {
            protocol: 2,
            config: self.config.clone(),
            assets,
        })
    }
}

pub fn read(path: &Path) -> Result<Catalog> {
    let catalog: Catalog = serde_json::from_slice(&fs::read(path)?)?;
    catalog.validate()?;
    Ok(catalog)
}

fn lock(path: &Path) -> Result<File> {
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(PathBuf::from(name))?;
    file.lock()?;
    Ok(file)
}

fn canonical_target(path: &Path) -> Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let target = parent
        .canonicalize()?
        .join(path.file_name().ok_or("catalog filename required")?);
    if fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("catalog must not be a symlink".into());
    }
    Ok(target)
}

fn save(path: &Path, catalog: &Catalog, create: bool) -> Result<()> {
    catalog.validate()?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("catalog parent required")?)?;
    temporary.write_all(&serde_json::to_vec_pretty(catalog)?)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    if create {
        temporary.persist_noclobber(path)?;
    } else {
        temporary.persist(path)?;
    }
    #[cfg(unix)]
    File::open(path.parent().ok_or("catalog parent required")?)?.sync_all()?;
    Ok(())
}

pub fn initialize(path: &Path, config: Config) -> Result<Catalog> {
    let path = canonical_target(path)?;
    let _lock = lock(&path)?;
    let catalog = Catalog {
        format_version: 1,
        config,
        assets: BTreeMap::new(),
    };
    save(&path, &catalog, true)?;
    Ok(catalog)
}

pub fn mutate(
    path: &Path,
    expected: Option<&str>,
    operation: impl FnOnce(&mut Catalog) -> Result<()>,
) -> Result<Catalog> {
    let path = canonical_target(path)?;
    let _lock = lock(&path)?;
    let mut catalog = read(&path)?;
    let current_revision = catalog.revision()?;
    if expected.is_some_and(|revision| current_revision != revision) {
        return Err("catalog changed; reload before applying".into());
    }
    operation(&mut catalog)?;
    save(&path, &catalog, false)?;
    Ok(catalog)
}

/// CLI paths are cwd-relative; stored paths are catalog-relative when possible.
pub fn source_path(catalog: &Path, source: &Path) -> Result<PathBuf> {
    let source = source.canonicalize()?;
    if !source.is_file() || source.extension().is_none_or(|e| e != "glb") {
        return Err("source must be a GLB file".into());
    }
    let parent = canonical_target(catalog)?
        .parent()
        .ok_or("catalog parent required")?
        .to_owned();
    Ok(source.strip_prefix(parent).unwrap_or(&source).to_owned())
}
