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

## Transition state

The replacement module is implemented and regression-tested first. The existing
`assets --catalog` CLI has **not yet migrated** to it; old catalog input is rejected
by the replacement rather than silently reinterpreted. Next, move the CLI to the
build plan and delete obsolete catalog/configuration/preview geometry code and
fixtures. Issue 8 remains open until that migration and naming audit are complete.
