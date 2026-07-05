# tp (token-pipeline) v0.2.0

When running shell commands, use `tp run` prefix for token-efficient output:
- `tp run git status` — compact git output
- `tp run cargo test` — test summary only
- `tp run ls -la` — compact file listing
- `tp read <file>` — smart file context extraction
- `echo "text" | tp shrink` — compress verbose text

tp preserves exit codes, errors, and code exactly. Only formatting noise is removed.
