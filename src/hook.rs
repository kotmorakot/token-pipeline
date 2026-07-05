use std::fs;
use std::io::Write;
use std::path::PathBuf;

const TP_COMMANDS: &[&str] = &[
    "git", "ls", "cat", "bat", "head", "tail", "less", "more", "cargo", "npm", "pnpm", "yarn",
    "bun", "pytest", "jest", "vitest", "rspec", "grep", "rg", "ag", "find", "fd", "docker",
    "podman", "kubectl", "oc", "helm", "env", "printenv", "curl", "wget", "httpie", "tree", "ps",
    "df", "make", "cmake", "ninja", "python", "python3", "node", "ruby", "php", "gh", "pip",
    "pip3", "uv", "tsc", "npx", "next", "dotnet", "terraform", "aws", "gcloud",
];

pub fn install_hook(target: &str) {
    match target {
        "bash" | "" => install_bash_wrappers(),
        "auto" => auto_detect_and_install(),
        "hermes" => install_hermes(),
        "cursor" => install_cursor(),
        "claude" => install_claude(),
        "codex" => install_codex(),
        "copilot" => install_copilot(),
        u if u.starts_with("add:") => add_single_wrapper(&u[4..]),
        _ => {
            eprintln!(
                "Unknown target: {}. Use: bash, auto, hermes, cursor, claude, codex, copilot, add:<cmd>",
                target
            );
            std::process::exit(1);
        }
    }
}

fn auto_detect_and_install() {
    println!("tp init: auto-detecting installed agents...");
    println!();

    let mut installed = Vec::new();

    if detect_hermes() {
        println!("  Detected: Hermes Agent");
        installed.push("hermes");
    }
    if detect_claude() {
        println!("  Detected: Claude Code");
        installed.push("claude");
    }
    if detect_cursor() {
        println!("  Detected: Cursor");
        installed.push("cursor");
    }
    if detect_codex() {
        println!("  Detected: Codex CLI");
        installed.push("codex");
    }
    if detect_copilot() {
        println!("  Detected: Copilot CLI");
        installed.push("copilot");
    }

    if installed.is_empty() {
        println!("  No known agents detected.");
        println!("  Installing bash wrappers only.");
        println!();
        install_bash_wrappers();
        return;
    }

    println!();
    println!("  Installing for {} agent(s)...", installed.len());
    println!();

    for agent in &installed {
        match *agent {
            "hermes" => install_hermes(),
            "claude" => install_claude(),
            "cursor" => install_cursor(),
            "codex" => install_codex(),
            "copilot" => install_copilot(),
            _ => {}
        }
    }

    println!();
    println!("  Auto-detect complete: configured {}", installed.join(", "));
    println!();
}

fn detect_hermes() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&format!("{}/.hermes", home)).exists()
        || which_exists("hermes")
}

fn detect_claude() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&format!("{}/.claude", home)).exists()
        || which_exists("claude")
}

fn detect_cursor() -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(".cursor").exists()
        || std::path::Path::new(&format!("{}/.cursor", home)).exists()
}

fn detect_codex() -> bool {
    which_exists("codex")
}

fn detect_copilot() -> bool {
    which_exists("github-copilot-cli") || which_exists("ghcs")
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn hooks_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(&home).join(".local/bin/tp-hooks")
}

fn install_bash_wrappers() {
    let dir = hooks_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("Failed to create hooks dir {}: {}", dir.display(), e);
        std::process::exit(1);
    }

    let mut count = 0u32;
    for cmd in TP_COMMANDS {
        let wrapper_path = dir.join(cmd);
        let wrapper_content = format!(
            "#!/bin/bash\n# tp wrapper for {cmd}\nexec tp run {cmd} \"$@\"\n",
            cmd = cmd
        );

        if let Err(e) = fs::write(&wrapper_path, &wrapper_content) {
            eprintln!("  Warning: failed to create wrapper for '{}': {}", cmd, e);
            continue;
        }

        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755)) {
            eprintln!("  Warning: failed to set permissions for '{}': {}", cmd, e);
        }
        count += 1;
    }

    println!(
        "  Created {} wrapper scripts in {}",
        count,
        dir.display()
    );
    println!();

    let bashrc_path = PathBuf::from(
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string()),
    )
    .join(".bashrc");

    let path_line = "\n# tp hooks (higher priority in PATH)\nexport PATH=\"$HOME/.local/bin/tp-hooks:$PATH\"\n";

    let bashrc = fs::read_to_string(&bashrc_path).unwrap_or_default();
    if bashrc.contains("tp-hooks") {
        println!("  tp-hooks already in ~/.bashrc PATH");
    } else {
        match fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&bashrc_path)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(path_line.as_bytes()) {
                    eprintln!("  Warning: failed to update ~/.bashrc: {}", e);
                } else {
                    println!("  Added tp-hooks to PATH in ~/.bashrc");
                }
            }
            Err(e) => eprintln!("  Warning: failed to open ~/.bashrc: {}", e),
        }
    }

    println!();
    println!("  tp auto-hooks installed!");
    println!("  {} commands are now auto-rewritten via tp.", count);
    println!();
    println!("  To bypass: use full path (/usr/bin/git status)");
    println!("  To uninstall: remove tp-hooks from PATH in ~/.bashrc");
    println!("  To add more: tp init add:<command>");
    println!();
}

fn add_single_wrapper(cmd: &str) {
    let dir = hooks_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("Failed to create hooks dir: {}", e);
        return;
    }

    let cmd = cmd.trim();
    if cmd.is_empty() {
        eprintln!("Usage: tp init add:<command>");
        return;
    }

    let wrapper_path = dir.join(cmd);
    let wrapper_content = format!(
        "#!/bin/bash\n# tp wrapper for {cmd}\nexec tp run {cmd} \"$@\"\n",
        cmd = cmd
    );

    if let Err(e) = fs::write(&wrapper_path, &wrapper_content) {
        eprintln!("Failed to create wrapper for '{}': {}", cmd, e);
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755)) {
        eprintln!("Warning: failed to set permissions for '{}': {}", cmd, e);
    }

    println!("  Added wrapper for '{}'", cmd);
    println!("  Location: {}", wrapper_path.display());
}

fn install_hermes() {
    println!("Installing Hermes Agent integration...");

    let vault_paths = [
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join("vault/CLAUDE.md"),
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join("vault/06-SYSTEM/CLAUDE.md"),
    ];

    let tp_rule = format!(
        "\n## tp Hooks (Auto-rewrite)\n\nWrapper scripts at `~/.local/bin/tp-hooks/` auto-rewrite commands through `tp run`:\n\n```bash\n{}\n```\n\nTo add more: `tp init add:<command>`\nTo bypass: use full path (`/usr/bin/git status`)\n",
        TP_COMMANDS
            .iter()
            .map(|c| format!("  {} -> tp run {}", c, c))
            .collect::<Vec<_>>()
            .join("\n")
    );

    for path in &vault_paths {
        if path.exists() {
            let content = fs::read_to_string(path).unwrap_or_default();
            if content.contains("tp Hooks") || content.contains("Auto-rewrite") {
                println!("  Already in {}", path.display());
            } else {
                match fs::write(path, format!("{}\n{}", content.trim(), tp_rule)) {
                    Ok(_) => println!("  Added to {}", path.display()),
                    Err(e) => eprintln!("  Warning: failed to update {}: {}", path.display(), e),
                }
            }
        }
    }

    install_bash_wrappers();
    println!("Hermes integration complete!");
}

fn install_cursor() {
    println!("Installing Cursor integration...");

    let cursor_rules_dir = PathBuf::from(".cursor/rules");
    if let Err(e) = fs::create_dir_all(&cursor_rules_dir) {
        eprintln!("  Failed to create .cursor/rules/: {}", e);
        return;
    }

    let rule_content = include_str!("../configs/cursor-rules.md");
    let rule_path = cursor_rules_dir.join("tp.md");

    match fs::write(&rule_path, rule_content) {
        Ok(_) => println!("  Created {}", rule_path.display()),
        Err(e) => eprintln!("  Failed to write rule: {}", e),
    }

    install_bash_wrappers();
    println!("Cursor integration complete!");
}

fn install_claude() {
    println!("Installing Claude Code integration...");

    let claude_content = include_str!("../configs/claude-CLAUDE.md");
    let claude_path = PathBuf::from("CLAUDE.md");

    if claude_path.exists() {
        let existing = fs::read_to_string(&claude_path).unwrap_or_default();
        if existing.contains("tp run") || existing.contains("token-pipeline") {
            println!("  tp rules already in CLAUDE.md");
        } else {
            match fs::write(
                &claude_path,
                format!("{}\n\n{}", existing.trim(), claude_content),
            ) {
                Ok(_) => println!("  Appended tp rules to CLAUDE.md"),
                Err(e) => eprintln!("  Failed to update CLAUDE.md: {}", e),
            }
        }
    } else {
        match fs::write(&claude_path, claude_content) {
            Ok(_) => println!("  Created CLAUDE.md"),
            Err(e) => eprintln!("  Failed to create CLAUDE.md: {}", e),
        }
    }

    install_bash_wrappers();
    println!("Claude Code integration complete!");
}

fn install_codex() {
    println!("Installing Codex CLI integration...");

    let codex_content = include_str!("../configs/codex-instructions.md");
    let codex_path = PathBuf::from("codex_instructions.md");

    match fs::write(&codex_path, codex_content) {
        Ok(_) => println!("  Created {}", codex_path.display()),
        Err(e) => eprintln!("  Failed to create codex_instructions.md: {}", e),
    }

    install_bash_wrappers();
    println!("Codex CLI integration complete!");
}

fn install_copilot() {
    println!("Installing Copilot CLI integration...");

    let copilot_content = include_str!("../configs/copilot-instructions.md");
    let github_dir = PathBuf::from(".github");
    if let Err(e) = fs::create_dir_all(&github_dir) {
        eprintln!("  Failed to create .github/: {}", e);
        return;
    }

    let copilot_path = github_dir.join("copilot-instructions.md");
    match fs::write(&copilot_path, copilot_content) {
        Ok(_) => println!("  Created {}", copilot_path.display()),
        Err(e) => eprintln!("  Failed to create copilot-instructions.md: {}", e),
    }

    install_bash_wrappers();
    println!("Copilot CLI integration complete!");
}
