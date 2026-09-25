# Roblox CLI development

- Primary product: a general-purpose offline Roblox build toolchain, not a
  LayerOne-specific importer. Conversion, script checks and place assembly must
  work without Studio, credentials or Roblox servers. Deployment is a separate
  stage that uploads prebuilt outputs and links remote IDs; never quietly replace
  local conversion with cloud conversion. Local gameplay is optional future work.
- Implement format encoders in Rust; other implementations are format references,
  not mandatory subprocess tools. Keep metric conversion and conventions in
  validated JSON. Record independent format evidence and test deterministic bytes.
- Native offline build plans replace the retired preview/catalog design; do not
  restore a parallel registry or preview geometry path.

- General-purpose, agent-first Roblox CLI. Assets are the first command group,
  not the scope of the whole tool. Do not build speculative command groups.
- The preview-only HTTP server and Studio plugin are retired. Do not recreate
  that transport or make offline builds depend on a companion process. Any future
  Studio automation is a separate feature, not a conversion/build dependency.
  No Electron or Node companion; any necessary executable remains Rust.
- CLI commands must return structured results/errors and meaningful exit status.
  Plan editing is not completed conversion. Never report temporary preview
  geometry as persistent, publishable Roblox assets.
- Keep project conventions in validated JSON configuration. Do not introduce
  a second source of project settings.
- No runtime game scripts and no automatic cloud publication.
- Preserve unrelated Studio instances. Build replacements before changing the
  managed library. Surface errors rather than returning empty success.
- Use apply_patch for manual edits. Run Rust unit/integration tests, Clippy and
  formatting before code commits. The removed preview plugin no longer needs
  lint or Rojo packaging; generated native model/place tests remain mandatory.
- Clearly distinguish CLI/package verification from live Studio acceptance.
