//! Compile-only Luau support. Never executes source and never uploads bytecode.
use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub optimization_level: u8,
    pub debug_level: u8,
    pub type_info_level: u8,
    pub coverage_level: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    pub format: &'static str,
    pub bytecode_version: u8,
    pub source_sha256: String,
    pub bytecode_sha256: String,
    pub bytes: usize,
    pub roblox_deployment_compatibility_verified: bool,
}

pub fn compile(source: &str, config: &Config) -> Result<Vec<u8>> {
    // These are documented compiler enum ranges, not resource policy limits.
    if config.optimization_level > 2
        || config.debug_level > 2
        || config.type_info_level > 1
        || config.coverage_level > 2
    {
        return Err("invalid Luau compiler level: optimization/debug/coverage accept 0..2, typeInfo accepts 0..1".into());
    }
    Ok(mlua::Compiler::new()
        .set_optimization_level(config.optimization_level)
        .set_debug_level(config.debug_level)
        .set_type_info_level(config.type_info_level)
        .set_coverage_level(config.coverage_level)
        .compile(source)?)
}

pub fn compile_file(source: &Path, output: &Path, config: &Config) -> Result<CompileResult> {
    let source = fs::read_to_string(source)?;
    let bytes = compile(&source, config)?;
    let result = CompileResult {
        format: "luau-bytecode",
        bytecode_version: *bytes.first().ok_or("compiler returned no bytecode")?,
        source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
        bytecode_sha256: format!("{:x}", Sha256::digest(&bytes)),
        bytes: bytes.len(),
        roblox_deployment_compatibility_verified: false,
    };
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(output)?;
    Ok(result)
}
