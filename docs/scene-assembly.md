# Offline native scene assembly — issue 7

`roblox build scene scene.json --output model.rbxm` constructs a native model
without Studio. Use `kind: "place"` and `.rbxl` output for a place artifact.
The root DataModel is implicit. [Example input](../examples/scene.json).

Each node has a stable input id, class, name, typed property map, reference map
and children. Property values use `rbx_types::Variant` JSON serialization, not
string guessing or inferred numeric types. Classes/properties are checked against
the bundled reflection database. References resolve by input id after all nodes
exist; unknown targets and duplicate ids fail. Name/Parent cannot be overridden
by arbitrary property values. Output creation is atomic and does not overwrite.

Admission checks the reflection serialization rule as well as the API name.
Runtime-only properties are rejected rather than silently dropped; aliases and
migrations cannot write the same serialized destination twice. Explicit values
must match the property's declared type, and instance references require a Ref
property. Regressions cover runtime-only mass, competing old/new animation
content properties, wrong value types and references assigned to booleans.

`scriptSource` may name a UTF-8 source file under the scene document directory for
Script, LocalScript or ModuleScript. Absolute paths and symlink/directory escapes
fail. A scene containing file or inline script source requires a `scriptCompiler`
object with the four levels shown in [compiler configuration](../examples/compiler.json).
Each source is compiled locally as a validation gate, then the original text is
packed into the native Source property. Source is never executed. This is syntax
and compilation validation, not whole-project type checking or runtime testing.

This uses the MIT rbx-dom/rbx_binary libraries, not a Rojo subprocess. Repeated
builds are tested for identical bytes, followed by native deserialization checks
on hierarchy, properties, scripts and references. The schema is general, with no
LayerOne names or conventions.

## Outstanding integration and reproducibility work

The current command does not automatically turn a conversion manifest into
MeshParts or bind collision data, pivots, materials or terrain. Those are still
issue 7 work; plain instance serialization alone does not close it. Likewise,
no remote asset ids are generated here and no game is published.

Serialization uses the pinned bundled reflection database. The dependency's
`debug_always_use_bundled` Cargo feature bypasses local lookup in both debug and
release builds, despite its name. A subprocess regression supplies a deliberately
invalid `RBX_DATABASE` file and proves identical output rather than relying on a
clean environment. This addresses the reflection lookup boundary, not all of
issue 8's remaining build/link/script requirements.
