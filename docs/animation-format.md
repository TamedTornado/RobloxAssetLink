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

Remaining: source clip adapters, rig binding against imported skeletons,
interpolation/sampling conversion, supported scale/morph policy and native
playback acceptance. No complete FBX/glTF animation converter or CLI command is
claimed yet; this is the output serializer and issue 4 remains open.
