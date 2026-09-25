# Asset-linked offline bundles — issues 7 and 8

`roblox build bundle build.json --output NEW_DIRECTORY` converts declared assets
and assembles native scenes in one local invocation. The plan contains `assets`
and `scenes` lists, each with explicit logical ids. Asset conversions select
`mesh`, `texture` or `audio` and supply the corresponding JSON configuration.
Scene sources use the existing typed scene format and compiler gate.

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
