# Combined synthetic H.264/AAC fixture

`video-aac.mp4` contains an original FFmpeg test pattern and synthetic 440 Hz
sine wave; no external footage or recording. Distributed under this repository's
MIT license. Four 64×48 video frames at 10 fps accompany 0.4 seconds of mono AAC
at 22050 Hz. Generated independently with FFmpeg 6.1.1-3ubuntu5:

```sh
ffmpeg -v error -f lavfi -i 'testsrc2=size=64x48:rate=10:duration=0.4' \
  -f lavfi -i 'sine=frequency=440:sample_rate=22050:duration=0.4' \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 64000 -map_metadata -1 \
  -fflags +bitexact -flags:v +bitexact -flags:a +bitexact \
  -n tests/fixtures/video-aac.mp4
```

SHA-256: `9154927c75d176540ffc7a66c4e74732dd5c43af897d62509fbb83b1e4380147`.
The FFmpeg executable is only a fixture-authoring tool, not a test dependency.
