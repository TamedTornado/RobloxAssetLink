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

Still open: broader source regressions (including FLAC), explicit resampling or
downmix policy if needed, video codec/container requirements and conversion,
native asset binding and deployment acceptance. `engineVerified` remains false.
Issue 6 is not complete merely because the audio sub-pipeline works.
