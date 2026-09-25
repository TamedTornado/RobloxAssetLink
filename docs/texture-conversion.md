# Local image conversion — issue 5

`roblox convert texture SOURCE --config CONFIG --output NEW_DIRECTORY` decodes
PNG/JPEG/BMP/TGA in process and writes deterministic PNG artifacts plus hashes.
This is not a Roblox cloud upload or a Studio call.

The JSON configuration requires `operation`, `maxWidth`, `maxHeight` and
`maxDecodedBytes`. These are caller policy, not hidden implementation limits.
The allocation limit applies to the decoder, not an OS process-memory quota.
No resizing is performed. Unsupported high bit depths, ICC profiles and EXIF
orientation fail explicitly rather than silently changing their meaning.

Operations:

- `color`: preserve 8-bit RGBA, including alpha; input samples must already be
  intended for sRGB color-map interpretation. This is not color management.
- `normalOpenGl`: RGB channel preservation for tangent-space normal maps.
- `normalDirectX`: invert the green channel to OpenGL convention.
- `gltfMetallicRoughness`: split green into roughness and blue into metalness,
  emitting separate 8-bit grayscale maps. Samples stay linear; no gamma operation.

Format/semantic evidence: [Roblox texture specifications](https://create.roblox.com/docs/art/modeling/texture-specifications)
and [SurfaceAppearance](https://create.roblox.com/docs/art/modeling/surface-appearance).
These require OpenGL tangent normals and grayscale metalness/roughness maps.
PNG, DDS, MP3 and Ogg samples also exist in the locally installed Studio content;
their presence is not proof of cloud upload behavior or of every client target.

Still open: mesh/material dependency integration, additional material semantics,
color-managed/HDR handling, mipmap/platform-cache requirements and native scene
acceptance. The command does not yet build SurfaceAppearance instances or a
TexturePack. Passing pixel tests does not close the full image/material issue.
