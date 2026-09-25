# Build-plan asset editing

The replacement for the preview-era GLB catalog operates on the **same JSON plan
that `build bundle` consumes**. It does not introduce another asset registry,
source scanner or preview configuration. A plan entry contains a stable logical
ID and its typed conversion configuration; paths remain relative to the plan.

The Rust `plan_assets` module implements initialization, read/revision, add,
replacement and removal. Initialization never overwrites. Edits use an OS file
lock plus atomic replacement; an optional expected revision rejects stale writes.
JSON schema, unique identities and skin/animation dependencies are validated
before committing. Concurrent editor tests require every successful addition to
survive. Source files and already-built outputs are never deleted or rewritten.

Removal checks declared scene bindings, nested node material/rig/asset bindings
and animation `bindTo` dependencies. Missing or invalid declared scene files
prevent proving that removal is safe, so removal fails without changing the plan.
An unreferenced missing conversion source can still be removed. External editors
do not participate in the file lock; do not concurrently edit the same plan or
scene files outside the cooperating commands.

Plan editing validates the configuration's structure and declared relationships,
not the source's convertibility. `build bundle` still performs full conversion,
source checks, policy validation and native assembly. Integration tests actually
build an edited plan, rather than treating a saved JSON document as build success.

## CLI

```sh
roblox assets --plan build.json init
roblox assets --plan build.json add wall --conversion wall-conversion.json
roblox assets --plan build.json list
roblox assets --plan build.json inspect wall
roblox assets --plan build.json edit wall --conversion replacement.json --expected-revision REVISION
roblox assets --plan build.json validate
roblox assets --plan build.json remove wall
```

The conversion file contains one typed conversion object, for example
`{"kind":"mesh","source":"wall.glb","config":{"metresPerStud":0.28}}`.
The conversion-file argument is cwd-relative; paths inside it are plan-relative.
IDs are explicit caller choices, not random UUIDs. `inspect` returns the declared
conversion and explicitly reports `sourceValidated: false`.

`validate` performs a real offline bundle build and integrity verification into
temporary output, then removes that output. Configured incremental caches may
be populated as during an ordinary build. It does not publish or prove engine
acceptance. Empty plans are editable but cannot validate as a complete bundle.

The CLI now uses this implementation exclusively. The old `--catalog`, preview
configuration, geometry decoder and catalog schema have been removed, with no
fallback or silent reinterpretation. Existing source assets and unrelated files
are not migrated or deleted automatically. See the
[bundle acceptance audit](bundle-acceptance.md) for issue 8's completed contract.
