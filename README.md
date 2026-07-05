# tp — token-pipeline

**Save tokens. Keep the signal. Drop the noise.**

`tp` sits between your AI agent and the shell/API. It compresses command output before it hits the context window, and compresses LLM responses before you pay for them.

Replaces [rtk-ai](https://github.com/rtk-ai/rtk) (CLI filtering) and [Caveman](https://github.com/juliusbrussee/caveman) (response compression) in one standalone binary.

```
  Agent  →  tp run git status  →  [filter]  →  compact output  →  Agent
  Agent  →  tp proxy           →  [cache + compress]  →  LLM API
```

---

## Why use tp?

| Problem | tp solution |
|---------|-------------|
| `git status` fills 200 tokens with decoration | `[main] clean` — 5 tokens |
| `cargo test` dumps 500 lines on success | `ok 42 tests passed` — one line |
| Same LLM prompt asked twice | BLAKE3 cache — instant, free |
| Verbose AI replies cost money | Caveman compression — terse prose, exact code |

**What stays exact:** exit codes, errors, stack traces, file paths, code blocks, URLs.

**What gets trimmed:** progress bars, duplicate lines, git headers, filler words, hedging.

---

## Install

**One-liner (recommended):**

```bash
curl -fsSL https://raw.githubusercontent.com/kotmorakot/token-pipline/main/install.sh | bash
```

**From source:**

```bash
git clone https://github.com/kotmorakot/token-pipline.git
cd token-pipline
cargo install --path .
```

Verify:

```bash
tp --version   # tp (token-pipeline) v1.0.0
```

---

## 30-second start

```bash
# 1. Filter a command (most common use)
tp run git status
tp run cargo test

# 2. See how much you saved
tp stats

# 3. Auto-wrap commands for your AI agent
tp init auto
```

After `tp init auto`, commands like `git status` are rewritten to `tp run git status` automatically.

---

## How it works

Three stages, two entry points:

```
CLI path:   shell command  →  Stage 1 Filter  →  compact output to agent

Proxy path: LLM request     →  Stage 2 Cache   →  Stage 3 Compress  →  LLM
                              (skip if cached)
```

| Stage | What it does | When |
|-------|--------------|------|
| **1. Filter** | Strips noise from command output | `tp run` |
| **2. Optimize** | BLAKE3 cache + prompt compression | `tp proxy` |
| **3. Compress** | Caveman-style terse responses | `tp proxy`, `tp shrink` |

---

## Commands

### Run & read

| Command | What it does |
|---------|--------------|
| `tp run <cmd>` | Run a command, print filtered output |
| `tp read <file>` | Smart file read — signatures for large files, full content for small |
| `tp rewrite "<cmd>"` | Preview how tp would wrap a compound command |

```bash
tp run git diff                    # changed lines only
tp read src/main.rs                # key functions, not every line
tp rewrite "cargo fmt && cargo test"
# → tp run cargo fmt && tp run cargo test
```

### Compress text

| Command | What it does |
|---------|--------------|
| `tp shrink` | Compress stdin (auto mode) |
| `tp shrink lite` | Remove filler, keep sentences |
| `tp shrink full` | Drop articles, use fragments (default) |
| `tp shrink ultra` | Maximum brevity |

```bash
echo "Sure! I'd be happy to help..." | tp shrink
```

### Proxy (LLM API)

```bash
tp proxy --port 8080 --upstream https://api.openai.com
export OPENAI_BASE_URL=http://localhost:8080/v1
```

| Endpoint | Purpose |
|----------|---------|
| `POST /v1/chat/completions` | Full pipeline: cache → compress prompt → compress response |
| `GET /v1/models` | Pass-through to upstream |
| `GET /health` | Status + cache stats |
| `GET /v1/stats` | Detailed savings analytics |
| `POST /v1/cache/clear` | Clear response cache |

Local LLMs (private IP like `http://10.x.x.x:8000`) skip prompt compression automatically.

### Analytics

| Command | What it does |
|---------|--------------|
| `tp stats` | Quick savings summary |
| `tp gain` | Per-category breakdown + cost estimates |
| `tp discover` | Scan shell history for unoptimized commands |
| `tp cache` | Show cache size; `tp cache clear` to reset |

### Setup

| Command | What it does |
|---------|--------------|
| `tp init auto` | Detect agents, install hooks for all |
| `tp init hermes` | Hermes Agent |
| `tp init claude` | Claude Code |
| `tp init cursor` | Cursor |
| `tp init codex` | Codex CLI |
| `tp init copilot` | Copilot CLI |
| `tp init bash` | PATH wrappers only |
| `tp config init` | Create `~/.config/tp/config.toml` |

---

## Supported commands (~50 filters)

tp has native filters for these. Anything else gets generic dedup + truncation.

| Category | Commands |
|----------|----------|
| Git | `status`, `diff`, `log`, `show`, `branch`, `push`, `pull`, `commit`, … |
| Build | `cargo`, `npm`, `pnpm`, `yarn`, `bun`, `make`, `cmake`, `dotnet`, `tsc` |
| Test | `cargo test`, `pytest`, `jest`, `vitest`, `rspec`, `dotnet test` |
| Files | `ls`, `find`, `fd`, `tree`, `cat`, `head`, `tail` |
| Search | `grep`, `rg`, `ag` |
| Containers | `docker`, `podman`, `kubectl`, `helm` |
| Infra | `terraform`, `aws`, `gcloud` |
| Other | `env`, `curl`, `wget`, `ps`, `df`, `gh`, `pip`, `python`, `node` |

---

## Configuration

Create a config file:

```bash
tp config init
```

Edit `~/.config/tp/config.toml`:

```toml
# Upstream LLM for tp proxy
upstream_url = "https://api.openai.com"

# Compression: lite | full | ultra
compression_mode = "full"

# Commands tp should NOT filter (run raw)
exclude_commands = ["ssh", "vim"]

# Response cache
cache_ttl_secs = 3600
cache_max_entries = 1000
```

CLI flags override config values.

---

## Agent integration

tp works with 5 AI coding agents. One command sets up everything:

```bash
tp init auto
```

This detects installed agents, installs PATH wrappers, and writes agent-specific config files:

| Agent | Config file created |
|-------|---------------------|
| Hermes | `AGENTS.md` (merged) |
| Claude Code | `CLAUDE.md` |
| Cursor | `.cursor/rules/tp.md` |
| Codex CLI | `codex_instructions.md` |
| Copilot CLI | `.github/copilot-instructions.md` |

**Bypass tp** when you need raw output: use the full path (`/usr/bin/git status`).

---

## Examples

**Before / after — git status:**

```
# Without tp (~180 tokens)
On branch main
Your branch is up to date with 'origin/main'.
nothing to commit, working tree clean

# With tp (~5 tokens)
[main] clean
```

**Before / after — cargo test (all pass):**

```
# Without tp (~400 tokens)
running 42 tests
test foo ... ok
test bar ... ok
... (40 more lines)

# With tp (~8 tokens)
ok test result: ok. 42 passed; 0 failed
```

**Proxy with local LLM:**

```bash
tp proxy --port 8080 --upstream http://10.7.55.64:8000
# Detects private IP → skips prompt compression (local = free tokens)
```

---

## Project layout

```
token-pipeline/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── input_filter.rs   # Stage 1: command output filters
│   ├── optimizer.rs      # Stage 2: BLAKE3 cache + prompt compress
│   ├── output_compress.rs # Stage 3: Caveman compression
│   ├── proxy.rs          # Async OpenAI-compatible proxy (axum)
│   ├── rewrite.rs        # Compound command rewriter
│   ├── read.rs           # tp read meta-command
│   ├── config.rs         # ~/.config/tp/config.toml
│   └── hook.rs           # tp init agent setup
├── configs/              # Agent instruction templates
├── tests/                # Integration tests
└── install.sh            # One-line installer
```

---

## Docs

| File | Description |
|------|-------------|
| [HOW_FILTERS_WORK_TH.md](./HOW_FILTERS_WORK_TH.md) | Filter principles explained (Thai, for junior devs) |
| [CHANGELOG.md](./CHANGELOG.md) | Release history |
| [MIGRATION.md](./MIGRATION.md) | Upgrade guide from 0.x |

---

## Development

```bash
cargo test          # 67 tests (unit + integration)
cargo build --release
./target/release/tp --version
```

---

## License

MIT
