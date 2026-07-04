/// Token savings analytics — track how much tp saved you

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
    fs::create_dir_all(&dir).ok();
    dir.join("stats.json")
}

fn load() -> Stats {
    let path = stats_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

fn save(stats: &Stats) {
    let path = stats_path();
    if let Ok(json) = serde_json::to_string_pretty(stats) {
        fs::write(path, json).ok();
    }
}

pub fn record(kind: &str, cmd: &str, raw_tokens: u64, filtered_tokens: u64, time_ms: u64) {
    let mut stats = load();

    let savings_pct = if raw_tokens > 0 {
        ((raw_tokens as f64 - filtered_tokens as f64) / raw_tokens as f64 * 100.0).max(0.0)
    } else {
        0.0
    };

    stats.total_commands += 1;
    stats.total_raw_tokens += raw_tokens;
    stats.total_filtered_tokens += filtered_tokens;
    stats.total_time_ms += time_ms;

    let cmd_short: String = cmd.split_whitespace().take(3).collect::<Vec<&str>>().join(" ");
    stats.records.push(Record {
        kind: kind.to_string(),
        cmd: cmd_short,
        raw_tokens,
        filtered_tokens,
        savings_pct,
        time_ms,
    });

    if stats.records.len() > 1000 {
        stats.records.drain(..stats.records.len() - 1000);
    }

    save(&stats);
}

pub fn record_proxy(raw_tokens: u64, optimized_tokens: u64, cache_hit: bool) {
    let mut stats = load();
    stats.proxy_requests += 1;
    if cache_hit {
        stats.proxy_cache_hits += 1;
    }
    stats.proxy_tokens_saved += raw_tokens.saturating_sub(optimized_tokens);
    save(&stats);
}

pub fn show_stats() {
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

    println!();
    println!("  Token Pipeline Stats");
    println!("  {}", "=".repeat(50));

    if stats.total_commands > 0 {
        println!();
        println!("  CLI Filter (tp run):");
        println!("    Commands:        {}", stats.total_commands);
        println!("    Raw tokens:      {}", stats.total_raw_tokens);
        println!("    Filtered tokens: {}", stats.total_filtered_tokens);
        println!(
            "    Tokens saved:    {} ({:.1}%)",
            saved, pct
        );
        println!(
            "    Avg latency:     {:.1}ms/cmd",
            stats.total_time_ms as f64 / stats.total_commands as f64
        );
    }

    if stats.proxy_requests > 0 {
        println!();
        println!("  Proxy (tp proxy):");
        println!("    Requests:        {}", stats.proxy_requests);
        println!(
            "    Cache hits:      {} ({:.1}%)",
            stats.proxy_cache_hits,
            stats.proxy_cache_hits as f64 / stats.proxy_requests as f64 * 100.0
        );
        println!("    Tokens saved:    {}", stats.proxy_tokens_saved);
    }

    let total_saved = saved + stats.proxy_tokens_saved;
    if total_saved > 0 {
        println!();
        println!("  Cost Estimates:");
        let rates = [
            ("GPT-4o", 2.50),
            ("Claude Sonnet", 3.00),
            ("GPT-4o-mini", 0.15),
        ];
        for (model, rate) in &rates {
            let usd = total_saved as f64 / 1_000_000.0 * rate;
            println!(
                "    {} (${}/1M): ${:.4} saved",
                model, rate, usd
            );
        }
    }

    if !stats.records.is_empty() {
        println!();
        println!("  Recent:");
        for rec in stats.records.iter().rev().take(10) {
            println!(
                "    [{:5}] {:30} {:5} -> {:5} ({:.0}%)",
                rec.kind, rec.cmd, rec.raw_tokens, rec.filtered_tokens, rec.savings_pct
            );
        }
    }

    println!();
}
