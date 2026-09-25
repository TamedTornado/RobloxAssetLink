use roblox_asset_link::scene::build;
use serde_json::json;
use std::fs;

fn specification() -> serde_json::Value {
    json!({"kind":"model","roots":[{
        "id":"model","class":"Model","name":"Kit","properties":{},"references":{"PrimaryPart":"part"},
        "children":[
            {"id":"part","class":"Part","name":"Block","properties":{"Anchored":{"Bool":true}},"references":{},"children":[]},
            {"id":"script","class":"ModuleScript","name":"Logic","properties":{},"references":{},"children":[],"scriptSource":"logic.luau"}
        ]
    }]})
}

#[test]
fn native_scene_preserves_hierarchy_scripts_references_and_repeatable_bytes() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("scene.json");
    fs::write(&source, serde_json::to_vec(&specification()).unwrap()).unwrap();
    fs::write(
        temporary.path().join("logic.luau"),
        "return {answer = 42}\n",
    )
    .unwrap();
    let first = temporary.path().join("first.rbxm");
    let second = temporary.path().join("second.rbxm");
    let result = build(&source, &first).unwrap();
    assert_eq!(result.instances, 3);
    assert!(!result.published);
    build(&source, &second).unwrap();
    let bytes = fs::read(&first).unwrap();
    assert_eq!(bytes, fs::read(&second).unwrap());
    let dom = rbx_binary::from_reader(bytes.as_slice()).unwrap();
    let model = dom.get_by_ref(dom.root().children()[0]).unwrap();
    assert_eq!(model.name, "Kit");
    assert_eq!(model.children().len(), 2);
    assert_eq!(
        model.properties.get(&"PrimaryPart".into()),
        Some(&rbx_dom_weak::types::Variant::Ref(model.children()[0]))
    );
    let module = dom.get_by_ref(model.children()[1]).unwrap();
    assert_eq!(
        module.properties.get(&"Source".into()),
        Some(&rbx_dom_weak::types::Variant::String(
            "return {answer = 42}\n".into()
        ))
    );
    assert!(build(&source, &first).is_err());
    assert_eq!(bytes, fs::read(&first).unwrap());
}

#[test]
fn malformed_scenes_fail_before_creating_output() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("scene.json");
    fs::write(temporary.path().join("logic.luau"), "return {}").unwrap();
    let out = temporary.path().join("out.rbxm");
    let mut missing = specification();
    missing["roots"][0]["references"]["PrimaryPart"] = "absent".into();
    let mut duplicate = specification();
    duplicate["roots"][0]["children"][1]["id"] = "part".into();
    let mut unknown = specification();
    unknown["roots"][0]["properties"]["NotAProperty"] = json!({"Bool":true});
    for value in [missing, duplicate, unknown] {
        fs::write(&source, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(build(&source, &out).is_err());
        assert!(!out.exists());
    }
}

#[test]
fn place_cli_builds_offline_and_script_paths_cannot_escape() {
    let temporary = tempfile::tempdir().unwrap();
    let source_root = temporary.path().join("source");
    fs::create_dir(&source_root).unwrap();
    let source = source_root.join("scene.json");
    let output = temporary.path().join("place.rbxl");
    fs::write(source_root.join("logic.luau"), "return {}").unwrap();
    let mut spec = specification();
    spec["kind"] = "place".into();
    spec["roots"][0]["class"] = "Workspace".into();
    spec["roots"][0]["references"] = json!({});
    fs::write(&source, serde_json::to_vec(&spec).unwrap()).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_roblox"))
        .env_clear()
        .args(["build", "scene"])
        .arg(&source)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["result"]["format"], "rbxl");
    let dom = rbx_binary::from_reader(fs::read(&output).unwrap().as_slice()).unwrap();
    assert_eq!(
        dom.get_by_ref(dom.root().children()[0])
            .unwrap()
            .class
            .as_str(),
        "Workspace"
    );

    fs::write(temporary.path().join("outside.luau"), "return 'outside'").unwrap();
    spec["roots"][0]["children"][1]["scriptSource"] = "../outside.luau".into();
    fs::write(&source, serde_json::to_vec(&spec).unwrap()).unwrap();
    let rejected = temporary.path().join("rejected.rbxl");
    assert!(
        build(&source, &rejected)
            .err()
            .unwrap()
            .to_string()
            .contains("source directory")
    );
    assert!(!rejected.exists());
}
