# Native skinned mesh encoding — issue 3

The Rust `skin::encode` implementation emits FileMesh v4.01 from canonical
geometry, four-influence vertex envelopes and a topologically ordered skeleton.
`roblox convert skin model.glb --config skin.json --output NEW_DIRECTORY`
imports GLB/glTF geometry and skin data through that encoder. Offline bundles
accept conversion kind `skin` with the same source and configuration.

The encoder writes geometry, bone envelopes, a single LOD, bone world-bind CFrames,
null-terminated names and draw subsets. Each subset's 26-bone palette is a fixed
native record bound. Triangles are partitioned deterministically across subsets;
vertices are reused inside a subset and duplicated only at subset boundaries.
Indices are remapped to the new streams. No triangles or influences are dropped.

Positive weights are normalized and quantized to bytes using largest remainders
so each envelope sums to 255. Empty/negative/non-finite weights, invalid joints,
duplicate/null-containing bone names and invalid parent ordering fail explicitly.
Bone culling distance is caller-supplied, not an embedded runtime policy.

References: [rbx_mesh v4 layout](https://github.com/krakow10/rbx_mesh/blob/master/src/mesh/v4.rs)
and [MaximumADHD's native reader](https://github.com/MaximumADHD/cage-mesh-deformer/blob/main/Modules/RobloxMesh.lua).
Tests decode our output independently, verify bones/names/bind poses/weights,
exercise a 30-bone mesh crossing the native palette boundary and reject malformed
inputs. This proves format conformance, not animated rendering in the current engine.

## glTF bind-space contract

Configuration explicitly supplies `meshNode`, `metresPerStud`,
`cullDistanceMetres` and `rigidTolerance`. The latter controls floating-point
rigidity validation, not runtime resource policy. [Example](../examples/skin-gltf.json).

The importer inverts each source inverse-bind matrix to obtain its native world
bind CFrame, converts translations and geometry to studs, orders parents before
children and remaps source joint-list indices into native bone indices. Mesh-node
transforms are deliberately ignored, as required by the
[glTF skinning specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#skins).
Bind transforms come from inverse-bind data, not a possibly different current
node pose. The manifest records source-node identity plus world/local bind CFrames.

The supported profile preserves normals, optional untextured UVs, tangents,
vertex color, triangle topology, material factors and up to four influences.
Missing UVs on untextured geometry use zero in the native required UV field;
texture-bearing inputs are rejected, not sampled with invented coordinates.
Additional influence sets, morphs, non-rigid bind matrices and intermediate
non-joint ancestors between joints are rejected rather than silently flattened.
Embedded animations are separate assets, not converted by the skin command.

The unchanged Khronos/Cesium **Rigged Simple** GLB is a licensed independent test
fixture (attribution beside the file). Tests decode all 160 native vertices and
188 faces and compare posed geometry against the source inverse-bind equations,
allowing only the expected byte-weight quantization error. A separate authored
fixture tests reversed source joint ordering, non-default metric scale, ignored
mesh-node translation and explicit invalid-input rejection. CLI/bundle output
and repeated conversion are checked for identical bytes.

Remaining: FBX rig input, native Bone instance/animation-binding integration and
engine acceptance. These tests prove format and CPU deformation, not current
Roblox animated rendering. Source or target assets are never uploaded here.

### Morph/facial follow-up inventory

Ordinary four-weight skinning does not implement glTF morph targets, source blend
shape animation, Roblox facial controls/pose metadata or cage deformation. Those
need distinct source semantics and native-format investigation. No universal
character/facial support is claimed; unsupported attributes fail before output.
