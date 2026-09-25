# Fractional-rate synthetic video fixture

`video-ntsc.mp4` contains six original synthetic 16×16 YUV420P H.264 test-pattern
frames at 30000/1001 fps, without audio or external footage. Distributed under
this repository's MIT license. Independently encoded using FFmpeg 6.1.1-3ubuntu5:

```sh
ffmpeg -v error -f lavfi -i 'testsrc2=size=16x16:rate=30000/1001' \
  -frames:v 6 -an -c:v libx264 -pix_fmt yuv420p -map_metadata -1 \
  -fflags +bitexact -flags:v +bitexact -n tests/fixtures/video-ntsc.mp4
```

SHA-256: `ddcafa9b391ed1f44e7a949c0babfac318fed20a11cc208afa0ef8ab9edc2a7d`.
FFprobe reports time base 1/30000 and duration 6006 ticks (0.2002 seconds).
The FFmpeg executable is not needed to run the test.
