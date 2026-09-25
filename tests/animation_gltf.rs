use roblox_asset_link::animation_gltf::{self, Config};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn config() -> Config {
    Config {
        animation_index: 0,
        root_node: 0,
        metres_per_stud: 0.5,
        name: "Motion".into(),
        looped: true,
        priority: "Action".into(),
    }
}

fn fixture(root: &Path) -> Value {
    // Independently authored metric glTF: a quarter-turned root moves along
    // world Y while its child rotates 180 degrees over two seconds.
    let values: &[f32] = &[
        0., 1., 2., 1., 2., 0., 1., 3., 0., 1., 4., 0., 0., 2., 0., 0., 0., 1., 0., 0., 1., 0.,
    ];
    let bytes: Vec<_> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    fs::write(root.join("motion.bin"), &bytes).unwrap();
    let half = std::f32::consts::FRAC_1_SQRT_2;
    json!({
        "asset":{"version":"2.0"},
        "nodes":[
            {"name":"Root","translation":[1.,2.,0.],"rotation":[0.,0.,half,half],"children":[1]},
            {"name":"Child","translation":[0.,1.,0.]}
        ],
        "buffers":[{"uri":"motion.bin","byteLength":bytes.len()}],
        "bufferViews":[
            {"buffer":0,"byteOffset":0,"byteLength":12},
            {"buffer":0,"byteOffset":12,"byteLength":36},
            {"buffer":0,"byteOffset":48,"byteLength":8},
            {"buffer":0,"byteOffset":56,"byteLength":32}
        ],
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"SCALAR","min":[0],"max":[2]},
            {"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":2,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[2]},
            {"bufferView":3,"componentType":5126,"count":2,"type":"VEC4"}
        ],
        "animations":[{"samplers":[{"input":0,"output":1},{"input":2,"output":3}],
            "channels":[{"sampler":0,"target":{"node":0,"path":"translation"}},
                        {"sampler":1,"target":{"node":1,"path":"rotation"}}]}]
    })
}

fn near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.00001,
        "{actual} != {expected}"
    );
}

#[test]
fn source_sampling_converts_rest_space_units_hierarchy_and_slerp() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("motion.gltf");
    fs::write(
        &source,
        serde_json::to_vec(&fixture(directory.path())).unwrap(),
    )
    .unwrap();
    let (clip, rig) = animation_gltf::read(&source, &config()).unwrap();
    assert_eq!(
        clip.frames.iter().map(|f| f.time).collect::<Vec<_>>(),
        [0., 1., 2.]
    );
    assert_eq!(rig[1].parent_node, Some(0));
    near(rig[1].rest_cframe[10], 2.);
    let start = &clip.frames[0].poses[0];
    near(start.cframe[0], 1.);
    near(start.cframe[9], 0.);
    let middle = &clip.frames[1].poses[0];
    near(middle.cframe[9], 2.);
    near(middle.cframe[10], 0.);
    let child = &middle.children[0];
    near(child.cframe[0], 0.);
    near(child.cframe[1], -1.);
    near(child.cframe[3], 1.);
    assert_eq!(child.name, "Child");
    near(child.cframe[10], 0.);

    let output = directory.path().join("motion.rbxm");
    let result = animation_gltf::convert(&source, &output, config()).unwrap();
    assert_eq!(result.artifact.keyframes, 3);
    let bytes = fs::read(&output).unwrap();
    let dom = rbx_binary::from_reader(bytes.as_slice()).unwrap();
    let sequence = dom.get_by_ref(dom.root().children()[0]).unwrap();
    assert_eq!(sequence.name, "Motion");
    assert_eq!(sequence.children().len(), 3);
    let frame = dom.get_by_ref(sequence.children()[1]).unwrap();
    let root_pose = dom.get_by_ref(frame.children()[0]).unwrap();
    let rbx_dom_weak::types::Variant::CFrame(value) = &root_pose.properties[&"CFrame".into()]
    else {
        panic!("missing pose");
    };
    near(value.position.x, 2.);
    let child_pose = dom.get_by_ref(root_pose.children()[0]).unwrap();
    let rbx_dom_weak::types::Variant::CFrame(child_transform) =
        &child_pose.properties[&"CFrame".into()]
    else {
        panic!("missing child rotation");
    };
    near(child_transform.orientation.x.y, -1.);
    near(child_transform.orientation.y.x, 1.);
    assert_eq!(bytes, roblox_asset_link::animation::encode(&clip).unwrap());
}

#[test]
fn glb_matches_external_gltf_and_rejects_corrupt_timestamps() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut document = fixture(root);
    let source = root.join("motion.gltf");
    fs::write(&source, serde_json::to_vec(&document).unwrap()).unwrap();
    let (expected, _) = animation_gltf::read(&source, &config()).unwrap();
    let binary = fs::read(root.join("motion.bin")).unwrap();
    document["buffers"][0]
        .as_object_mut()
        .unwrap()
        .remove("uri");
    let mut json = serde_json::to_vec(&document).unwrap();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let length = u32::try_from(12 + 8 + json.len() + 8 + binary.len()).unwrap();
    let mut glb = Vec::new();
    for word in [0x46546c67u32, 2, length, json.len() as u32, 0x4e4f534a] {
        glb.extend(word.to_le_bytes());
    }
    glb.extend(json);
    glb.extend((binary.len() as u32).to_le_bytes());
    glb.extend(0x004e4942u32.to_le_bytes());
    glb.extend(binary);
    let source_glb = root.join("motion.glb");
    fs::write(&source_glb, glb).unwrap();
    let (actual, _) = animation_gltf::read(&source_glb, &config()).unwrap();
    assert_eq!(
        roblox_asset_link::animation::encode(&actual).unwrap(),
        roblox_asset_link::animation::encode(&expected).unwrap()
    );

    let mut bad = fs::read(root.join("motion.bin")).unwrap();
    bad[4..8].copy_from_slice(&0f32.to_le_bytes());
    fs::write(root.join("motion.bin"), bad).unwrap();
    let error = animation_gltf::read(&source, &config())
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("strictly increasing"), "{error}");
}

#[test]
fn unsupported_and_malformed_source_semantics_fail_explicitly() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("motion.gltf");
    let base = fixture(directory.path());
    let mut step = base.clone();
    step["animations"][0]["samplers"][0]["interpolation"] = "STEP".into();
    let mut scale = base.clone();
    scale["nodes"][0]["scale"] = json!([2., 1., 1.]);
    let mut duplicate = base.clone();
    duplicate["nodes"][1]["name"] = "Root".into();
    let mut remote = base.clone();
    remote["buffers"][0]["uri"] = "https://example.invalid/motion.bin".into();
    let mut scaled_animation = base.clone();
    scaled_animation["animations"][0]["channels"][0]["target"]["path"] = "scale".into();
    let mut zero_rotation = base.clone();
    zero_rotation["nodes"][0]["rotation"] = json!([0., 0., 0., 0.]);
    for (value, expected) in [
        (step, "LINEAR"),
        (scale, "unscaled"),
        (duplicate, "uniquely named"),
        (remote, "network access"),
        (scaled_animation, "scale and morph"),
        (zero_rotation, "unit quaternions"),
    ] {
        fs::write(&source, serde_json::to_vec(&value).unwrap()).unwrap();
        let output = directory.path().join("rejected.rbxm");
        let error = animation_gltf::convert(&source, &output, config())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(expected), "expected {expected}, got {error}");
        assert!(!output.exists());
    }
    fs::write(&source, serde_json::to_vec(&base).unwrap()).unwrap();
    let mut outside = config();
    outside.root_node = 1;
    assert!(
        animation_gltf::read(&source, &outside)
            .err()
            .unwrap()
            .to_string()
            .contains("outside")
    );
}

#[test]
fn gltf_animation_cli_and_bundle_match_without_external_tools() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = root.join("motion.gltf");
    fs::write(&source, serde_json::to_vec(&fixture(root)).unwrap()).unwrap();
    let policy = root.join("config.json");
    fs::write(&policy, serde_json::to_vec(&config()).unwrap()).unwrap();
    let output = root.join("motion.rbxm");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_roblox"))
        .env_clear()
        .args(["convert", "animation-gltf"])
        .arg(&source)
        .arg("--config")
        .arg(&policy)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    let response: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(response["result"]["rig"][1]["parentNode"], 0);
    let plan = json!({"assets":[{"id":"motion","conversion":{
        "kind":"animationGltf","source":"motion.gltf","config":config()
    }}],"scenes":[]});
    let plan_path = root.join("build.json");
    fs::write(&plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
    let bundle = root.join("bundle");
    let manifest = roblox_asset_link::bundle::build(&plan_path, &bundle).unwrap();
    assert_eq!(
        fs::read(output).unwrap(),
        fs::read(bundle.join(&manifest.files[0].path)).unwrap()
    );
}
