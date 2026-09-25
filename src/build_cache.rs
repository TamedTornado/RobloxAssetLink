//! Local immutable conversion cache. Entries contain data, never executable code.
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub directory: PathBuf,
}

pub(crate) struct Cache {
    root: PathBuf,
    pub toolchain: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    key: String,
    files: BTreeMap<String, String>,
    metadata: Value,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: Value,
    sha256: String,
}

pub(crate) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn file_hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut state = Sha256::new();
    let mut chunk = [0; 64 * 1024]; // I/O chunk, not a resource limit.
    loop {
        let length = file.read(&mut chunk)?;
        if length == 0 {
            break;
        }
        state.update(&chunk[..length]);
    }
    Ok(format!("{:x}", state.finalize()))
}

fn inventory(root: &Path, directory: &Path, files: &mut BTreeMap<String, String>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            inventory(root, &path, files)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("non-UTF8 cache path")?
                .to_owned();
            files.insert(relative, file_hash(&path)?);
        } else {
            return Err(
                "cache files must be regular files/directories, not symlinks or devices".into(),
            );
        }
    }
    Ok(())
}

// All native libraries mapped into this process contribute their actual bytes,
// not just version strings. That includes transitive FFmpeg/libvpx dependencies.
#[cfg(target_os = "linux")]
fn toolchain() -> Result<String> {
    let mut paths = BTreeSet::new();
    paths.insert(std::env::current_exe()?.canonicalize()?);
    for line in fs::read_to_string("/proc/self/maps")?.lines() {
        let Some(start) = line.find('/') else {
            continue;
        };
        let path = &line[start..];
        if path.ends_with(" (deleted)") || path.contains("\\") {
            return Err("cannot fingerprint deleted or escaped mapped library paths".into());
        }
        paths.insert(PathBuf::from(path).canonicalize()?);
    }
    let hashes: Vec<_> = paths
        .iter()
        .map(|path| file_hash(path))
        .collect::<Result<_>>()?;
    digest(&hashes)
}

#[cfg(not(target_os = "linux"))]
fn toolchain() -> Result<String> {
    Err("conversion caching currently requires Linux native-library fingerprinting; uncached builds remain available".into())
}

impl Cache {
    pub fn open(root: &Path, output: &Path) -> Result<Self> {
        // Initialize codecs before inventory so their mapped dependencies are present.
        ffmpeg_next::init()?;
        let toolchain = toolchain()?;
        fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let output = output.canonicalize()?;
        if root.starts_with(&output) || output.starts_with(&root) {
            return Err("cache and bundle output directories must not overlap".into());
        }
        Ok(Self { root, toolchain })
    }

    fn entry(&self, key: &str) -> Result<PathBuf> {
        if key.len() != 64 || !key.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("invalid cache key".into());
        }
        Ok(self.root.join(key))
    }

    fn validate(&self, key: &str) -> Result<Option<Record>> {
        let entry = self.entry(key)?;
        match fs::symlink_metadata(&entry) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err("cache entry is not a regular directory".into());
            }
            Ok(_) => {}
        }
        if !fs::symlink_metadata(entry.join("record.json"))?
            .file_type()
            .is_file()
            || !fs::symlink_metadata(entry.join("files"))?
                .file_type()
                .is_dir()
        {
            return Err("cache record/files must be a regular file and directory".into());
        }
        let envelope: Envelope = serde_json::from_slice(&fs::read(entry.join("record.json"))?)?;
        if digest(&envelope.record)? != envelope.sha256 {
            return Err("cache metadata checksum mismatch".into());
        }
        let record: Record = serde_json::from_value(envelope.record)?;
        if record.version != 1 || record.key != key {
            return Err("cache record version/key mismatch".into());
        }
        let mut actual = BTreeMap::new();
        inventory(&entry.join("files"), &entry.join("files"), &mut actual)?;
        if actual != record.files {
            return Err("cache output inventory/hash mismatch".into());
        }
        Ok(Some(record))
    }

    pub fn restore(&self, key: &str, output: &Path) -> Result<Option<Value>> {
        let Some(record) = self.validate(key)? else {
            return Ok(None);
        };
        fs::create_dir(output)?;
        let result = copy_files(&self.entry(key)?.join("files"), output, &record.files);
        if let Err(error) = result {
            fs::remove_dir_all(output).map_err(|cleanup| {
                format!("cache restore failed: {error}; cleanup failed: {cleanup}")
            })?;
            return Err(error);
        }
        Ok(Some(record.metadata))
    }

    pub fn store(&self, key: &str, source: &Path, metadata: Value) -> Result<()> {
        let mut files = BTreeMap::new();
        inventory(source, source, &mut files)?;
        if let Some(existing) = self.validate(key)? {
            if existing.files != files || existing.metadata != metadata {
                return Err("same conversion cache key produced different outputs".into());
            }
            return Ok(());
        }
        let temporary = tempfile::tempdir_in(&self.root)?;
        let destination = temporary.path().join("files");
        fs::create_dir(&destination)?;
        copy_files(source, &destination, &files)?;
        let record = serde_json::to_value(Record {
            version: 1,
            key: key.into(),
            files,
            metadata,
        })?;
        let envelope = Envelope {
            sha256: digest(&record)?,
            record,
        };
        fs::write(
            temporary.path().join("record.json"),
            serde_json::to_vec(&envelope)?,
        )?;
        match fs::rename(temporary.path(), self.entry(key)?) {
            Ok(()) => Ok(()),
            Err(error) => {
                // Another builder may have published the identical immutable entry.
                if let Some(existing) = self.validate(key)?
                    && serde_json::to_value(existing)? == envelope.record
                {
                    return Ok(());
                }
                Err(error.into())
            }
        }
    }
}

fn copy_files(source: &Path, output: &Path, files: &BTreeMap<String, String>) -> Result<()> {
    for (name, expected) in files {
        let target = output.join(name);
        fs::create_dir_all(target.parent().ok_or("missing cache output parent")?)?;
        fs::copy(source.join(name), &target)?;
        if file_hash(&target)? != *expected {
            return Err("cache file changed while being copied".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(root: &Path) -> (Cache, PathBuf, String) {
        let source = root.join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("nested/artifact"), b"native bytes").unwrap();
        fs::write(source.join("manifest.json"), b"conversion metadata").unwrap();
        let directory = root.join("cache");
        fs::create_dir(&directory).unwrap();
        let cache = Cache {
            root: directory,
            toolchain: "test executable".into(),
        };
        let key = digest(&"test input/configuration").unwrap();
        (cache, source, key)
    }

    #[test]
    fn complete_sidecar_inventory_and_metadata_are_verified() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let (cache, source, key) = fixture(root);
        let metadata = json!({"rig":{"matrix":[0.10000000149011612, -0.00000000000001341]},"files":["nested/artifact"]});
        cache.store(&key, &source, metadata.clone()).unwrap();
        assert_eq!(
            cache.restore(&key, &root.join("output")).unwrap(),
            Some(metadata)
        );
        let entry = cache.entry(&key).unwrap();
        let path = entry.join("record.json");
        let original = fs::read(&path).unwrap();
        let mut envelope: Value = serde_json::from_slice(&original).unwrap();
        envelope["record"]["metadata"]["rig"]["matrix"][0] = json!(42);
        fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(
            cache
                .restore(&key, &root.join("bad-metadata"))
                .unwrap_err()
                .to_string()
                .contains("metadata checksum")
        );
        assert!(!root.join("bad-metadata").exists());
        fs::write(&path, original).unwrap();
        fs::write(entry.join("files/manifest.json"), b"changed sidecar").unwrap();
        assert!(
            cache
                .restore(&key, &root.join("bad-sidecar"))
                .unwrap_err()
                .to_string()
                .contains("inventory/hash")
        );
    }

    #[test]
    fn concurrent_publication_is_atomic_and_rejects_inconsistent_results() {
        let temporary = tempfile::tempdir().unwrap();
        let (cache, source, key) = fixture(temporary.path());
        std::thread::scope(|scope| {
            let first = scope.spawn(|| cache.store(&key, &source, json!({"kind":"mesh"})));
            let second = scope.spawn(|| cache.store(&key, &source, json!({"kind":"mesh"})));
            first.join().unwrap().unwrap();
            second.join().unwrap().unwrap();
        });
        assert_eq!(fs::read_dir(&cache.root).unwrap().count(), 1);
        assert!(
            cache
                .store(&key, &source, json!({"kind":"different"}))
                .unwrap_err()
                .to_string()
                .contains("different outputs")
        );
        assert!(cache.entry("../outside").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_cache_payloads() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let (cache, source, key) = fixture(root);
        cache.store(&key, &source, json!({})).unwrap();
        let file = cache.entry(&key).unwrap().join("files/nested/artifact");
        fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(source.join("nested/artifact"), &file).unwrap();
        assert!(
            cache
                .restore(&key, &root.join("output"))
                .unwrap_err()
                .to_string()
                .contains("symlinks")
        );
        assert!(!root.join("output").exists());
    }
}
