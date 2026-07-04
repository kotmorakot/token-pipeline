# Token Pipeline Integration

## Shell Commands
Prefix shell commands with `tp run` to compress output and save context window tokens.

```bash
tp run git status      # compact status
tp run git diff        # changed lines only
tp run cargo test      # failures summary
tp run ls -la          # compact listing
```

## Text Compression
Pipe large text through `tp shrink`:
```bash
cat verbose_output.log | tp shrink
```

## What tp Does
- Removes formatting noise from command output
- Deduplicates repeated lines
- Summarizes test/build results (shows failures only when all pass)
- Masks sensitive env vars
- Never modifies: exit codes, errors, code, paths, URLs

## What tp Does NOT Do
- Does not change command behavior
- Does not suppress errors
- Does not modify code content
- Does not alter exit codes
