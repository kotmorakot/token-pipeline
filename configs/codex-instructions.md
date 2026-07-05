# tp (token-pipeline) v0.2.0

Prefix shell commands with `tp run` to reduce token usage:
- `tp run git status` instead of `git status`
- `tp run cargo test` instead of `cargo test`
- `tp read <file>` for smart file context extraction
- `echo "text" | tp shrink` to compress verbose output

tp preserves exit codes, errors, and code exactly. Only noise is removed.
