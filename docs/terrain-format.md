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

That container test alone proves property preservation, not voxel interpretation
or engine acceptance. The version-one wire codec below now additionally decodes
and reproduces the independent payload byte-for-byte. No mesh converter output
is labeled terrain as a workaround.

## SmoothGrid version-one wire codec

`terrain_grid::{decode, encode}` operates on explicit raw cells and chunk
coordinates. It requires positive externally supplied `maxChunks` and `maxCells`
budgets on both read and write. These are resource policy, not invented terrain
dimensions. Unsupported versions, exponents, truncation, runs crossing chunk
boundaries, duplicate coordinates and unrepresentable cell channels fail.

The independent Rojo payload is 14,987 bytes: version 1, chunk exponent 5, sixteen
chunks of 32³ cells (524,288 cells total). Its chunk coordinates range from -1 to
1 in X/Z and -1 to 0 in Y. The first chunk is (-1,-1,-1), containing 32,570 cells
with raw material 0 and 198 with raw material 2. Decoding and canonical RLE
encoding reproduces **every original byte**, not merely a self-authored fixture.
This does not establish the named material mapping or engine behavior.

Wire structure established from the fixture and the installed reader:

- Byte 0 is the serialization version, byte 1 the chunk edge exponent. Edge
  length is `1 << exponent`; the native version-one reader rejects exponents >8.
- Chunks continue to the end of the payload without a chunk-count header.
- Each chunk starts with twelve coordinate-delta bytes. For shifts 24,16,8,0,
  the stream supplies X,Y,Z bytes in that order. Add each reconstructed delta to
  the previous signed 32-bit coordinate with wrapping arithmetic; initial
  coordinates are zero. These are not three contiguous big-endian integers.
- Cells are X-fastest, then Z, then Y. RLE records fill exactly `edge³` cells.
- A record header's low six bits hold a raw material slot. Bit 6 adds an explicit
  occupancy byte; otherwise the encoded occupancy defaults to 255.
- Bit 7 adds a run-count byte, with length `byte + 1` (maximum 256). Without bit
  7, length is one. The special bit-7/count-zero record consumes one additional
  auxiliary channel byte and still represents one cell, not an extended run.
- The native reader forces material-zero occupancy to zero. The auxiliary byte
  survives only for material slots >1 with occupancy !=255. The encoder rejects
  inputs that this normalization would discard instead of silently losing data.

The wire codec intentionally exposes raw material, occupancy and auxiliary bytes.
The sparse voxel adapter below supplies the separately established public material
mapping and normalized occupancy conversion. Mixed solid/liquid auxiliary-channel
authoring, PhysicsGrid generation and newer fragment formats remain unfinished.

## Metric sparse voxel adapter

`terrain_voxels::build` converts named sparse voxels into the wire grid. Each voxel
has an integer X/Y/Z cell offset, project material alias and normalized occupancy.
JSON configuration supplies `metresPerStud`, `voxelSizeMetres`, `originMetres`,
`alignmentToleranceMetres`, `chunkExponent`, resource `limits`, and a `materials`
map from project aliases to native names. No LayerOne dimensions or names are
embedded. Output chunks are sorted, making bytes independent of source ordering.

Native voxel resolution is four studs, as documented by Roblox's
[terrain overview](https://create.roblox.com/docs/parts/terrain), not an arbitrary
generator limit. Declared metric voxel size must match that resolution within
the explicit tolerance. The metric origin must be grid aligned; coordinates use
Euclidean division/remainders so cells at negative positions enter the right
chunks. No implicit resampling or origin snapping outside tolerance occurs.

The installed WriteVoxels implementation at `0x1444e06f0` scans the 23-element
public-enum table at `0x1484f1b80`; the matched index is the native material slot.
Cross-checking the values against the independent pinned reflection database and
the [official Material enum](https://create.roblox.com/docs/reference/engine/enums/Material)
establishes these slots in order:

`Air, Water, Grass, Slate, Concrete, Brick, Sand, WoodPlanks, Rock, Glacier, Snow,
Sandstone, Mud, Basalt, Ground, CrackedLava, Asphalt, Cobblestone, Ice, LeafyGrass,
Salt, Limestone, Pavement`.

The native method falls back to slot 2 for other enums; this converter deliberately
rejects unsupported materials instead of silently turning them into Grass. Empty
aliases, unknown aliases, duplicate cell positions and coordinate overflow fail.

At `0x1444e0b33`–`0x1444e0b98`, positive occupancy becomes single precision,
multiplies by 256, subtracts 0.5, truncates and clamps to [0,255]. Constants are
read from `0x1484f23f0` (256) and `0x1484b4450` (0.5). This is **not** rounding
`occupancy*255`: 0.5 encodes as 127. Zero occupancy produces an empty cell; a tiny
positive occupancy can produce raw occupancy zero with a non-Air material.
Inputs must be finite in [0,1]; Air with nonzero occupancy is rejected. This
profile authors one material per voxel, including water-only cells, not mixed
solid/liquid cells or edits merged into existing terrain.

Tests map every table entry independently, check quantization boundaries, metric
origins, negative chunk boundaries, native X/Z/Y order, deterministic source-order
independence and configured resource rejection. Reconstructing the independent
Rojo fixture as sparse Grass/Rock voxels and re-encoding reproduces all original
bytes. This is stronger than a self-generated roundtrip, but still not live
rendering/physics acceptance. Heightmap/CLI/bundle conversion and PhysicsGrid
handling remain to be completed.

### Installed-reader evidence

Read-only static inspection of `version-c792f79abddd41bd/RobloxStudioBeta.exe`,
SHA-256 `a0f2e5dfeaacc86a8329f6e41b8082940a64837dca707899a6a7350a0c9a49bf`:

- The deserialize entry at `0x143eacc80` reads the initial version. Version 1
  follows the path at `0x143eaceb5`; version 2 dispatches to `0x143eaec40`, and
  higher versions take another path. This is SmoothGrid, not ClusterGridV3.
- `0x143eaced9` reads the exponent and `0x143eacef2` applies its bound.
  `0x143eacf60`–`0x143eacfe2` accumulates interleaved coordinate deltas.
- `0x143ec9640` fills a chunk using run records from `0x143ec9510`. The latter
  implements flags 0x40/0x80, the count-zero auxiliary byte, material mask 0x3f
  and normalization. `0x143ec9773` rejects run overflow.
- The copy loops at `0x143ec9850`–`0x143ec98b6`, together with the dimension
  constructor at `0x143e76b40`, establish X/Z/Y linearization.
- A separate fragment reader at `0x143ead530` checks `VXFG` and fragment versions
  1–2, with a channel-count bound of 31. No fragment encoder is claimed here.

These observations establish a meaningful retained input format and explain the
independent fixture exactly. They are **not** execution of the proprietary reader,
live engine acceptance or proof that PhysicsGrid can be omitted. No executable
bytes, proprietary assets, decompiled source or leaked source are committed.

[Issue 10](https://github.com/TamedTornado/RobloxAssetLink/issues/10) tracks the
missing Rust voxel/heightmap-to-native encoding, format investigation, chunk/
occupancy/material validation and integration. Resolution, metric dimensions,
material mappings and resource policy must be validated external configuration.
It is the separate terrain implementation issue requested by issue 7, not a
claim that terrain generation is finished.
