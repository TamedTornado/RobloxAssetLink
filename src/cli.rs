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
    /// Verify a local bundle's declared runtime hashes and native references.
    VerifyBundle { directory: PathBuf },
    /// Compile Luau locally without executing source.
    Compile {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Assemble native scene artifacts offline from explicitly typed JSON.
    Build {
        #[command(subcommand)]
        command: Build,
    },
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
enum Build {
    /// Mux preconverted local VP9 video and Vorbis/Opus audio into WebM.
    Media {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Convert assets and assemble locally linked scenes from a JSON plan.
    Bundle {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Scene {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Subcommand)]
enum Convert {
    /// Transcode a local video/audio source to combined VP9/Vorbis WebM.
    Media {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Convert a supported local silent video to VP9/WebM in process.
    Video {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Extract a core glTF PBR material and bake factors into native texture maps.
    MaterialGltf {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Build a native SurfaceAppearance and local texture maps from material JSON.
    Material {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Bake FBX motion locally using explicit sampling configuration.
    AnimationFbx {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Import rigid-bind GLB/glTF or linear FBX skins as native v4.01 meshes.
    Skin {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Convert rigid LINEAR glTF/GLB skeletal animation locally.
    AnimationGltf {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Encode canonical rest-relative animation JSON as a native KeyframeSequence.
    Animation {
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Decode audio and encode Ogg Vorbis locally, preserving sample rate/channels.
    Audio {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Normalize images and PBR maps locally to PNG artifacts.
    Texture {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Encode static GLB/glTF, FBX or OBJ geometry as native Roblox meshes.
    Mesh {
        source: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Optional local collision recipe; payload engine acceptance is unverified.
        #[arg(long)]
        collision_config: Option<PathBuf>,
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
            Convert::AnimationFbx {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        return Ok(
            json!({"ok":true,"scope":"offlineConversion","result":roblox_asset_link::animation_fbx::convert(source,output,config)?}),
        );
    }
    if let Command::Convert {
        command:
            Convert::Skin {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        return Ok(
            json!({"ok":true,"scope":"offlineConversion","result":roblox_asset_link::skin_import::convert(source,output,&config)?}),
        );
    }
    if let Command::Convert {
        command:
            Convert::AnimationGltf {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        return Ok(
            json!({"ok":true,"scope":"offlineConversion","result":roblox_asset_link::animation_gltf::convert(source,output,config)?}),
        );
    }
    if let Command::Convert {
        command: Convert::Animation { source, output },
    } = &cli.command
    {
        return Ok(
            json!({"ok":true,"scope":"offlineConversion","result":roblox_asset_link::animation::convert(source,output)?}),
        );
    }
    if let Command::Build {
        command: Build::Media { source, output },
    } = &cli.command
    {
        return Ok(
            json!({"ok":true,"scope":"offlineMediaBuild","result":roblox_asset_link::media_mux::build(source,output)?}),
        );
    }
    if let Command::Build {
        command: Build::Bundle { source, output },
    } = &cli.command
    {
        return Ok(
            json!({"ok":true,"scope":"offlineBundleBuild","result":roblox_asset_link::bundle::build(source,output)?}),
        );
    }
    if let Command::Convert {
        command:
            Convert::Audio {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        return Ok(
            json!({"ok":true,"scope":"offlineConversion","result":roblox_asset_link::audio::convert(source,output,&config)?}),
        );
    }
    if let Command::Compile {
        source,
        config,
        output,
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        return Ok(
            json!({"ok":true,"scope":"offlineCompilation","result":roblox_asset_link::scripts::compile_file(source,output,&config)?}),
        );
    }
    if let Command::Build {
        command: Build::Scene { source, output },
    } = &cli.command
    {
        return Ok(
            json!({"ok":true,"scope":"offlineSceneBuild","result":roblox_asset_link::scene::build(source,output)?}),
        );
    }
    if let Command::Convert {
        command:
            Convert::Texture {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        let manifest = roblox_asset_link::texture::convert(source, output, &config)?;
        return Ok(json!({"ok":true,"scope":"offlineConversion","result":manifest}));
    }
    if let Command::Convert {
        command: Convert::Material { source, output },
    } = &cli.command
    {
        let manifest = roblox_asset_link::material::convert(source, output)?;
        return Ok(json!({"ok":true,"scope":"offlineConversion","result":manifest}));
    }
    if let Command::Convert {
        command:
            Convert::Mesh {
                source,
                config,
                output,
                collision_config,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        let collision = collision_config
            .as_ref()
            .map(|path| -> Result<_> {
                Ok(serde_json::from_slice::<
                    roblox_asset_link::collision::Recipe,
                >(&std::fs::read(path)?)?)
            })
            .transpose()?;
        let manifest = roblox_asset_link::convert::convert_with_collision(
            source,
            output,
            &config,
            collision.as_ref(),
        )?;
        return Ok(json!({"ok":true,"scope":"offlineConversion","result":manifest}));
    }
    if let Command::Convert {
        command:
            Convert::MaterialGltf {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        let manifest = roblox_asset_link::material_gltf::convert(source, output, &config)?;
        return Ok(json!({"ok":true,"scope":"offlineConversion","result":manifest}));
    }
    if let Command::Convert {
        command:
            Convert::Video {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        let manifest = roblox_asset_link::video::convert(source, output, &config)?;
        return Ok(json!({"ok":true,"scope":"offlineConversion","result":manifest}));
    }
    if let Command::Convert {
        command:
            Convert::Media {
                source,
                config,
                output,
            },
    } = &cli.command
    {
        let config = serde_json::from_slice(&std::fs::read(config)?)?;
        let manifest = roblox_asset_link::media_transcode::convert(source, output, &config)?;
        return Ok(json!({"ok":true,"scope":"offlineConversion","result":manifest}));
    }
    if let Command::VerifyBundle { directory } = &cli.command {
        let result = roblox_asset_link::bundle_verify::verify(directory)?;
        return Ok(json!({"ok":true,"scope":"offlineBundleVerification","result":result}));
    }
    let Command::Assets {
        catalog: path,
        command,
    } = cli.command
    else {
        return Ok(json!({"ok":true,"result":{
            "commandGroups":["assets","convert","build","compile"],"assetOperations":["init","add","edit","remove","list","inspect","validate","config"],
            "localLuauCompilation":true,
            "offlineSceneSerialization":true,
            "offlineAssetBundleBuild":true,
            "offlineConversion":[
                "static-gltf-to-mesh-v2","static-fbx-to-mesh-v2","static-obj-to-mesh-v2",
                "textures-to-png","material-to-surface-appearance","gltf-material-to-surface-appearance",
                "collision-to-csgphs-v5","audio-to-ogg-vorbis","silent-video-to-webm-vp9",
                "combined-source-to-webm-vp9-vorbis","preconverted-media-to-webm",
                "canonical-animation-to-rbxm","rigid-linear-gltf-animation-to-rbxm",
                "sampled-fbx-animation-to-rbxm","rigid-bind-gltf-to-skinned-mesh-v4",
                "linear-fbx-to-skinned-mesh-v4"
            ],"offlineGameBuild":false,
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
