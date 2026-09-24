# Asset Link development

- Rust executable, Luau Studio plugin. No Electron or Node companion.
- Keep project conventions in the required JSON configuration. The plugin UI
  edits that same file; do not introduce a second source of project settings.
- No runtime game scripts and no automatic cloud publication.
- Preserve unrelated Studio instances. Build replacements before changing the
  managed library. Surface errors rather than returning empty success.
- Use apply_patch for manual edits. Run Rust unit/integration tests, Clippy,
  formatting, plugin lint, and Rojo packaging before committing.
- Clearly distinguish CLI/package verification from live Studio acceptance.
