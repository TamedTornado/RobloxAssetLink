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

`scriptSource` may name a UTF-8 source file under the scene document directory for
Script, LocalScript or ModuleScript. Absolute paths and symlink/directory escapes
fail. This packs source text; it does not yet type-check, compile or run Luau.

This uses the MIT rbx-dom/rbx_binary libraries, not a Rojo subprocess. Repeated
builds are tested for identical bytes, followed by native deserialization checks
on hierarchy, properties, scripts and references. The schema is general, with no
LayerOne names or conventions.

## Outstanding integration and reproducibility work

The current command does not automatically turn a conversion manifest into
MeshParts or bind collision data, pivots, materials or terrain. Those are still
issue 7 work; plain instance serialization alone does not close it. Likewise,
no remote asset ids are generated here and no game is published.

Serialization explicitly selects the pinned bundled reflection database. However,
rbx_binary 3.0.0's constructor first consults its local override mechanism. The
command surfaces any error rather than allowing the constructor to panic, and
does not use the override for output semantics. Eliminating that initial lookup
is still required for issue 8's strict hermetic-build acceptance. Do not claim
that requirement complete based on byte repeatability in a clean environment.
