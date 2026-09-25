# Synthetic video fixture

`video-64x48.mp4` is an original synthetic FFmpeg test pattern, distributed under
this repository's MIT license. It contains no external footage: four silent
64×48 progressive YUV420P H.264 frames at 10 fps.

Generated independently using FFmpeg 6.1.1-3ubuntu5:

```sh
ffmpeg -v error -f lavfi -i 'testsrc2=size=64x48:rate=10:duration=0.4' \
  -an -c:v libx264 -pix_fmt yuv420p -map_metadata -1 \
  -fflags +bitexact -flags:v +bitexact -n tests/fixtures/video-64x48.mp4
```

SHA-256: `e136221cfd90d7905f5b71069b097cc6c91baa02e92970e661f9f1fcaf95b09a`.
The executable is needed only for fixture generation, not to run tests.
