# Migration Guide: 0.x to 1.0.0

## Breaking Changes

### RTK dependency removed
tp no longer falls back to `rtk` for unknown commands. All commands are either filtered natively or passed through with `generic_compact`.

**Action**: You can uninstall `rtk` if tp was your only consumer.

### Proxy rewritten (tiny_http -> axum)
The proxy is now async. Behavior is identical but startup output format changed slightly.

**Action**: No action needed. Same endpoints, same API.

### Version constant
`tp --version` now shows `v1.0.0`.

## New Features to Adopt

### `tp init auto`
Auto-detects Hermes, Claude Code, Cursor, Codex CLI, and Copilot CLI.
```bash
tp init auto
```

### `tp read <file>`
Smart file reading for LLM context:
```bash
tp read src/main.rs    # signatures for large files
tp read .              # directory summary
```

### `tp rewrite <cmd>`
Preview how tp would rewrite a compound command:
```bash
tp rewrite "cargo fmt && cargo test"
# Output: tp run cargo fmt && tp run cargo test
```

### `tp config init`
Create a default config file:
```bash
tp config init
# Creates ~/.config/tp/config.toml
```

## Data Migration

### Cache files
Old cache files (32-char filenames) are still loaded. New entries use full 64-char BLAKE3 hashes. No migration needed -- old entries work until TTL expires.

### Stats
Stats format unchanged. `~/.local/share/token-pipeline/stats.json` is fully compatible.

### Hook wrappers
Existing wrappers in `~/.local/bin/tp-hooks/` continue to work. Run `tp init bash` to update with new commands (dotnet, terraform, etc.).
