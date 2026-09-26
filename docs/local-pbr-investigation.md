# Local PBR iteration investigation

## Native evidence (September 25, 2026)

Inspected the installed Studio 0.740.19.7400931 executable, SHA-256
`a0f2e5dfeaacc86a8329f6e41b8082940a64837dca707899a6a7350a0c9a49bf`.
This is read-only format research: no executable patches, injected code,
permission changes or feature-flag overrides.

Later diagnostics below used hardware breakpoints and one temporary rendering
flag, restored afterward. No executable code or authentication/security controls
were modified.

### Local runtime packs are not uploaded XML descriptors

Both renderer implementations explicitly handle local content (native content
kind 3). At `0x1448187d4–0x14481880e`, the local branch constructs
`%s%s.ktx`, using a three-entry suffix table at `0x14c3fdf48`:
`color`, `normal`, `specular`. The newer implementation repeats this at
`0x1448256c2–0x1448256f1`. Both iterate over three texture slots.

Thus a local TexturePack reference supplies a filename prefix; the renderer
requests three KTX files. It does not treat that reference as the small XML
descriptor accepted by the cloud TexturePack upload route. The initial local
XML probe exercised the wrong representation.

The KTX reader at `0x1448fb4b0` supports uncompressed RGBA8: its checks at
`0x1448fb759–0x1448fb78c` include unsigned-byte type `0x1401`, type size 1,
RGBA8 internal format `0x8058`, and RGBA `0x1908`. Container structure comes
from the [Khronos KTX 1 specification](https://registry.khronos.org/KTX/specs/1.0/ktxspec.v1.html).
This supplies an uncompressed diagnostic profile, not a compression-parity claim.

### Channel layout from the installed shaders

The installed `shaders/shaders_glsl3.pack` contains readable compiled GLSL.
Its SHA-256 is
`5f158487a657cd682bbf25b0a1dbbcaf8eaf9ab24c683693c021bc3a346f35c2`.
The SurfaceAppearance fragment variant identifies its `SurfaceAppearanceConsts`
and samples DiffuseMapTexture, NormalMapTexture and SpecularMapTexture:

- Color texture: RGB color, alpha opacity/overlay/tint mask.
- Normal texture: X in alpha, Y in green; shader reconstructs positive Z.
  Ordinary RGB normal-map bytes cannot simply be passed through unchanged.
- Specular texture: red metalness, green roughness, blue emissive mask.

These are observations of this renderer variant, not a promise that every
platform/codec profile shares the same physical channel encoding.

### The source-map warning is a different boundary

The local-source-map message is referenced by `0x14413f7b0`,
`0x144141590` and `0x144bf71f0`. The first dispatches on content kind and
emits the warning for kind 3; the renderer nevertheless has its own explicit
local pack loader. Do not conclude that all local PBR rendering is impossible
from this warning alone.

The invalid-source-pack error is emitted in `0x143931890` and `0x1439329b0`.
The generator validates its source references before producing/uploading the
descriptor. This is not evidence of invalid DDS pixel data.

## Live probes so far

- A locally built place with a locally built tangent sphere rendered local PNG
  and RGBA8 DDS color textures through MeshPart.TextureID in actual play mode.
- Local PNG/DDS source maps plus local XML TexturePack references rendered gray.
- A pack-only scene with the derived KTX prefix and no source-map properties
  also rendered gray. The KTX naming discovery alone is not acceptance proof.
- One Studio startup hung before test execution. Its owned process was
  terminated; a fresh launch ran. This was not classified as a format failure.

All test meshes/maps were local files. No asset uploads or place publications
were made. Studio itself still runs its usual account/built-in service traffic;
these tests are not a network-isolated Studio proof.

Further static-pack probes confirmed that all three KTX images pass Studio's
image decoder (`PreloadAsync` reports Success). Serialization immediately after
loading retains the local pack prefix and source-map references. Both legacy
TexturePack and modern TexturePackContent variants remain gray. An `openat`
trace showed the three explicitly requested decoder-control images being opened,
but no other pack files requested by the material renderer. This locates the
remaining static-pack problem before image decoding; the exact material
selection/invalidation cause is not yet established.

## Local full-PBR rendering proved (September 26, 2026)

Studio's existing local-image API provides a working path without uploading the
material sources:

1. Load each local PNG with `AssetService:CreateEditableImageAsync`, using
   `Content.fromUri("rbxasset://...")`.
2. Pass `Content.fromObject(image)` for ColorMap, NormalMap, RoughnessMap and
   MetalnessMap to `AssetService:CreateSurfaceAppearanceAsync`.
3. Attach the returned SurfaceAppearance to the local mesh and retain the images.

An editor test and a separate client play-mode test both rendered five controls:
color only, color plus normal, color plus roughness, color plus metalness, and all
four maps. The client log reported `LOCAL_PLAY_MATERIAL_OK` for all five, and
the captured viewport visibly shows normal/shading, highlight and reflection
differences. This is visual acceptance, not pixel-exact cloud-output parity.
The play test ended normally with `LOCAL_PLAY_RESULT {"completed":true}`.

The reproducible fixture and actual play-mode screenshot are retained in
`tests/fixtures/local-pbr/`. No material uploads or place publications were made.
Studio still contacts its ordinary built-in/account services; this is not a
network-isolation claim.

### Reproduction

With Studio closed, copy the fixture's two content subdirectories into the
installed Studio `content` directory, preserving their names. Open `scene.rbxl`
with Studio's `--task RunScript --localPlaceFile <absolute scene path>
--runScriptFile <absolute play.luau path> --outputFile <absolute log path>
--quitAfterExecution` arguments. Under Wine, supply Windows-form absolute paths.
Use maximum player graphics quality for comparing the material controls; restore
the previous preference afterward. The test ends after 90 seconds. Inspect the
client viewport during that interval and the native Studio log for all five
success markers; a script source echoed into the output file is not evidence
that its statements executed.

The fixture begins with the unsuccessful static local-pack properties, then
replaces those materials with the in-memory API results. Initial invalid-pack
warnings are expected; any `LOCAL_PLAY_MATERIAL_ERROR` is a failure of the
working-path test. The harness does not save or publish changes.

### Scope of the proof

This proves local source textures can produce full PBR in actual Studio play
mode without a texture-upload round trip. It does **not** prove direct static
KTX/DDS TexturePack binding, persistence of EditableImage-backed materials in a
saved place, replication to other clients, or a production deployment route.
It performs local decoding/material construction at test startup. The harness
is acceptance-only and is not a runtime script added to product game builds.
Shipping a development adapter or implementing the static-pack path remains
separate work; no new product CLI command is claimed here.

## Static binding blocker traced in the executable

The local-image workaround above is **not completion of the agreed offline
prebuilt-material workflow**. Jason explicitly rejected substituting startup
material construction for loading prebuilt assets.

Follow-up native investigation established:

- Initial and 15-second-delayed serialization both preserve the local pack
  prefix. Studio is not simply deleting the serialized TexturePack property.
- The old renderer calls `0x1439289b0` at `0x144618441`, then tests the result
  at `0x144618446` before reading the pack property. The newer renderer has the
  equivalent gate at `0x144820ae0–0x144820ae7`.
- An isolated live test hit a hardware breakpoint at `0x144618446` with
  `RAX = 0`: admission fails before the static pack loader is reached.
- That check enters `0x143934650`, constructs a source descriptor, and looks
  it up in a generator-owned map. A second hardware breakpoint at
  `0x143934988` observed `R14 = [RSI + 0x58]`, the end/sentinel entry. The
  descriptor was absent, and the function takes its false-return branch.
- Even the successful-cache-entry path for SurfaceAppearance reads its
  TexturePack and compares it against a generated numeric asset reference:
  `0x143934b0c` reads the property; the subsequent `0x1428dd680` formats
  `rbxassetid://%lld` (format string at `0x148795578`). This is an additional
  mismatch with a directly serialized `rbxasset://` filename prefix, not a
  KTX byte-encoding problem.
- Studio's rendering-only `FFlagDebugDisableTexturePackAssetManagerPreview`
  was temporarily enabled for a separate play-mode test. The five static
  materials still rendered gray. The setting was removed after the test.
  This switch suppresses the preview request wrapper, not the readiness gate.

Hardware breakpoints were detached normally; no instructions, return values,
cache entries or account/security settings were changed by the debugger.

These results explain the observed failed static fixture. They do not prove
that every possible Studio integration is impossible. They do establish that
changing KTX headers or retaining the saved pack URI is insufficient: a working
integration must also address Studio's native generator/cache admission path.
No working file-only solution has been found, so no production converter or
binding command has been presented as finished. Patching that engine behavior
or introducing native cache integration is a separate architectural step, not
an encoder regression fix.

## Independent reverse-engineering report and local-render branch

The newer [May 2026 regression report](https://devforum.roblox.com/t/local-assets-do-not-work-when-used-with-surfaceappearances-and-materialvariants/4638719/4)
contains an August 22 analysis by developer `tabby0x` (not a Roblox staff fix).
They identify the existing `RenderSurfaceAppearanceLocalAssets` branch and
report that local maps need an empty TexturePack. Their proposed fix checks for
local source maps before TexturePackGenerator's cache lookup, clears the pack
reference, and returns through the existing local-rendering path. They also
identify a mismatch between temporary (`rbxtemp://`, kind 4) and installed local
(`rbxasset://`, kind 3) asset handling. The report remains open.

Our installed binary contains that exact flag, registered at `0x1446d1e20`
with value storage at `0x14d8aa938`. The branch at `0x1446ce677` requires that
flag, an empty pack reference, and a positive local-source test. The local test
at `0x1446d1970` checks source maps for content kind 3. Its alternate renderer
at `0x1446d2780` requests the individual material maps. Thus the numeric-pack
cache gate is not evidence that Roblox rendering inherently requires uploaded
textures; there is a separate native local path.

A camera-only play test enabled this flag and included an empty-pack PNG-map
control. A read-only debugger check confirmed the live flag byte was `0x01`.
The PBR control still rendered gray in this build. Therefore enabling the flag
alone is not a verified repair, and the independent report's proposed native
fix has not been applied or validated here. The temporary flag was removed.
The next investigation belongs to selecting and preserving that local branch,
not cloud publication, runtime EditableImage construction, or KTX header tuning.
