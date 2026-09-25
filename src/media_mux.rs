//! Offline packaging of preconverted video and audio without codec changes.
use crate::Result;
use ffmpeg_next as av;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Specification {
    pub video: PathBuf,
    pub audio: PathBuf,
    pub audio_offset_millis: u32,
    pub max_packets: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: &'static str,
    pub video_sha256: String,
    pub audio_sha256: String,
    pub sha256: String,
    pub audio_offset_millis: u32,
    pub video_packets: u64,
    pub audio_packets: u64,
    pub engine_verified: bool,
}

fn open(
    root: &Path,
    source: &Path,
    medium: av::media::Type,
) -> Result<(av::format::context::Input, String)> {
    if source.is_absolute() {
        return Err("media inputs must be relative paths".into());
    }
    let path = root.join(source).canonicalize()?;
    if !path.starts_with(root) || !path.is_file() {
        return Err("media input must be a contained regular file".into());
    }
    let hash = format!("{:x}", Sha256::digest(fs::read(&path)?));
    let mut options = av::Dictionary::new();
    options.set("protocol_whitelist", "file");
    options.set(
        "format_whitelist",
        if medium == av::media::Type::Video {
            "matroska,webm"
        } else {
            "ogg"
        },
    );
    let input = av::format::input_with_dictionary(&path, options)?;
    if input.nb_streams() != 1 {
        return Err("each media input must have exactly one stream".into());
    }
    let stream = input.stream(0).ok_or("missing media stream")?;
    let parameters = stream.parameters();
    if parameters.medium() != medium {
        return Err("wrong media input stream type".into());
    }
    let supported = match medium {
        av::media::Type::Video => parameters.id() == av::codec::Id::VP9,
        av::media::Type::Audio => {
            matches!(parameters.id(), av::codec::Id::VORBIS | av::codec::Id::OPUS)
        }
        _ => false,
    };
    if !supported {
        return Err("mux requires preconverted VP9 video and Vorbis/Opus audio".into());
    }
    let time_base = stream.time_base();
    if time_base.numerator() <= 0 || time_base.denominator() <= 0 {
        return Err("invalid media time base".into());
    }
    Ok((input, hash))
}

fn next(input: &mut av::format::context::Input) -> Result<Option<av::Packet>> {
    let mut packet = av::Packet::empty();
    match packet.read(input) {
        Ok(()) => {
            if packet.is_corrupt() || packet.pts().is_none() || packet.dts().is_none() {
                return Err("corrupt media packet or missing timestamp".into());
            }
            Ok(Some(packet))
        }
        Err(av::Error::Eof) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn build(source: &Path, output: &Path) -> Result<Manifest> {
    use av::Rescale;
    av::init()?;
    let source = source.canonicalize()?;
    let root = source
        .parent()
        .ok_or("media specification parent missing")?;
    let spec: Specification = serde_json::from_slice(&fs::read(&source)?)?;
    if spec.max_packets == 0 {
        return Err("maxPackets must be positive".into());
    }
    let (video, video_hash) = open(root, &spec.video, av::media::Type::Video)?;
    let (audio, audio_hash) = open(root, &spec.audio, av::media::Type::Audio)?;
    let mut inputs = [video, audio];
    let input_bases = [
        inputs[0].stream(0).unwrap().time_base(),
        inputs[1].stream(0).unwrap().time_base(),
    ];
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut target = av::format::output_as(temporary.path(), "webm")?;
    for input in &inputs {
        let stream = input.stream(0).ok_or("input stream disappeared")?;
        let mut out = target.add_stream(av::encoder::find(stream.parameters().id()))?;
        out.set_parameters(stream.parameters());
        out.set_time_base(stream.time_base());
        out.set_rate(stream.rate());
        out.set_avg_frame_rate(stream.avg_frame_rate());
    }
    let mut options = av::Dictionary::new();
    options.set("fflags", "+bitexact");
    options.set("avoid_negative_ts", "disabled");
    let remaining = target.write_header_with(options)?;
    if remaining.iter().next().is_some() {
        return Err("mux writer rejected deterministic timestamp options".into());
    }
    drop(remaining);
    let mut pending = [next(&mut inputs[0])?, next(&mut inputs[1])?];
    let mut counts = [0u64; 2];
    let mut previous = [None, None];
    while pending.iter().any(Option::is_some) {
        let when = |index: usize| {
            pending[index].as_ref().map(|packet| {
                i128::from(
                    packet
                        .dts()
                        .unwrap()
                        .rescale(input_bases[index], av::Rational(1, 1000)),
                ) + if index == 1 {
                    i128::from(spec.audio_offset_millis)
                } else {
                    0
                }
            })
        };
        let index = match (when(0), when(1)) {
            (Some(a), Some(b)) => usize::from(b < a),
            (Some(_), None) => 0,
            (None, Some(_)) => 1,
            (None, None) => break,
        };
        if counts.iter().sum::<u64>() >= spec.max_packets {
            return Err("media exceeds maxPackets".into());
        }
        let mut packet = pending[index].take().unwrap();
        let dts = packet.dts().unwrap();
        if previous[index].is_some_and(|previous| dts < previous) {
            return Err("non-monotonic media packet timestamps".into());
        }
        previous[index] = Some(dts);
        let output_base = target
            .stream(index)
            .ok_or("missing output stream")?
            .time_base();
        packet.rescale_ts(input_bases[index], output_base);
        if index == 1 {
            let offset =
                i64::from(spec.audio_offset_millis).rescale(av::Rational(1, 1000), output_base);
            packet.set_pts(Some(
                packet
                    .pts()
                    .unwrap()
                    .checked_add(offset)
                    .ok_or("media timestamp overflow")?,
            ));
            packet.set_dts(Some(
                packet
                    .dts()
                    .unwrap()
                    .checked_add(offset)
                    .ok_or("media timestamp overflow")?,
            ));
        }
        packet.set_stream(index);
        packet.set_position(-1);
        packet.write_interleaved(&mut target)?;
        counts[index] += 1;
        pending[index] = next(&mut inputs[index])?;
    }
    if counts.contains(&0) {
        return Err("media inputs must not be empty".into());
    }
    target.write_trailer()?;
    drop(target);
    let manifest = Manifest {
        format: "webm-vp9-audio",
        video_sha256: video_hash,
        audio_sha256: audio_hash,
        sha256: format!("{:x}", Sha256::digest(fs::read(temporary.path())?)),
        audio_offset_millis: spec.audio_offset_millis,
        video_packets: counts[0],
        audio_packets: counts[1],
        engine_verified: false,
    };
    temporary.persist_noclobber(output)?;
    Ok(manifest)
}
