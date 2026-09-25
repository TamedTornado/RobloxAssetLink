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

## Installed executable: TexturePack is not itself a pixel codec

Static inspection on September 25 used RobloxStudioBeta.exe with SHA-256
`a0f2e5dfeaacc86a8329f6e41b8082940a64837dca707899a6a7350a0c9a49bf`.
No Studio execution, injection or cloud request was involved.

- Function `0x1473149d0` writes XML beginning with `roblox` and
  `texturepack_version` 2, followed by numeric usage, alpha mode and tiling.
- Its six-entry channel-name table at `0x14c4644f0` contains `color`, `normal`,
  `metalness`, `roughness`, `emissive` and `height`. It visits six base channels
  and optional six-channel layers. Helper `0x147315090` writes the channel tag
  and its content reference, followed by the closing tag.
- Parser `0x1473136f0` looks up `texturepack_version` and compares it with 2 at
  `0x14731389c`. This is concrete parser/writer evidence, not just a string match.
- The DDS reader is a separate function, `0x1448f97d0`, with specific invalid
  header/dimension/DX10-format error paths.

Therefore the observed TexturePack v2 path describes texture references and
material usage rather than packing all pixels into a proprietary image codec.
This does not establish every referenced image's runtime encoding. Usage-version
semantics, accepted content-reference forms, target-specific texture processing
and native acceptance still need verification before shipping a writer. No
unknown enum meanings or image-compression steps have been guessed into code.
