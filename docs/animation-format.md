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

Remaining: source clip adapters, rig binding against imported skeletons,
interpolation/sampling conversion, supported scale/morph policy and native
playback acceptance. No FBX/glTF animation converter is claimed yet; the CLI
requires canonical rest-relative JSON. Issue 4 remains open.
