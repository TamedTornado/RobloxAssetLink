//! Combined-source video/audio conversion through the same offline codecs.
use crate::{Result, audio, media_mux, video};
use ffmpeg_next as av;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub video: video::Config,
    pub audio: audio::Config,
    pub max_packets: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub video: video::Manifest,
    pub audio: audio::Conversion,
    pub combined: media_mux::Manifest,
    pub source_audio_padding_frames_removed: u64,
}

fn extract_audio(source: &Path, output: &Path, max_frames: u64) -> Result<(i64, u64)> {
    use av::Rescale;
    let mut options = av::Dictionary::new();
    options.set("protocol_whitelist", "file");
    options.set("format_whitelist", "mov,matroska,webm");
    options.set("enable_drefs", "0");
    options.set("use_absolute_path", "0");
    let mut input = av::format::input_with_dictionary(&source, options)?;
    let stream = input
        .streams()
        .find(|s| s.parameters().medium() == av::media::Type::Audio)
        .ok_or("missing audio stream")?;
    let index = stream.index();
    let time_base = stream.time_base();
    let codec = stream.parameters().id();
    let duration = stream.duration();
    let mut decoder = av::codec::context::Context::from_parameters(stream.parameters())?
        .decoder()
        .audio()?;
    decoder.check(av::codec::decoder::Check::EXPLODE);
    decoder.conceal(av::codec::decoder::Conceal::empty());
    let rate = decoder.rate();
    let channels = decoder.channels();
    if rate == 0 || !matches!(channels, 1 | 2) {
        return Err("combined-source audio requires mono/stereo and a known sample rate".into());
    }
    if time_base.numerator() <= 0 || time_base.denominator() <= 0 {
        return Err("invalid audio time base".into());
    }
    let expected = if duration > 0 {
        Some(u64::try_from(
            duration.rescale(time_base, av::Rational(1, i32::try_from(rate)?)),
        )?)
    } else {
        None
    };
    if expected.is_some_and(|n| n > max_frames) {
        return Err("audio exceeds maxDecodedFrames".into());
    }
    let mut pcm = Vec::new();
    let mut count = 0u64;
    let mut decoded_count = 0u64;
    let mut origin = None;
    let mut drain = |decoder: &mut av::decoder::Audio| -> Result<()> {
        loop {
            let mut frame = av::frame::Audio::empty();
            match decoder.receive_frame(&mut frame) {
                Ok(()) => {
                    if frame.is_corrupt()
                        || frame.rate() != rate
                        || frame.channels() != channels
                        || frame.format()
                            != av::format::Sample::F32(av::format::sample::Type::Planar)
                    {
                        return Err("combined-source audio requires constant planar float32 mono/stereo; corrupt or changing formats are rejected".into());
                    }
                    let pts = frame.timestamp().ok_or("audio frame timestamp missing")?;
                    let start = *origin.get_or_insert(pts);
                    let relative = pts
                        .checked_sub(start)
                        .ok_or("audio timestamp overflow")?
                        .rescale(time_base, av::Rational(1, i32::try_from(rate)?));
                    if relative < 0 || relative as u64 != decoded_count {
                        return Err("audio source has discontinuous timestamps".into());
                    }
                    let samples = frame.samples() as u64;
                    decoded_count = decoded_count
                        .checked_add(samples)
                        .ok_or("audio sample count overflow")?;
                    let valid = expected.map_or(samples, |n| samples.min(n.saturating_sub(count)));
                    if valid != samples && codec != av::codec::Id::AAC {
                        return Err("unexpected decoded audio duration mismatch".into());
                    }
                    if valid == 0 || count.checked_add(valid).is_none_or(|n| n > max_frames) {
                        return Err("audio exceeds declared duration or maxDecodedFrames".into());
                    }
                    for sample in 0..usize::try_from(valid)? {
                        for channel in 0..usize::from(channels) {
                            let value = frame.plane::<f32>(channel)[sample];
                            if !value.is_finite() {
                                return Err("non-finite decoded audio".into());
                            }
                            pcm.extend_from_slice(&value.to_le_bytes());
                        }
                    }
                    count += valid;
                }
                Err(av::Error::Eof) => return Ok(()),
                Err(av::Error::Other { errno }) if errno == av::error::EAGAIN => return Ok(()),
                Err(error) => return Err(error.into()),
            }
        }
    };
    loop {
        let mut packet = av::Packet::empty();
        match packet.read(&mut input) {
            Ok(()) if packet.stream() == index => {
                if packet.is_corrupt() {
                    return Err("corrupt audio packet".into());
                }
                decoder.send_packet(&packet)?;
                drain(&mut decoder)?;
            }
            Ok(()) => {}
            Err(av::Error::Eof) => break,
            Err(error) => return Err(error.into()),
        }
    }
    decoder.send_eof()?;
    drain(&mut decoder)?;
    if count == 0 || expected.is_some_and(|n| n != count) {
        return Err("decoded audio disagrees with declared sample duration".into());
    }
    let length = u32::try_from(pcm.len())?;
    let mut wav = b"RIFF".to_vec();
    wav.extend(
        length
            .checked_add(48)
            .ok_or("PCM exceeds RIFF size bound")?
            .to_le_bytes(),
    );
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(3u16.to_le_bytes()); // IEEE float PCM format tag.
    wav.extend(channels.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend(
        rate.checked_mul(u32::from(channels) * 4)
            .ok_or("PCM byte rate overflow")?
            .to_le_bytes(),
    );
    wav.extend((channels * 4).to_le_bytes());
    wav.extend(32u16.to_le_bytes());
    wav.extend(b"fact");
    wav.extend(4u32.to_le_bytes());
    wav.extend(u32::try_from(count)?.to_le_bytes());
    wav.extend(b"data");
    wav.extend(length.to_le_bytes());
    wav.extend(pcm);
    fs::write(output, wav)?;
    Ok((
        origin
            .ok_or("missing audio origin")?
            .rescale(time_base, av::Rational(1, av::ffi::AV_TIME_BASE)),
        decoded_count - count,
    ))
}

pub fn convert(source: &Path, output: &Path, config: &Config) -> Result<Manifest> {
    let source = source.canonicalize()?;
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let video = video::convert_track(&source, &root.join("video.webm"), &config.video, true)?;
    let (audio_origin, source_audio_padding_frames_removed) = extract_audio(
        &source,
        &root.join("audio.wav"),
        config.audio.max_decoded_frames,
    )?;
    let offset = audio_origin
        .checked_sub(video.source_start_micros)
        .ok_or("media offset overflow")?;
    if offset < 0 {
        return Err(
            "audio preceding video requires signed media offset support, not implemented".into(),
        );
    }
    let millis = u32::try_from(offset.checked_add(500).ok_or("media offset overflow")? / 1000)?;
    let audio = audio::convert(
        &root.join("audio.wav"),
        &root.join("audio.ogg"),
        &config.audio,
    )?;
    let specification = serde_json::json!({"video":"video.webm","audio":"audio.ogg","audioOffsetMillis":millis,"maxPackets":config.max_packets});
    let path = root.join("media.json");
    fs::write(&path, serde_json::to_vec(&specification)?)?;
    let combined = media_mux::build(&path, output)?;
    Ok(Manifest {
        video,
        audio,
        combined,
        source_audio_padding_frames_removed,
    })
}
