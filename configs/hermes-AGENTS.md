# Token Pipeline v0.2.0 — Agent Instructions

## What is tp?

`tp` is a CLI middleware that reduces token consumption by filtering command outputs and compressing LLM responses. It sits between you and the system, making every interaction more efficient.

## Rules

### 1. Command Execution
Always use `tp run` to execute shell commands:

```bash
# Instead of:         Use:
git status            tp run git status
cargo test            tp run cargo test
ls -la                tp run ls -la
```

### 2. File Reading
For reading files with smart context extraction:

```bash
tp read src/main.rs       # signatures for large files, full for small
tp read .                 # directory summary
```

### 3. Text Compression
For large text inputs:

```bash
cat large_file.rs | tp shrink        # auto-detect mode
cat file.rs | tp shrink ultra        # maximum compression
```

### 4. Supported Commands (Auto-Filtered)
- `git status|diff|log|show|branch|push|pull` — compact git output
- `ls|find|tree` — compact file listings
- `cargo test|build|check|clippy|fmt` — failures/errors only
- `npm|yarn|pnpm test|build` — test summaries
- `grep|rg|ag` — grouped search results
- `docker ps|images|logs|compose` — compact container info
- `kubectl get|logs|describe` — compact k8s output
- `helm list|install|upgrade` — compact helm output
- `dotnet build|test|restore` — .NET summaries
- `terraform plan|apply` — change summaries
- `cat|head|tail` — smart file read
- Any other command — generic dedup + truncation

### 5. Statistics
```bash
tp stats              # summary
tp gain               # detailed analytics
tp discover           # find missed savings
```

### 6. Compound Commands
tp handles compound commands transparently when hooks are installed:
```bash
cargo fmt && cargo test       # both get filtered
git add . && git commit -m x  # both get filtered
```

## Safety Guarantees

NEVER modified: exit codes, error messages, stack traces, file paths, code content, URLs.
IS compressed: verbose formatting, duplicate lines, boilerplate, progress bars, redundant whitespace.
