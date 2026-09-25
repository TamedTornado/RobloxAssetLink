# Terrain output inventory — issues 7 and 10

Terrain is not a triangle-mesh asset. A native Terrain instance can carry
`SmoothGrid` voxel bytes and `PhysicsGrid` bytes, a typed `MaterialColors` palette,
and water appearance properties. TerrainRegion also exposes SmoothGrid according
to the independent [format investigation](https://github.com/RobloxAPI/spec/issues/7).
That investigation does not provide a complete modern encoder specification;
the legacy ClusterGridV3 format must not be silently substituted.

Current local scene assembly preserves existing payloads as typed properties.
A pinned, independently authored Rojo terrain fixture is decoded, rebuilt under
Workspace in a native place, and decoded again. The test checks exact SmoothGrid
and PhysicsGrid bytes, material colors and water properties, plus deterministic
repeat output. See the fixture attribution for its MPL-2.0 source and the one-time
XML-to-binary normalization. CI does not run Rojo or Studio for this test.

This proves native container/property preservation, **not** knowledge of every
voxel encoded by those bytes, payload generation, or current engine acceptance.
No mesh converter output should be labeled terrain as a workaround.

[Issue 10](https://github.com/TamedTornado/RobloxAssetLink/issues/10) tracks the
missing Rust voxel/heightmap-to-native encoding, format investigation, chunk/
occupancy/material validation and integration. Resolution, metric dimensions,
material mappings and resource policy must be validated external configuration.
It is the separate terrain implementation issue requested by issue 7, not a
claim that terrain generation is finished.
