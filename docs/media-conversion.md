# Local media conversion — issue 6

`roblox convert audio SOURCE --config CONFIG --output audio.ogg` decodes supported
WAV/PCM, MP3, Ogg Vorbis and FLAC inputs through Symphonia and encodes Ogg Vorbis
through the linked Vorbis library. No codec subprocess, Studio or server is used.

Configuration requires `quality` (Vorbis's documented -0.2 through 1 range) and
`maxDecodedFrames` (caller resource policy). The current profile preserves the
sample rate and mono/stereo channels; it rejects multi-track/multichannel inputs
instead of silently downmixing. No resampling is implemented. Codec errors and
declared-duration mismatches are surfaced. Output uses a source-hash-derived Ogg
serial, not randomness, and does not overwrite existing files.

Tests synthesize PCM fixtures, transcode mono/stereo audio, independently decode
Vorbis output and check frame counts, rate, channels, waveform error and byte
repeatability. Non-default frame policy, truncation and invalid quality fail.
Local smoke tests also converted the installed Studio `action_jump.mp3` and
`oof.ogg` without adding proprietary media to the repository.

The latter exposed final-block padding: Symphonia returned 7808 samples while
the Vorbis final granule and a separate libvorbis decoder both reported 7327.
The converter honors the Vorbis granule duration and reports the 481 removed
padding frames, rather than adding audible duration or hiding a mismatch. Other
codec duration mismatches still fail. Vorbis inputs require known final duration.

Evidence: installed content includes native MP3 and Ogg Vorbis files;
[Roblox audio documentation](https://create.roblox.com/docs/audio/assets) lists
audio upload inputs. These are distinct observations, not proof that hosting
preconverted output will never trigger additional server processing.

The checked-in synthetic FLAC fixture is encoded independently by FFmpeg rather
than our conversion library. Tests decode/transcode it locally, then use a
separate Vorbis decoder to verify 4410 mono frames at 22050 Hz and waveform MSE
against its analytic sine wave. Repeated outputs are byte-identical; truncation
and a 4409-frame policy reject without output. FFmpeg is not a test dependency.

The video, combined-source and native scene-binding paths are documented below.
Resampling and downmixing are explicitly unsupported, not implicit processing.
Deployment acceptance is separate from offline conversion; `engineVerified`
remains false. See the acceptance audit at the end of this document.

## Video investigation: concrete local candidates, not upload assumptions

Read-only static inspection used the installed RobloxStudioBeta.exe with SHA-256
`a0f2e5dfeaacc86a8329f6e41b8082940a64837dca707899a6a7350a0c9a49bf`.
No Studio process, service or local content tree was modified.

- Function `0x1439f39c0–0x1439f4041` references the diagnostic
  `VideoFrameSampler::open` at instruction `0x1439f3a2f`; a separate close-codecs
  function is `0x1439f2be0–0x1439f2dd4`. These identify a Roblox sampler path,
  not merely a WebRTC class with a similar name.
- Function `0x1434d7150–0x1434d7e1b` references Roblox RVideo's
  `WebmInputFormat::open` diagnostic at `0x1434d71bd`. RTTI independently names
  `WebmInputFormat` in `RBX::RVideo`. Format registration is identifiable at
  `0x1434b57f0–0x1434b586c` through `rvformat_register_all`.
- RVideo-specific codec diagnostics identify VPX software encode/decode and
  Opus/Vorbis implementations; WebM codec identifiers V_VP8, V_VP9, A_OPUS and
  A_VORBIS occur in the executable. These are candidates for further call-path
  inspection, **not proof that every combination is accepted by VideoFrame**.
- The bundled Studio UI file `StudioContent/textures/R15Migrator/start-page-anim.mp4`
  independently probes as H.264 High, YUV420P, 350×200, 20 fps, 320 frames,
  16 seconds. This proves a local UI asset exists, not that the game player
  accepts that profile; Studio UI and engine consumers must not be conflated.

Further tracing identified `0x1434d8a00–0x1434d8b0a`, called from RVideo WebM input-open.
It compares exact codec identifiers and returns distinct values for V_VP8,
V_VP9, V_AV1, A_OPUS and A_VORBIS, with zero for unknown IDs. This establishes
native reader dispatch, not every platform's decoder or VideoFrame acceptance.
No direct sampler-to-RVideo call chain or live playback of our output has been
proved. The output profile targets that identified native reader; it is not an
inference from the unrelated Studio UI MP4 or generic WebRTC codec strings.

## In-process video profile

`roblox convert video SOURCE --config CONFIG --output video.webm` uses linked
FFmpeg/libvpx through Rust `ffmpeg-next`, not a codec executable. Reimplementing
H.264/VP9 would be substantial codec work rather than Roblox packaging work.
Bundle conversion kind `video` emits `video.webm`; scene VideoFrame.VideoContent
uses the existing asset binding mechanism.

Config requires positive `maxWidth`, `maxHeight`, `maxFrames`, `bitrate`, plus
VP9 `crf` in the codec's fixed 0–63 range. Supported inputs are local MP4/MOV or
Matroska/WebM with one silent video stream, known constant frame rate, progressive
8-bit YUV420P and even dimensions. Dimensions/rate are retained and the first
presentation timestamp rebased to zero. Color-space/range/primaries/transfer/
chroma metadata and sample aspect ratio are passed to the encoder. No resizing
or implicit pixel-format/color conversion occurs.

Audio/subtitle/multiple streams, stream side data including rotation, missing
timestamps, variable timing, interlacing, changing dimensions/pixel formats,
decode errors and policy violations fail rather than dropping content. Network
protocols and playlist demuxers are excluded; MOV external data references are
disabled. Frame counts and duration are checked, allowing container timestamp
quantization. Output is atomic/no-clobber after successful trailer writing.
No PID/thread cap is configured.

Builds require FFmpeg development libraries, pkg-config and libclang; runtime
requires compatible libavcodec/libavformat/libavutil and libvpx support. Pin the
OS image and codec versions for reproducible CI. The manifest records linked
library versions. Same-build byte determinism is tested, not guaranteed across
codec upgrades or architectures. Distribution must account for linked library
licenses, including GPL components if present in the system FFmpeg build; this
repository's source license does not override those terms.

Tests decode the VP9 output of an independently encoded synthetic H.264 fixture,
check dimensions/timestamps, deterministic bytes, no-overwrite, limits and
truncation, then run CLI without PATH and bundle/native scene binding. This is a
working **silent-video profile**; combined audio/video conversion follows below.
Other color/timing profiles are explicitly unsupported, and actual Roblox
playback/deployment acceptance is unverified. `engineVerified` stays
false: codec/container correctness is not engine acceptance.

### Pixel fidelity and fractional-rate regression

The synthetic H.264 fixture is decoded alongside its VP9 output and compared
frame-by-frame across Y, U and V planes, stripping decoder stride padding. Each
plane must have MSE below 25 in 8-bit sample units at the fixture's configured
quality. This is a test-specific distortion bound, not a guarantee for arbitrary
content or a hidden production quality threshold. Decoder/demux errors are not
treated as successful end-of-stream by this comparison helper.

A separately encoded 30000/1001 fps fixture exposed two timing defects: the
WebM stream did not retain the nominal frame-rate metadata, and CFR validation
rejected ordinary millisecond timestamp quantization. The writer now explicitly
sets stream time base/rates; validation allows nearest-tick rounding while
requiring strictly increasing timestamps. MP4→WebM→WebM retains six frames and
the original rational rate. Unit tests reject duplicate, dropped, reversed and
irregular timestamps. Timing failures report the frame, PTS, origin and rational
time bases rather than a context-free error.

## Packaging preconverted audio and video

`roblox build media media.json --output combined.webm` packages one existing
VP9 WebM video and one Ogg Vorbis/Opus audio stream without re-encoding. JSON
requires `video`, `audio`, `audioOffsetMillis` (nonnegative integer), and positive
`maxPackets` (total packet policy). Input paths must stay under the specification
directory. Bundle conversion kind `media` takes this specification as its `source`
and produces `video.webm`, usable by native VideoFrame scene bindings.

Inputs retain their existing timeline; the explicit offset delays audio relative
to that timeline. Codec preroll/negative initial audio timestamps and packet side
data are preserved rather than forcibly rebased. Streams may have different
lengths; neither is implicitly truncated, padded or looped. Codec/type mismatches,
extra streams, corrupt packet flags, absent/backwards timestamps and exhausted
packet policy fail. This is packaging of already converted assets, not a general
codec validator or a substitute for source conversion. No network protocol,
codec executable, Studio or cloud service is involved.

Tests prove unchanged compressed packet payloads, audio offset within container
timestamp resolution, deterministic bytes, no-overwrite, CLI/bundle equivalence,
and exact decoded Vorbis samples before/after muxing. An independent Opus fixture
additionally proves preservation of initial codec delay and final padding through
the WebM container. All 9600 decoded samples survive unchanged. Input conversion
and runtime playback remain separate; actual Roblox VideoFrame acceptance is
still unverified. Combined-source conversion is described below.

The mux verification exposed a pre-existing audio-container interoperability
defect: the short Vorbis output's single data page let FFmpeg infer the initial
overlap incorrectly, producing 4154 samples where libvorbis produced 4410.
The audio writer now emits explicit packet-page granules (the libogg zero-size
flush request), making the initial zero granule visible. This is a deliberate
container-layout invariant, not a hidden memory/timeout/quality limit. The
regression requires 4410 decoded samples through FFmpeg both before and after
muxing, while the independent libvorbis tests continue to require the same count.
Compressed sample data is unchanged by muxing; the source writer's Ogg framing
is corrected rather than weakening the sample-count assertion.

## Combined-source conversion

`roblox convert media SOURCE --config CONFIG --output combined.webm` handles a
local MP4/MOV or Matroska/WebM with exactly one video and one audio stream.
The JSON config contains the existing `video` and `audio` policy objects plus
positive `maxPackets` for final muxing. Bundle kind `mediaSource` accepts the
same source/config and emits one combined `video.webm` artifact.

The video uses the same validated VP9 conversion, not a second encoder path.
Audio is decoded in process to temporary lossless float WAV, then uses the same
Vorbis writer and muxer. Mono/stereo planar float32 decoder output is supported;
other formats, changing rates/channel counts and discontinuous timestamps fail.
There is no implicit downmix or resampling. The source's actual video origin is
recorded; the audio origin determines its mux offset, rounded to milliseconds.
Audio beginning before video is explicitly unsupported, rather than truncated.

AAC may decode a padded final block beyond the declared stream sample duration.
Only that codec's final excess is cropped; the manifest reports
`sourceAudioPaddingFramesRemoved`. Other duration mismatches fail. A fixture
with 8820 real audio samples requires removal of 396 padded samples. Independent
final WebM decoding verifies all 8820 samples and a waveform MSE below 0.0001
against the synthetic sine; the four video frames, rate and both codecs are
retained. CLI-without-PATH, deterministic repeat, bundle equivalence, no-overwrite
and policy failure tests pass. Intermediate files are automatically removed.
These are offline codec/container tests, not Roblox runtime acceptance.

## Upload-source formats are a different contract

Checked against official documentation on September 25, 2026:

- [Audio imports](https://create.roblox.com/docs/audio/assets) admit MP3, Ogg,
  WAV and FLAC. The documentation explicitly describes transcoding during
  Studio import. Acceptance of Ogg as an upload source therefore does not prove
  byte-preserving publication of our Vorbis output.
- [Video uploads](https://create.roblox.com/docs/ui/video-frames) admit MP4 and
  MOV, not our WebM output. The native RVideo reader evidence above does not
  override this public upload contract. Uploading the original MP4 instead would
  move conversion back to Roblox; we do not silently do that.

The local converters transcode to their chosen output profiles; the media muxer
copies already converted packets without transcoding. Neither operation requires
Roblox servers. Determining a supported publication route for preconverted media
belongs to issue 9 and remains unfinished. No assets were uploaded for this audit.

## Issue 6 acceptance audit

Audited implementation `928b6e1`, with its passing 144-test source suite reused.
All 16 media integration tests passed again with network access disabled:

```sh
unshare --user --map-root-user --net cargo test --offline --locked \
  --test audio --test video --test media_mux --test media_transcode
```

| Requirement | Evidence |
| --- | --- |
| Distinguish native consumption from upload inputs and identify transcoding | Installed native audio files; hashed binary's RVideo WebM reader and exact codec dispatch above; official upload contracts explicitly distinguished. Local conversion versus packet-copy paths documented. |
| Specify project-independent audio/video profiles | Mono/stereo Vorbis, progressive even-sized YUV420P CFR VP9, and combined VP9/Vorbis; explicit JSON quality/resource policy, retained rates and dimensions, no project names or project-specific limits. |
| In-process codecs, justify external tools | Symphonia/libvorbis and linked FFmpeg/libvpx; no codec executable used by conversion. Codec implementation cost and native dependencies/licenses documented above. |
| Metadata, duration, malformed inputs and deterministic packaging | `tests/audio.rs`, `tests/video.rs`, `tests/media_mux.rs`, `tests/media_transcode.rs`: independent source fixtures, decoded audio/pixel comparisons, fractional frame rates, priming/padding, packet identity, offsets, limits, corruption, no-overwrite, repeated bytes and CLI/bundle equivalence. |
| No server-side build conversion | All 16 integration tests pass in an isolated network namespace; CLI tests clear the environment, including PATH and credentials. Native scene bindings are serialized locally. |
| Explicit unsupported formats | Profiles above reject unsupported channel/sample/pixel layouts, timing, extra streams, transforms and external references rather than silently discard or reinterpret them. No promise of arbitrary FFmpeg input support. |

**Verdict: the issue's offline profile implementation and validation requirements
are satisfied.** This is not proof of live VideoFrame playback, every platform's
codec support, or publication of preconverted bytes. Those claims remain false
or unverified; manifests continue to report `engineVerified: false`. The overall
conversion goal remains open, including issue 9's deployment boundary.
