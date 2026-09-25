# Offline scene and terrain acceptance audit

Implementation audited at `928b6e1`; source suite: 144 passing tests. The focused
terrain conversion and physics tests also passed in a network namespace with no
network access. No source code changed for this audit, so passing validation is
reused rather than running the same expensive suite for documentation edits.

## Issue 7: place/model assembly

| Requirement | Current evidence |
| --- | --- |
| Rust assembly of local asset outputs and scene data into native models/places | `scene::build_with_assets`, `bundle::build`; RBXM and RBXL CLI/integration tests in `tests/scenes.rs` and `tests/bundles.rs`. Uses pinned rbx-dom/rbx_binary in process. |
| Preserve hierarchy, pivots and properties | Decoded assertions for nested models, PrimaryPart referents, WorldPivotData, PivotOffset, CFrame, Size and script Source in `tests/scenes.rs`. Unsupported/nonserialized properties and conflicting aliases are rejected. |
| Stable local references, no cloud IDs required | Logical asset/file keys link to bundle-local Content/ContentId URIs or embedded binary properties; bundle tests check exact mesh/texture references and collision bytes. No upload call in the build path. |
| Terrain inventory, separate implementation issue if needed | `docs/terrain-format.md`, issue 10, terrain conversion assets and native Terrain property bindings. |
| Representative native fixtures and round trips | Rojo native terrain fixture, Khronos glTF and Maya/ufbx rig fixtures; `tests/terrain.rs`, `tests/rig_asset.rs`, `tests/rig_binding.rs`, plus deterministic scene round trips. |
| General-purpose rather than LayerOne-specific | Explicit typed scene data, configured units and stable caller-provided identities; no LayerOne project names/dimensions in the assembly contract. |

**Verdict: the stated offline assembly requirements are satisfied.** Automatic
derivation of a complete authored scene from arbitrary source geometry is not
implemented; callers supply scene data, including desired placement/size/pivots.
That is not a hidden implementation of such automation, nor a prerequisite added
to this issue's explicit scene-data assembly contract.

## Issue 10: terrain conversion

| Requirement | Current evidence |
| --- | --- |
| Establish SmoothGrid encoding/versioning and PhysicsGrid relationship legitimately | Installed-binary reader/writer addresses and hash in `docs/terrain-format.md`; independent unchanged Rojo fixture with retained MPL license/provenance. SmoothGrid v1 is not ClusterGridV3. PhysicsGrid v2's first group requests lazy mask reconstruction from voxel regions. |
| Rust voxel/material/occupancy and heightmap conversion with validated metric/configurable policy | `terrain_voxels`, `terrain_heightmap`, `terrain`; 8/16-bit scalar PNG heightmaps, configured metric origin/units, material aliases, image/cell/chunk/index budgets. |
| Negative coordinates, boundaries, occupancy/material identity; explicit supported ranges | Euclidean chunk addressing and native X/Z/Y order tests; all 23 base material slots independently cross-checked; native float occupancy quantizer; version/range/malformed/overflow rejection tests. Mixed solid/liquid authoring and newer fragment formats are explicitly unsupported, not silently reinterpreted. |
| Native model/place/bundle integration without Studio/cloud/runtime game generation | `convert terrain`, `Conversion::Terrain`, binary-property linking, heightmap dependency/cache invalidation tests. Optional native lazy index emits its own PhysicsGrid artifact. No runtime terrain-authoring script. |
| Independent payloads, decoded equivalence, malformed/configuration/determinism coverage | SmoothGrid fixture reproduced byte-for-byte both through raw decoding and named sparse voxels; PhysicsGrid raw fixture reproduced byte-for-byte; generated lazy-index coordinate set equals the independent fixture's unique entries. Native place tests embed both rebuilt payloads. |
| Preserve source repositories/assets; no implied publication | CLI no-overwrite and source-preservation regressions; separate new output directories; no cloud publication performed. Fixture originals/provenance retained. |

**Verdict: the stated offline conversion requirements are satisfied for the
documented SmoothGrid v1 / PhysicsGrid v2 profiles.** This does not mean every
native version or every possible terrain authoring operation is supported.

## Explicit acceptance boundary

Neither issue 7 nor issue 10 requires execution in the live Roblox engine before
its offline-format acceptance can be recorded. Issue 10 expressly requires that
binary roundtrip and actual engine acceptance be distinguished. All manifests
continue to report `engineVerified: false`. The lazy physics index is not fully
precomputed contact geometry; normal on-demand engine mask work is expected from
the inspected path. Live rendering, collision queries and publication have not
been proved by these tests and are not claimed by closing these issues.

This is not completion of the overall asset-conversion goal. Issue 2 explicitly
requires native collision consumer acceptance and remains open. Texture/material
coverage (5), media-profile acceptance review (6), preview retirement/repository
naming (8), and the separate preconverted-asset deployment boundary (9) also
remain to be finished or audited against their own requirements.
