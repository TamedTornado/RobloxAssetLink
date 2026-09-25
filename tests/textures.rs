use image::{Rgba, RgbaImage};
use roblox_asset_link::texture::{Config, Operation, convert};
use std::{fs, process::Command};

fn config(operation: Operation) -> Config {
    Config {
        operation,
        max_width: 2,
        max_height: 2,
        max_decoded_bytes: 4096,
    }
}

#[test]
fn color_alpha_normal_and_pbr_channels_are_preserved_or_transformed_exactly() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source.png");
    let mut pixels = RgbaImage::new(2, 1);
    pixels.put_pixel(0, 0, Rgba([10, 20, 30, 40]));
    pixels.put_pixel(1, 0, Rgba([50, 60, 70, 80]));
    pixels.save(&source).unwrap();

    let color = temporary.path().join("color");
    convert(&source, &color, &config(Operation::Color)).unwrap();
    assert_eq!(
        image::open(color.join("color.png")).unwrap().to_rgba8(),
        pixels
    );
    let before = fs::read(color.join("color.png")).unwrap();
    assert!(convert(&source, &color, &config(Operation::Color)).is_err());
    assert_eq!(fs::read(color.join("color.png")).unwrap(), before);

    let normal = temporary.path().join("normal");
    convert(&source, &normal, &config(Operation::NormalDirectX)).unwrap();
    assert_eq!(
        image::open(normal.join("normal.png"))
            .unwrap()
            .to_rgb8()
            .get_pixel(0, 0)
            .0,
        [10, 235, 30]
    );

    let pbr = temporary.path().join("pbr");
    convert(&source, &pbr, &config(Operation::GltfMetallicRoughness)).unwrap();
    assert_eq!(
        image::open(pbr.join("roughness.png"))
            .unwrap()
            .into_luma8()
            .into_raw(),
        [20, 60]
    );
    assert_eq!(
        image::open(pbr.join("metalness.png"))
            .unwrap()
            .into_luma8()
            .into_raw(),
        [30, 70]
    );
    let repeat = temporary.path().join("repeat");
    convert(&source, &repeat, &config(Operation::GltfMetallicRoughness)).unwrap();
    for name in ["roughness.png", "metalness.png", "manifest.json"] {
        assert_eq!(
            fs::read(pbr.join(name)).unwrap(),
            fs::read(repeat.join(name)).unwrap()
        );
    }
}

#[test]
fn explicit_decode_limits_and_bit_depth_rejection_are_enforced() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source.png");
    RgbaImage::new(2, 1).save(&source).unwrap();
    let out = temporary.path().join("rejected");
    let mut policy = config(Operation::Color);
    policy.max_width = 1;
    assert!(convert(&source, &out, &policy).is_err());
    assert!(!out.exists());
    policy.max_width = 2;
    policy.max_decoded_bytes = 1;
    assert!(convert(&source, &out, &policy).is_err());
    assert!(!out.exists());
    let hdr = image::ImageBuffer::<Rgba<u16>, Vec<u16>>::new(1, 1);
    hdr.save(&source).unwrap();
    assert!(
        convert(&source, &out, &config(Operation::Color))
            .err()
            .unwrap()
            .to_string()
            .contains("8-bit")
    );
    assert!(!out.exists());
}

#[test]
fn standalone_scalar_maps_preserve_linear_samples_without_luminance_conversion() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("scalar.png");
    image::GrayImage::from_raw(2, 1, vec![37, 191])
        .unwrap()
        .save(&source)
        .unwrap();
    for (operation, semantic) in [
        (Operation::Roughness, "roughness"),
        (Operation::Metalness, "metalness"),
    ] {
        let output = temp.path().join(semantic);
        let manifest = convert(&source, &output, &config(operation)).unwrap();
        assert_eq!(manifest.textures[0].color_space, "linear");
        assert_eq!(
            image::open(output.join(format!("{semantic}.png")))
                .unwrap()
                .to_luma8()
                .into_raw(),
            [37, 191]
        );
    }
}

#[test]
fn texture_cli_returns_structured_results_without_credentials() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source.png");
    RgbaImage::new(2, 1).save(&source).unwrap();
    let policy = temporary.path().join("config.json");
    fs::write(
        &policy,
        r#"{"operation":"normalOpenGl","maxWidth":2,"maxHeight":2,"maxDecodedBytes":4096}"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_roblox"))
        .env_clear()
        .args(["convert", "texture"])
        .arg(source)
        .arg("--config")
        .arg(policy)
        .arg("--output")
        .arg(temporary.path().join("out"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["textures"][0]["semantic"], "normal");
    assert_eq!(value["result"]["textures"][0]["colorSpace"], "linear");
}
