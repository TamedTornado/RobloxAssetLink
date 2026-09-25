# Offline conversion implementation

The product is a general-purpose Rust build toolchain. Source-format adapters
(GLB/glTF, FBX, OBJ and later formats) feed shared geometry and native encoders.
No project-specific naming rules, Studio, credentials, network or conversion
subprocess belongs in the build path. Uploading and remote ID linking are a
separate deployment stage.

## First implemented increment

`roblox convert mesh SOURCE --config CONFIG --output NEW_DIRECTORY`

Source adapters support GLB, glTF, FBX and OBJ. glTF buffers can be local files
under the source directory or embedded base64 data. Network URIs, absolute paths
and directory escapes are rejected. FBX/OBJ parsing uses the MIT/Unlicense ufbx library linked
into the executable, not a conversion subprocess. FBX unit metadata is converted
to metres; OBJ requires explicit `objMetresPerUnit` in JSON because the format
does not define units. External OBJ material libraries currently fail explicitly.
The encoder is independent of these adapters and accepts typed Rust mesh
data. The command emits native v2.00 mesh files plus a deterministic manifest.
JSON configuration requires `metresPerStud`; it has no Studio/plugin settings.
Output directories must not exist. Source files are never modified.

The static adapter bakes scene transforms into positions, applies inverse
transpose normals, fixes reflected winding, retains UV0 and records material
factors in the manifest. This is geometry conversion, not finished scene assembly:
pivot/hierarchy reconstruction belongs to the place/model issue. Textures,
skinning, animation, morphs, extra vertex channels and extensions currently fail
explicitly. GLB/glTF and FBX/OBJ adapters preserve vertex colors and available
tangents, including alpha and transformed tangent handedness. FBX tangent data
requires source bitangents to determine handedness; UV convention conversion is
accounted for in its sign. Missing colors are white and missing
tangents use the historical zero marker; no tangent generation is claimed.
Collision is opt-in through a separate JSON recipe and is otherwise explicitly
reported as not generated. See [collision implementation](collision-format.md).
Material factors are metadata,
not completed Roblox material objects.

## Mesh v2.00 layout

Reference: [rbx_mesh v2 structures](https://github.com/krakow10/rbx_mesh/blob/master/src/mesh/v2.rs),
MIT OR Apache-2.0. No Python implementation is invoked or copied.

- ASCII signature `version 2.00` followed by LF.
- Little-endian header: u16 header size 12; u8 vertex stride 40; u8 face stride 12;
  u32 vertex count; u32 face count.
- Each vertex: position (3 f32), normal (3 f32), UV (2 f32), packed tangent (4 bytes),
  color (4 u8). The tangent bytes are not a third UV float.
- Tangents use biased values: `(byte - 127) / 127`, with handedness byte 0 or 254.
  The structural Rust reference represents them as i8 but does not unpack their
  semantics. The bias is independently documented by
  [MaximumADHD's mesh reader](https://github.com/MaximumADHD/cage-mesh-deformer/blob/main/Modules/RobloxMesh.lua).
  Tests check its documented negative-Z tangent example and reflected transforms.
- Each face: three zero-based u32 indices.

These lengths are protocol constants, not runtime limits. The 32-bit counts are
checked rather than truncated. Non-finite attributes, zero normals and invalid
indices fail before emission.

Evidence levels remain separate: exact-layout/unit tests, source conversion tests,
independent decoder acceptance, engine rendering/physics acceptance and upload
acceptance. The current tests also decode emitted files using `rbx_mesh` 0.7.0
(a test-only dependency compatible with Rust 1.93) and compare FBX and GLB exports
of the same Blender-authored doorway, including position, normal and UV corners.
These checks do not establish engine or upload acceptance. No Studio or cloud
acceptance has been claimed for these outputs.

## Issue inventory

1. Source formats to native static mesh; GLB/glTF, FBX and OBJ adapters implemented,
   broader attribute coverage and native acceptance still open.
2. Native collision payloads and local cooking.
3. Skinned mesh, rig and weights.
4. Animation representations.
5. Images and materials.
6. Audio/video profiles.
7. Place/model assembly, including terrain inventory.
8. Reproducible bundles, local references and script validation.
9. Separate deployment of preconverted outputs.

All issues are in this repository. Repository rename is deferred; the executable
remains `roblox`. Existing experimental preview code is not used by conversion and
will be retired as replacement capabilities are validated.
