use crate::{Result, bundle, bundle_verify, plan_assets};
use clap::Subcommand;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub enum Command {
    /// Initialize an empty offline build plan; never overwrite.
    Init,
    List,
    /// Add a logical asset using a conversion JSON object. Source paths are plan-relative.
    Add {
        id: String,
        #[arg(long)]
        conversion: PathBuf,
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Replace an asset's conversion while retaining its logical ID.
    Edit {
        id: String,
        #[arg(long)]
        conversion: PathBuf,
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Remove an unreferenced plan entry, never source files or published assets.
    Remove {
        id: String,
        #[arg(long)]
        expected_revision: Option<String>,
    },
    Inspect {
        id: String,
    },
    /// Execute a real offline build into temporary output and verify it.
    Validate,
}

pub fn execute(path: &Path, command: Command) -> Result<Value> {
    let result = match command {
        Command::Init => {
            let plan = plan_assets::initialize(path)?;
            json!({"revision":plan_assets::revision(&plan)?,"plan":plan})
        }
        Command::List => {
            let plan = plan_assets::read(path)?;
            json!({"revision":plan_assets::revision(&plan)?,"assets":plan["assets"]})
        }
        Command::Add {
            id,
            conversion,
            expected_revision,
        } => put(path, id, conversion, false, expected_revision)?,
        Command::Edit {
            id,
            conversion,
            expected_revision,
        } => put(path, id, conversion, true, expected_revision)?,
        Command::Remove {
            id,
            expected_revision,
        } => {
            let plan = plan_assets::remove(path, &id, expected_revision.as_deref())?;
            json!({"removed":id,"sourceFilesDeleted":false,"revision":plan_assets::revision(&plan)?})
        }
        Command::Inspect { id } => {
            let plan = plan_assets::read(path)?;
            let entry = plan["assets"]
                .as_array()
                .ok_or("asset array required")?
                .iter()
                .find(|asset| asset["id"] == id)
                .ok_or_else(|| format!("unknown asset: {id}"))?;
            json!({"entry":entry,"revision":plan_assets::revision(&plan)?,"sourceValidated":false})
        }
        Command::Validate => {
            let temporary = tempfile::tempdir()?;
            let output = temporary.path().join("bundle");
            let manifest = bundle::build(path, &output)?;
            let verification = bundle_verify::verify(&output)?;
            json!({"artifacts":manifest.files.len(),"scenes":manifest.scenes.len(),"verification":verification,"temporaryOutput":true})
        }
    };
    Ok(json!({"ok":true,"scope":"offlineBuildPlan","published":false,"result":result}))
}

fn put(
    path: &Path,
    id: String,
    conversion: PathBuf,
    replace: bool,
    expected: Option<String>,
) -> Result<Value> {
    let conversion = serde_json::from_slice(&fs::read(conversion)?)?;
    let plan = plan_assets::put(path, &id, conversion, replace, expected.as_deref())?;
    Ok(json!({"id":id,"revision":plan_assets::revision(&plan)?,"sourceValidated":false}))
}
