# Roblox CLI (RobloxAssetLink repository)

General-purpose, CLI-first Roblox tooling for agents and scripts. **Assets are
the first command group, not the boundary of the tool.** This repository retains
its original name while the implementation moves beyond the GLB-preview design.

## Offline build toolchain

The target is now **local source asset conversion and game assembly**, with no
Roblox server or Studio dependency during builds. Deployment will be separate.
See [format inventory and implementation evidence](docs/offline-conversion.md).

Implemented static input adapters: GLB/glTF, FBX and OBJ. Broader attribute/material
coverage remains tracked under the format issues:

```sh
roblox convert mesh model.glb --config examples/conversion.json --output converted
```

This emits native mesh files and a manifest, not a preview. Optional local
collision cooking is available with `--collision-config examples/collision-hull.json`;
see [collision format evidence](docs/collision-format.md). Neither output is yet
claimed as a complete game or engine/deployment-verified asset.

`roblox convert texture` also performs local PNG normalization, normal-map
convention conversion and PBR channel splitting; see
[texture conversion](docs/texture-conversion.md) for its explicit configuration.

`roblox build scene` assembles native model/place files, and `roblox compile`
compiles Luau without execution. Scene script sources must pass that local
compiler gate before serialization. See [scene assembly](docs/scene-assembly.md)
and [Luau compilation](docs/luau-compilation.md); these are not yet the complete
asset-linked game build or deployment pipeline.

`roblox convert audio` performs in-process Ogg Vorbis conversion; see
[media conversion](docs/media-conversion.md) for its current profile and tests.

`roblox build bundle` combines declared conversions with asset-linked native
scenes. See [offline bundles](docs/offline-bundles.md). This does not publish or
claim native engine acceptance.

`roblox convert animation` converts canonical rest-relative clip JSON to native
KeyframeSequence models, also available in offline bundles. FBX/glTF clip import
is not implemented yet. See [animation conversion](docs/animation-format.md).

## Executables and current scope

- `roblox`: short-lived commands with JSON results, JSON errors on stderr, and
  nonzero failure exits. Local asset catalog management does not require Studio,
  a server, a plugin, Electron, or Node.
- `roblox-server`: separate, optional Studio adapter process. Currently serves
  explicit catalog entries and shares configuration editing with the plugin.
- Luau plugin: Studio-side adapter. The existing mesh preview implementation is
  still experimental; it is **not** proof of persistent asset import.

**Implemented:** catalog initialization, registration, metadata/source editing,
removal, listing, inspection, validation, and project configuration. Entries have
stable IDs independent of filenames and display names. Commands explicitly report
`scope: "assetCatalog"` and `studioApplied: false`.

**Not implemented:** completed persistent Studio imports, publish/update of hosted
assets, dependency-aware deletion of actual Roblox assets, scene placement,
general Studio command execution, and command completion acknowledgments from
Studio. Do not interpret catalog success as any of these outcomes.

## Build

```sh
cargo build --release --locked
mkdir -p dist
rojo build plugin.project.json -o dist/AssetLink.rbxm
```

The Rust binaries are `target/release/roblox` and `target/release/roblox-server`.
There is no combined `serve` subcommand in the CLI. Tool versions for plugin
build/lint are recorded in `rokit.toml`.

## CLI workflow

```sh
roblox capabilities
roblox assets --catalog ./assets.json init --config ./project-settings.json
roblox assets --catalog ./assets.json add ./models/door.glb --id door --name Door
roblox assets --catalog ./assets.json list
roblox assets --catalog ./assets.json inspect door
roblox assets --catalog ./assets.json edit door --name Entrance
roblox assets --catalog ./assets.json edit door --source ./models/door-v2.glb
roblox assets --catalog ./assets.json validate
roblox assets --catalog ./assets.json config
roblox assets --catalog ./assets.json config --file ./revised-settings.json
roblox assets --catalog ./assets.json remove door
```

`add` validates a GLB and registers its source; it does not move/copy/delete that
source or upload it. Omit `--id` to generate a stable UUID. `edit` retains the ID.
`remove` removes only the catalog entry, preserving source files and hosted
assets. An active experimental preview sync will reconcile its managed templates
to that catalog; it does not know about other scene references.

CLI source arguments are relative to the current working directory. Sources under
the catalog directory are stored as catalog-relative paths; other sources use
absolute paths. Missing sources fail validation but can still be removed from the
catalog. **No directory scan implicitly registers files.**

CLI and adapter writes share an OS file lock and replace the catalog atomically.
Initialization never overwrites an existing file. A failed edit leaves the catalog
unchanged. Commit the JSON catalog and source files; the sibling `.json.lock` file
is coordination state, not project data. External editors do not participate in
the lock: avoid simultaneous manual writes while a CLI/UI operation is applying.

## Data-driven configuration

Use `examples/building-kit.json` as an editable example, not application policy.
Initialization embeds that config in the catalog; the **catalog then becomes the
single configuration source**. Both CLI and plugin edit its `config` member.

- `projectId`: stable managed-library identity; changing it requires a new catalog.
- `metresPerStud`: GLB metres to Studio units at the import boundary.
- `pollSeconds`: positive live-preview polling interval.
- `destination`: Workspace, ServerStorage, or ReplicatedStorage.
- `containerName`: managed library folder name.
- `defaultPart`: anchoring, collision/touch/query flags, double-sidedness,
  collision fidelity, and transparency (`null` uses GLB material alpha).
- `rules`: ordered mesh-name-prefix matches with complete part settings. First
  match wins; an empty list disables naming rules.

Unknown fields and invalid values are rejected. Configuration replacement
validates existing registered geometry before committing.

## Optional development adapter

```sh
roblox-server --catalog ./assets.json --port 34873
```

The server binds IPv4 loopback, prints a per-session token, rejects browser-origin
requests, and requires that token on every request. It reads only explicitly
registered sources. Source GLBs must be self-contained; external buffers are
rejected. `/config` edits only the selected catalog, under the same lock as the
CLI; a revision check rejects stale panel edits. It cannot upload assets.

Install `dist/AssetLink.rbxm` in the actual Studio local Plugins folder (the Wine
installation's folder when applicable). Enter the printed endpoint/token in its
panel. The panel exposes configuration and experimental preview controls.
Snapshots carry stable catalog IDs; renaming no longer changes template identity.

The adapter's static GLB path preserves geometry transforms, normals, UVs, base
colour, alpha, and origin/pivot. It rejects unsupported skins, animation, morphs,
required extensions, textures, vertex colours, emissive materials, and alpha masks.
It does not reproduce PBR roughness/metalness. The plugin remains unverified for
live Studio import, collision, save/reopen, Play, and undo. It must not be shipped
as a completed persistent-import solution. No runtime game script is generated.

## Existing automation and next boundary

Roblox has a [built-in Studio MCP server](https://create.roblox.com/docs/studio/mcp)
for script/data-model editing and playtesting. Roblox's earlier
[Rust MCP reference server](https://github.com/Roblox/studio-rust-mcp-server) is
archived in favor of that built-in implementation. [Lune](https://lune-org.github.io/docs/roblox/1-introduction/)
provides offline place/model manipulation. Prefer these supported capabilities
where appropriate; do not invent another general transport merely because this
repository started with a local asset adapter.

The current development boundary is the offline conversion/build pipeline and
separate deployment of its artifacts, not another Studio transport. The adapter
is retained only until its replacement is validated. Persistent import must be
verified with save/reopen and Play **without the bridge running** before an
`import` command may report success. Publishing is an explicit separate action.

## Validation

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
stylua --check plugin
selene plugin
rojo build plugin.project.json -o dist/AssetLink.rbxm
```

Tests exercise real Blender GLBs, non-default conventions, CLI lifecycle and
rollback, missing sources, explicit registration, concurrent CLI writers, and
authenticated server/configuration operations. Packaging is not live acceptance.
