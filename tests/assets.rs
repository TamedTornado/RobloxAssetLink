use roblox_asset_link::{config::Config, read_asset, snapshot};
use std::path::Path;

fn config() -> Config {
    serde_json::from_str(include_str!("../examples/building-kit.json")).unwrap()
}

#[test]
fn real_blender_assets_have_expected_geometry_and_materials() {
    let snapshot = snapshot(Path::new("tests/fixtures"), &config()).unwrap();
    assert_eq!(snapshot.assets.len(), 3);
    let door = snapshot
        .assets
        .iter()
        .find(|a| a.name == "doorway")
        .unwrap();
    assert_eq!(door.meshes.len(), 3);
    let max_y = door
        .meshes
        .iter()
        .flat_map(|m| &m.vertices)
        .map(|v| v[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((max_y - 15.).abs() < 0.0001);
    let stairs = snapshot.assets.iter().find(|a| a.name == "stair").unwrap();
    assert_eq!(stairs.meshes.len(), 2);
    assert!(
        stairs
            .meshes
            .iter()
            .any(|m| m.settings.transparency == Some(1.))
    );
    let glass = snapshot.assets.iter().find(|a| a.name == "glass").unwrap();
    assert!(glass.meshes[0].color[3] < 1.);
    for mesh in snapshot.assets.iter().flat_map(|a| &a.meshes) {
        assert_eq!(mesh.normals.len(), mesh.vertices.len());
        assert_eq!(mesh.uvs.len(), mesh.vertices.len());
        assert!(
            mesh.triangles
                .iter()
                .flatten()
                .all(|i| (*i as usize) < mesh.vertices.len())
        );
    }
}

#[test]
fn nondefault_units_rules_and_revisions_are_honored() {
    let path = Path::new("tests/fixtures/stair.glb");
    let original = read_asset(path, &config()).unwrap();
    let mut changed = config();
    changed.metres_per_stud *= 2.;
    changed.rules.clear();
    let result = read_asset(path, &changed).unwrap();
    assert_ne!(original.revision, result.revision);
    for (a, b) in original.meshes.iter().zip(&result.meshes) {
        for (v, w) in a.vertices.iter().zip(&b.vertices) {
            for axis in 0..3 {
                assert_eq!(v[axis] / 2., w[axis]);
            }
        }
        assert_eq!(b.settings.transparency, None);
    }
}
