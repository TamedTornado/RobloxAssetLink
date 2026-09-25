# DDS import/upload rejection: executable investigation

## Finding

The tested failure is an **image-import/upload admission boundary**, not a
demonstrated defect in our DDS header or compression settings. Studio contains
a DDS runtime reader, but the traced bulk-import image classifier excludes DDS.
The same cloud Image endpoint also rejects an unchanged DDS shipped by Roblox.

This report traces the importer, not merely the earlier runtime reader. It does
not claim that a client executable reveals the cloud validator's source code or
that every possible private Roblox upload interface has been exhaustively tested.

## Exact executable

Installed Studio `0.740.19.7400931`, deployment
`version-c792f79abddd41bd`, executable SHA-256:
`a0f2e5dfeaacc86a8329f6e41b8082940a64837dca707899a6a7350a0c9a49bf`.
Addresses below are image virtual addresses. Inspection was read-only: no
executable patch, process injection, capability bypass, or feature-flag change.

## The real import gate

1. Initializer `0x14061d1a0` constructs the image suffix set at
   `0x14d92bcf8`. It supplies exactly six QString literals, stored at
   `0x14c427638`, `0x14c427658`, `0x14c427678`, `0x14c4276a0`,
   `0x14c4276c0`, and `0x14c4276e0`.
2. Decoding those Qt UTF-16 literal payloads gives **bmp, gif, jpeg, jpg, png,
   tga**. DDS is absent. This is an executable lookup table, not just a file
   dialog's presentation filter.
3. Classifier `0x1462b6b30` lowercases its input using imported
   `QString::toLower` at IAT `0x14843a2e0`, then checks that set. An image match
   returns asset-type value 1 at `0x1462b6bc9`.
4. The texture-check path in `0x146310910` obtains the actual filename suffix
   through `QFileInfo::suffix` at `0x146311128` (IAT `0x14843b428`), calls the
   classifier at `0x146311139`, and checks the result at `0x146311140`.
   Its unsupported branch references
   `Studio.App.UploadMeshRequest.TextureFormatIsUnsupportedPlease` at
   `0x146311173`, followed by the unsupported-texture error presentation.
5. Separately, `UploadImageRequest::run` is identified by its own log string in
   `0x14630ef80–0x14630fdac`. It opens the source file, builds an upload request,
   and dispatches modern/older upload paths. The older request construction
   helper `0x14630ed30` sets the Image asset type to 1. Merely having a DDS
   decoder elsewhere in the executable does not add DDS to the import set.

For corroboration only, the UI's image filter initialized in `0x140654fe0`
also lists `*.bmp *.gif *.jpeg *.jpg *.png *.tga`. The lookup-table and caller
trace above are the stronger evidence. The generic string `Unsupported image
format` at `0x148ae3d40` belongs to a capture-related region and is not treated
as evidence identifying the upload validator.

## Independent control: Roblox's own DDS fails the same upload

The test uploaded the **unchanged** installed file
`content/textures/noise.dds` as Image, with its honest MIME type
`image/vnd-ms.dds` and expectedPrice 0. No rename, header modification, conversion
or alternate MIME disguise was used.

- SHA-256: `2a7ce8c77df2a3fb99ba70630019c33976a3714b7d17a103f60c55c488e72435`.
- 32 × 32, uncompressed 32-bit ARGB8888, one mip level; legacy DDS header.
- Pixel-format flags 0x41, with RGB masks 0x00ff0000/0x0000ff00/0x000000ff
  and alpha mask 0xff000000. The installed file includes NVTT/UVER metadata.
- Operation `485a1564-6885-4228-9f96-b811d4841ffd` completed with
  `InvalidArgument / Unsupported image format.` No asset was created.

This is not our encoder failing a self-authored test. The cloud rejected the
vendor's own intact runtime DDS too, at dimensions well below any relevant
size limit. Earlier probes also showed that Studio loads our BC4, L8 and RGBA8
DDS outputs, while the Image upload endpoint rejects them. See
[the acceptance report](cloud-acceptance-2026-09-25.md) and
[native DDS reader constraints](texture-conversion.md#dds-reader-constraints-executable-and-live-boundary-checks).

## Consequence for the toolchain

- Do not change DDS masks, mip counts, dimensions or compression merely to
  chase this Image-upload error. The evidence identifies a route/format mismatch.
- Keep DDS as a supported local/native output where the consuming path supports
  it. Runtime decoding and creator-image upload acceptance are separate contracts.
- The demonstrated Image publication route is our locally produced PNG profile.
  Selecting that profile must be explicit; do not silently decode/re-encode a
  requested DDS or disguise its bytes as PNG.
- This does not prove a native DDS publication route exists. No such route has
  been verified. Any future alternative must pass its own admission and runtime
  checks before being advertised.

Jason excluded video uploading from the current task; ID verification is deferred
and is no longer a completion blocker for this work.
