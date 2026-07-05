use std::env;
use std::io::{self, Read};
use std::process::Command;
use std::time::Instant;

mod config;
mod discover;
mod error;
mod gain;
mod hook;
mod input_filter;
mod optimizer;
mod output_compress;
mod proxy;
mod read;
mod rewrite;
mod stats;

const VERSION: &str = "1.0.0";

fn main() {
    let raw_args: Vec<String> = env::args().skip(1).collect();

    if raw_args.is_empty() {
        print_help();
        return;
    }

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

    let cfg = config::Config::load();

    match args[0].as_str() {
        "run" => {
            if args.len() < 2 {
                eprintln!("Usage: tp run <command> [args...]");
                std::process::exit(1);
            }
            run_command(&args[1..], ultra, &cfg);
        }
        "proxy" => {
            let port = parse_flag(&args, "--port").unwrap_or_else(|| "8080".to_string());
            let upstream = parse_flag(&args, "--upstream")
                .or_else(|| cfg.upstream_url.clone())
                .unwrap_or_else(|| "http://localhost:8000".to_string());
            let mode = parse_flag(&args, "--mode").unwrap_or_else(|| cfg.compression_mode.clone());
            proxy::start_proxy(&port, &upstream, &mode);
        }
        "shrink" => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).unwrap_or_default();
            let mode = if args.len() > 1 {
                let m = &args[1];
                if m == "lite" || m == "full" || m == "ultra" {
                    m.as_str()
                } else {
                    "full"
                }
            } else if input.len() > 5000 {
                "ultra"
            } else if input.len() > 1000 {
                "full"
            } else {
                "lite"
            };
            let result = output_compress::compress_text(&input, mode);
            print!("{}", result);
            let raw_len = input.len();
            let out_len = result.len();
            if raw_len > 0 {
                let pct = (raw_len - out_len) as f64 / raw_len as f64 * 100.0;
                eprintln!(
                    "\x1b[2m[tp shrink: {} -> {} chars ({:.0}% saved)]\x1b[0m",
                    raw_len, out_len, pct
                );
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
        "read" => {
            if args.len() < 2 {
                eprintln!("Usage: tp read <file|dir>");
                std::process::exit(1);
            }
            let result = read::read_file(&args[1]);
            print!("{}", result);
        }
        "rewrite" => {
            if args.len() < 2 {
                eprintln!("Usage: tp rewrite <command string>");
                std::process::exit(1);
            }
            let cmd_str = args[1..].join(" ");
            let rewritten = rewrite::rewrite_command(&cmd_str, &cfg);
            println!("{}", rewritten);
        }
        "config" => {
            if args.get(1).map(|s| s.as_str()) == Some("init") {
                config::write_default_config();
            } else {
                println!("Config: {:?}", cfg);
                println!("\nUse `tp config init` to create default config.");
            }
        }
        "--help" | "-h" | "help" => print_help(),
        "--version" | "-V" => println!("tp (token-pipeline) v{}", VERSION),
        _ => run_command(&args, ultra, &cfg),
    }
}

fn run_command(args: &[String], ultra: bool, cfg: &config::Config) {
    let start = Instant::now();
    let (cmd, cmd_args) = (args[0].as_str(), &args[1..]);
    let full_cmd = args.join(" ");

    if cfg.is_excluded(cmd) {
        let mut command = Command::new(cmd);
        command.args(cmd_args);
        strip_tp_from_path(&mut command);
        match command.status() {
            Ok(status) => std::process::exit(status.code().unwrap_or(0)),
            Err(e) => {
                eprintln!("tp: failed to execute '{}': {}", cmd, e);
                std::process::exit(127);
            }
        }
    }

    let mut command = Command::new(cmd);
    command.args(cmd_args);
    strip_tp_from_path(&mut command);

    match command.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let raw = format!("{}{}", stdout, stderr);
            let exit_code = out.status.code().unwrap_or(-1);

            let filtered =
                input_filter::apply_with_ultra(&full_cmd, &stdout, &stderr, exit_code, ultra);
            print!("{}", filtered);

            let elapsed = start.elapsed();
            let raw_tokens = estimate_tokens(&raw);
            let filtered_tokens = estimate_tokens(&filtered);
            stats::record(
                "run",
                &full_cmd,
                raw_tokens,
                filtered_tokens,
                elapsed.as_millis() as u64,
            );

            let savings = raw.len().saturating_sub(filtered.len());
            if savings > 10 {
                let pct = savings as f64 / raw.len() as f64 * 100.0;
                eprintln!(
                    "\x1b[2m[tp: {} -> {} chars ({:.0}% saved) {:.0}ms]\x1b[0m",
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

fn strip_tp_from_path(command: &mut Command) {
    if let Ok(path) = std::env::var("PATH") {
        let clean_path: Vec<&str> = path
            .split(':')
            .filter(|p| !p.contains("tp-hooks") && !p.contains("token-pipeline"))
            .collect();
        command.env("PATH", clean_path.join(":"));
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
tp (token-pipeline) v{version}
CLI output filter + LLM proxy optimizer — replaces rtk + caveman

USAGE:
  tp [FLAGS] run <command> [args...]     Run command with output filtering
  tp proxy [options]                     Start OpenAI-compatible optimization proxy
  tp shrink [MODE]                       Compress stdin text (lite|full|ultra)
  tp stats                               Show token savings summary
  tp gain                                Detailed token savings analytics
  tp discover                            Find missed savings opportunities
  tp cache [clear]                       Show/clear response cache
  tp read <file|dir>                      Smart file reading for LLM context
  tp rewrite <cmd>                       Show tp-rewritten version of a command
  tp init [target]                       Install auto-hooks (bash|hermes|cursor|claude|codex|copilot)
  tp config [init]                       Show config / create default config
  tp help                                Show this help

FLAGS:
  -u, --ultra-compact    Maximum compression (extension counts, telegraphic)

PROXY OPTIONS:
  --port PORT        Listen port (default: 8080)
  --upstream URL     Upstream LLM API (from config or default)
  --mode MODE        Compression: lite|full|ultra (default: full)

EXAMPLES:
  tp run git status              Compact git status
  tp run git diff                Changed lines only
  tp run cargo test              Failures summary
  tp run dotnet test             .NET test summary
  tp -u run ls -la               Ultra-compact listing

  tp proxy --port 8080           Start proxy server
  echo "verbose text" | tp shrink   Compress any text

CONFIG:
  ~/.config/tp/config.toml       Global settings
  tp config init                 Create default config file

PIPELINE STAGES:
  1. Input   — command output filtering (remove noise, keep signal)
  2. Optimize — BLAKE3 cache + prompt compression
  3. Output  — Caveman-style response compression (terse prose)
"#,
        version = VERSION
    );
}
