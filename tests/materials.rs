use image::{Rgba, RgbaImage};
use rbx_dom_weak::types::{Content, Variant};
use roblox_asset_link::material::convert;
use serde_json::{Value, json};
use std::{fs, process::Command};

fn specification() -> Value {
    json!({
        "name":"Paint", "alphaMode":"Transparency", "color":[1,0.5,0.25],
        "localUriPrefix":"rbxasset://kit/paint/",
        "maps":[
            {"source":"pixels.png", "config":{"operation":"color", "maxWidth":2,"maxHeight":2,"maxDecodedBytes":4096}},
            {"source":"pixels.png", "config":{"operation":"gltfMetallicRoughness", "maxWidth":2,"maxHeight":2,"maxDecodedBytes":4096}}
        ]
    })
}

#[test]
fn material_native_properties_maps_and_deterministic_cli_output() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    RgbaImage::from_pixel(1, 1, Rgba([10, 20, 30, 40]))
        .save(root.join("pixels.png"))
        .unwrap();
    let source = root.join("material.json");
    fs::write(&source, specification().to_string()).unwrap();
    let out = root.join("out");
    let result = convert(&source, &out).unwrap();
    assert_eq!(result.maps.len(), 3);
    assert!(!result.engine_verified);
    assert_eq!(result.maps["roughness"].color_space, "linear");

    let bytes = fs::read(out.join("material.rbxm")).unwrap();
    let dom = rbx_binary::from_reader(bytes.as_slice()).unwrap();
    let material = dom.get_by_ref(dom.root().children()[0]).unwrap();
    assert_eq!(material.class.as_str(), "SurfaceAppearance");
    assert_eq!(material.name, "Paint");
    assert_eq!(
        material.properties[&"ColorMapContent".into()],
        Variant::Content(Content::from_uri("rbxasset://kit/paint/map-0/color.png"))
    );
    assert_eq!(
        material.properties[&"RoughnessMapContent".into()],
        Variant::Content(Content::from_uri(
            "rbxasset://kit/paint/map-1/roughness.png"
        ))
    );
    assert_eq!(
        material.properties[&"AlphaMode".into()],
        Variant::Enum(rbx_dom_weak::types::Enum::from_u32(1))
    );
    assert_eq!(
        image::open(out.join(&result.maps["roughness"].file))
            .unwrap()
            .to_luma8()
            .into_raw(),
        [20]
    );
    assert_eq!(
        image::open(out.join(&result.maps["metalness"].file))
            .unwrap()
            .to_luma8()
            .into_raw(),
        [30]
    );
    assert_eq!(
        image::open(out.join(&result.maps["color"].file))
            .unwrap()
            .to_rgba8()
            .into_raw(),
        [10, 20, 30, 40]
    );

    let repeat = root.join("repeat");
    let cli = Command::new(env!("CARGO_BIN_EXE_roblox"))
        .env_clear()
        .args(["convert", "material"])
        .arg(&source)
        .arg("--output")
        .arg(&repeat)
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    for file in [
        "material.rbxm",
        "manifest.json",
        "map-0/color.png",
        "map-1/roughness.png",
        "map-1/metalness.png",
    ] {
        assert_eq!(
            fs::read(out.join(file)).unwrap(),
            fs::read(repeat.join(file)).unwrap()
        );
    }
    assert!(convert(&source, &out).is_err());
    assert_eq!(fs::read(out.join("material.rbxm")).unwrap(), bytes);
}

#[test]
fn invalid_maps_paths_limits_and_semantics_leave_no_output() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    RgbaImage::from_pixel(2, 1, Rgba([10, 20, 30, 255]))
        .save(root.join("pixels.png"))
        .unwrap();
    let source = root.join("material.json");
    let out = root.join("out");
    let mut cases = Vec::new();
    let mut duplicate = specification();
    let repeated_map = duplicate["maps"][0].clone();
    duplicate["maps"].as_array_mut().unwrap().push(repeated_map);
    cases.push(duplicate);
    for (pointer, value) in [
        ("/maps/0/source", json!("../pixels.png")),
        ("/maps/0/config/maxWidth", json!(1)),
        ("/maps/0/config/operation", json!("roughness")),
        ("/alphaMode", json!("invented")),
        ("/color/0", json!(2)),
        ("/localUriPrefix", json!("https://example.com/")),
        ("/localUriPrefix", json!("rbxasset://../escape/")),
    ] {
        let mut value_spec = specification();
        *value_spec.pointer_mut(pointer).unwrap() = value;
        cases.push(value_spec);
    }
    for spec in cases {
        fs::write(&source, spec.to_string()).unwrap();
        assert!(convert(&source, &out).is_err(), "{spec}");
        assert!(!out.exists());
    }
}
