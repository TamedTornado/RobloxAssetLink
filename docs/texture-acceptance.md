# Offline texture/material acceptance audit

Implementation audited at `1dd7f0f`. The complete Rust suite passed after native
TexturePack integration, as did Clippy with warnings denied, formatting, plugin
lint/format and Rojo packaging. Material, glTF, mesh-material and bundle tests
also passed in a network-disabled namespace. Source is unchanged for this audit;
passing evidence is reused rather than rerunning the suite for documentation.

## Issue 5 requirements

| Requirement | Evidence and boundary |
| --- | --- |
| Inventory native image/texture representations and material instance data; distinguish source formats from caches | `texture-conversion.md` separates image upload sources from installed PC DDS assets, native DDS reader dispatch, mip chains and special normal metadata. It documents the TexturePack XML descriptor separately from pixel encodings and SurfaceAppearance instance data. Platform-specific compression is not inferred from upload extensions. |
| Necessary image transforms and PBR channel mapping locally in Rust | `texture`, `material_gltf`, `material`: normalization, DirectX-to-OpenGL green inversion, metallic/roughness channel splitting, scalar-map validation, linear factor baking, sRGB conversion, alpha handling, explicit tint and SurfaceAppearance properties. Unsupported source semantics fail rather than being approximated. |
| Alpha, color space, normals, mipmaps and compression based on evidence | Explicit straight/premultiplied-alpha color mip filters, linear scalar mip filtering and normal renormalization; PNG, DDS L8/RGBA8 and BC4 outputs. Native DDS/TexturePack paths and standard container/codec evidence are recorded. The unexplained A2D5 marker is not guessed into an encoder. |
| Local dependency references | Native SurfaceAppearance map properties and TexturePack ContentId; local XML map references; hashes for maps and descriptor; standalone, glTF, mesh and bundle linking. `verify-bundle` validates descriptor links even when a tampered descriptor's file hash is updated. |
| Deterministic tests and independent compatibility fixtures | `tests/textures.rs`, `tests/materials.rs`, `tests/material_gltf.rs`, `tests/textured_mesh.rs`; independent Khronos assets and retained provenance, independent FFmpeg DDS decoding, exact scalar/color/normal test vectors, repeat bytes and native instance roundtrips. TexturePack golden syntax comes from the inspected native writer, not an invented schema. |
| No server cooking or Studio build dependency | Rust converters, native libraries and local serialization only. Environment-cleared CLI tests and network-disabled integration tests. Binary inspection was development-time format research, not a build/runtime dependency. |

**Verdict: issue 5's stated offline conversion requirements are satisfied for the
documented profiles.** This is not a claim of universal format or shader support.

## Supported-profile limits, not silent substitutions

- Color and normal textures can use PNG or uncompressed RGBA8 DDS; scalar maps
  additionally support L8 and BC4 DDS. BC1/BC3 color/normal encoding, mobile GPU
  formats and every renderer cache variant are not implemented. The issue asks
  for evidence-based compression requirements, not every possible compressor.
  No evidence established that a supported texture must use those extra codecs.
- ICC/HDR processing, FBX material extraction and unsupported glTF shader/sampler
  semantics remain explicitly unsupported. Mesh geometry support does not imply
  arbitrary source-material support. These restrictions are documented and
  surfaced as errors, not hidden behind a successful partial import.
- The TexturePack implementation is the unlayered SurfaceAppearance usage-0
  profile. Other usages, layered materials and arbitrary XML ingestion are not
  claimed. No cloud IDs are needed to build its local dependency graph.
- No live renderer parity, platform-wide acceptance or byte-preserving cloud
  publication has been demonstrated. `engineVerified` remains false. Issue 9
  must separately establish what upload interfaces accept without delegating
  conversion back to Roblox.

Earlier progress reports described color/normal block compression as outstanding.
It remains a useful optimization, but adding it as a mandatory closure gate would
expand this issue beyond its actual acceptance criteria. Uncompressed native
color/normal output is implemented, not being replaced with a source-only upload.

The overall asset-conversion goal remains active. Collision issue 2 explicitly
requires native consumer acceptance; issue 8 still includes preview retirement
and naming; issue 9's deployment boundary is unfinished.
