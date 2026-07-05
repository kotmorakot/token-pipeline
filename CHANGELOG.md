# Changelog

## [1.0.0] - 2026-07-05

### Added
- **Rewrite engine** (`tp rewrite`) for transparent compound command splitting (`&&`, `||`, `;`, `|`)
- **`tp read`** meta-command for smart file reading (signatures, config formatting, binary detection)
- **Async proxy** rewritten with axum + tokio for production-grade performance
- **SSE streaming compression** for `stream: true` requests
- **Local LLM auto-detection** -- skips prompt compression for private IPs
- **`tp init auto`** -- auto-detects installed agents and configures all
- **5 agent integrations**: Hermes, Claude Code, Cursor, Codex CLI, Copilot CLI
- **`tp config init`** -- creates `~/.config/tp/config.toml` with defaults
- **`.NET CLI filters`** -- `dotnet build`, `dotnet test`, `dotnet restore`
- **Terraform filters** -- `terraform plan`, `terraform apply`, `terraform init`
- **Helm filters** -- `helm list`, `helm install`, `helm upgrade`
- **Docker Compose filter** improvements
- **kubectl describe** filter with key field extraction
- Full BLAKE3 cache keys (64 chars), TTL expiration, LRU eviction
- Comprehensive test suite: 54 unit tests + 13 integration tests
- GitHub Actions CI/CD with cross-compilation for 4 platforms
- `install.sh` for one-line installation

### Changed
- **RTK dependency removed** -- tp is now fully standalone
- Proxy rewritten from `tiny_http` (sync) to `axum` + `tokio` (async)
- Cache keys use full 64-char BLAKE3 hashes instead of truncated 32-char
- `env_compact` uses precise sensitive patterns (`SECRET`, `TOKEN`, `PASSWORD`, `CREDENTIAL`, `PRIVATE`) instead of over-matching on `KEY` and `AUTH`
- `ls_compact` handles both plain and long format (`ls` and `ls -la`)
- `smart_read` returns full content for files under 100 lines
- `git_diff` uses correct file header placement without `rposition("---")` bug
- Error handling uses `eprintln!` warnings instead of silent `.ok()` swallowing
- All dead code warnings eliminated

### Removed
- RTK fallback (`use_rtk` branch in `run_command`)
- `ConstraintValidator`, `ValidationResult`, `extract_json_from_response` (dead code)
- `forward_post_raw`, `DeltaChoice`, `DeltaMessage`, `StreamChunk` (dead code)
- `HistoryEntry` from discover.rs (dead code)
- `tiny_http` dependency (replaced by axum)
- `Box::leak` usage in tree_compact and df_compact

## [0.1.0] - 2026-07-04

### Added
- Initial release
- 3-stage pipeline: Input filter -> KatGPT-RS optimizer -> Caveman compress
- CLI tool (`tp run`, `tp shrink`, `tp stats`, `tp gain`, `tp discover`)
- OpenAI-compatible proxy with BLAKE3 caching
- RTK fallback for unknown commands
- Bash wrapper hooks (`tp init`)
