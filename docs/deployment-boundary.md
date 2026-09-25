# Preconverted-asset deployment — issue 9

Deployment is not implemented yet. Offline conversion/build must remain usable
without credentials, Studio or Roblox servers. A local output is not automatically
an admissible upload. Nothing has been uploaded or published during this audit.

## Published interface gaps

Checked September 25, 2026 against Roblox's
[Assets API guide](https://create.roblox.com/docs/cloud/guides/usage-assets):

| Local artifact | Documented admission / remaining proof |
| --- | --- |
| Native `.mesh` | Mesh uploads are documented for content obtained from Asset Delivery. Acceptance of our independently encoded meshes is not established. |
| PNG | Listed image input. Admission does not prove unchanged runtime bytes or no further server processing. |
| DDS / TexturePack XML | Not listed image inputs. No supported direct publication route established. |
| Ogg Vorbis | Listed audio input; byte-preserving deployment is not established. |
| VP9/Vorbis WebM | Video uploads list MP4/MOV, not WebM. Substituting source MP4 would abandon the preconverted-output boundary. |
| RBXM models/animations | Listed inputs, but the guide warns externally edited native files may fail upload or use. Our generated files remain untested. |
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

The destination still needs to be created; local implementation of mapping,
relinking and API-contract tests also remains work to do. Issue 9 stays
open; the overall goal is not complete or marked blocked by this audit.
