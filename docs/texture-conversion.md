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
- `roughness` / `metalness`: standalone linear scalar maps. Require equal RGB
  components and opaque alpha, preserving the scalar exactly rather than applying
  an implicit luminance conversion.

Format/semantic evidence: [Roblox texture specifications](https://create.roblox.com/docs/art/modeling/texture-specifications)
and [SurfaceAppearance](https://create.roblox.com/docs/art/modeling/surface-appearance).
These require OpenGL tangent normals and grayscale metalness/roughness maps.
PNG, DDS, MP3 and Ogg samples also exist in the locally installed Studio content;
their presence is not proof of cloud upload behavior or of every client target.

Mesh/material dependency integration is implemented below. Still open are
mipmap/platform-cache requirements and TexturePack encoding/semantics. Color
management and HDR remain explicitly unsupported profiles. Passing pixel tests
does not close the full image/material issue.

## Native material assembly

`roblox convert material material.json --output NEW_DIRECTORY` builds a native
`material.rbxm` SurfaceAppearance, normalized maps and a dependency/hash manifest.
It uses the documented [SurfaceAppearance properties](https://create.roblox.com/docs/reference/engine/classes/SurfaceAppearance)
and the pinned reflection database for serialization and AlphaMode values.

The specification requires `name`, `alphaMode` (for example `Transparency`),
`color` (three native Color components in [0,1]), `localUriPrefix` and `maps`.
Each map has a contained relative `source` and the texture `config` described
above. A packed metallic/roughness input supplies both maps; conflicting inputs
for one semantic fail. Empty maps allow an explicitly untextured appearance.
Unknown fields fail. Color is a native tint, **not** a glTF linear base-color
factor; this command does not silently approximate source material shaders.

`localUriPrefix`, for example `rbxasset://kit/paint/`, declares the mount location
of the output directory. It is not a filesystem path or an automatic Roblox
content installation. Map references are serialized with this prefix; moving
outputs requires relinking. Only safe local URI segments are accepted. The
command never uploads, installs content into Studio or manufactures remote IDs.
In a bundle, the linker replaces this standalone prefix with the asset's actual
bundle-local directory before native serialization; the source JSON is unchanged.
Core glTF source-material extraction is described below. Static glTF mesh
conversion can also associate primitives with their converted materials through
the explicit policy in [mesh conversion](offline-conversion.md). FBX material
extraction remains outstanding.

Tests independently decode the native instance properties, check exact channel
values, compare repeated CLI outputs byte-for-byte, and cover conflicting maps,
invalid enums, paths, non-grayscale scalar inputs, configured decode limits,
rollback and no-overwrite behavior. `engineVerified` remains false: native
serialization correctness is not proof that the renderer accepts raw map
references without additional TexturePack/cache processing.

## glTF/GLB source material import

`roblox convert material-gltf SOURCE --config CONFIG --output NEW_DIRECTORY`
extracts one selected core metallic/roughness material. Configuration requires
`materialIndex`, `name`, `localUriPrefix`, `maxWidth`, `maxHeight` and
`maxDecodedBytes`. Bundle conversion kind `materialGltf` accepts the same source
and config, relinks dependencies, and supports the same scene material attachment.

Following the [glTF material specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#materials),
base RGB samples are decoded from sRGB, multiplied by linear base-color factors,
then encoded back to 8-bit sRGB. Alpha is multiplied without gamma conversion;
OPAQUE forces full coverage and BLEND preserves the multiplied coverage. Native
Transparency mode is used with the resulting alpha map. Roughness/metalness use
G/B samples multiplied by their factors, without gamma conversion. Missing maps
produce constant 1×1 textures. Each bake rounds to 8-bit; it is not lossless for
arbitrary floating-point factors. Native Color is white to avoid a second tint.
OpenGL normal RGB samples are preserved independently of the base-color factors.

External local images, PNG/JPEG base64 data URIs and GLB buffer-view images work
without network access. Decode policies and ICC/HDR/orientation rejection are
shared with texture conversion. Image paths cannot escape the source directory.
Temporary intermediates are automatically removed; source files are untouched.

Unsupported semantics fail: material extensions, occlusion/emission, alpha MASK,
double-sided geometry, non-UV0 bindings, nonunit normal scale, nonrepeat wrapping
and non-linear sampler settings. There is no implicit shader approximation or
UV-set reassignment. Double-sidedness and vertex-color multiplication require
geometry integration; importing this material alone does not build its source
mesh. The static mesh importer accepts supported textured source materials when
its explicit material-conversion policy is configured.

Tests cover exact known factor results (including gamma-sensitive values), alpha,
normal samples, image encodings, malformed/unsupported inputs, configured limits,
repeatability, CLI and bundle references, plus the independently authored
Khronos RiggedSimple material fixture. These do not prove renderer parity.

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

## PC native texture inventory and mip chains

Read-only inspection of the same installed version's `PlatformContent` found
31 DDS files, all under `pc`. They are runtime/platform assets, not evidence that
the upload API accepts DDS. Their standard headers provide more specific evidence
than the mere presence of filenames:

| Representative path under `PlatformContent/pc/textures` | Encoding | Dimensions | Stored mip count | Matching files |
| --- | --- | --- | --- | --- |
| `plastic/diffuse.dds` | DXT1 | 128 × 2048 | 12 | 2 |
| `plastic/normal.dds` | DXT5 with additional metadata described below | 128 × 2048 | 12 | 2 |
| `water/normal_01.dds` | DXT5 | 256 × 256 | 9 | 25 |
| `brdfLUT.dds` | DX10, DXGI 34 (R16G16_FLOAT) | 256 × 256 | 1 | 1 |
| `wangIndex.dds` | 16-bit luminance/alpha, masks 0xff / 0xff00 | 128 × 128 | 0 (base image) | 1 |

Counts group identical pixel-format headers, not identical dimensions or content.
The diffuse, plastic-normal and water-normal byte sizes are respectively 174984,
349840 and 87536. These exactly match a 128-byte header plus complete block-
compressed mip chains down to 1 × 1 under the documented
[DDS layout](https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-file-layout-for-textures).
The LUT is 262292 bytes: 148-byte DX10 header plus 256 × 256 × 4 pixel bytes.
[DXGI 34](https://learn.microsoft.com/en-us/windows/win32/api/dxgiformat/ne-dxgiformat-dxgi_format)
denotes two half-float components. The index file is 32896 bytes: a 128-byte
header plus 128 × 128 × 2 bytes. No external images are copied into this repo.

Representative SHA-256 hashes, in table order:

- `7a8f26459881d30aab83e8a47cf01871a6110514cd77ac221703586f3abf76c0`
- `b36dc03dde37e5687214699e212d7f6957c20c27c6cdb1b7e2728396f868ff7d`
- `cf6a9349b316f78158f345094a456e52c70f929048f7c0c0a3a8eebff8517cc5`
- `5cc50688061dc9ed221d41591a129e6037db5b7d836532081cdad03a6559d004`
- `2b1bd51ec8cc19a918fe5c6578c02db9568533e20e3e31909e8fc85a8706b681`

The plastic normal header has pixel flags `0x80000004` and the bytes `A2D5`
where ordinary FOURCC DDS leaves the RGB bit-count field unused. Its meaning
has not been established. Do not treat its compressed channels as ordinary
OpenGL RGB normals merely because its main FOURCC is DXT5. The water normal
files do not have this extra marker. A universal normal-map compression recipe
would therefore be premature.

The inspected native DDS reader checks the magic at `0x1448f9983`, reads the
DX10 extension when present, dispatches legacy DXT1/3/5 and ATI1/2 at
`0x1448f9c31–0x1448f9c94`, and handles uncompressed scalar/two-channel layouts
at `0x1448f9b6a–0x1448f9c22`. Mip-count handling at
`0x1448f9d44–0x1448f9dbd` distinguishes absent/base-only from supplied chains;
later branches select mip levels using caller flags. These observations establish
a real native DDS ingestion path, not that every surface must use DDS or that
every device supports the same payload.

**Implementation consequence:** PNG normalization is not yet the whole offline
texture pipeline. Add explicitly selected native DDS/mipmap outputs with semantic
filtering and independent decoding tests; do not silently replace PNGs with a
guessed platform cache. Keep TexturePack material descriptors separate from pixel
encoding. Further evidence is needed for the marked normal encoding and the
renderer/TexturePack relationship. Issue 5 remains open.
