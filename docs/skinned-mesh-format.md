# Native skinned mesh encoding — issue 3

The Rust `skin::encode` implementation emits FileMesh v4.01 from canonical
geometry, four-influence vertex envelopes and a topologically ordered skeleton.
It does not yet import rigs from FBX/glTF or expose a skinned conversion command;
those adapters must resolve source bind-space semantics before issue 3 is complete.

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

Remaining: source rig/skin adapters, bind-space conversion, non-rigid transform
handling, source influence policy, native Bone instance integration, animation
binding and engine acceptance. Morph/facial formats are not silently treated as
ordinary skinning and remain unsupported.
