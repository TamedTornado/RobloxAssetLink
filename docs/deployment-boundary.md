# Preconverted-asset deployment — issue 9

Product CLI cloud upload/publication is not implemented yet. Local scene linking is available
through [`deploy link-scene`](deployment-linking.md). Offline conversion/build must remain usable
without credentials, Studio or Roblox servers. A local output is not automatically
an admissible upload. [Live acceptance experiments](cloud-acceptance-2026-09-25.md)
now establish six native artifact types working after upload, with explicit
remaining failures and gaps. A native place has now been published, downloaded
byte-for-byte, and visually verified in actual Studio client play mode with
working color, normal, roughness and metalness maps. See the report's version-8
PBR proof and `tests/fixtures/pbr-runtime/`.

## Published interface gaps

Checked September 25, 2026 against Roblox's
[Assets API guide](https://create.roblox.com/docs/cloud/guides/usage-assets):

| Local artifact | Documented admission / remaining proof |
| --- | --- |
| Native `.mesh` | Our independently encoded v2 and v4.01 files were accepted and loaded through typed MeshParts in Studio. Byte-preserving remote storage is not established. |
| PNG | Accepted and successfully preloaded in an ImageLabel. This does not prove unchanged runtime bytes or no further server processing. |
| DDS | Studio loads our BC4, L8 and RGBA8 outputs. The Image upload endpoint rejects BC4 with InvalidImage and the uncompressed controls with Unsupported image format. Engine readability and upload admission are different boundaries. |
| TexturePack XML | Accepted as TexturePack and now visibly verified in actual client play mode. Upload individual PNG maps, publish their descriptor, link its ID and allow asynchronous runtime representation generation. Native place upload alone did not generate a pack from the individual map properties. |
| Ogg Vorbis | Accepted, decoded and reported the expected duration in Studio; byte-preserving deployment is not established. |
| VP9/Vorbis WebM | The zero-price VP9 WebM probe was blocked by HTTP 403 requiring account ID verification, before format admission. The guide lists MP4/MOV; substituting source MP4 would abandon the preconverted-output boundary. |
| RBXM models/animations | Our generated model and KeyframeSequence were accepted and loaded with checked hierarchy/dimensions and keyframe/pose data. |
| Collision and terrain payloads | Embedded into native instances locally, not separate documented upload types. Native container admission alone would not prove their runtime use. |

The guide is not evidence that a forbidden format becomes acceptable just by
changing its filename or MIME type. Do not mislabel payloads or quietly use the
GLB/FBX cloud importer as a fallback.

[Place publishing](https://create.roblox.com/docs/cloud/guides/usage-place-publishing)
accepts native place files for existing universe/place destinations. However, it
expressly limits updates involving EditableImage, EditableMesh, PartOperation,
SurfaceAppearance and BaseWrap. Our PBR materials use SurfaceAppearance, so this
restriction is relevant to the actual artifacts, not merely a hypothetical edge
case. Uploading a place and receiving a version number cannot by itself establish
that all its changed material data was deployed correctly.

## Required separation and evidence

The eventual deployment commands must consume verified build outputs, never
source conversion recipes. They need explicit ownership/destination, per-artifact
upload receipts, logical identity-to-remote-ID mapping, reference relinking,
observable operation failures, and publishing of the resulting native place.
Credentials and network calls must remain outside CI build tests. Acceptance
must distinguish input admission, completed upload/moderation and actual runtime
use; none should be inferred from the others.

Jason subsequently authorized creation of a new disposable test experience and
scoped uploads/publication, and allowed Studio under Wine for acceptance. Do not
ask him to select an existing game: create a separate test destination without
touching LayerOne or unrelated projects. That authorization does not prove that
unsupported native formats have a working endpoint. Authentication must use the
test destination's authorized mechanism, not unrelated credentials.

The private destination is universe 10767994924, place 99388199272385. Local typed
scene mapping/relinking and its unit/integration tests are implemented. Live
publication and PBR rendering are proven, but product CLI authenticated upload
receipts, dependency upload ordering/readiness and publication remain to implement.
Video upload is out of scope; PNG upload with Roblox-side compression replaces
DDS upload as the accepted deployment path. Issue 9 stays open for CLI work.
