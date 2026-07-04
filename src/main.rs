use std::env;
use std::io::{self, Read};
use std::process::Command;
use std::time::Instant;

mod input_filter;
mod optimizer;
mod output_compress;
mod proxy;
mod stats;
mod gain;
mod hook;
mod discover;

fn main() {
    let raw_args: Vec<String> = env::args().skip(1).collect();

    if raw_args.is_empty() {
        print_help();
        return;
    }

    // Parse global flags
    let mut ultra = false;
    let mut args = Vec::new();
    for a in &raw_args {
        match a.as_str() {
            "-u" | "--ultra-compact" => ultra = true,
            _ => args.push(a.clone()),
        }
    }

    if args.is_empty() {
        print_help();
        return;
    }

    match args[0].as_str() {
        "run" => {
            if args.len() < 2 {
                eprintln!("Usage: tp run <command> [args...]");
                std::process::exit(1);
            }
            run_command(&args[1..], ultra);
        }
        "proxy" => {
            let port = parse_flag(&args, "--port").unwrap_or_else(|| "8080".to_string());
            let upstream = parse_flag(&args, "--upstream")
                .unwrap_or_else(|| "http://10.7.55.64:8000".to_string());
            let mode = parse_flag(&args, "--mode").unwrap_or_else(|| "full".to_string());
            proxy::start_proxy(&port, &upstream, &mode);
        }
        "shrink" => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).unwrap_or_default();
            let mode = if args.len() > 1 {
                let m = &args[1];
                if m == "lite" || m == "full" || m == "ultra" { m.as_str() } else { "full" }
            } else {
                // Auto-detect mode based on content length
                if input.len() > 5000 { "ultra" }
                else if input.len() > 1000 { "full" }
                else { "lite" }
            };
            let result = output_compress::compress_text(&input, mode);
            print!("{}", result);
            let raw_len = input.len();
            let out_len = result.len();
            if raw_len > 0 {
                let pct = (raw_len - out_len) as f64 / raw_len as f64 * 100.0;
                eprintln!("\x1b[2m[tp shrink: {} → {} chars ({:.0}% saved)]\x1b[0m", raw_len, out_len, pct);
            }
        }
        "stats" => stats::show_stats(),
        "gain" => gain::show_gain(),
        "cache" => {
            if args.get(1).map(|s| s.as_str()) == Some("clear") {
                optimizer::clear_disk_cache();
                println!("Cache cleared.");
            } else {
                optimizer::show_cache_info();
            }
        }
        "init" => {
            let target = args.get(1).map(|s| s.as_str()).unwrap_or("bash");
            hook::install_hook(target);
        }
        "discover" => discover::show_discover(),
        "--help" | "-h" | "help" => print_help(),
        "--version" | "-V" => println!("tp (token-pipeline) v0.1.0"),
        _ => run_command(&args, ultra),
    }
}

fn run_command(args: &[String], ultra: bool) {
    let start = Instant::now();
    let (cmd, cmd_args) = (args[0].as_str(), &args[1..]);
    let full_cmd = args.join(" ");

    let mut command = Command::new(cmd);
    command.args(cmd_args);

    // Strip tp-hooks from PATH to avoid wrapper recursion
    if let Ok(path) = std::env::var("PATH") {
        let clean_path: Vec<&str> = path
            .split(':')
            .filter(|p| !p.contains("tp-hooks") && !p.contains("token-pipeline"))
            .collect();
        command.env("PATH", clean_path.join(":"));
    }

    let output = command.output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let raw = format!("{}{}", stdout, stderr);

            let filtered = input_filter::apply_with_ultra(&full_cmd, &stdout, &stderr, out.status.code().unwrap_or(-1), ultra);

            print!("{}", filtered);

            let elapsed = start.elapsed();
            let raw_tokens = estimate_tokens(&raw);
            let filtered_tokens = estimate_tokens(&filtered);

            stats::record("run", &full_cmd, raw_tokens, filtered_tokens, elapsed.as_millis() as u64);

            let savings = raw.len().saturating_sub(filtered.len());
            if savings > 10 {
                let pct = savings as f64 / raw.len() as f64 * 100.0;
                eprintln!(
                    "\x1b[2m[tp: {} → {} chars ({:.0}% saved) {:.0}ms]\x1b[0m",
                    raw.len(),
                    filtered.len(),
                    pct,
                    elapsed.as_millis()
                );
            }

            std::process::exit(out.status.code().unwrap_or(0));
        }
        Err(e) => {
            eprintln!("tp: failed to execute '{}': {}", cmd, e);
            std::process::exit(127);
        }
    }
}

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as f64 / 3.5).ceil() as u64
}

fn parse_flag(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn print_help() {
    println!(
        r#"
tp (token-pipeline) v0.1.0
Full pipeline: RTK filter → KatGPT-RS optimize → Caveman compress

USAGE:
  tp [FLAGS] run <command> [args...]     Run command with output compression
  tp proxy [options]                     Start OpenAI-compatible optimization proxy
  tp shrink [MODE]                       Compress stdin text (lite|full|ultra)
  tp stats                               Show token savings statistics
  tp gain                                Detailed token savings analytics
  tp discover                            Find missed savings opportunities
  tp cache [clear]                       Show/clear response cache
  tp init [target]                       Install auto-hooks (bash|hermes)
  tp help                                Show this help

FLAGS:
  -u, --ultra-compact    Maximum compression (extension counts, telegraphic format)

PROXY OPTIONS:
  --port PORT        Listen port (default: 8080)
  --upstream URL     Upstream LLM API (default: http://10.7.55.64:8000)
  --mode MODE        Compression: lite|full|ultra (default: full)

EXAMPLES:
  tp run git status              Compact git status
  tp run git diff                Changed lines only
  tp run cargo test              Failures summary
  tp run ls -la                  Compact listing
  tp -u run ls -la               Ultra-compact (file counts by extension)

  tp proxy --port 8080           Start proxy server
  echo "verbose text" | tp shrink   Compress any text

GLOBAL FLAGS:
  -u, --ultra-compact    Extra compression on ls, git status, etc.

PIPELINE STAGES:
  1. Input   — RTK-style command output filtering (remove noise)
  2. Optimize — BLAKE3 cache + constraint validation
  3. Output  — Caveman-style response compression (terse prose)
"#
    );
}
