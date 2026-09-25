# Link a prebuilt scene to remote asset IDs

`roblox deploy link-scene <bundle-directory> --scene <logical-scene-id> --mapping <mapping.json> --output <new-place.rbxl>`

This is a local deployment preparation command, **not an uploader**. It verifies
the bundle, pins its manifest and each binding's source artifact hash, and rewrites
only native `Content` / `ContentId` properties referencing owned bundle artifacts.
Use `.rbxm` output for model scenes and `.rbxl` for place scenes.

The mapping document has `bundleManifestSha256` and a `bindings` array. Each
binding contains `asset`, `file`, `sourceArtifactSha256`, and `remoteId` (a canonical
positive decimal string). The first three identify the exact prebuilt local
artifact; `remoteId` is the caller-supplied Roblox asset ID. No source conversion
recipe, project convention or guessed filename mapping is involved.

All owned content references in the selected scene must have bindings. Missing,
duplicate, unknown and stale bindings fail before creating output. Unused bindings
are allowed so one verified bundle mapping can serve multiple scenes. Output must
be outside the source bundle and must not exist; linking never overwrites source
files, existing output or the bundle manifest.

Embedded collision/terrain bytes and ordinary strings (including script text)
are not text-replaced. Built-in and already external references are preserved.
Referenced native models and TexturePack files must themselves have been linked
and uploaded separately; this command does not recursively upload dependencies.

Caller-supplied IDs are **not upload receipts** and are not remotely checked. The
result explicitly reports `published: false`, `remoteIdsVerified: false`, and
`engineVerified: false`, alongside input/output hashes and the reference count.
Do not infer remote ownership, moderation, payload identity or engine acceptance
from successful local linking. Credentials and network are not used.
