use clap::{Parser, Subcommand};
use roblox_asset_link::{
    Result,
    catalog::{self, Entry},
    config,
};
use serde_json::{Value, json};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "roblox",
    about = "CLI-first Roblox project automation",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert assets locally. No catalog, Studio, credentials or network.
    Convert {
        #[command(subcommand)]
        command: Convert,
    },
    /// Report implemented capabilities without requiring Studio or a catalog.
    Capabilities,
    /// Manage explicit asset registrations. These commands do not claim Studio import.
    Assets {
        #[arg(long, global = true)]
        catalog: Option<PathBuf>,
        #[command(subcommand)]
        command: Assets,
    },
}

#[derive(Subcommand)]
enum Convert {
    /// Encode static GLB, FBX or OBJ geometry as native Roblox meshes.
    Mesh {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Subcommand)]
enum Assets {
    /// Create a catalog using a JSON project configuration; never overwrite.
    Init {
        #[arg(long)]
        config: PathBuf,
    },
    List,
    /// Register a GLB source after validating it; leaves the source untouched.
    Add {
        source: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Edit metadata or replace a source while retaining stable asset identity.
    Edit {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        source: Option<PathBuf>,
    },
    /// Remove only the catalog entry, not source files or published Roblox assets.
    Remove {
        id: String,
    },
    Inspect {
        id: String,
    },
    Validate,
    /// Read configuration or replace it with a validated JSON file.
    Config {
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

fn execute(cli: Cli) -> Result<Value> {
    if let Command::Convert {
        command:
            Convert::Mesh {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        let manifest = roblox_asset_link::convert::convert(source, output, &config)?;
        return Ok(json!({"ok":true,"scope":"offlineConversion","result":manifest}));
    }
    let Command::Assets {
        catalog: path,
        command,
    } = cli.command
    else {
        return Ok(json!({"ok":true,"result":{
            "commandGroups":["assets","convert"],"assetOperations":["init","add","edit","remove","list","inspect","validate","config"],
            "offlineConversion":["static-glb-to-mesh-v2","static-fbx-to-mesh-v2","static-obj-to-mesh-v2"],"offlineGameBuild":false,
            "serverExecutable":"roblox-server","requiresStudioForCatalog":false,
            "persistentStudioImport":false,"studioCommandExecution":false
        }}));
    };
    let path = path.ok_or("--catalog is required for asset operations")?;
    let result = match command {
        Assets::Init { config: file } => {
            let catalog = catalog::initialize(&path, config::read(&file)?)?;
            json!({"revision": catalog.revision()?, "catalog": catalog})
        }
        Assets::List => {
            let catalog = catalog::read(&path)?;
            json!({"revision": catalog.revision()?, "assets": catalog.assets})
        }
        Assets::Add { source, id, name } => {
            let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let name = name
                .or_else(|| {
                    source
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(str::to_owned)
                })
                .ok_or("provide a display name with --name")?;
            let source = catalog::source_path(&path, &source)?;
            let catalog = catalog::mutate(&path, None, |catalog| {
                if catalog.assets.contains_key(&id) {
                    return Err(format!("asset already exists: {id}").into());
                }
                catalog.assets.insert(id.clone(), Entry { name, source });
                catalog.asset(&path, &id)?;
                Ok(())
            })?;
            json!({"id":id, "asset":catalog.assets[&id], "revision":catalog.revision()?})
        }
        Assets::Edit { id, name, source } => {
            if name.is_none() && source.is_none() {
                return Err("edit requires --name or --source".into());
            }
            let source = source
                .map(|p| catalog::source_path(&path, &p))
                .transpose()?;
            let catalog = catalog::mutate(&path, None, |catalog| {
                let entry = catalog
                    .assets
                    .get_mut(&id)
                    .ok_or_else(|| format!("unknown asset: {id}"))?;
                if let Some(name) = name {
                    entry.name = name;
                }
                if let Some(source) = source {
                    entry.source = source;
                }
                catalog.asset(&path, &id)?;
                Ok(())
            })?;
            json!({"id":id,"asset":catalog.assets[&id],"revision":catalog.revision()?})
        }
        Assets::Remove { id } => {
            let catalog = catalog::mutate(&path, None, |catalog| {
                catalog
                    .assets
                    .remove(&id)
                    .ok_or_else(|| format!("unknown asset: {id}"))?;
                Ok(())
            })?;
            json!({"removed":id,"sourceFilesDeleted":false,"revision":catalog.revision()?})
        }
        Assets::Inspect { id } => {
            let catalog = catalog::read(&path)?;
            let asset = catalog.asset(&path, &id)?;
            let triangles: usize = asset.meshes.iter().map(|m| m.triangles.len()).sum();
            json!({"id":id,"entry":catalog.assets[&id],"sourceRevision":asset.revision,
                "meshCount":asset.meshes.len(),"triangleCount":triangles})
        }
        Assets::Validate => {
            let catalog = catalog::read(&path)?;
            let snapshot = catalog.snapshot(&path)?;
            json!({"validated":snapshot.assets.len(),"revision":catalog.revision()?})
        }
        Assets::Config { file } => {
            let catalog = if let Some(file) = file {
                let config = config::read(&file)?;
                catalog::mutate(&path, None, |catalog| {
                    if catalog.config.project_id != config.project_id {
                        return Err("projectId cannot change".into());
                    }
                    catalog.config = config;
                    catalog.snapshot(&path)?;
                    Ok(())
                })?
            } else {
                catalog::read(&path)?
            };
            json!({"config":catalog.config,"revision":catalog.revision()?})
        }
    };
    Ok(json!({"ok":true,"scope":"assetCatalog","studioApplied":false,"result":result}))
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if !error.use_stderr() {
                print!("{error}");
                return ExitCode::SUCCESS;
            }
            eprintln!(
                "{}",
                json!({"ok":false,"error":{"code":"invalid_arguments","message":error.to_string()}})
            );
            return ExitCode::FAILURE;
        }
    };
    match execute(cli) {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "{}",
                json!({"ok":false,"error":{"code":"operation_failed","message":error.to_string()}})
            );
            ExitCode::FAILURE
        }
    }
}
