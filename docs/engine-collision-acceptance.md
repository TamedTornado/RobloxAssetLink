# Native collision acceptance, September 25, 2026

## Result

Studio `0.740.19.7400931` (installed Windows executable under Vinegar/Wine)
passed `ROBLOX_TOOLCHAIN_COLLISION_ACCEPTANCE_PASS`. The tested executable SHA-256
is `a0f2e5dfeaacc86a8329f6e41b8082940a64837dca707899a6a7350a0c9a49bf`.

The offline-generated native place SHA-256 is
`e06a17c04ff78f4ef547f1be84bbcc702fe4b9ba3aeb8f69d3fd4844bf977ff5`.
All three cases passed at identity and a translated/rotated placement:

| Recipe | Geometry / negative control | Volume from native mass / density |
| --- | --- | --- |
| Hull | Tetrahedron, sloped face and empty bounding-box corner | 10.6666667 |
| Box | The same tetrahedron **mesh bytes**, full box collision | 64 |
| Decomposition | Two separated tetrahedra; gap remains empty | 21.3333333 |

Native masses at default density approximately 0.7 were respectively
7.4666667, 44.7999992 and 14.9333334. Assertions also compare centers of mass
and each inertia tensor column against analytic solid-volume results using
`AngularAccelerationToTorque`, with zero angular velocity. This checks the
off-diagonal tetrahedron terms and the parallel-axis contribution of the
disconnected shape, not just total mass.

No GLB/FBX import, CreateMeshPartAsync, collision cooking API, remote asset ID,
or cloud service participated in fixture construction. Studio only loaded the
prebuilt place, mesh files and embedded CSGPHS v5 data. The identical visual mesh
with different box/hull behavior is a control against silently substituting
collision derived from the visual geometry.

## Reproduce

1. Build offline with
   `roblox build bundle tests/fixtures/engine-collision/build.json --output <new-directory>`.
2. Verify using `roblox verify-bundle <new-directory>`.
3. For this isolated engine acceptance experiment only, stage the bundle's
   uniquely named `assets/<hash>` directories under Studio's `content/assets`.
   Do not overwrite any existing content. `rbxasset://` resolves against that
   engine content root, not the place's sibling directory. This is test staging,
   **not** a game deployment mechanism.
4. Stage the native place and `validate.luau` at paths the Wine sandbox can read.
   Host `/tmp` is not necessarily Wine's `/tmp`; the first smoke attempt exposed
   that mismatch explicitly as “Failed to read script file”.
5. Use the [official Studio CLI](https://create.roblox.com/docs/studio/command-line-interface):
   `RobloxStudioBeta.exe --task RunScript --localPlaceFile <place.rbxl> --runScriptFile <validate.luau> --outputFile <new-log> --quitAfterExecution`.
6. Require the actual emitted PASS line with version/cases, not the echoed
   script source or merely a zero launcher exit code. Preserve assertion failures
   and engine diagnostics. The tested engine also emitted unrelated built-in
   MaterialManager/UI teardown errors; these do not replace the assertion result.
7. Remove only this experiment's staged asset directories after testing.

The ordinary Rust test builds and verifies the fixture and compiles its Luau
assertions without launching Studio. Native acceptance is a separate explicit
step: Studio is not a converter, CI prerequisite, or offline build dependency.
No upload, publication, multiplayer, mobile performance or arbitrary-mesh
acceptance is claimed by this test.
