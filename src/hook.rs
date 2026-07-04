/// `tp init` — Install auto-hooks for AI tools
///
/// Installs a bash hook that automatically rewrites commands
/// (e.g., `git status` → `tp run git status`)

use std::fs;
use std::path::PathBuf;

pub fn install_hook(target: &str) {
    match target {
        "bash" | "" => install_bash(),
        "hermes" => install_hermes(),
        _ => {
            eprintln!("Unknown target: {}. Use: bash, hermes", target);
            std::process::exit(1);
        }
    }
}

fn hook_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(&home).join(".local/share/token-pipeline")
}

fn install_bash() {
    let dir = hook_dir();
    fs::create_dir_all(&dir).ok();

    // Copy hook script
    let hook_src = include_str!("../scripts/hook.sh");
    let hook_dst = dir.join("hook.sh");
    fs::write(&hook_dst, hook_src).ok();

    println!("Hook installed to: {}", hook_dst.display());

    // Check if already in bashrc
    let bashrc_path = PathBuf::from(
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string()),
    ).join(".bashrc");

    let bashrc = fs::read_to_string(&bashrc_path).unwrap_or_default();
    if bashrc.contains("token-pipeline/hook.sh") {
        println!("  ✓ Already sourced in ~/.bashrc");
    } else {
        let line = format!("\n# Token Pipeline hook\nsource ~/.local/share/token-pipeline/hook.sh\n");
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&bashrc_path)
            .ok()
            .map(|mut f| { use std::io::Write; f.write_all(line.as_bytes()).ok() });
        println!("  ✓ Added to ~/.bashrc");
    }

    println!();
    println!("✅ Auto-hooks installed!");
    println!("   Commands like `git status`, `ls -la`, `cargo test` will");
    println!("   automatically run through tp for token optimization.");
    println!();
    println!("   To uninstall: remove the source line from ~/.bashrc");
    println!("   To bypass: use \\command (e.g. \\git status)");
}

fn install_hermes() {
    println!("Installing Hermes Agent integration...");

    // Add to vault CLAUDE.md if it exists
    let vault_paths = [
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string())).join("vault/CLAUDE.md"),
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string())).join("vault/06-SYSTEM/CLAUDE.md"),
    ];

    let tp_rule = "\n## Token Pipeline\n\nUse `tp run <command>` instead of plain shell commands to save 40-87% tokens.\n\n```bash\ntp run git status\ntp run ls -la\ntp run cargo test\n```\n";

    for path in &vault_paths {
        if path.exists() {
            let content = fs::read_to_string(path).unwrap_or_default();
            if content.contains("Token Pipeline") || content.contains("tp run") {
                println!("  ✓ Already in {}", path.display());
            } else {
                fs::write(path, format!("{}\n{}", content.trim(), tp_rule)).ok();
                println!("  ✓ Added to {}", path.display());
            }
        }
    }

    println!("✅ Hermes integration complete!");
    println!("   Hermes will now use tp automatically for shell commands.");
}
