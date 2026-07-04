# Token Pipeline

Use `tp run <command>` for shell commands to compress output and save tokens.
Use `tp shrink` to compress large text inputs via stdin pipe.

Examples:
  tp run git status → compact git status  
  tp run cargo test → failures summary only
  cat file.rs | tp shrink → key parts only

tp preserves: exit codes, errors, code blocks, paths, URLs.
tp removes: formatting noise, duplicate lines, verbose decorations.
