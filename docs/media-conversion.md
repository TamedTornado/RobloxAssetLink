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

Still open: broader source regressions, explicit resampling or
downmix policy if needed, video codec/container requirements and conversion,
native asset binding and deployment acceptance. `engineVerified` remains false.
Issue 6 is not complete merely because the audio sub-pipeline works.

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

The next video implementation decision must distinguish the Roblox sampler's
actual format/codec dispatch from Studio UI playback and from generic WebRTC.
WebM/VPX is a concrete candidate now, but a reader/dispatch trace and known-good
consumer sample are still needed before claiming a native target profile.
Further tracing identified `0x1434d8a00–0x1434d8b0a`, called from WebM input-open.
It compares exact codec identifiers and returns distinct values for V_VP8,
V_VP9, V_AV1, A_OPUS and A_VORBIS, with zero for unknown IDs. This establishes
reader dispatch, not every platform's decoder or VideoFrame acceptance.

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
working **silent-video profile**, not completion of general video conversion.
Direct conversion of source videos containing audio, broader color/timing profiles and
actual Roblox playback/deployment acceptance remain open. `engineVerified` stays
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
and runtime playback remain separate: combined MP4 audio/video source conversion
and actual Roblox VideoFrame acceptance are still unverified/unfinished.

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
