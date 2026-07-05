/// `tp init` — Install auto-hooks for AI tools
///
/// Installs wrapper scripts that automatically rewrite commands
/// (e.g., `git status` → `tp run git status`)
///
/// Approach: Creates tiny wrapper scripts in ~/.local/bin/tp-hooks/
/// and adds that directory to PATH. This is more reliable than
/// DEBUG trap because:
///   - Works in interactive AND non-interactive shells
///   - Preserves pipes, redirects, exit codes
///   - No trap hijacking
///   - Each command explicitly wrapped

use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Commands that tp can intercept
const TP_COMMANDS: &[&str] = &[
    "git", "ls", "cat", "bat", "head", "tail", "less", "more",
    "cargo", "npm", "pnpm", "yarn", "bun",
    "pytest", "jest", "vitest", "rspec",
    "grep", "rg", "ag", "find", "fd",
    "docker", "podman", "kubectl", "oc", "helm",
    "env", "printenv", "curl", "wget", "httpie",
    "tree", "ps", "df",
    "make", "cmake", "ninja",
    "python", "python3", "node", "ruby", "php",
    "gh", "pip", "pip3", "uv", "tsc", "npx", "next",
    // rtk fallback wrappers
    "aws", "gcloud", "dotnet", "diff", "log", "summary",
    "json", "deps", "psql", "terraform", "pulumi", "glab",
];

pub fn install_hook(target: &str) {
    match target {
        "bash" | "" => install_bash_wrappers(),
        "hermes" => install_hermes(),
        u if u.starts_with("add:") => add_single_wrapper(&u[4..]),
        _ => {
            eprintln!("Unknown target: {}. Use: bash, hermes, add:<cmd>", target);
            std::process::exit(1);
        }
    }
}

fn hooks_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(&home).join(".local/bin/tp-hooks")
}

fn install_bash_wrappers() {
    let dir = hooks_dir();
    fs::create_dir_all(&dir).ok();

    let mut count = 0u32;
    for cmd in TP_COMMANDS {
        let wrapper_path = dir.join(cmd);
        let wrapper_content = format!(
            r#"#!/bin/bash
# tp wrapper for {cmd}
exec tp run {cmd} "$@"
"#,
            cmd = cmd
        );

        if let Err(e) = fs::write(&wrapper_path, &wrapper_content) {
            eprintln!("  ⚠️  Failed to create wrapper for '{}': {}", cmd, e);
            continue;
        }

        // Make executable
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755)).ok();
        count += 1;
    }

    println!("  ✓ Created {} wrapper scripts in {}", count, dir.display());
    println!();

    // Add to PATH in bashrc
    let bashrc_path = PathBuf::from(
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string()),
    ).join(".bashrc");

    let path_line = format!("\n# tp hooks (higher priority in PATH)\nexport PATH=\"$HOME/.local/bin/tp-hooks:$PATH\"\n");

    let bashrc = fs::read_to_string(&bashrc_path).unwrap_or_default();
    if bashrc.contains("tp-hooks") {
        println!("  ✓ tp-hooks already in ~/.bashrc PATH");
    } else {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&bashrc_path)
            .unwrap();
        file.write_all(path_line.as_bytes()).ok();
        println!("  ✓ Added tp-hooks to PATH in ~/.bashrc");
    }

    println!();
    println!("  ╔═══════════════════════════════════════════════╗");
    println!("  ║  tp auto-hooks installed!                    ║");
    println!("  ║                                              ║");
    println!("  ║  Now `git status` → auto → `tp run git`     ║");
    println!("  ║  Now `ls -la`     → auto → `tp run ls`      ║");
    println!("  ║  Now `cargo test` → auto → `tp run cargo`   ║");
    println!("  ╚═══════════════════════════════════════════════╝");
    println!();
    println!("  {} commands are now auto-rewritten via tp!", count);
    println!();
    println!("  To bypass: use full path (/usr/bin/git status)");
    println!("  To uninstall: remove tp-hooks from PATH in ~/.bashrc");
    println!("  To add more commands: tp init add:<command>");
    println!();
}

fn add_single_wrapper(cmd: &str) {
    let dir = hooks_dir();
    fs::create_dir_all(&dir).ok();

    let cmd = cmd.trim();
    if cmd.is_empty() {
        eprintln!("Usage: tp init add:<command>");
        return;
    }

    let wrapper_path = dir.join(cmd);
    let wrapper_content = format!(
        r#"#!/bin/bash
# tp wrapper for {cmd}
exec tp run {cmd} "$@"
"#,
        cmd = cmd
    );

    fs::write(&wrapper_path, &wrapper_content).ok();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755)).ok();

    println!("  ✓ Added wrapper for '{}'", cmd);
    println!("  Location: {}", wrapper_path.display());
}

fn install_hermes() {
    println!("Installing Hermes Agent integration...");

    // Add to vault CLAUDE.md
    let vault_paths = [
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join("vault/CLAUDE.md"),
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join("vault/06-SYSTEM/CLAUDE.md"),
    ];

    let tp_rule = format!(
        r#"
## tp Hooks (Auto-rewrite)

Wrapper scripts installed at `~/.local/bin/tp-hooks/` auto-rewrite these commands through `tp run`:

```bash
{}
```

To add more: `tp init add:<command>`
To bypass: use full path (`/usr/bin/git status`)
"#,
        TP_COMMANDS.iter().map(|c| format!("  {} → tp run {}", c, c)).collect::<Vec<_>>().join("\n")
    );

    for path in &vault_paths {
        if path.exists() {
            let content = fs::read_to_string(path).unwrap_or_default();
            if content.contains("tp Hooks") || content.contains("Auto-rewrite") {
                println!("  ✓ Already in {}", path.display());
            } else {
                fs::write(path, format!("{}\n{}", content.trim(), tp_rule)).ok();
                println!("  ✓ Added to {}", path.display());
            }
        }
    }

    // Also install the bash wrappers
    install_bash_wrappers();

    println!("✅ Hermes integration complete!");
    println!("   Hermes will now auto-rewrite commands through tp.");
}
