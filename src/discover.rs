use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const TP_CAPABLE: &[&str] = &[
    "git", "ls", "cat", "bat", "head", "tail", "cargo", "npm", "pnpm", "yarn", "bun", "pytest",
    "jest", "vitest", "rspec", "grep", "rg", "ag", "find", "fd", "docker", "podman", "kubectl",
    "oc", "helm", "env", "printenv", "curl", "wget", "tree", "ps", "df", "make", "cmake",
    "ninja", "python", "python3", "node", "ruby", "php", "gh", "pip", "pip3", "uv", "tsc",
    "npx", "next", "dotnet", "terraform", "aws", "gcloud",
];

const TP_NOT_YET: &[&str] = &["nuxt", "pulumi", "ansible", "vagrant"];

fn shell_history_paths() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut paths = Vec::new();

    let bash_hist = PathBuf::from(&home).join(".bash_history");
    if bash_hist.exists() {
        paths.push(bash_hist);
    }

    let zsh_hist = PathBuf::from(&home).join(".zsh_history");
    if zsh_hist.exists() {
        paths.push(zsh_hist);
    }

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

    match stats.get("records").and_then(|r| r.as_array()) {
        Some(records) => records
            .iter()
            .filter_map(|r| r.get("cmd")?.as_str().map(|s| s.to_string()))
            .collect(),
        None => Vec::new(),
    }
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
            if let Some(pos) = l.find(';') {
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

    let mut all_cmds: Vec<String> = Vec::new();

    for path in &history_paths {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let cmds = if name == "stats.json" {
            parse_tp_stats()
        } else if name == ".zsh_history" {
            parse_zsh_history(path)
        } else {
            parse_bash_history(path)
        };
        all_cmds.extend(cmds);
    }

    let mut unoptimized: BTreeMap<String, u32> = BTreeMap::new();
    let mut already_optimized: BTreeMap<String, u32> = BTreeMap::new();

    for cmd in &all_cmds {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let is_tp = parts[0] == "tp"
            && parts
                .get(1)
                .map(|s| *s == "run")
                .unwrap_or(false);
        let is_rtk = parts[0] == "rtk";

        if is_tp || is_rtk {
            let actual_cmd = if is_tp {
                parts.get(2).unwrap_or(&"")
            } else {
                parts.get(1).unwrap_or(&"")
            };
            if !actual_cmd.is_empty() {
                *already_optimized
                    .entry(actual_cmd.to_string())
                    .or_insert(0) += 1;
            }
        } else if TP_CAPABLE.contains(&parts[0]) {
            *unoptimized.entry(parts[0].to_string()).or_insert(0) += 1;
        }
    }

    println!();
    println!("  tp Discover -- Missed Savings Opportunities");
    println!(
        "  {}",
        "=".repeat(58)
    );
    println!();

    let total_unoptimized: u32 = unoptimized.values().sum();
    let total_optimized: u32 = already_optimized.values().sum();

    if total_unoptimized == 0 && total_optimized > 0 {
        println!("  All commands already optimized!");
        println!("     {} commands are using tp.", total_optimized);
        println!();
        return;
    }

    println!(
        "  Found {} unoptimized commands ({} already optimized)",
        total_unoptimized, total_optimized
    );
    println!();

    if !unoptimized.is_empty() {
        println!("  Commands you should run through tp:");
        println!();

        let mut sorted: Vec<_> = unoptimized.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        println!("  {:<12} {:>6} {:>12}", "Command", "Count", "Est. Savings");
        println!("  {}", "-".repeat(40));

        for (cmd, count) in sorted.iter().take(15) {
            let est_saved = **count as u64 * 350;
            println!("  {:<12} {:>6} {:>10} tokens", cmd, count, est_saved);
        }
        println!();

        println!("  How to fix:");
        println!("    tp init              Install auto-hooks (bash)");
        println!("    tp init hermes       Install for Hermes Agent");
        println!("    Or manually: tp run <command> instead of <command>");
        println!();

        let missing_in_history: Vec<&str> = all_cmds
            .iter()
            .filter_map(|c| {
                let p = c.split_whitespace().next().unwrap_or("");
                if TP_NOT_YET.contains(&p) {
                    Some(p)
                } else {
                    None
                }
            })
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if !missing_in_history.is_empty() {
            println!("  Commands tp could support (not yet implemented):");
            for cmd in missing_in_history.iter().take(5) {
                println!("    -> {}", cmd);
            }
            println!();
        }
    }

    let total_est_tokens: u64 = unoptimized.values().map(|c| *c as u64 * 350).sum();
    if total_est_tokens > 0 {
        println!("  Estimated additional savings: {} tokens", total_est_tokens);
        println!(
            "  ({:.4} USD at GPT-4o rates)",
            total_est_tokens as f64 / 1_000_000.0 * 2.50
        );
    }

    println!();
}
