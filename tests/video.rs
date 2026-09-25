use roblox_asset_link::video::{Config, convert};
use std::{fs, path::Path};

fn config() -> Config {
    Config {
        max_width: 64,
        max_height: 48,
        max_frames: 4,
        bitrate: 100000,
        crf: 20,
    }
}

#[test]
fn h264_to_webm_is_local_deterministic_and_preserves_frame_timing() {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new("tests/fixtures/video-64x48.mp4");
    let first = temp.path().join("first.webm");
    let result = convert(source, &first, &config()).unwrap();
    assert_eq!(
        (
            result.width,
            result.height,
            result.frames,
            result.frame_rate
        ),
        (64, 48, 4, [10, 1])
    );
    assert!(!result.engine_verified);
    let second = temp.path().join("second.webm");
    convert(source, &second, &config()).unwrap();
    let before = fs::read(&first).unwrap();
    assert_eq!(before, fs::read(&second).unwrap());
    assert!(convert(source, &first, &config()).is_err());
    assert_eq!(before, fs::read(&first).unwrap());
    let mut input = ffmpeg_next::format::input(&first).unwrap();
    let stream = input.stream(0).unwrap();
    assert_eq!(stream.parameters().id(), ffmpeg_next::codec::Id::VP9);
    let time_base = stream.time_base();
    let mut decoder = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())
        .unwrap()
        .decoder()
        .video()
        .unwrap();
    let mut timestamps = Vec::new();
    for (_, packet) in input.packets() {
        decoder.send_packet(&packet).unwrap();
        let mut frame = ffmpeg_next::frame::Video::empty();
        while decoder.receive_frame(&mut frame).is_ok() {
            assert_eq!((frame.width(), frame.height()), (64, 48));
            timestamps.push(frame.timestamp().unwrap());
        }
    }
    decoder.send_eof().unwrap();
    let mut frame = ffmpeg_next::frame::Video::empty();
    while decoder.receive_frame(&mut frame).is_ok() {
        timestamps.push(frame.timestamp().unwrap());
    }
    assert_eq!(timestamps.len(), 4);
    for (index, timestamp) in timestamps.into_iter().enumerate() {
        assert!((timestamp as f64 * f64::from(time_base) - index as f64 / 10.0).abs() < 0.001);
    }
}

#[test]
fn video_limits_bad_input_and_truncation_never_publish_partial_output() {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new("tests/fixtures/video-64x48.mp4");
    for config in [
        Config {
            max_frames: 3,
            ..config()
        },
        Config {
            max_width: 32,
            ..config()
        },
        Config {
            crf: 64,
            ..config()
        },
    ] {
        let output = temp.path().join("rejected.webm");
        assert!(convert(source, &output, &config).is_err());
        assert!(!output.exists());
    }
    let mut bytes = fs::read(source).unwrap();
    bytes.truncate(bytes.len() / 2);
    let truncated = temp.path().join("truncated.mp4");
    fs::write(&truncated, bytes).unwrap();
    let output = temp.path().join("rejected.webm");
    assert!(convert(&truncated, &output, &config()).is_err());
    assert!(!output.exists());
}

#[test]
fn video_cli_and_bundle_need_no_codec_executable_and_bind_native_video_content() {
    use serde_json::json;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::copy("tests/fixtures/video-64x48.mp4", root.join("video.mp4")).unwrap();
    let config = json!({"maxWidth":64,"maxHeight":48,"maxFrames":4,"bitrate":100000,"crf":20});
    fs::write(root.join("config.json"), config.to_string()).unwrap();
    let cli = std::process::Command::new(env!("CARGO_BIN_EXE_roblox"))
        .env_clear()
        .args(["convert", "video"])
        .arg(root.join("video.mp4"))
        .arg("--config")
        .arg(root.join("config.json"))
        .arg("--output")
        .arg(root.join("video.webm"))
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(response["result"]["frames"], 4);
    let plan = json!({"assets":[{"id":"movie","conversion":{"kind":"video","source":"video.mp4","config":config}}],"scenes":[{"id":"screen","source":"scene.json"}]});
    fs::write(root.join("build.json"), plan.to_string()).unwrap();
    let scene = json!({"kind":"model","roots":[{"id":"screen","class":"VideoFrame","name":"Screen","properties":{},"references":{},"children":[],"assets":{"VideoContent":{"asset":"movie","file":"video.webm"}}}]});
    fs::write(root.join("scene.json"), scene.to_string()).unwrap();
    let output = root.join("bundle");
    let result = roblox_asset_link::bundle::build(&root.join("build.json"), &output).unwrap();
    assert_eq!(
        fs::read(root.join("video.webm")).unwrap(),
        fs::read(output.join(&result.files[0].path)).unwrap()
    );
    let dom = rbx_binary::from_reader(fs::File::open(output.join(&result.scenes[0].path)).unwrap())
        .unwrap();
    let screen = dom.get_by_ref(dom.root().children()[0]).unwrap();
    assert_eq!(
        screen.properties[&"VideoContent".into()],
        rbx_dom_weak::types::Variant::Content(rbx_dom_weak::types::Content::from_uri(
            &result.files[0].local_uri
        ))
    );
}
