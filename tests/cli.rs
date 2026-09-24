use serde_json::Value;
use std::{path::Path, process::Command};

fn run(path: &Path, args: &[&str], success: bool) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_roblox"))
        .args(["assets", "--catalog"])
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(if success {
        &output.stdout
    } else {
        &output.stderr
    })
    .unwrap()
}

#[test]
fn complete_cli_lifecycle_preserves_sources_and_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("assets.json");
    run(
        &path,
        &["init", "--config", "examples/building-kit.json"],
        true,
    );
    run(
        &path,
        &["init", "--config", "examples/building-kit.json"],
        false,
    );
    let output = run(
        &path,
        &["add", "tests/fixtures/doorway.glb", "--id", "door"],
        true,
    );
    assert_eq!(output["studioApplied"], false);
    let old = run(&path, &["inspect", "door"], true);
    run(&path, &["edit", "door", "--name", "Entrance"], true);
    let renamed = run(&path, &["inspect", "door"], true);
    assert_eq!(renamed["result"]["id"], "door");
    assert_ne!(
        renamed["result"]["sourceRevision"],
        old["result"]["sourceRevision"]
    );
    run(
        &path,
        &["edit", "door", "--source", "tests/fixtures/glass.glb"],
        true,
    );
    let bytes = std::fs::read(&path).unwrap();
    run(
        &path,
        &["add", "tests/fixtures/doorway.glb", "--id", "door"],
        false,
    );
    run(&path, &["edit", "door", "--source", "missing.glb"], false);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(run(&path, &["validate"], true)["result"]["validated"], 1);
    run(&path, &["remove", "door"], true);
    assert!(Path::new("tests/fixtures/doorway.glb").exists());
    assert!(Path::new("tests/fixtures/glass.glb").exists());
    assert_eq!(
        run(&path, &["list"], true)["result"]["assets"],
        serde_json::json!({})
    );
    run(&path, &["remove", "door"], false);
}

#[test]
fn concurrent_cli_writers_do_not_lose_registrations() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("assets.json");
    run(
        &path,
        &["init", "--config", "examples/building-kit.json"],
        true,
    );
    let handles: Vec<_> = (0..8)
        .map(|index| {
            let path = path.clone();
            std::thread::spawn(move || {
                run(
                    &path,
                    &[
                        "add",
                        "tests/fixtures/glass.glb",
                        "--id",
                        &format!("glass-{index}"),
                    ],
                    true,
                )
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(
        run(&path, &["list"], true)["result"]["assets"]
            .as_object()
            .unwrap()
            .len(),
        8
    );
}

#[test]
fn unregistered_files_are_not_implicitly_imported_and_missing_sources_can_be_removed() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("assets.json");
    let source = temporary.path().join("copy.glb");
    std::fs::copy("tests/fixtures/glass.glb", &source).unwrap();
    run(
        &path,
        &["init", "--config", "examples/building-kit.json"],
        true,
    );
    assert_eq!(run(&path, &["validate"], true)["result"]["validated"], 0);
    run(
        &path,
        &["add", source.to_str().unwrap(), "--id", "copy"],
        true,
    );
    let catalog = roblox_asset_link::catalog::read(&path).unwrap();
    assert_eq!(catalog.assets["copy"].source, Path::new("copy.glb"));
    std::fs::remove_file(source).unwrap();
    run(&path, &["validate"], false);
    run(&path, &["remove", "copy"], true);
}

#[test]
fn capabilities_and_argument_errors_are_machine_readable() {
    let output = Command::new(env!("CARGO_BIN_EXE_roblox"))
        .arg("capabilities")
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["persistentStudioImport"], false);

    for arguments in [vec!["assets", "list"], vec!["assets", "not-a-command"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_roblox"))
            .args(arguments)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["error"]["message"].as_str().is_some());
    }
}
