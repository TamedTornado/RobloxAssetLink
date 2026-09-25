# Local image conversion — issue 5

`roblox convert texture SOURCE --config CONFIG --output NEW_DIRECTORY` decodes
PNG/JPEG/BMP/TGA in process and writes deterministic PNG artifacts plus hashes.
This is not a Roblox cloud upload or a Studio call.

The JSON configuration requires `operation`, `maxWidth`, `maxHeight` and
`maxDecodedBytes`. These are caller policy, not hidden implementation limits.
The allocation limit applies to the decoder, not an OS process-memory quota.
No resizing is performed. Unsupported high bit depths, ICC profiles and EXIF
orientation fail explicitly rather than silently changing their meaning.

Output defaults to PNG. Linear roughness/metalness maps can explicitly select
`"output":{"format":"ddsL8","mipmaps":true,"maxOutputBytes":1048576}`.
The budget is caller-supplied and applies to each emitted DDS including its header
and every mip level. This profile emits native uncompressed 8-bit luminance DDS,
not a compressed TexturePack. Setting `mipmaps` false emits only the base level.
Color and normal maps reject this scalar-only output profile.

Mip levels use linear area-weighted scalar averaging with nearest-integer
quantization at each level. Odd dimensions include all edge texels; dimensions
halve (floor, minimum one) through 1 × 1. No color gamma, normal renormalization,
or guessed normal-channel swizzle is applied to these scalar maps. Independent
FFmpeg decoding checks the base level; format-header and exact mip-byte tests
check the chain, deterministic bytes and configured budget. Material/bundle tests
verify native local DDS references and atomic failure cleanup. Renderer acceptance
is still unverified.

Color and normal operations can select uncompressed `"format":"ddsRgba8"`,
with required `mipmaps`, `maxOutputBytes`, and `mipFilter`. Available filters:

- `colorStraightAlpha`: decode RGB from sRGB, area-average in linear space,
  encode back to sRGB; average alpha independently. Preserves hidden color's
  contribution, useful for tint/overlay semantics.
- `colorPremultipliedAlpha`: weight linear RGB by alpha, average, then unpremultiply
  before storing straight-alpha RGBA. Fully transparent mip texels become black.
  This prevents transparent pixels' hidden RGB from bleeding into visible edges.
- `normal`: average tangent-space vectors then normalize and encode back to RGB.
  Exactly cancelling vectors fail rather than inventing a direction. Normal-map
  input conversion still handles the explicit DirectX/OpenGL convention first.

Color operations require a color filter; normal operations require `normal`.
The filter is required even with mipmaps disabled so enabling them cannot silently
pick a policy. Base pixels are retained exactly; filtering/quantization applies
only to lower levels. The DDS uses ordinary RGBA8 masks and no proprietary swizzle.
Its legacy header does not carry an sRGB tag; interpretation remains the material
map's semantic, as recorded in the texture manifest. Independent FFmpeg decoding
checks base RGBA/alpha; known-value tests cover gamma, alpha policy and normal
renormalization. Color/normal block compression remains unfinished.

The same scalar operations also accept `"format":"ddsBc4"`, with the same
required `mipmaps` and `maxOutputBytes` fields. This emits unsigned BC4 blocks
under the native reader's ATI1 FOURCC. Each 4 × 4 block stores min/max scalar
endpoints and nearest interpolated palette indices; constant blocks are exact.
This is an explicitly lossy range-fit profile, not an optimal endpoint search.
Partial edge blocks replicate their final row/column; mip tail levels still
occupy a complete eight-byte block. Mips are generated from uncompressed scalar
samples before compression, never from already lossy blocks. Tests independently
decode BC4 through FFmpeg, check non-block-aligned dimensions and sample error,
exact constant-block bytes, mip-tail sizes, repeatability and budget rejection.
No compression library, external executable or cloud cooking is involved.

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

Mesh/material dependency integration and the local TexturePack profile are
implemented below. Broader platform-cache profiles remain unverified. Color
management and HDR remain explicitly unsupported profiles. Passing pixel tests
does not close the full image/material issue.

## Native material assembly

`roblox convert material material.json --output NEW_DIRECTORY` builds a native
`material.rbxm` SurfaceAppearance, normalized maps and a dependency/hash manifest.
It uses the documented [SurfaceAppearance properties](https://create.roblox.com/docs/reference/engine/classes/SurfaceAppearance)
and the pinned reflection database for serialization and AlphaMode values.

It also emits `texturepack.xml`, a native unlayered v2 descriptor, and sets the
serialized `SurfaceAppearance.TexturePack` ContentId to that local artifact.
The descriptor uses the same map references and alpha mode as the instance.
Its hash/reference is recorded under `texturePack` in the material manifest.
Standalone, glTF, mesh-material and bundle paths include the descriptor;
bundle relinking happens before encoding. No runtime script or Studio
preprocessing step generates this file.

`verify-bundle` checks descriptor hashes and resolves their channel references.
It accepts our canonical local descriptor profile, not arbitrary XML, DTDs,
remote content, layered packs or alternate usages. A regression changes a
descriptor reference and updates its file hash; verification still rejects the
dangling dependency. This is not a renderer or publishing attestation.

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

Optional `outputs` selects the final encoding independently for `color`, `normal`,
`roughness` and `metalness`. When provided, all four fields are required, each
using a texture output object such as `{"format":"png"}` or the explicit DDS
profiles above. If omitted, all maps use PNG. The same `outputs` object is accepted
inside static mesh conversion's `materials` policy, so a GLB-to-mesh build does not
discard the caller's texture choices. CLI, bundle and material dependency hashes
operate on the chosen final artifacts. Material baking still uses temporary PNG
intermediates for exact normalized samples; these are not published dependencies.

For example, color can use RGBA DDS with a selected alpha filter, roughness BC4,
metalness L8, and normals PNG. Incompatible operation/filter combinations fail in
the shared texture converter rather than being reinterpreted. Source material
semantics and sampler restrictions below remain unchanged by output selection.

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
and native acceptance require separate verification. The implemented writer uses
only the concrete SurfaceAppearance profile below, not unknown usage enums or
guessed image-compression steps.

### SurfaceAppearance-specific descriptor path

Further read-only tracing of the same executable narrows the descriptor contract:

- The v2 parser accepts unsigned `usage` values 0–4 (`0x1473138ec`), alpha-mode
  values 0–3 (`0x147313973`), and tiling values 0–2 (`0x1473139c6`); larger values
  branch to failure. These are protocol bounds, not project resource policy.
- The writer conditionally emits a `version` attribute on `usage`, using a table
  at `0x14c3dded0`. The first five entries are 0, 0, 0, 1, 0. This version is
  separate from the outer `texturepack_version` value of 2. A generic writer
  cannot treat every usage as having identical semantics.
- `0x1441630e0` constructs descriptor data via `0x143939ce0`, passing a zero
  usage-version selector at `0x144163123`. It then calls the XML serialization
  wrapper at `0x144163282` when the alternate checked-serialization feature is
  disabled; the other path calls `0x147314900`.
- Inside `0x143939ce0`, the type test calls the independently identified
  SurfaceAppearance class descriptor `0x140f6d110` at `0x143939d75`.
  Its SurfaceAppearance branch copies the four base map content references and
  the emissive reference, sets usage to **0** (`0x14393a096`), copies instance
  field `0x208` to the descriptor alpha field, and sets tiling to **0**
  (`0x14393a0a3`). This is stronger evidence for a SurfaceAppearance profile than
  choosing enum values because the parser happens to accept them. The alpha
  field's public-property mapping was subsequently confirmed below.
- The named UGC validation callback `0x14408a7a0` calls this same v2 parser at
  `0x14408a955`. It compares the parsed base/emissive content references against
  SurfaceAppearance getters (`0x14408a9da–0x14408aa50`) and requires no layers.
  This establishes an actual SurfaceAppearance descriptor consumer, but does not
  prove rendering of locally generated descriptors or byte-preserving upload.

Follow-up checks used for implementation:

- AlphaMode registration `0x1402ee920` binds the SurfaceAppearance class descriptor
  and getter `0x1409b4fd0`. The getter reads offset `0x140` of the property
  subobject. Setter `0x140f6ded0` shows its `0xc8` adjustment to the full instance:
  `0x140 + 0xc8 = 0x208`, matching the descriptor builder's copied field.
- Channel writer `0x147315090` emits the channel tag, content string and closing
  tag. Ordinary URI content is copied at `0x147315327–0x14731533b`; no nested
  `url` element is inserted. Our writer requires safe ASCII path segments,
  excluding XML metacharacters rather than implementing arbitrary escaping.
- Channel parser `0x147314530` reads element text and constructs native content;
  it is not a packed pixel decoder. The outer closing string is `</roblox>`
  without a trailing newline. Golden-layout tests follow these inspected bytes.
- Native RBXM roundtrip tests confirm the pinned serializer retains `TexturePack`
  ContentId. CLI repeatability, bundle descriptor/map linking and malformed or
  dangling-reference tests cover the implemented local profile.

No process was launched or modified to obtain this evidence; live engine
acceptance and publication of the descriptor remain unverified.

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
texture pipeline. The scalar DDS/mipmap profile above starts the native-output
implementation alongside the uncompressed RGBA profile; compressed color/normal
profiles remain unfinished. Do not silently replace PNGs with a
guessed platform cache. Keep TexturePack material descriptors separate from pixel
encoding. Further evidence is needed for the marked normal encoding and the
renderer/TexturePack relationship. Issue 5 remains open.
