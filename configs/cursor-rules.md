---
description: Token Pipeline CLI optimization rules
globs: ["*"]
---

# tp (token-pipeline) v0.2.0

When executing shell commands in the terminal, prefix with `tp run` for automatic output filtering:

```bash
tp run git status
tp run cargo test
tp run dotnet build
```

For reading files into context, use `tp read`:
```bash
tp read src/main.rs
```

For compressing verbose text: `echo "text" | tp shrink`

tp preserves exit codes, errors, and code exactly. Only formatting noise is removed.
