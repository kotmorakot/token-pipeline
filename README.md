# Token Pipeline (tp)

**Full token optimization pipeline: RTK filter + KatGPT-RS optimize + Caveman compress**

ลด token usage 40-87% ทุกครั้งที่ AI agent รัน command หรือเรียก LLM

```
┌─────────────────────────────────────────────────────┐
│   Token Pipeline                                     │
│                                                      │
│   Stage 1: INPUT     RTK-style command filtering     │
│   Stage 2: OPTIMIZE  BLAKE3 cache + validation       │
│   Stage 3: OUTPUT    Caveman-style compression       │
│                                                      │
│   Result: 40-87% fewer tokens                        │
└─────────────────────────────────────────────────────┘
```

## Install

```bash
cd token-pipeline
cargo build --release
# Binary อยู่ที่ target/release/tp

# (Optional) copy ไปที่ PATH
cp target/release/tp ~/.local/bin/
```

## Quick Start

### CLI Mode — filter command output

```bash
tp run git status      # [main] staged: + file.rs
tp run git diff        # changed lines only
tp run git log -n 5    # one-line commits
tp run cargo test      # "ok 12 passed" (or failures detail)
tp run ls -la          # "4 dirs, 8 files"
tp run find . -name "*.rs"  # grouped by directory
```

### Shrink Mode — compress any text

```bash
echo "Sure! I'd be happy to help..." | tp shrink
# → removes filler, keeps substance

cat verbose_log.txt | tp shrink ultra
# → maximum compression
```

### Proxy Mode — optimize LLM API calls

```bash
# Start the proxy
tp proxy --port 8080 --upstream http://your-llm:8000

# Point your tool to the proxy
export OPENAI_BASE_URL=http://localhost:8080/v1
```

The proxy:
1. Compresses input prompts (removes filler)
2. Injects terse-response system prompt
3. Caches responses (BLAKE3 hash)
4. Compresses LLM output (Caveman-style)

### Statistics

```bash
tp stats
# Token Pipeline Stats
# Commands:        5
# Tokens saved:    1413 (87.1%)
```

## Architecture

```
User/Agent → tp run <cmd> → execute → filter output → compressed result
                                          ↑
                                    Stage 1: Input Filter
                                    (per-command rules)

IDE/Agent → tp proxy :8080 → compress prompt → cache check → upstream LLM
         ← compressed response ← cache store ← compress response ←
                    ↑                                      ↑
              Stage 3: Output                        Stage 2: Optimize
              (Caveman compress)                     (BLAKE3 cache)
```

## Integration with AI Tools

### Hermes Agent / Claude Code / Codex
Copy the appropriate config from `configs/`:
```bash
cp configs/hermes-AGENTS.md  /your-project/AGENTS.md
cp configs/claude-CLAUDE.md  /your-project/CLAUDE.md
cp configs/cursor-rules.md   /your-project/.cursor/rules/tp.md
```

### IDE Proxy (VS Code, Cursor, etc.)
1. Start: `tp proxy --port 8080 --upstream http://your-llm:8000`
2. Set your IDE's API base URL to `http://localhost:8080`
3. Every LLM call automatically gets optimized

## Compression Modes

| Mode | What it does | Savings |
|------|-------------|---------|
| `lite` | Remove filler/hedging, keep full sentences | 10-20% |
| `full` | Drop articles, use fragments, short words | 20-40% |
| `ultra` | Maximum compression, telegraphic style | 30-50% |

## Safety Guarantees

What tp **NEVER** modifies:
- Exit codes
- Error messages and stack traces
- Code blocks
- File paths and URLs
- Command syntax
- Technical terms
- Numbers and versions

## Documentation

- `HOW_FILTERS_WORK_TH.md` — หลักการ filter สำหรับ Junior Dev (ภาษาไทย)
- `configs/` — Integration configs for each AI tool

## Inspired By

- [RTK](https://github.com/rtk-ai/rtk) — CLI proxy for token reduction
- [KatGPT-RS](https://github.com/nickarls/katgpt-rs) — Neuro-symbolic optimization
- [Caveman](https://github.com/JuliusBrussee/caveman) — Output token compression
