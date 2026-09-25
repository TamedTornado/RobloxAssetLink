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
`animationIndex`, `rootNode`, `metresPerStud`, `rigidTolerance`, `name`, `looped`
and `priority`. The required tolerance controls numerical admission of rigid
matrices, unit quaternions and identity-scale roundoff; it is not a hidden limit.
No frame-rate or resource limit is invented: the timeline is the sorted union
of source key times, preserving seconds. Translation uses linear interpolation;
rotation uses normalized, shortest-path quaternion SLERP. Track endpoints clamp
outside their own time range. This preserves the supported source curves without
introducing a fixed-rate sampling policy.

The selected subtree must contain uniquely named joints with rigid TRS or matrix
rest transforms. Every selected clip channel must belong to it. No source nodes are
silently removed or retargeted. Source right-handed Y-up coordinates are retained;
metres become studs at the boundary. Each output pose is
`inverse(localRest) * localAnimated`, not the absolute source transform. The
result manifest's `rig.joints` records node indices, parents, names and local rest CFrames for
binding against a target Bone hierarchy. The target must use that hierarchy/rest
data; an arbitrary existing rig is not automatically compatible. `rigSha256`
identifies the complete rig metadata for stable dependency tracking; it does
not assert that an independently built target rig has been checked against it.

The importer rejects STEP/CUBICSPLINE, non-identity scale/morph animation,
non-rigid rest transforms, extensions, malformed times/rotations, ambiguous joint names,
out-of-rig channels and remote/escaped buffers. glTF has no standard clip event
marker field; canonical JSON supports explicit markers separately. It does not
create MeshParts/Bones or bind skin weights yet.

Scale channels whose samples are all identity within the configured numerical
tolerance are preserved as timeline samples without introducing scale. This
handles exporter roundoff, not actual scale animation. Matrix and TRS admission
share the same rigidity checks with skin bind conversion, rejecting shear,
reflection, perspective and non-finite values.

Tests use a hand-authored metric fixture with a rotated rest pose and mismatched
translation/rotation key times. They check midpoint SLERP, rest-relative offsets,
hierarchy, GLB/external-buffer equivalence, native CFrame decoding, CLI/bundle
equivalence and rejection paths. These checks are not playback acceptance.

The unchanged licensed Khronos/Cesium Rigged Simple fixture also exercises a
matrix rest pose and near-identity scale keys. Every one of its 50 native decoded
frames is reconstructed as `rest * pose` and compared with the independent source
translation/rotation samples. Tests verify strict/non-default tolerance behavior
and repeatable rig/artifact hashes.

Semantics follow the [glTF animation specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#animations)
and [Bone transform documentation](https://create.roblox.com/docs/reference/engine/classes/Bone).

## FBX source clips

`roblox convert animation-fbx motion.fbx --config animation.json --output motion.rbxm`
uses the linked-in ufbx animation baker. Bundles accept `animationFbx` with the
same configuration. [Example](../examples/animation-fbx.json).

The caller selects `animationIndex` and `rootNode` (ufbx typed node id), units,
rigidity tolerance, clip name/loop/priority, and all sampling/budget settings:
`resampleRate`, `minimumSampleRate`, `maxKeyframeSegments`, `maxOutputFrames`.
The minimum rate is the threshold above which ufbx considers source keys already
sampled. Rotation resampling remains enabled; key reduction is disabled. The
output is explicitly marked `sampledApproximation: true`, not an exact copy of
arbitrary FBX cubic/Euler curves. There is no claimed continuous error bound.

Units are converted with ModifyGeometry, matching skin import. The importer
bakes the transform chain into local translation/quaternion samples, emits
`inverse(rest) * animated` poses and retains the joint-rest metadata/hash. Source
time is kept intact during baking and rebased exactly once at output; the
manifest retains `sourceTimeBegin`. This prevents the first-frame loss caused by
mixing ufbx's trimmed key times with its untrimmed playback bounds.

Actual scale animation, stepped motion, animated layer weights, non-rigid
transforms, non-transform motion and motion outside the selected rig fail.
Redundant metadata channels such as constant Visibility are allowed only when
their values/tangents prove constant and composed evaluation equals the original
property. They are listed in `unchangedProperties`, not silently ignored. Equal
cubic endpoint values alone are not sufficient evidence of constant motion.

The unchanged ufbx Maya wiggle fixture is decoded back from native RBXM and every
pose is reconstructed and compared with direct source evaluation. Tests cover
first/last times, repeated bytes and rig hashes, higher sampling rates, explicit
output budgets, no-overwrite behavior, CLI/bundle parity and the metadata proof.
Builds use neither a codec subprocess, Studio nor a remote conversion service.

## Root context and target binding

Both source converters emit the same `rig` contract: `rootParentCframe` plus
`joints`. CFrames contain nine row-major rotation components followed by three
translation components in studs. The parent CFrame preserves the static ancestry
above the selected root; roots without an external parent use identity. Animated
out-of-rig ancestry is rejected, as are ambiguous, cyclic or non-rigid parents.
The FBX profile also rejects constraints requiring source-side baking.

Reconstruction is `rootParent * rootRest * rootPose`, followed recursively by
each child's `rest * pose`. A target must preserve the joint names, hierarchy and
rest basis. For an otherwise untransformed container, rootParent can be folded
into the root Bone's rest CFrame; it must not also be applied to that hierarchy
again. Arbitrary rig retargeting is not performed. Do not mistake a matching
name for a matching rest basis or silently apply the clip to a different rig.

The fingerprint hashes JSON serialization of the ordered pair
`["roblox-animation-rig-v1", rig]`, including root context. A parent-placement
change can leave native local pose bytes unchanged but must change this rig
fingerprint. Tests explicitly verify that distinction, invalid parent rejection,
and whole-hierarchy world-space reconstruction against both independent source
fixtures, beyond merely testing local offsets.

The source-animation conversion requirements of issue 4 are covered by these
profiles, explicit unsupported-semantics failures, coordinate/time/interpolation
documentation, native round trips and offline CLI/bundle tests. Native Bone
instance construction and actual scene binding remain scene integration in
issue 7; playback and cloud acceptance are not claimed by conversion tests.
