# Published PBR runtime acceptance

`published-v8.rbxl` is the exact downloaded version 8 of private acceptance place
99388199272385, universe 10767994924. SHA-256:
`8ff5657eb6ce1637ac31a5c5ec3bb805b298139df21286e76ae305c6d3ee2edf`.
It was assembled locally and published with the place API, not saved through
Studio. It contains no game scripts. Remote IDs are disposable acceptance inputs,
not project conventions or offline build dependencies.

Five identical, original tangent-equipped UV spheres compare color alone, color
plus normal, color plus roughness, color plus metalness, and all four maps.
Mesh: 128273190276226. Maps: color 121746111803604, normal 95213658056821,
roughness 136450311959603, metalness 118473853785120.
Pack descriptors: 88856640313512, 120307194770369, 79807309168451,
109219752895700, 114053513275441 respectively. All descriptors were authored
locally, uploaded as TexturePack with expectedPrice 0, and processed by Roblox.

The accepted visual result is `docs/images/pbr-published-runtime-proof.png`.
It shows actual client play mode, not an editor or cloud thumbnail.

## Repeating the visual test

- Use a dedicated Studio session and the unchanged place file.
- Set the player's normal graphics-quality preference to manual level 10 for
  this diagnostic, retaining its previous value for restoration afterward.
  Editor RenderSettings quality alone does not establish the player setting.
- Run `play.luau` through Studio's RunScript acceptance interface. It adds only
  temporary camera, labels and a test deadline. It does not set material maps,
  replace meshes, upload assets, or repair the loaded scene.
- Inspect the running client viewport after the assets load. Color must be
  visible on every sphere; normal must change shading, roughness must change
  the highlight, and metalness must change reflections. A returned `completed`
  flag proves test execution ended, not that these visual assertions passed.
- Capture the actual viewport and check it. On this Wine/Xwayland installation,
  ImageMagick `import -window <observed-test-window-id>` successfully captures
  the Studio window; the plugin capture API was not needed.
- Let the test end and restore the previous graphics-quality preference.

The scene explicitly selects current Realistic lighting with diffuse/specular
scales 1 and global shadows. It includes Studio's standard
`RBX_LightingTechnologyUnifiedMigration=true` Lighting attribute, independently
observed in a Studio-published place. This prevents the obsolete Compatibility
Lighting migration dialog; it is scene metadata, not a disabled feature flag.
The canonical serialized time is `TimeOfDay`, not the nonserialized ClockTime
convenience property. Do not reopen the obsolete compatibility-lighting fixtures.

Automatic/low player graphics suppressed PBR differences in earlier runs.
The raw four-map place without pack references rendered gray in play mode.
Uploads being Active/Approved did not imply compressed representations were
ready: the engine initially reported those representations were being generated.
