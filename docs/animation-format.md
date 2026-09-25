# Native animation output — issue 4

`animation::encode` serializes a canonical clip to native RBXM containing a
KeyframeSequence, timed Keyframes, hierarchical Poses and event markers. Loop,
priority, blend weights, easing settings, names and transforms are explicit data.
Enum numbers come from the pinned reflection database, not hand-maintained aliases.

Pose transforms are **joint-rest-relative** CFrames in studs. They are not raw
FBX/glTF node transforms. The encoder deliberately requires the adapter to perform
that conversion; it must not silently interpret absolute source poses as offsets.

Tests decode the native object graph and check animation properties, poses,
markers and repeated bytes. Invalid enum names, blend weights and unordered or
non-finite times are rejected. Source scripts, Studio and cloud APIs are not used.

References: [KeyframeSequence](https://create.roblox.com/docs/reference/engine/classes/KeyframeSequence)
and [Pose](https://create.roblox.com/docs/reference/engine/classes/Pose).

`roblox convert animation clip.json --output clip.rbxm` exposes the canonical
encoder without Studio or network access. Output creation is atomic and never
overwrites an existing file. Structured output includes source/artifact hashes,
keyframe count and `engineVerified: false`.

Offline bundle plans accept `{"kind":"animation","source":"clip.json"}`.
The resulting `animation.rbxm` can be referenced by an Animation's `AnimationContent`
through the scene asset map. This is a local link awaiting deployment, not a
claim that an arbitrary local URI is playable in Roblox.

CLI and bundle integration tests independently decode the resulting scene,
check the animation reference, deterministic artifacts, source preservation,
no-overwrite behavior and failed-build cleanup.

## glTF / GLB source clips

`roblox convert animation-gltf motion.glb --config animation.json --output motion.rbxm`
imports rigid LINEAR translation/rotation tracks. A bundle can use conversion
kind `animationGltf` with the same source/config. Configuration explicitly gives
`animationIndex`, `rootNode`, `metresPerStud`, `name`, `looped` and `priority`.
No frame-rate or resource limit is invented: the timeline is the sorted union
of source key times, preserving seconds. Translation uses linear interpolation;
rotation uses normalized, shortest-path quaternion SLERP. Track endpoints clamp
outside their own time range. This preserves the supported source curves without
introducing a fixed-rate sampling policy.

The selected subtree must contain uniquely named joints with unscaled TRS rest
transforms. Every selected clip channel must belong to it. No source nodes are
silently removed or retargeted. Source right-handed Y-up coordinates are retained;
metres become studs at the boundary. Each output pose is
`inverse(localRest) * localAnimated`, not the absolute source transform. The
result manifest records node indices, parents, names and local rest CFrames for
binding against a target Bone hierarchy. The target must use that hierarchy/rest
data; an arbitrary existing rig is not automatically compatible.

The importer rejects STEP/CUBICSPLINE, scale/morph channels, matrix rest
transforms, extensions, malformed times/rotations, ambiguous joint names,
out-of-rig channels and remote/escaped buffers. glTF has no standard clip event
marker field; canonical JSON supports explicit markers separately. It does not
create MeshParts/Bones or bind skin weights yet.

Tests use a hand-authored metric fixture with a rotated rest pose and mismatched
translation/rotation key times. They check midpoint SLERP, rest-relative offsets,
hierarchy, GLB/external-buffer equivalence, native CFrame decoding, CLI/bundle
equivalence and rejection paths. These checks are not playback acceptance.

Semantics follow the [glTF animation specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#animations)
and [Bone transform documentation](https://create.roblox.com/docs/reference/engine/classes/Bone).

Remaining: FBX clips, broader interpolation profiles, integration with imported
skin/Bone instances and native playback acceptance. Issue 4 remains open.
