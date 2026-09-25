# Roblox CLI development

- Primary product: a general-purpose offline Roblox build toolchain, not a
  LayerOne-specific importer. Conversion, script checks and place assembly must
  work without Studio, credentials or Roblox servers. Deployment is a separate
  stage that uploads prebuilt outputs and links remote IDs; never quietly replace
  local conversion with cloud conversion. Local gameplay is optional future work.
- Implement format encoders in Rust; other implementations are format references,
  not mandatory subprocess tools. Keep metric conversion and conventions in
  validated JSON. Record independent format evidence and test deterministic bytes.
- The experimental Studio preview is not the offline build architecture. Retire
  it as its replacement is validated; do not add new build dependencies on it.

- General-purpose, agent-first Roblox CLI. Assets are the first command group,
  not the scope of the whole tool. Do not build speculative command groups.
- Separate Rust CLI and server executables; Luau Studio adapter only where
  necessary. No Electron or Node companion. Prefer supported existing Studio
  automation capabilities before implementing another transport.
- CLI commands must return structured results/errors and meaningful exit status.
  Catalog registration is not a completed import. Never report temporary preview
  geometry as persistent, publishable Roblox assets.
- Keep project conventions in the required JSON configuration. The plugin UI
  edits that same file; do not introduce a second source of project settings.
- No runtime game scripts and no automatic cloud publication.
- Preserve unrelated Studio instances. Build replacements before changing the
  managed library. Surface errors rather than returning empty success.
- Use apply_patch for manual edits. Run Rust unit/integration tests, Clippy,
  formatting, plugin lint, and Rojo packaging before committing.
- Clearly distinguish CLI/package verification from live Studio acceptance.
