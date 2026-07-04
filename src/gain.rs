/// `tp gain` — Detailed token savings analytics (inspired by `rtk gain`)
///
/// Shows per-command-type breakdown, session info, and efficiency meter.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct Stats {
    total_commands: u64,
    total_raw_tokens: u64,
    total_filtered_tokens: u64,
    total_time_ms: u64,
    proxy_requests: u64,
    proxy_cache_hits: u64,
    proxy_tokens_saved: u64,
    records: Vec<Record>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    kind: String,
    cmd: String,
    raw_tokens: u64,
    filtered_tokens: u64,
    savings_pct: f64,
    time_ms: u64,
}

fn stats_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(&home).join(".local/share/token-pipeline");
    dir.join("stats.json")
}

fn load() -> Stats {
    let path = stats_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

/// Categorize a command into a group for the breakdown
fn categorize(cmd: &str) -> &'static str {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return "other";
    }
    match parts[0] {
        "git" => "git",
        "ls" | "dir" | "exa" | "eza" | "find" | "fd" | "tree" => "files",
        "cat" | "bat" | "head" | "tail" | "less" | "more" => "read",
        "cargo" => "cargo",
        "npm" | "pnpm" | "yarn" | "bun" | "pip" | "uv" => "pkg",
        "pytest" | "jest" | "vitest" | "rspec" | "go" | "make" | "cmake" | "ninja" => "build",
        "docker" | "podman" => "docker",
        "kubectl" | "oc" | "helm" => "k8s",
        "grep" | "rg" | "ag" => "search",
        "env" | "printenv" | "curl" | "wget" | "httpie" => "net",
        "python" | "python3" | "node" | "ruby" | "php" => "run",
        _ => "other",
    }
}

pub fn show_gain() {
    let stats = load();

    if stats.total_commands == 0 && stats.proxy_requests == 0 {
        println!("No data yet. Use `tp run <command>` or `tp proxy` to start.");
        return;
    }

    let saved = stats
        .total_raw_tokens
        .saturating_sub(stats.total_filtered_tokens);
    let pct = if stats.total_raw_tokens > 0 {
        saved as f64 / stats.total_raw_tokens as f64 * 100.0
    } else {
        0.0
    };

    // ── Header ──
    println!();
    println!("  tp Token Savings Report");
    println!("  ════════════════════════════════════════════════════════════");
    println!();
    println!("  Total commands:    {}", stats.total_commands);
    println!("  Input tokens:      {}", stats.total_raw_tokens);
    println!("  Output tokens:     {}", stats.total_filtered_tokens);
    println!("  Tokens saved:      {} ({:.1}%)", saved, pct);
    println!("  Total exec time:   {}ms (avg {:.0}ms/cmd)", stats.total_time_ms,
             if stats.total_commands > 0 { stats.total_time_ms as f64 / stats.total_commands as f64 } else { 0.0 });

    // ── Efficiency Meter ──
    let pct_int = pct as u32;
    let bar_len = pct_int.min(100) as usize / 5;  // 20 chars max
    let empty_len = 20usize.saturating_sub(bar_len);
    println!("  Efficiency: █{}░{} {:.1}%", "█".repeat(bar_len), "░".repeat(empty_len), pct);
    println!();

    // ── By Command Type ──
    let mut by_cat: BTreeMap<&str, Vec<&Record>> = BTreeMap::new();
    for rec in &stats.records {
        let cat = categorize(&rec.cmd);
        by_cat.entry(cat).or_default().push(rec);
    }

    if !by_cat.is_empty() {
        println!("  By Command Type");
        println!("  ────────────────────────────────────────────────────────────────");
        println!("  {:<10} {:>8} {:>10} {:>12} {:>8} {:>8}", "Category", "Count", "Raw Tkns", "Filtered Tkns", "Saved%", "Impact");
        println!("  ────────────────────────────────────────────────────────────────");

        let mut sorted: Vec<_> = by_cat.iter().collect();
        sorted.sort_by(|(_, a), (_, b)| {
            let a_saved: u64 = a.iter().map(|r| r.raw_tokens.saturating_sub(r.filtered_tokens)).sum();
            let b_saved: u64 = b.iter().map(|r| r.raw_tokens.saturating_sub(r.filtered_tokens)).sum();
            b_saved.cmp(&a_saved)
        });

        for (cat, recs) in &sorted {
            let count = recs.len();
            let raw: u64 = recs.iter().map(|r| r.raw_tokens).sum();
            let filtered: u64 = recs.iter().map(|r| r.filtered_tokens).sum();
            let cat_saved = raw.saturating_sub(filtered);
            let cat_pct = if raw > 0 { cat_saved as f64 / raw as f64 * 100.0 } else { 0.0 };

            // Impact bar (relative to total saved)
            let impact = if saved > 0 {
                let impact_pct = cat_saved as f64 / saved as f64;
                let impact_bars = (impact_pct * 10.0).ceil() as usize;
                format!("{:█<10}", "█".repeat(impact_bars.min(10)))
            } else {
                "░░░░░░░░░░".to_string()
            };

            println!("  {:<10} {:>8} {:>10} {:>12} {:>7.1}% {:>10}", cat, count, raw, filtered, cat_pct, impact);
        }
        println!();
    }

    // ── Cost Estimates ──
    let total_saved = saved + stats.proxy_tokens_saved;
    if total_saved > 0 {
        println!("  Cost Estimates (per 1M tokens):");
        println!();
        let rates = [
            ("GPT-4o",     2.50,   "high"),
            ("Claude Son", 3.00,   "high"),
            ("GPT-4o-mini",0.15,   "low"),
            ("DeepSeek V4",0.50,   "mid"),
        ];
        for (model, rate, tier) in &rates {
            let usd = total_saved as f64 / 1_000_000.0 * rate;
            println!("    {:<14} ${:<5.2}/1M  →  ${:<7.4} saved  [{} cost]", model, rate, usd, tier);
        }
        println!();
    }

    // ── Top Savers ──
    if !stats.records.is_empty() {
        println!("  Top 5 Biggest Saves:");
        println!();
        let mut sorted: Vec<&Record> = stats.records.iter().collect();
        sorted.sort_by(|a, b| {
            let a_saved = a.raw_tokens.saturating_sub(a.filtered_tokens);
            let b_saved = b.raw_tokens.saturating_sub(b.filtered_tokens);
            b_saved.cmp(&a_saved)
        });

        for rec in sorted.iter().take(5) {
            let saved_tk = rec.raw_tokens.saturating_sub(rec.filtered_tokens);
            println!("    {:<30} {:>5} → {:>5} ({:.0}%) [+{} tokens]", rec.cmd, rec.raw_tokens, rec.filtered_tokens, rec.savings_pct, saved_tk);
        }
        println!();
    }

    // ── Recent History ──
    if !stats.records.is_empty() {
        println!("  Recent Commands:");
        println!();
        for rec in stats.records.iter().rev().take(10) {
            println!("    [{:5}] {:<30} {:>5} → {:>5} ({:>4.0}%) {:>4}ms",
                rec.kind, rec.cmd, rec.raw_tokens, rec.filtered_tokens, rec.savings_pct, rec.time_ms);
        }
        println!();
    }

    // ── Proxy Stats ──
    if stats.proxy_requests > 0 {
        println!("  Proxy (tp proxy):");
        println!("    Requests:        {}", stats.proxy_requests);
        println!("    Cache hits:      {} ({:.1}%)",
            stats.proxy_cache_hits,
            stats.proxy_cache_hits as f64 / stats.proxy_requests as f64 * 100.0
        );
        println!("    Tokens saved:    {}", stats.proxy_tokens_saved);
        println!();
    }
}
