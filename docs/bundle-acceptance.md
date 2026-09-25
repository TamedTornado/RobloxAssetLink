# Offline bundle acceptance audit — issue 8

Implementation audited at `b916618`. The complete Rust suite passed following
preview/catalog removal, with focused CLI tests rerun after capability/result
metadata corrections. Clippy with warnings denied and formatting passed. The
new CLI and plan-editor integration tests also passed with network disabled.
Earlier conversion, scene, terrain and media network-disabled evidence remains
applicable; there is no Studio executable or service dependency in the build.

| Issue requirement | Authoritative implementation/evidence |
| --- | --- |
| General-purpose offline build command | `roblox build bundle`; typed conversion and scene plans in `bundle::Plan`. No project-specific naming, implicit source scan or upload step. |
| Versioned manifest linking assets, scripts, models and places | `bundle::Manifest`, native scene/script serialization, per-artifact identities, URIs and SHA-256 hashes. `tests/bundles.rs`, `tests/scenes.rs`, rig/material/terrain integration suites decode real outputs. |
| Distinguish source packaging from bytecode and consumer requirements | `docs/luau-compilation.md`, `scripts` and `scene`: open-source compilation validates scripts without executing them; native Script.Source is packaged rather than assuming compiled bytecode is accepted by Roblox. Bytecode deployment compatibility is not claimed. |
| Validated JSON conventions | Typed conversion policies, metric units, compiler levels, image/audio/video/terrain budgets and scene properties. Reflection/type checks and non-default-policy regressions. |
| Build/validate without network or Studio | Environment-cleared CLI tests, network namespace tests and the removed HTTP/plugin path. `assets validate` executes the real bundle builder into temporary output and runs native-reference integrity checks. |
| Deterministic hashes and reference resolution | Repeated native builds and `tests/bundle_verify.rs`; missing/corrupt/unlisted/escaping outputs and dangling native/TexturePack references rejected. External references reported, never fetched or declared verified. |
| Missing dependencies, incremental rebuilds and failure cleanup | `bundle_inputs`, `build_cache`, `tests/build_cache.rs`: source closure, config/tool fingerprint, dependency-key invalidation, actual hits/misses, corrupt-cache rejection, rechecked scripts, owned-output rollback and source preservation. |
| Retire preview-only path | Server, plugin, protocol-facing executable, GLB preview decoder, catalog schema/configuration and preview example removed. `assets --plan` edits the builder's own plan, with atomic revision-checked writes and dependency-aware removal. Old flags/documents are explicit rejection tests, not fallback paths. Shared geometry math remains because native converters use it. |
| Repository naming | Repository renamed in place to public `TamedTornado/RobloxToolchain`; `master` remains the default. Git origin points to the new URL, existing issues/history are retained, CLI name remains `roblox`. |

**Verdict: issue 8's offline build and transition requirements are satisfied.**

## Limits and nonclaims

The repository directory and internal Rust crate identifier retain their existing
names; neither is a competing implementation or runtime protocol. The public
project name now describes a toolchain, not a live asset bridge. Renaming did not
alter Studio installations, deployed games, credentials or unrelated repositories.

Incremental caching is explicitly Linux-only; uncached builds are separate from
that optional profile. The cache is integrity-checked but not an authenticity or
hostile-writer security boundary. Source inventories detect change, not a locked
filesystem snapshot. Scene authoring remains explicit JSON, not automatic level
design or a GUI editor. Those capabilities were not promised by this build issue.

Build manifests still report `published: false` and `engineVerified: false`.
Successful serialization and compilation do not prove renderer/physics behavior,
all-platform media support, or successful publication. Collision issue 2 and
deployment issue 9 remain open; closing issue 8 does not complete the overall goal.
