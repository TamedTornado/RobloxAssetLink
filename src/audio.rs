//! Local audio decode and Ogg Vorbis encoding; no server-side transcoding.
use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, ErrorKind, Write},
    num::{NonZeroU8, NonZeroU32},
    path::Path,
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_VORBIS, DecoderOptions},
    errors::Error,
    formats::FormatOptions,
    io::MediaSourceStream,
    probe::Hint,
};
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub quality: f32,
    pub max_decoded_frames: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversion {
    pub format: &'static str,
    pub sample_rate: u32,
    pub channels: u8,
    pub frames: u64,
    pub padding_frames_removed: u64,
    pub sha256: String,
    pub engine_verified: bool,
}

pub fn convert(source: &Path, output: &Path, config: &Config) -> Result<Conversion> {
    if !config.quality.is_finite()
        || !(-0.2..=1.).contains(&config.quality)
        || config.max_decoded_frames == 0
    {
        return Err("Vorbis quality must be in [-0.2,1] and maxDecodedFrames positive".into());
    }
    let bytes = fs::read(source)?;
    let hash = Sha256::digest(&bytes);
    let serial = i32::from_le_bytes(hash[..4].try_into()?);
    let stream = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = source.extension().and_then(|v| v.to_str()) {
        hint.with_extension(extension);
    }
    let options = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let mut format = symphonia::default::get_probe()
        .format(&hint, stream, &options, &Default::default())?
        .format;
    if format.tracks().len() != 1 {
        return Err("audio conversion requires exactly one track".into());
    }
    let track = &format.tracks()[0];
    let id = track.id;
    let sample_rate = NonZeroU32::new(
        track
            .codec_params
            .sample_rate
            .ok_or("audio sample rate missing")?,
    )
    .ok_or("invalid audio sample rate")?;
    let channels = NonZeroU8::new(u8::try_from(
        track
            .codec_params
            .channels
            .ok_or("audio channel layout missing")?
            .count(),
    )?)
    .ok_or("audio channel count is zero")?;
    if channels.get() > 2 {
        return Err(
            "this audio profile supports mono/stereo; implicit downmixing is forbidden".into(),
        );
    }
    let expected_frames = track.codec_params.n_frames;
    let vorbis = track.codec_params.codec == CODEC_TYPE_VORBIS;
    if vorbis && expected_frames.is_none() {
        return Err("Vorbis conversion requires a final granule duration".into());
    }
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions { verify: true })?;
    let mut encoder =
        VorbisEncoderBuilder::new_with_serial(sample_rate, channels, Vec::new(), serial)
            // Zero requests packet-page boundaries, making initial granule zero
            // explicit even for short clips. Otherwise FFmpeg can mis-infer the
            // initial overlap and discard real samples. This is container layout,
            // not a resource limit or quality tuning budget.
            .minimum_page_data_size(Some(0))
            .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
                target_quality: config.quality,
            })
            .build()?;
    let mut frames = 0_u64;
    let mut padding_frames_removed = 0_u64;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        };
        if packet.track_id() != id {
            return Err("unexpected additional audio track".into());
        }
        let decoded = decoder.decode(&packet)?;
        if decoded.spec().rate != sample_rate.get()
            || decoded.spec().channels.count() != usize::from(channels.get())
        {
            return Err("midstream audio format changes are not supported".into());
        }
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buffer.copy_interleaved_ref(decoded);
        let decoded_count = buffer.samples().len() / usize::from(channels.get());
        // The final Vorbis granule is authoritative, not the padded decode block.
        // Never truncate frame mismatches for other source codecs silently.
        let count = if vorbis {
            decoded_count.min(usize::try_from(
                expected_frames
                    .ok_or("Vorbis duration missing")?
                    .saturating_sub(frames),
            )?)
        } else {
            decoded_count
        };
        padding_frames_removed += (decoded_count - count) as u64;
        frames = frames
            .checked_add(count as u64)
            .ok_or("decoded frame count overflow")?;
        if frames > config.max_decoded_frames {
            return Err("audio exceeds configured maxDecodedFrames".into());
        }
        let mut planar = vec![Vec::with_capacity(count); usize::from(channels.get())];
        for frame in buffer
            .samples()
            .chunks_exact(usize::from(channels.get()))
            .take(count)
        {
            for (channel, sample) in frame.iter().enumerate() {
                if !sample.is_finite() {
                    return Err("audio contains non-finite samples".into());
                }
                planar[channel].push(*sample);
            }
        }
        if count > 0 {
            encoder.encode_audio_block(&planar)?;
        }
    }
    if frames == 0 || expected_frames.is_some_and(|expected| expected != frames) {
        return Err(format!(
            "audio frame count mismatch: decoded {frames}, declared {expected_frames:?}"
        )
        .into());
    }
    if decoder.finalize().verify_ok == Some(false) {
        return Err("audio integrity verification failed".into());
    }
    let bytes = encoder.finish()?;
    let result = Conversion {
        format: "ogg-vorbis",
        sample_rate: sample_rate.get(),
        channels: channels.get(),
        frames,
        padding_frames_removed,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        engine_verified: false,
    };
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(output)?;
    Ok(result)
}
