use roblox_asset_link::convert::{Config, convert};
use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn u32_at(bytes: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn real_assets_encode_deterministically_with_consistent_native_layout() {
    let temporary = tempfile::tempdir().unwrap();
    let config = Config {
        metres_per_stud: 0.28,
    };
    for name in ["doorway", "stair"] {
        let source = format!("tests/fixtures/{name}.glb");
        let first = temporary.path().join(name);
        let second = temporary.path().join(format!("{name}-again"));
        let manifest = convert(Path::new(&source), &first, &config).unwrap();
        convert(Path::new(&source), &second, &config).unwrap();
        assert!(!manifest.collision_generated);
        assert!(!manifest.meshes.is_empty());
        assert_eq!(
            fs::read(first.join("manifest.json")).unwrap(),
            fs::read(second.join("manifest.json")).unwrap()
        );

        for entry in manifest.meshes {
            let bytes = fs::read(first.join(&entry.file)).unwrap();
            assert_eq!(bytes, fs::read(second.join(&entry.file)).unwrap());
            assert_eq!(&bytes[..13], b"version 2.00\n");
            assert_eq!(&bytes[13..17], &[12, 0, 40, 12]);
            assert_eq!(u32_at(&bytes, 17), entry.vertices);
            assert_eq!(u32_at(&bytes, 21), entry.triangles);
            let face_start = 25 + entry.vertices * 40;
            assert_eq!(bytes.len(), face_start + entry.triangles * 12);
            for index in (face_start..bytes.len()).step_by(4) {
                assert!(u32_at(&bytes, index) < entry.vertices);
            }
            for vertex in 0..entry.vertices {
                let offset = 25 + vertex * 40;
                let normal_length: f32 = (0..3)
                    .map(|axis| f32_at(&bytes, offset + 12 + axis * 4).powi(2))
                    .sum();
                assert!((normal_length - 1.).abs() < 0.00001);
            }
        }
    }
}

#[test]
fn metric_configuration_is_respected_and_existing_outputs_are_preserved() {
    let temporary = tempfile::tempdir().unwrap();
    let source = Path::new("tests/fixtures/stair.glb");
    let a = temporary.path().join("a");
    let b = temporary.path().join("b");
    let manifest = convert(
        source,
        &a,
        &Config {
            metres_per_stud: 0.28,
        },
    )
    .unwrap();
    convert(
        source,
        &b,
        &Config {
            metres_per_stud: 0.56,
        },
    )
    .unwrap();
    for entry in manifest.meshes {
        let first = fs::read(a.join(&entry.file)).unwrap();
        let second = fs::read(b.join(&entry.file)).unwrap();
        for vertex in 0..entry.vertices {
            for axis in 0..3 {
                let offset = 25 + vertex * 40 + axis * 4;
                assert_eq!(f32_at(&first, offset) / 2., f32_at(&second, offset));
            }
        }
    }
    let before = fs::read(a.join("manifest.json")).unwrap();
    assert!(
        convert(
            source,
            &a,
            &Config {
                metres_per_stud: 1.
            }
        )
        .is_err()
    );
    assert_eq!(before, fs::read(a.join("manifest.json")).unwrap());
    for scale in [0., -1., f32::NAN, f32::INFINITY] {
        let output = temporary.path().join("invalid");
        assert!(
            convert(
                source,
                &output,
                &Config {
                    metres_per_stud: scale
                }
            )
            .is_err()
        );
        assert!(!output.exists());
    }
}

#[test]
fn unsupported_materials_fail_without_silently_losing_transmission() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("glass");
    let error = convert(
        Path::new("tests/fixtures/glass.glb"),
        &output,
        &Config {
            metres_per_stud: 0.28,
        },
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("extensions"));
    assert!(!output.exists());
}

#[test]
fn cli_conversion_needs_no_catalog_server_or_credentials() {
    let temporary = tempfile::tempdir().unwrap();
    let output_path = temporary.path().join("converted");
    let output = Command::new(env!("CARGO_BIN_EXE_roblox"))
        .env_clear()
        .args([
            "convert",
            "mesh",
            "tests/fixtures/doorway.glb",
            "--config",
            "examples/conversion.json",
            "--output",
        ])
        .arg(&output_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["scope"], "offlineConversion");
    assert!(output_path.join("manifest.json").exists());

    let unsupported = temporary.path().join("example.fbx");
    fs::write(&unsupported, b"not an implemented input adapter").unwrap();
    let failed = temporary.path().join("failed");
    assert!(
        convert(
            &unsupported,
            &failed,
            &Config {
                metres_per_stud: 1.
            }
        )
        .is_err()
    );
    assert!(!failed.exists());
}
