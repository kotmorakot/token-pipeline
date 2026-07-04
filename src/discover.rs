/// `tp discover` — Find missed savings opportunities
///
/// Scans shell history and shows commands that could be optimized by tp

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct HistoryEntry {
    cmd: String,
    count: u32,
    estimated_raw_chars: u64,
    estimated_saved_chars: u64,
}

/// Commands that tp CAN optimize
const TP_CAPABLE: &[&str] = &[
    "git", "ls", "cat", "bat", "head", "tail",
    "cargo", "npm", "pnpm", "yarn", "bun",
    "pytest", "jest", "vitest", "rspec",
    "grep", "rg", "ag", "find", "fd",
    "docker", "podman", "kubectl", "oc", "helm",
    "env", "printenv", "curl", "wget",
    "tree", "ps", "df",
    "make", "cmake", "ninja",
    "python", "python3", "node", "ruby", "php",
    "gh",
];

/// Common commands that tp could optimize but doesn't yet
const TP_CAPABLE_BUT_MISSING: &[&str] = &[
    "tsc", "npx", "next", "nuxt",
    "pip", "pip3", "uv",
    "aws", "gcloud",
    "terraform", "pulumi",
    "cargo", "rustc",
];

fn shell_history_paths() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut paths = Vec::new();

    // Bash history
    let bash_hist = PathBuf::from(&home).join(".bash_history");
    if bash_hist.exists() {
        paths.push(bash_hist);
    }

    // Zsh history
    let zsh_hist = PathBuf::from(&home).join(".zsh_history");
    if zsh_hist.exists() {
        paths.push(zsh_hist);
    }

    // tp stats file (recent commands)
    let tp_stats = PathBuf::from(&home).join(".local/share/token-pipeline/stats.json");
    if tp_stats.exists() {
        paths.push(tp_stats);
    }

    paths
}

fn parse_tp_stats() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let path = PathBuf::from(&home).join(".local/share/token-pipeline/stats.json");

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let stats: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let records = match stats.get("records") {
        Some(r) => match r.as_array() {
            Some(a) => a,
            None => return Vec::new(),
        },
        None => return Vec::new(),
    };

    records
        .iter()
        .filter_map(|r| r.get("cmd")?.as_str().map(|s| s.to_string()))
        .collect()
}

fn parse_bash_history(path: &PathBuf) -> Vec<String> {
    let content = fs::read_to_string(path).unwrap_or_default();
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect()
}

fn parse_zsh_history(path: &PathBuf) -> Vec<String> {
    let content = fs::read_to_string(path).unwrap_or_default();
    content
        .lines()
        .filter_map(|l| {
            // Zsh format: ": 1234567890:0;command"
            if let Some(pos) = l.find(";") {
                Some(l[pos + 1..].trim().to_string())
            } else {
                Some(l.trim().to_string())
            }
        })
        .filter(|l| !l.is_empty())
        .collect()
}

pub fn show_discover() {
    let history_paths = shell_history_paths();

    if history_paths.is_empty() {
        println!("No shell history found.");
        println!("Run some commands first, then try again.");
        return;
    }

    // Collect all commands from all sources
    let mut all_cmds: Vec<String> = Vec::new();

    for path in &history_paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let cmds = if name == "stats.json" {
            parse_tp_stats()
        } else if name == ".zsh_history" {
            parse_zsh_history(path)
        } else {
            parse_bash_history(path)
        };
        all_cmds.extend(cmds);
    }

    // Count unoptimized commands by category
    let mut unoptimized: BTreeMap<String, u32> = BTreeMap::new();
    let mut already_optimized: BTreeMap<String, u32> = BTreeMap::new();

    for cmd in &all_cmds {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let is_tp = parts[0] == "tp" && parts.get(1).map(|s| *s == "run").unwrap_or(false);
        let is_rtk = parts[0] == "rtk";

        if is_tp || is_rtk {
            let actual_cmd = if is_tp { parts.get(2).unwrap_or(&"") } else { parts.get(1).unwrap_or(&"") };
            if !actual_cmd.is_empty() {
                *already_optimized.entry(actual_cmd.to_string()).or_insert(0) += 1;
            }
        } else if TP_CAPABLE.contains(&parts[0]) {
            *unoptimized.entry(parts[0].to_string()).or_insert(0) += 1;
        }
    }

    // ── Report ──
    println!();
    println!("  tp Discover — Missed Savings Opportunities");
    println!("  ════════════════════════════════════════════════════════════");
    println!();

    let total_unoptimized: u32 = unoptimized.values().sum();
    let total_optimized: u32 = already_optimized.values().sum();

    if total_unoptimized == 0 && total_optimized > 0 {
        println!("  ✅ All commands already optimized!");
        println!("     {} commands are using tp.", total_optimized);
        println!();
        return;
    }

    println!("  Found {} unoptimized commands ({} already optimized)",
        total_unoptimized, total_optimized);
    println!();

    if !unoptimized.is_empty() {
        println!("  Commands you should run through tp:");
        println!();

        let mut sorted: Vec<_> = unoptimized.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        println!("  {:<12} {:>6} {:>12}", "Command", "Count", "Est. Savings");
        println!("  ────────────────────────────────────────────");

        for (cmd, count) in sorted.iter().take(15) {
            // Estimate: each unoptimized command costs ~500 tokens on average
            let est_saved = **count as u64 * 350;
            println!("  {:<12} {:>6} {:>10} tokens", cmd, count, est_saved);
        }
        println!();

        // Show how to fix
        println!("  How to fix:");
        println!("    tp init              Install auto-hooks (bash)");
        println!("    tp init hermes       Install for Hermes Agent");
        println!("    Or manually: tp run <command> instead of <command>");
        println!();

        // Show tp's current capability gaps
        println!("  Commands tp could support (not yet implemented):");
        let mut missing_count = 0;
        for cmd in all_cmds.iter().filter(|c| {
            let p = c.split_whitespace().next().unwrap_or("");
            TP_CAPABLE_BUT_MISSING.contains(&p) && !TP_CAPABLE.contains(&p)
        }) {
            if missing_count < 5 {
                println!("    → tp could optimize: {}", cmd);
                missing_count += 1;
            }
        }
        if missing_count > 0 {
            println!();
        }
    }

    // How much they could save
    let total_est_tokens: u64 = unoptimized.values().map(|c| *c as u64 * 350).sum();
    if total_est_tokens > 0 {
        println!("  Estimated additional savings: {} tokens", total_est_tokens);
        println!("  ({:.4} USD at GPT-4o rates)", total_est_tokens as f64 / 1_000_000.0 * 2.50);
    }

    println!();
}
