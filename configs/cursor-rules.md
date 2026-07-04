---
description: Token optimization with tp CLI
globs: ["**/*"]
alwaysApply: true
---

# Token Pipeline

When executing shell commands, prefer `tp run <command>` to compress output and reduce context window usage.

## Auto-filtered commands:
- `tp run git status|diff|log` — compact git output
- `tp run cargo test|build` — failures/errors only  
- `tp run ls|find|tree` — compact listings
- `tp run grep|rg` — grouped results

## Text compression:
- `cat large_file | tp shrink` — compress verbose text

## Safety:
- Exit codes preserved exactly
- Error messages never suppressed
- Code blocks never modified
- Paths and URLs exact
