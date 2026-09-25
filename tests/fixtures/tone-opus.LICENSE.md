# Opus padding fixture

`tone-opus.ogg` is an Opus encoding of this repository's original synthetic
`tone-22050.flac` test signal, under the repository's MIT license. There is no
third-party recording content. The decoded signal is 9600 mono samples at 48 kHz.

Independently generated using FFmpeg 6.1.1-3ubuntu5:

```sh
ffmpeg -v error -i tests/fixtures/tone-22050.flac -c:a libopus -ar 48000 \
  -b:a 32000 -map_metadata -1 -fflags +bitexact -flags:a +bitexact \
  -n tests/fixtures/tone-opus.ogg
```

SHA-256: `ae51c1cd14a6341af0cac05effc741736c188ae992a0a73af98da8301c661007`.
Tests use the checked-in fixture and linked libraries, not the FFmpeg executable.
