# Roblox CLI development

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
