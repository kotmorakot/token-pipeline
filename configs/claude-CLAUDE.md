# tp (token-pipeline) v0.2.0

## Shell Commands
Always prefix shell commands with `tp run`:
```bash
tp run git status
tp run cargo test
tp run ls -la
```

## File Reading
Use `tp read` for smart file context:
```bash
tp read src/main.rs       # extracts signatures from large files
tp read configs/           # directory listing
```

## Text Compression
```bash
cat file | tp shrink       # compress verbose text
```

## What tp Does
- Filters command output to reduce tokens (removes noise, keeps signal)
- Preserves exit codes, errors, code blocks, paths exactly
- Only compresses formatting, duplicates, and boilerplate

## Available Commands
- `tp run <cmd>` — run with output filtering
- `tp read <file>` — smart file reading
- `tp shrink [mode]` — compress stdin (lite|full|ultra)
- `tp stats` / `tp gain` — savings analytics
- `tp discover` — find unoptimized commands
