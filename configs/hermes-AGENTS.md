# Token Pipeline — Agent Instructions

## What is Token Pipeline?

`tp` is a CLI middleware that reduces token consumption by filtering command outputs and compressing LLM responses. It sits between you (the AI agent) and the system, making every interaction more efficient.

## Rules for Using tp

### 1. Command Execution
Always use `tp run` to execute shell commands instead of running them directly:

```bash
# Instead of:
git status

# Use:
tp run git status
```

This automatically compresses command output, removing noise that wastes your context window.

### 2. Supported Commands (Auto-Filtered)
These commands get smart filtering:
- `tp run git status|diff|log|show|branch` — compact git output
- `tp run ls|find|tree` — compact file listings  
- `tp run cargo test|build|check` — failures/errors only
- `tp run npm test|run build` — test summaries
- `tp run grep|rg` — grouped search results
- `tp run docker ps|images|logs` — compact container info
- `tp run cat|head|tail` — smart file read (key parts only)
- Any other command — generic dedup + truncation

### 3. Text Compression
For large text inputs, pipe through `tp shrink`:

```bash
cat large_file.rs | tp shrink
```

### 4. Statistics
Check how much you've saved:
```bash
tp run stats
```

## Important Notes

- `tp` never modifies the actual command behavior — only the OUTPUT is compressed
- Exit codes are preserved exactly
- Error messages are shown in full
- Code blocks in outputs are never modified
- If a filter seems wrong, run the command directly (without `tp run`)

## Compression Guarantee

What is NEVER modified:
- Exit codes
- Error messages and stack traces  
- File paths
- Code content
- URLs
- Command syntax

What IS compressed:
- Verbose formatting (git's decorative headers)
- Duplicate lines
- Boilerplate text (progress bars, loading indicators)
- Redundant whitespace
