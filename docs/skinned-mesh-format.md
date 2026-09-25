# Native skinned mesh encoding — issue 3

Skin conversion also emits `rig.rbxm`: native Bone roots/children with source
names and local bind CFrames. The manifest records `rigFile` and `rigSha256`;
bundles include the artifact and its hash. Both glTF and FBX producers share this
writer. Parent indices must precede children and names must be nonempty/unique.
This is bind-pose output, not an animation rest-pose guess.

Scene MeshParts can attach this artifact with `rig` referencing the same logical
skin asset as their explicit `MeshContent` binding. Existing explicit Bone roots,
other parent classes, cross-asset rigs and using the rig file as geometry fail.
Imported trees admit only uniquely named Bone nodes and bind CFrame properties.
The scene instance count includes imported Bones (and imported materials), not
only input nodes with explicit ids.

Tests compare independent glTF/FBX bind hierarchies and transforms with decoded
native Bones and the assembled scene. Native cardinal-axis rotation compression
can remove tiny source roundoff; comparison uses the explicit import tolerance.
The tool still does not infer MeshPart placement/scale or prove engine deformation.

The Rust `skin::encode` implementation emits FileMesh v4.01 from canonical
geometry, four-influence vertex envelopes and a topologically ordered skeleton.
`roblox convert skin model.glb --config skin.json --output NEW_DIRECTORY`
imports GLB/glTF or FBX geometry and skin data through that encoder. Offline bundles
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

## FBX source profile

The same `convert skin` command and JSON schema accept `.fbx`; `meshNode` is the
ufbx node's typed id. Parsing uses the existing linked-in ufbx library, with no
external material loading. Units and axes are converted using ModifyGeometry so
centimetres do not become artificial scale on every bone.

The input must contain one linear skin deformer on the selected mesh. For each
cluster, `bind_to_world * geometry_to_bone` must agree on a shared geometry bind
transform. That transform is baked into mesh positions/normals; bone world binds
are encoded separately. Triangulated corners retain their original control-point
indices so material splits cannot detach weights from geometry. Joint order is
remapped before native subset/palette encoding.

Unsupported dual-quaternion blending, vertex caches, blend shapes, non-rigid
bone binds, inconsistent geometry binds and more than four positive influences
are explicit failures. No top-four weight truncation is performed. The source
mesh may have a non-rigid geometry transform when it can be baked into geometry;
that is different from an unrepresentable scaled/sheared bone bind.

The unchanged ufbx Maya transformed-skin fixture supplies independent FBX data.
Tests compare native decoded deformation with `ufbx::get_skin_vertex_matrix`,
including control-point mapping, transformed geometry, unit conversion and
quantized weights. CLI and repeated output are verified. The fixture also exposed
and regresses a false rejection of opaque Lambert materials whose black
transparency/emission colors have factors of one.

The conversion requirements in issue 3 are covered by the native-format tests,
GLB/glTF and FBX source fixtures, deformation oracles, malformed-input tests and
offline CLI/bundle integration. Native Bone instance/animation wiring belongs
to scene/animation integration (issues 7/4); engine rendering remains unverified.
These checks prove format and CPU deformation, not current Roblox animated
rendering. Source or target assets are never uploaded here.

### Morph/facial follow-up inventory

Ordinary four-weight skinning does not implement glTF morph targets, source blend
shape animation, Roblox facial controls/pose metadata or cage deformation. Those
need distinct source semantics and native-format investigation. No universal
character/facial support is claimed; unsupported attributes fail before output.
