# Asset-linked offline bundles — issues 7 and 8

## Read-only bundle verification

`roblox verify-bundle DIRECTORY` checks the versioned manifest, unique artifact
identities and paths, contained regular files, matching local URIs and SHA-256
hashes. It parses every listed RBXM/RBXL artifact and checks bundle-owned local
content references against the artifact inventory. Nonempty external references
(including engine built-ins) are reported, never fetched or declared verified.
Instance and content-object references must resolve within their native container.

This command does not modify the bundle or require credentials, Studio or network
access. It verifies runtime artifacts listed in the top-level manifest, not source
inputs or unlisted conversion sidecars. It is not a signature/authenticity check:
someone who changes both a file and its manifest hash can pass the hash check.
Parsing a native container is not codec, rendering, physics or publishing proof;
the result always reports `engineVerified: false`. Incremental cache reuse is
separate unfinished work and must additionally validate source dependencies,
conversion metadata and tool/configuration fingerprints.

`roblox build bundle build.json --output NEW_DIRECTORY` converts declared assets
and assembles native scenes in one local invocation. The plan contains `assets`
and `scenes` lists, each with explicit logical ids. Asset conversions select
`mesh`, `texture` or `audio` and supply the corresponding JSON configuration.
`animation` accepts canonical rest-relative clip JSON and emits a native
KeyframeSequence. `animationGltf` imports the explicit rigid LINEAR glTF/GLB
profile described in [animation conversion](animation-format.md).
`animationFbx` uses the same native clip output with explicit FBX baking settings.
Either source-animation conversion can set `bindTo` to a skin asset id. The
builder resolves that dependency independent of list order, then performs the
motion-preserving [rest-basis conversion](animation-format.md#explicit-skin-bind-pose-rebasing).
It requires a real skin conversion with matching units and hierarchy; unknown
targets or wrong conversion types fail before publishing a bundle.
`skin` imports rigid-bind GLB/glTF and linear FBX skins into native v4.01 meshes and keeps
the source-node/bind hierarchy in its conversion manifest.
Scene sources use the existing typed scene format and compiler gate.

`material` accepts a `source` material JSON document as described in
[texture/material conversion](texture-conversion.md). It emits a native
SurfaceAppearance and all referenced normalized maps. The bundle linker replaces
the standalone document's URI prefix with the actual bundle asset directory
before serializing the material. Maps appear individually in the top-level
artifact manifest so deployment can resolve every dependency.
`materialGltf` performs source GLB/glTF material extraction with explicit material
index and decode configuration before the same linking/attachment stage. See the
material documentation for supported core PBR semantics and rejection boundaries.

A MeshPart scene node may specify
`"material": {"asset":"paint","file":"material.rbxm"}` to attach that native
SurfaceAppearance as a child. This is not a URL assigned to a property: the
builder reads and embeds the native instance, preserving its properties and
already-linked map references. Other parent classes, multiple material roots,
non-SurfaceAppearance artifacts, nested children and an additional explicit
SurfaceAppearance child are rejected. This does not import arbitrary native
models or execute scripts. The material attachment has no separately addressable
scene id; use an explicit scene child when other nodes must reference it.

Input paths are relative to the plan directory. Absolute paths, source escapes,
duplicate ids, missing dependencies and existing output directories fail. Logical
ids are not filesystem paths: output directories use their SHA-256 digests, so
project naming conventions cannot accidentally become path traversal.

Scene nodes may bind local assets through an `assets` property map:

```json
{
  "MeshContent": {"asset": "walls", "file": "node-1-primitive-0.mesh"},
  "PhysicalConfigData": {"asset": "walls", "file": "node-1-primitive-0.mesh.physics"}
}
```

The receiving property's reflection type controls binding: Content/ContentId
receive bundle-local `rbxasset://assets/...` URIs, while SharedString/NetAssetRef
and BinaryString receive the actual artifact bytes. SurfaceAppearance texture
bindings use the same mechanism. Unknown or incompatible properties do not get
guessed or silently converted. No Roblox ids are requested during the build.

The top-level manifest records asset identities, relative artifact paths, local
URIs, hashes and native scene hashes. Tests run the FBX + collision + texture +
MeshPart/SurfaceAppearance path, then deserialize the scene and verify exact
collision bytes and mesh/texture references. Repeated bundles are byte-identical.
Missing bindings remove only the newly owned build output and preserve sources.

These are linkable build artifacts, not proof of current engine rendering.
Local URIs need deployment linking (or an explicitly configured local runtime
content root); the tool does not install anything into Studio. Geometry-derived
pivots/sizing, source hierarchy automation, rigs/animations, terrain, video and
deployment acceptance remain unfinished. Both `published` and `engineVerified`
are false. Incremental caching is not implemented yet.
