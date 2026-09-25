use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn run(plan: &Path, args: &[&str], success: bool) -> Value {
    let result = Command::new(env!("CARGO_BIN_EXE_roblox"))
        .env_clear()
        .args(["assets", "--plan"])
        .arg(plan)
        .args(args)
        .output()
        .unwrap();
    assert_eq!(
        result.status.success(),
        success,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(if success {
        &result.stdout
    } else {
        &result.stderr
    })
    .unwrap()
}

fn setup(root: &Path) -> std::path::PathBuf {
    image::GrayImage::from_pixel(1, 1, image::Luma([128]))
        .save(root.join("source.png"))
        .unwrap();
    let recipe = root.join("conversion.json");
    fs::write(
        &recipe,
        json!({"kind":"texture","source":"source.png","config":{
        "operation":"roughness","maxWidth":1,"maxHeight":1,"maxDecodedBytes":4096}})
        .to_string(),
    )
    .unwrap();
    recipe
}

#[test]
fn cli_edits_the_real_build_plan_and_validates_native_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let plan = root.join("build.json");
    let recipe = setup(root);
    let recipe = recipe.to_str().unwrap();
    let initial = run(&plan, &["init"], true);
    assert_eq!(initial["scope"], "offlineBuildPlan");
    run(&plan, &["init"], false);
    run(&plan, &["add", "paint", "--conversion", recipe], true);
    assert_eq!(
        run(&plan, &["list"], true)["result"]["assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        run(&plan, &["inspect", "paint"], true)["result"]["sourceValidated"],
        false
    );
    run(
        &plan,
        &[
            "edit",
            "paint",
            "--conversion",
            recipe,
            "--expected-revision",
            initial["result"]["revision"].as_str().unwrap(),
        ],
        false,
    );
    run(&plan, &["edit", "paint", "--conversion", recipe], true);
    let validation = run(&plan, &["validate"], true);
    assert_eq!(validation["result"]["verification"]["artifactsVerified"], 1);
    assert_eq!(
        validation["result"]["verification"]["engineVerified"],
        false
    );
    fs::remove_file(root.join("source.png")).unwrap();
    run(&plan, &["validate"], false);
    run(&plan, &["remove", "paint"], true);
    assert!(root.join("conversion.json").exists());
}

#[test]
fn concurrent_cli_writers_preserve_all_plan_entries() {
    let temp = tempfile::tempdir().unwrap();
    let plan = temp.path().join("build.json");
    let recipe = setup(temp.path());
    run(&plan, &["init"], true);
    let mut children = Vec::new();
    for index in 0..6 {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_roblox"))
                .env_clear()
                .args(["assets", "--plan"])
                .arg(&plan)
                .arg("add")
                .arg(format!("paint-{index}"))
                .arg("--conversion")
                .arg(&recipe)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    assert_eq!(
        run(&plan, &["list"], true)["result"]["assets"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[test]
fn retired_catalog_and_transport_commands_are_rejected_without_fallback() {
    for args in [
        vec!["assets", "--catalog", "old.json", "list"],
        vec!["serve"],
        vec!["assets", "list"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_roblox"))
            .env_clear()
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["ok"], false);
    }
    let output = Command::new(env!("CARGO_BIN_EXE_roblox"))
        .arg("capabilities")
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["assetDocument"], "offlineBuildPlan");
    assert!(value["result"].get("serverExecutable").is_none());
}
