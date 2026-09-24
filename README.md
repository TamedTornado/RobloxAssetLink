# Roblox Asset Link

A small **Rust CLI + Luau Studio plugin** for local, edit-time GLB previews.
No Electron, Node, cloud account, asset upload, or game-startup geometry generator.
The implementation uses Roblox's `EditableMesh` and `CreateMeshPartAsync` APIs;
Freeway's open-source workflow demonstrated this approach. This is a new
implementation, not a bundled Freeway application.

## Build and use

```sh
cargo build --release --locked
mkdir -p dist
rojo build plugin.project.json -o dist/AssetLink.rbxm

# Copy the example and adapt it for your project first.
target/release/roblox-asset-link check /path/to/glbs --config /path/to/config.json
target/release/roblox-asset-link serve /path/to/glbs --config /path/to/config.json
```

Install `dist/AssetLink.rbxm` in Studio's **local Plugins folder**. For Wine,
use that Studio installation's Plugins folder, not a Linux-native guessed path.
Open the Asset Link toolbar panel. Enter the loopback URL and session token
printed by `serve`, then Load configuration and Sync now. Grant the plugin its
localhost HTTP permission if Studio prompts. No Roblox credentials are needed.

The session token is deliberately not persisted in plugin settings. The bridge
binds only IPv4 loopback. Browser-origin requests are rejected; all routes require
the token. It serves only top-level `.glb` files from the selected directory;
symlinks and external GLB buffers are not followed. The only writable path is the
explicit configuration file. Stop the CLI with Ctrl-C.

Live sync polls at the configured interval, stops on errors, and pauses in Play.
Errors appear in both the panel and Studio Output. A malformed asset prevents a
partial snapshot. Unchanged asset revisions are not rebuilt within a session.

## One project configuration

`examples/building-kit.json` is an **example**, not built-in application policy.
There are no built-in collision naming conventions or unit conversion settings.
The file is required and validated. Unknown fields and invalid values fail.

- `projectId`: stable ownership identity for this library. Do not change it after
  importing; use another configuration for a different library.
- `metresPerStud`: conversion at the import boundary; GLB coordinates are metres.
- `pollSeconds`: live refresh interval, positive and finite.
- `destination`: `Workspace`, `ServerStorage`, or `ReplicatedStorage`.
- `containerName`: name of the plugin-managed asset library folder.
- `defaultPart`: anchored, canCollide, canTouch, canQuery, doubleSided,
  collisionFidelity, and transparency. `null` transparency uses material alpha.
- `rules`: ordered mesh-node `namePrefix` rules, each with complete part settings.
  The first match wins. An empty list disables naming rules entirely.

The dockable plugin panel exposes the **same JSON**, including all rules, in a
multiline editor. Apply validates it against the current assets and atomically
replaces the config file. A revision check rejects edits based on a stale loaded
configuration; reload after external edits. Avoid simultaneous external writes
during Apply: the filesystem itself does not provide an external-editor CAS.

Each import preserves node transforms, normals, UVs, base colour, alpha, source
origin/pivot, and configured collision properties. A separate model is created
for each GLB. The library is a collection of templates, not a laid-out scene.
The entire changed batch builds before existing templates are replaced; unrelated
instances are not deleted. Existing model placement is retained on replacement.
Managed models and their contents are generated: author changes in Blender, not
inside the managed templates. Copies outside the library are not live-linked.

## Scope and limitations

This first version supports static triangle geometry and untextured base-colour
materials. Skins, animation, morph targets, required glTF extensions, textures,
vertex colours, emissive materials, and alpha masks are rejected explicitly.
PBR roughness/metalness are not reproduced; this is a geometry/base-colour preview.
Normals and UV0 must be exported. Roblox's own mesh limits still apply; import
failures preserve the preceding managed library.

**Preview is not publication.** No assets are uploaded. Live EditableMesh
references are rebuilt when syncing after a plugin reload. Do not treat a saved
place or `.rbxm` containing this preview as a tested portable/publishable asset.
Studio Play behaviour, save/reopen behaviour, collision cooking, and undo require
live acceptance testing on the installed Studio version. These have not yet been
verified by the CLI/packaging tests.

## Validation

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
stylua --check plugin
selene plugin
rojo build plugin.project.json -o dist/AssetLink.rbxm
```

Tests include actual Blender-exported GLBs, non-default units and naming rules,
transform math, invalid configuration, authenticated HTTP requests, and disk
configuration updates with stale-edit rejection. The initial LayerOne kit also
passed parsing: **20 assets, 72 meshes, 1,032 triangles**. Plugin packaging/linting
is not a substitute for live Studio testing.
