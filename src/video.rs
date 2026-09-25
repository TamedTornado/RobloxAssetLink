//! In-process local video conversion. Native Roblox acceptance is not implied.
use crate::Result;
use ffmpeg_next as av;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub max_width: u32,
    pub max_height: u32,
    pub max_frames: u64,
    pub bitrate: usize,
    pub crf: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub frame_rate: [i32; 2],
    pub sha256: String,
    pub engine_verified: bool,
    pub codec_library_version: String,
}

fn pending(error: av::Error) -> bool {
    error == av::Error::Eof
        || error
            == av::Error::Other {
                errno: av::error::EAGAIN,
            }
}

fn packets(
    encoder: &mut av::encoder::Video,
    output: &mut av::format::context::Output,
    time_base: av::Rational,
) -> Result<()> {
    loop {
        let mut packet = av::Packet::empty();
        match encoder.receive_packet(&mut packet) {
            Ok(()) => {
                packet.set_stream(0);
                packet.set_duration(1);
                packet.rescale_ts(
                    time_base,
                    output.stream(0).ok_or("missing output stream")?.time_base(),
                );
                packet.write_interleaved(output)?;
            }
            Err(error) if pending(error) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

struct Timeline {
    input: av::Rational,
    output: av::Rational,
    origin: Option<i64>,
    count: u64,
    limit: u64,
}

fn frames(
    decoder: &mut av::decoder::Video,
    encoder: &mut av::encoder::Video,
    output: &mut av::format::context::Output,
    timeline: &mut Timeline,
) -> Result<()> {
    loop {
        let mut frame = av::frame::Video::empty();
        match decoder.receive_frame(&mut frame) {
            Ok(()) => {
                if frame.is_corrupt() {
                    return Err("decoder marked a video frame corrupt".into());
                }
                if frame.format() != av::format::Pixel::YUV420P
                    || frame.width() != encoder.width()
                    || frame.height() != encoder.height()
                    || frame.is_interlaced()
                {
                    return Err(
                        "video profile requires constant progressive YUV420P dimensions".into(),
                    );
                }
                if timeline.count >= timeline.limit {
                    return Err("video exceeds maxFrames".into());
                }
                let pts = frame.timestamp().ok_or("video frame has no timestamp")?;
                let origin = *timeline.origin.get_or_insert(pts);
                let actual = (i128::from(pts) - i128::from(origin))
                    * i128::from(timeline.input.numerator())
                    * i128::from(timeline.output.denominator());
                let expected = i128::from(timeline.count)
                    * i128::from(timeline.output.numerator())
                    * i128::from(timeline.input.denominator());
                if actual != expected {
                    return Err("variable or discontinuous video frame timing is not supported by this profile".into());
                }
                frame.set_pts(Some(i64::try_from(timeline.count)?));
                frame.set_kind(av::picture::Type::None);
                encoder.send_frame(&frame)?;
                packets(encoder, output, timeline.output)?;
                timeline.count += 1;
            }
            Err(error) if pending(error) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

pub fn convert(source: &Path, output: &Path, config: &Config) -> Result<Manifest> {
    if config.max_width == 0
        || config.max_height == 0
        || config.max_frames == 0
        || config.bitrate == 0
        || config.crf > 63
    {
        return Err("video limits/bitrate must be positive and VP9 CRF must be in [0,63]".into());
    }
    av::init()?;
    let source = source.canonicalize()?;
    if !source.is_file() {
        return Err("video input must be a local regular file".into());
    }
    let mut options = av::Dictionary::new();
    options.set("protocol_whitelist", "file");
    options.set("format_whitelist", "mov,matroska,webm");
    options.set("enable_drefs", "0");
    options.set("use_absolute_path", "0");
    options.set("err_detect", "explode");
    let mut input = av::format::input_with_dictionary(&source, options)?;
    let declared_duration = input.duration();
    if input.nb_streams() != 1 {
        return Err("video profile requires one video stream; audio/subtitles must not be silently discarded".into());
    }
    let stream = input.stream(0).ok_or("missing video stream")?;
    if stream.parameters().medium() != av::media::Type::Video {
        return Err("input is not a video stream".into());
    }
    if stream.side_data().next().is_some() {
        return Err("video stream side data requires explicit conversion, not implemented".into());
    }
    let rate = stream.avg_frame_rate();
    let time_base = stream.time_base();
    let declared_frames = stream.frames();
    if rate.numerator() <= 0
        || rate.denominator() <= 0
        || time_base.numerator() <= 0
        || time_base.denominator() <= 0
    {
        return Err("video requires a known positive frame rate and time base".into());
    }
    let mut decoder = av::codec::context::Context::from_parameters(stream.parameters())?
        .decoder()
        .video()?;
    decoder.check(av::codec::decoder::Check::EXPLODE);
    decoder.conceal(av::codec::decoder::Conceal::empty());
    let (width, height) = (decoder.width(), decoder.height());
    if width == 0
        || height == 0
        || width > config.max_width
        || height > config.max_height
        || width % 2 != 0
        || height % 2 != 0
    {
        return Err("video dimensions must be positive/even and within configured limits".into());
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut target = av::format::output_as(temporary.path(), "webm")?;
    let codec = av::encoder::find_by_name("libvpx-vp9")
        .ok_or("linked FFmpeg lacks the libvpx-vp9 encoder")?;
    let mut encoder = av::codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()?;
    encoder.set_width(width);
    encoder.set_height(height);
    encoder.set_format(av::format::Pixel::YUV420P);
    encoder.set_time_base(rate.invert());
    encoder.set_frame_rate(Some(rate));
    encoder.set_bit_rate(config.bitrate);
    encoder.set_aspect_ratio(decoder.aspect_ratio());
    encoder.set_colorspace(decoder.color_space());
    encoder.set_color_range(decoder.color_range());
    // These AVCodecContext metadata fields have no safe setter in ffmpeg-next.
    // The context is uniquely borrowed and remains alive throughout this block.
    unsafe {
        let context = &mut *encoder.as_mut_ptr();
        context.color_primaries = decoder.color_primaries().into();
        context.color_trc = decoder.color_transfer_characteristic().into();
        context.chroma_sample_location = decoder.chroma_location().into();
    }
    encoder.set_flags(av::codec::Flags::BITEXACT | av::codec::Flags::GLOBAL_HEADER);
    let mut options = av::Dictionary::new();
    options.set("crf", &config.crf.to_string());
    let mut encoder = encoder.open_with(options)?;
    target.add_stream(codec)?.set_parameters(&encoder);
    let mut header = av::Dictionary::new();
    header.set("fflags", "+bitexact");
    let remaining = target.write_header_with(header)?;
    if remaining.iter().next().is_some() {
        return Err("WebM writer did not accept deterministic mux settings".into());
    }
    drop(remaining);
    let mut timeline = Timeline {
        input: time_base,
        output: rate.invert(),
        origin: None,
        count: 0,
        limit: config.max_frames,
    };
    loop {
        let mut packet = av::Packet::empty();
        match packet.read(&mut input) {
            Ok(()) => {
                if packet.is_corrupt() {
                    return Err("corrupt video packet".into());
                }
                decoder.send_packet(&packet)?;
                frames(&mut decoder, &mut encoder, &mut target, &mut timeline)?;
            }
            Err(av::Error::Eof) => break,
            Err(error) => return Err(error.into()),
        }
    }
    decoder.send_eof()?;
    frames(&mut decoder, &mut encoder, &mut target, &mut timeline)?;
    if timeline.count == 0 || (declared_frames > 0 && timeline.count != declared_frames as u64) {
        return Err("decoded video frame count disagrees with declared duration".into());
    }
    if declared_duration > 0 {
        let seconds = timeline.count as f64 * f64::from(timeline.output);
        let declared = declared_duration as f64 / av::ffi::AV_TIME_BASE as f64;
        // Container duration timestamps are quantized to their time base.
        let quantum = f64::from(time_base) + 1.0 / av::ffi::AV_TIME_BASE as f64;
        if (seconds - declared).abs() > quantum {
            return Err("decoded video duration disagrees with container duration".into());
        }
    }
    encoder.send_eof()?;
    packets(&mut encoder, &mut target, timeline.output)?;
    target.write_trailer()?;
    drop(target);
    let manifest = Manifest {
        format: "webm-vp9",
        width,
        height,
        frames: timeline.count,
        frame_rate: [rate.numerator(), rate.denominator()],
        sha256: format!("{:x}", Sha256::digest(fs::read(temporary.path())?)),
        engine_verified: false,
        codec_library_version: format!(
            "libavcodec:{};libavformat:{}",
            av::codec::version(),
            av::format::version()
        ),
    };
    temporary.persist_noclobber(output)?;
    Ok(manifest)
}
