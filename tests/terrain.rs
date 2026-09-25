use roblox_asset_link::scene;
use serde_json::json;
use std::{collections::BTreeMap, fs};

#[test]
fn independent_native_terrain_voxels_palette_and_water_survive_place_assembly() {
    let fixture =
        rbx_binary::from_reader(fs::File::open("tests/fixtures/terrain-rojo.rbxm").unwrap())
            .unwrap();
    let terrain = fixture.get_by_ref(fixture.root().children()[0]).unwrap();
    assert_eq!(terrain.class.as_str(), "Terrain");
    let mut properties = BTreeMap::new();
    for name in [
        "SmoothGrid",
        "PhysicsGrid",
        "MaterialColors",
        "WaterColor",
        "WaterReflectance",
        "WaterTransparency",
        "WaterWaveSize",
        "WaterWaveSpeed",
    ] {
        let value = terrain
            .properties
            .get(&name.into())
            .unwrap_or_else(|| panic!("fixture is missing {name}"));
        properties.insert(name.to_owned(), value.clone());
    }
    let rbx_dom_weak::types::Variant::BinaryString(grid) = &properties["SmoothGrid"] else {
        panic!("expected binary terrain voxels");
    };
    let grid_bytes: &[u8] = grid.as_ref();
    assert!(!grid_bytes.is_empty());
    let spec = json!({"kind":"place","roots":[{"id":"world","class":"Workspace","name":"Workspace","properties":{},"references":{},"children":[
        {"id":"terrain","class":"Terrain","name":"Terrain","properties":properties,"references":{},"children":[]}
    ]}]});
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("scene.json");
    fs::write(&source, spec.to_string()).unwrap();
    let output = temporary.path().join("place.rbxl");
    scene::build(&source, &output).unwrap();
    let decoded = rbx_binary::from_reader(fs::File::open(&output).unwrap()).unwrap();
    let world = decoded.get_by_ref(decoded.root().children()[0]).unwrap();
    let terrain = decoded.get_by_ref(world.children()[0]).unwrap();
    for (name, value) in properties {
        assert_eq!(
            terrain.properties.get(&name.as_str().into()),
            Some(&value),
            "changed {name}"
        );
    }
    let second = temporary.path().join("again.rbxl");
    scene::build(&source, &second).unwrap();
    assert_eq!(fs::read(output).unwrap(), fs::read(second).unwrap());
}
