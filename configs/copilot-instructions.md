# Token Pipeline

Prefix shell commands with `tp run` to automatically compress output, reducing context window usage.

- `tp run git status` → compact status
- `tp run cargo test` → failures only
- `tp run ls -la` → compact listing  
- `cat file | tp shrink` → compress large text

tp never modifies exit codes, errors, code blocks, or paths. Only formatting noise is removed.
