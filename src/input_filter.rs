use std::collections::HashMap;

#[allow(dead_code)]
pub fn apply(cmd: &str, stdout: &str, stderr: &str, exit_code: i32) -> String {
    apply_with_ultra(cmd, stdout, stderr, exit_code, false)
}

#[allow(dead_code)]
pub fn is_known_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "git"
            | "ls"
            | "dir"
            | "exa"
            | "eza"
            | "cat"
            | "bat"
            | "head"
            | "tail"
            | "less"
            | "more"
            | "cargo"
            | "npm"
            | "pnpm"
            | "yarn"
            | "bun"
            | "pytest"
            | "jest"
            | "vitest"
            | "rspec"
            | "go"
            | "grep"
            | "rg"
            | "ag"
            | "find"
            | "fd"
            | "docker"
            | "podman"
            | "kubectl"
            | "oc"
            | "helm"
            | "env"
            | "printenv"
            | "curl"
            | "wget"
            | "httpie"
            | "tree"
            | "ps"
            | "df"
            | "make"
            | "cmake"
            | "ninja"
            | "python"
            | "python3"
            | "node"
            | "ruby"
            | "php"
            | "gh"
            | "pip"
            | "pip3"
            | "uv"
            | "tsc"
            | "npx"
            | "next"
            | "dotnet"
            | "terraform"
            | "aws"
            | "gcloud"
    )
}

pub fn apply_with_ultra(
    cmd: &str,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    ultra: bool,
) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return format!("{}{}", stdout, stderr);
    }

    match parts[0] {
        "git" => apply_git(parts.get(1).copied().unwrap_or(""), stdout, stderr, exit_code),
        "ls" | "dir" | "exa" | "eza" => ls_compact(stdout, ultra),
        "cat" | "bat" | "head" | "tail" | "less" | "more" => {
            smart_read(stdout, parts.get(1).copied())
        }
        "cargo" => apply_cargo(parts.get(1).copied().unwrap_or(""), stdout, stderr),
        "npm" | "pnpm" | "yarn" | "bun" => apply_js_runner(&parts, stdout, stderr),
        "pytest" | "jest" | "vitest" | "rspec" | "go" => test_compact(stdout, stderr),
        "grep" | "rg" | "ag" => grep_compact(stdout),
        "find" | "fd" => find_compact(stdout),
        "docker" | "podman" => {
            apply_docker(parts.get(1).copied().unwrap_or(""), stdout, stderr, ultra)
        }
        "kubectl" | "oc" => apply_kubectl(parts.get(1).copied().unwrap_or(""), stdout, stderr),
        "helm" => apply_helm(parts.get(1).copied().unwrap_or(""), stdout, stderr),
        "gh" => apply_gh(parts.get(1).copied().unwrap_or(""), stdout, stderr),
        "env" | "printenv" => env_compact(stdout),
        "curl" | "wget" | "httpie" => truncate(stdout, 2000),
        "tree" => tree_compact(stdout),
        "ps" => ps_compact(stdout),
        "df" => df_compact(stdout),
        "pip" | "pip3" | "uv" => apply_pip(parts.get(1).copied().unwrap_or(""), stdout),
        "tsc" | "npx" | "next" => build_compact(stdout, stderr),
        "make" | "cmake" | "ninja" => build_compact(stdout, stderr),
        "dotnet" => apply_dotnet(parts.get(1).copied().unwrap_or(""), stdout, stderr, exit_code),
        "terraform" => apply_terraform(parts.get(1).copied().unwrap_or(""), stdout, stderr),
        "aws" | "gcloud" => generic_compact(stdout, stderr),
        "python" | "python3" | "node" | "ruby" | "php" => {
            if exit_code != 0 {
                errors_only(stdout, stderr)
            } else {
                generic_compact(stdout, stderr)
            }
        }
        _ => {
            if exit_code != 0 && !stderr.is_empty() {
                errors_only(stdout, stderr)
            } else {
                generic_compact(stdout, stderr)
            }
        }
    }
}

fn apply_git(sub: &str, stdout: &str, stderr: &str, exit_code: i32) -> String {
    match sub {
        "status" => git_status(stdout),
        "diff" => git_diff(stdout),
        "log" => git_log(stdout),
        "show" => git_show(stdout),
        "branch" => git_branch(stdout),
        "add" | "commit" | "push" | "pull" | "checkout" | "switch" | "merge" | "rebase"
        | "stash" | "fetch" | "reset" | "restore" | "cherry-pick" | "tag" => {
            git_action(sub, stdout, stderr, exit_code)
        }
        _ => format!("{}{}", stdout, stderr),
    }
}

fn apply_cargo(sub: &str, stdout: &str, stderr: &str) -> String {
    match sub {
        "test" => test_compact(stdout, stderr),
        "build" | "check" | "clippy" => build_compact(stdout, stderr),
        "fmt" => {
            if stdout.trim().is_empty() && stderr.trim().is_empty() {
                "ok cargo fmt\n".to_string()
            } else {
                format!("{}{}", stdout, stderr)
            }
        }
        "run" => generic_compact(stdout, stderr),
        _ => format!("{}{}", stdout, stderr),
    }
}

fn apply_js_runner(parts: &[&str], stdout: &str, stderr: &str) -> String {
    if parts.get(1).copied() == Some("test") {
        test_compact(stdout, stderr)
    } else if parts.get(1).copied() == Some("run") && parts.get(2).copied() == Some("build") {
        build_compact(stdout, stderr)
    } else {
        generic_compact(stdout, stderr)
    }
}

fn apply_docker(sub: &str, stdout: &str, stderr: &str, _ultra: bool) -> String {
    match sub {
        "ps" => docker_ps(stdout),
        "images" => docker_images(stdout),
        "logs" => dedup_lines(stdout),
        "compose" => docker_compose_compact(stdout, stderr),
        "build" => build_compact(stdout, stderr),
        _ => format!("{}{}", stdout, stderr),
    }
}

fn apply_kubectl(sub: &str, stdout: &str, stderr: &str) -> String {
    match sub {
        "get" => kubectl_get(stdout),
        "logs" => dedup_lines(stdout),
        "describe" => kubectl_describe(stdout),
        "apply" | "create" | "delete" => {
            let lines: Vec<&str> = stdout
                .lines()
                .chain(stderr.lines())
                .filter(|l| !l.trim().is_empty())
                .collect();
            if lines.len() <= 5 {
                format!("{}{}", stdout, stderr)
            } else {
                format!("{} resources affected:\n{}\n", lines.len(), lines[..5.min(lines.len())].join("\n"))
            }
        }
        _ => format!("{}{}", stdout, stderr),
    }
}

fn apply_helm(sub: &str, stdout: &str, stderr: &str) -> String {
    match sub {
        "list" | "ls" => {
            let lines: Vec<&str> = stdout.lines().collect();
            if lines.len() <= 1 {
                return "no releases\n".to_string();
            }
            let header = lines[0];
            let count = lines.len() - 1;
            if count <= 10 {
                stdout.to_string()
            } else {
                format!("{}\n({} releases, showing first 10)\n{}\n", header, count, lines[1..11.min(lines.len())].join("\n"))
            }
        }
        "install" | "upgrade" => {
            let meaningful: Vec<&str> = stdout
                .lines()
                .chain(stderr.lines())
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty()
                        && !t.starts_with("W:")
                        && !t.starts_with("coalesce")
                })
                .collect();
            if meaningful.len() <= 10 {
                meaningful.join("\n") + "\n"
            } else {
                format!("{}\n... ({} more lines)\n", meaningful[..5].join("\n"), meaningful.len() - 5)
            }
        }
        _ => format!("{}{}", stdout, stderr),
    }
}

fn apply_gh(sub: &str, stdout: &str, _stderr: &str) -> String {
    match sub {
        "pr" => truncate_list(stdout, "PRs"),
        "issue" => truncate_list(stdout, "issues"),
        "run" => truncate_list(stdout, "workflow runs"),
        _ => stdout.to_string(),
    }
}

fn truncate_list(stdout: &str, label: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 15 {
        return stdout.to_string();
    }
    let mut result = format!("{} {}:\n", lines.len(), label);
    for line in lines.iter().take(10) {
        result.push_str(&format!("  {}\n", line));
    }
    result.push_str(&format!("  ... +{} more\n", lines.len() - 10));
    result
}

fn apply_pip(sub: &str, stdout: &str) -> String {
    match sub {
        "list" => {
            let lines: Vec<&str> = stdout.lines().collect();
            if lines.len() <= 5 {
                return stdout.to_string();
            }
            format!("{} packages\n", lines.len().saturating_sub(2))
        }
        "freeze" => {
            let lines: Vec<&str> = stdout.lines().collect();
            if lines.len() <= 20 {
                return stdout.to_string();
            }
            format!("{} packages\n", lines.len())
        }
        _ => stdout.to_string(),
    }
}

fn apply_dotnet(sub: &str, stdout: &str, stderr: &str, exit_code: i32) -> String {
    match sub {
        "build" => build_compact(stdout, stderr),
        "test" => test_compact(stdout, stderr),
        "run" => {
            if exit_code != 0 {
                errors_only(stdout, stderr)
            } else {
                generic_compact(stdout, stderr)
            }
        }
        "restore" => {
            let combined = format!("{}{}", stdout, stderr);
            let lines: Vec<&str> = combined.lines().collect();
            let errors: Vec<&&str> = lines
                .iter()
                .filter(|l| l.contains("error") || l.contains("Error"))
                .collect();
            if errors.is_empty() {
                "ok dotnet restore\n".to_string()
            } else {
                let mut result = format!("{} restore errors:\n", errors.len());
                for e in errors.iter().take(10) {
                    result.push_str(&format!("  {}\n", e.trim()));
                }
                result
            }
        }
        "publish" => build_compact(stdout, stderr),
        _ => format!("{}{}", stdout, stderr),
    }
}

fn apply_terraform(sub: &str, stdout: &str, stderr: &str) -> String {
    match sub {
        "plan" => {
            let combined = format!("{}{}", stdout, stderr);
            let lines: Vec<&str> = combined.lines().collect();
            let summary: Vec<&&str> = lines
                .iter()
                .filter(|l| {
                    l.contains("Plan:") || l.contains("to add") || l.contains("to change")
                        || l.contains("to destroy") || l.contains("No changes")
                        || l.contains("Error")
                })
                .collect();
            if !summary.is_empty() {
                summary.iter().map(|l| l.trim()).collect::<Vec<_>>().join("\n") + "\n"
            } else {
                generic_compact(stdout, stderr)
            }
        }
        "apply" | "destroy" => {
            let combined = format!("{}{}", stdout, stderr);
            let lines: Vec<&str> = combined.lines().collect();
            let meaningful: Vec<&&str> = lines
                .iter()
                .filter(|l| {
                    l.contains("Apply complete") || l.contains("Destroy complete")
                        || l.contains("Error") || l.contains("created")
                        || l.contains("destroyed")
                })
                .collect();
            if !meaningful.is_empty() {
                meaningful.iter().map(|l| l.trim()).collect::<Vec<_>>().join("\n") + "\n"
            } else {
                generic_compact(stdout, stderr)
            }
        }
        "init" => {
            if stderr.contains("Error") || stdout.contains("Error") {
                errors_only(stdout, stderr)
            } else {
                "ok terraform init\n".to_string()
            }
        }
        _ => generic_compact(stdout, stderr),
    }
}

// ─── Git Filters ─────────────────────────────────────────────────

fn git_status(stdout: &str) -> String {
    if stdout.contains("nothing to commit") && stdout.contains("working tree clean") {
        let branch = extract_branch(stdout);
        return format!("[{}] clean\n", branch);
    }

    let mut staged = Vec::new();
    let mut modified = Vec::new();
    let mut untracked = Vec::new();
    let mut section = "";

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Changes to be committed") {
            section = "staged";
        } else if trimmed.starts_with("Changes not staged") {
            section = "modified";
        } else if trimmed.starts_with("Untracked files") {
            section = "untracked";
        } else if trimmed.starts_with("(use \"git") || trimmed.is_empty() {
            continue;
        } else if (line.starts_with('\t') || line.starts_with("  ")) && !section.is_empty() {
            let name = trimmed
                .trim_start_matches("new file:")
                .trim_start_matches("modified:")
                .trim_start_matches("deleted:")
                .trim_start_matches("renamed:")
                .trim_start_matches("copied:")
                .trim();
            if name.is_empty() {
                continue;
            }
            match section {
                "staged" => staged.push(format!("  + {}", name)),
                "modified" => modified.push(format!("  M {}", name)),
                "untracked" => untracked.push(format!("  ? {}", name)),
                _ => {}
            }
        }
    }

    let branch = extract_branch(stdout);
    let mut result = format!("[{}]\n", branch);

    if !staged.is_empty() {
        result.push_str("staged:\n");
        for s in &staged {
            result.push_str(s);
            result.push('\n');
        }
    }
    if !modified.is_empty() {
        result.push_str("modified:\n");
        for m in &modified {
            result.push_str(m);
            result.push('\n');
        }
    }
    if !untracked.is_empty() {
        result.push_str("untracked:\n");
        for u in &untracked {
            result.push_str(u);
            result.push('\n');
        }
    }

    if staged.is_empty() && modified.is_empty() && untracked.is_empty() {
        format!(
            "[{}] {}\n",
            branch,
            stdout.lines().last().unwrap_or("unknown state")
        )
    } else {
        result
    }
}

fn extract_branch(stdout: &str) -> &str {
    stdout
        .lines()
        .find(|l| l.starts_with("On branch "))
        .map(|l| l.trim_start_matches("On branch ").trim())
        .unwrap_or("?")
}

fn git_diff(stdout: &str) -> String {
    if stdout.trim().is_empty() {
        return "no changes\n".to_string();
    }

    let mut result = Vec::new();
    let mut current_file = String::new();
    let mut file_adds = 0usize;
    let mut file_dels = 0usize;
    let mut file_start_idx = 0usize;

    for line in stdout.lines() {
        if line.starts_with("diff --git") {
            if !current_file.is_empty() {
                result.insert(
                    file_start_idx,
                    format!("--- {} (+{} -{}) ---", current_file, file_adds, file_dels),
                );
            }
            current_file = line
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_start_matches("b/")
                .to_string();
            file_adds = 0;
            file_dels = 0;
            file_start_idx = result.len();
        } else if line.starts_with("@@") {
            let hunk_info = line.split("@@").nth(1).unwrap_or("").trim();
            result.push(format!("  @@ {} @@", hunk_info));
        } else if line.starts_with('+') && !line.starts_with("+++") {
            result.push(format!("  {}", line));
            file_adds += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            result.push(format!("  {}", line));
            file_dels += 1;
        }
    }

    if !current_file.is_empty() {
        result.insert(
            file_start_idx,
            format!("--- {} (+{} -{}) ---", current_file, file_adds, file_dels),
        );
    }

    if result.is_empty() {
        "no changes\n".to_string()
    } else {
        result.join("\n") + "\n"
    }
}

fn git_log(stdout: &str) -> String {
    let mut entries = Vec::new();
    let mut hash = String::new();
    let mut author = String::new();
    let mut msg = String::new();

    for line in stdout.lines() {
        if line.starts_with("commit ") {
            if !hash.is_empty() {
                let short_hash = &hash[..7.min(hash.len())];
                entries.push(format!("{} {} ({})", short_hash, msg.trim(), author));
            }
            hash = line.trim_start_matches("commit ").trim().to_string();
            if hash.contains(' ') {
                hash = hash.split_whitespace().next().unwrap_or("").to_string();
            }
            msg.clear();
            author.clear();
        } else if line.starts_with("Author:") {
            author = line
                .trim_start_matches("Author:")
                .trim()
                .split('<')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
        } else if !line.starts_with("Date:")
            && !line.trim().is_empty()
            && !hash.is_empty()
            && msg.is_empty()
        {
            msg = line.trim().to_string();
        }
    }

    if !hash.is_empty() {
        let short_hash = &hash[..7.min(hash.len())];
        entries.push(format!("{} {} ({})", short_hash, msg.trim(), author));
    }

    if entries.is_empty() {
        stdout.to_string()
    } else {
        entries.join("\n") + "\n"
    }
}

fn git_show(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 60 {
        return stdout.to_string();
    }

    let mut result = Vec::new();
    for line in &lines {
        let trimmed = line.trim();
        if line.starts_with("commit ")
            || line.starts_with("Author:")
            || line.starts_with("Date:")
            || line.starts_with("diff --git")
            || line.starts_with("@@")
            || line.starts_with('+')
            || line.starts_with('-')
            || (!trimmed.is_empty()
                && !trimmed.starts_with("index ")
                && !trimmed.starts_with("---")
                && !trimmed.starts_with("+++")
                && !line.starts_with(' '))
        {
            result.push(*line);
        } else if !trimmed.is_empty()
            && (line.starts_with("    ") || line.starts_with('\t'))
            && (result
                .last()
                .map(|l: &&str| l.starts_with("Date:"))
                .unwrap_or(false)
                || result.is_empty())
        {
            result.push(*line);
        }
    }

    if result.len() < lines.len() / 2 {
        result.join("\n") + "\n"
    } else {
        stdout.to_string()
    }
}

fn git_branch(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 20 {
        return stdout.to_string();
    }

    let current = lines
        .iter()
        .find(|l| l.starts_with("* "))
        .copied()
        .unwrap_or("* ?");
    let others: Vec<&str> = lines
        .iter()
        .filter(|l| !l.starts_with("* "))
        .copied()
        .collect();
    format!("{}\n({} other branches)\n", current.trim(), others.len())
}

fn git_action(sub: &str, stdout: &str, stderr: &str, exit_code: i32) -> String {
    if exit_code == 0 {
        let meaningful: Vec<&str> = stdout
            .lines()
            .chain(stderr.lines())
            .filter(|l| {
                let t = l.trim();
                !t.is_empty()
                    && !t.starts_with("hint:")
                    && !t.starts_with("remote: Counting")
                    && !t.starts_with("remote: Compressing")
                    && !t.starts_with("remote: Total")
                    && !t.starts_with("Receiving objects")
                    && !t.starts_with("Resolving deltas")
                    && !t.starts_with("Unpacking objects")
            })
            .collect();

        if meaningful.is_empty() {
            format!("ok: git {}\n", sub)
        } else if meaningful.len() <= 5 {
            format!("ok: git {}\n{}\n", sub, meaningful.join("\n"))
        } else {
            format!(
                "ok: git {}\n{}\n... ({} more lines)\n",
                sub,
                meaningful[..3].join("\n"),
                meaningful.len() - 3
            )
        }
    } else {
        format!(
            "FAIL: git {} (exit {})\n{}{}",
            sub, exit_code, stdout, stderr
        )
    }
}

// ─── File/Directory Filters ──────────────────────────────────────

fn ls_compact(stdout: &str, ultra: bool) -> String {
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return stdout.to_string();
    }

    let is_long_format = lines.iter().any(|l| {
        let first = l.chars().next().unwrap_or(' ');
        matches!(first, 'd' | '-' | 'l' | 'c' | 'b' | 'p' | 's')
            && l.split_whitespace().count() >= 8
    });

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("total ") {
            continue;
        }

        if is_long_format {
            let name = trimmed.split_whitespace().last().unwrap_or(trimmed);
            if name == "." || name == ".." {
                continue;
            }
            if trimmed.starts_with('d') {
                dirs.push(name.trim_end_matches('/'));
            } else {
                files.push(name);
            }
        } else {
            for name in trimmed.split_whitespace() {
                if name == "." || name == ".." {
                    continue;
                }
                if name.ends_with('/') {
                    dirs.push(name.trim_end_matches('/'));
                } else {
                    files.push(name);
                }
            }
        }
    }

    if dirs.is_empty() && files.is_empty() {
        return stdout.to_string();
    }

    if ultra {
        let mut ext_counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut count_other = 0usize;
        for f in &files {
            if f.contains('.') {
                let ext = f
                    .rsplit('.')
                    .next()
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                *ext_counts.entry(ext).or_insert(0) += 1;
            } else {
                count_other += 1;
            }
        }
        let mut result = format!("{} dirs, {} files\n", dirs.len(), files.len());
        for (ext, count) in &ext_counts {
            result.push_str(&format!("  .{}: {} files\n", ext, count));
        }
        if count_other > 0 {
            result.push_str(&format!("  other: {} files\n", count_other));
        }
        result
    } else {
        let mut result = format!("{} dirs, {} files\n", dirs.len(), files.len());
        for d in &dirs {
            result.push_str(&format!("  {}/\n", d));
        }
        for f in &files {
            result.push_str(&format!("  {}\n", f));
        }
        result
    }
}

fn smart_read(stdout: &str, filename: Option<&str>) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    let name = filename.unwrap_or("file");

    if lines.len() <= 100 {
        return stdout.to_string();
    }

    let mut result = Vec::new();
    result.push(format!("# {} ({} lines)", name, lines.len()));

    let mut key_lines: Vec<(usize, &str)> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let is_key = t.starts_with("pub ")
            || t.starts_with("fn ")
            || t.starts_with("struct ")
            || t.starts_with("enum ")
            || t.starts_with("impl ")
            || t.starts_with("trait ")
            || t.starts_with("mod ")
            || t.starts_with("use ")
            || t.starts_with("type ")
            || t.starts_with("class ")
            || t.starts_with("def ")
            || t.starts_with("function ")
            || t.starts_with("export ")
            || t.starts_with("import ")
            || t.starts_with("from ")
            || t.starts_with("const ")
            || t.starts_with("interface ")
            || t.starts_with("#[")
            || t.starts_with("///")
            || t.starts_with("//!")
            || t.starts_with("# ")
            || t.starts_with("## ");

        if is_key {
            key_lines.push((i + 1, line));
        }
    }

    if key_lines.is_empty() {
        if lines.len() <= 30 {
            result.extend(
                lines
                    .iter()
                    .enumerate()
                    .map(|(i, l)| format!("{:4}| {}", i + 1, l)),
            );
        } else {
            for i in 0..15 {
                result.push(format!("{:4}| {}", i + 1, lines[i]));
            }
            result.push("  ...".to_string());
            for i in lines.len().saturating_sub(10)..lines.len() {
                result.push(format!("{:4}| {}", i + 1, lines[i]));
            }
        }
    } else {
        let max_keys = if lines.len() > 200 { 40 } else { 60 };
        for (_i, (lineno, line)) in key_lines.iter().enumerate().take(max_keys) {
            result.push(format!("{:4}| {}", lineno, line));
        }
        if key_lines.len() > max_keys {
            result.push(format!(
                "  ... +{} key lines omitted",
                key_lines.len() - max_keys
            ));
        }
    }

    result.join("\n") + "\n"
}

fn find_compact(stdout: &str) -> String {
    let paths: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();

    if paths.len() <= 25 {
        return stdout.to_string();
    }

    let mut by_dir: HashMap<String, Vec<String>> = HashMap::new();
    for path in &paths {
        let (dir, file) = match path.rsplit_once('/') {
            Some((d, f)) => (d.to_string(), f.to_string()),
            None => (".".to_string(), path.to_string()),
        };
        by_dir.entry(dir).or_default().push(file);
    }

    let mut result = format!("{} files:\n", paths.len());
    let mut dirs: Vec<_> = by_dir.iter().collect();
    dirs.sort_by(|(a, _), (b, _)| a.cmp(b));

    for (dir, files) in dirs {
        if files.len() <= 3 {
            for f in files {
                result.push_str(&format!("  {}/{}\n", dir, f));
            }
        } else {
            result.push_str(&format!("  {}/ ({} files)\n", dir, files.len()));
        }
    }
    result
}

fn tree_compact(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 40 {
        return stdout.to_string();
    }

    let mut result = Vec::new();

    for line in &lines {
        let depth = line
            .len()
            .saturating_sub(
                line.trim_start_matches(|c: char| {
                    c == ' ' || c == '\u{2502}' || c == '\u{251c}' || c == '\u{2514}'
                        || c == '\u{2500}' || c == '|'
                })
                .len(),
            );

        if depth / 4 <= 2 {
            result.push(line.to_string());
        }
    }

    if result.len() < lines.len() {
        let omitted = lines.len() - result.len();
        result.push(format!(
            "({} deeper entries omitted, {} total)",
            omitted,
            lines.len()
        ));
    }

    result.join("\n") + "\n"
}

// ─── Test Filters ────────────────────────────────────────────────

fn test_compact(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}{}", stdout, stderr);
    let lines: Vec<&str> = combined.lines().collect();

    let mut failures = Vec::new();
    let mut summary_line = String::new();
    let mut in_failure = false;
    let mut failure_buf = Vec::new();
    let mut test_count = 0u32;

    for line in &lines {
        let t = line.trim();

        if t.contains("FAILED")
            || t.contains("FAIL ")
            || t.contains("panicked at")
            || t.contains("AssertionError")
            || t.contains("error[E")
        {
            in_failure = true;
            failure_buf.push(line.to_string());
        } else if in_failure {
            if t.is_empty() || t.starts_with("test ") || t.starts_with("----") {
                if !failure_buf.is_empty() {
                    failures.push(failure_buf.join("\n"));
                    failure_buf.clear();
                }
                in_failure = false;
            } else {
                failure_buf.push(line.to_string());
            }
        }

        if t.contains("test result:")
            || t.contains("Tests:")
            || (t.contains("passed") && t.contains("failed"))
        {
            summary_line = t.to_string();
        }

        if t.starts_with("test ") && t.contains("...") {
            test_count += 1;
        }
    }

    if !failure_buf.is_empty() {
        failures.push(failure_buf.join("\n"));
    }

    if failures.is_empty() {
        if !summary_line.is_empty() {
            format!("ok {}\n", summary_line)
        } else if test_count > 0 {
            format!("ok {} tests passed\n", test_count)
        } else {
            let last_meaningful = lines
                .iter()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or(&"done");
            format!("ok {}\n", last_meaningful.trim())
        }
    } else {
        let mut result = format!("FAILED: {} failures\n\n", failures.len());
        for (i, f) in failures.iter().enumerate().take(10) {
            result.push_str(&format!("--- failure {} ---\n{}\n\n", i + 1, f));
        }
        if failures.len() > 10 {
            result.push_str(&format!("... +{} more\n", failures.len() - 10));
        }
        if !summary_line.is_empty() {
            result.push_str(&format!("\n{}\n", summary_line));
        }
        result
    }
}

fn build_compact(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}{}", stdout, stderr);
    let lines: Vec<&str> = combined.lines().collect();

    let issues: Vec<&str> = lines
        .iter()
        .filter(|l| {
            (l.contains("error") || l.contains("warning") || l.contains("Error"))
                && !l.trim().starts_with("Compiling")
                && !l.trim().starts_with("Downloading")
                && !l.trim().starts_with("Downloaded")
        })
        .copied()
        .collect();

    let summary: Vec<&str> = lines
        .iter()
        .filter(|l| {
            l.contains("Finished")
                || l.contains("error:")
                || l.contains("warning:")
                || l.contains("Error:")
        })
        .map(|l| l.trim())
        .collect();

    if issues.is_empty() {
        format!("ok {}\n", summary.last().unwrap_or(&"build complete"))
    } else {
        let mut result = format!("{} issues:\n", issues.len());
        for e in issues.iter().take(20) {
            result.push_str(&format!("  {}\n", e.trim()));
        }
        if issues.len() > 20 {
            result.push_str(&format!("  ... +{} more\n", issues.len() - 20));
        }
        result
    }
}

// ─── Grep/Search Filters ────────────────────────────────────────

fn grep_compact(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 25 {
        return stdout.to_string();
    }

    let mut by_file: HashMap<&str, Vec<&str>> = HashMap::new();
    for line in &lines {
        if let Some((file, content)) = line.split_once(':') {
            by_file.entry(file).or_default().push(content);
        } else {
            by_file.entry("(no file)").or_default().push(line);
        }
    }

    let mut result = format!("{} matches in {} files:\n", lines.len(), by_file.len());
    let mut files: Vec<_> = by_file.iter().collect();
    files.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));

    for (file, matches) in files.iter().take(15) {
        result.push_str(&format!("  {} ({} matches)\n", file, matches.len()));
        for m in matches.iter().take(3) {
            result.push_str(&format!("    {}\n", m.trim()));
        }
        if matches.len() > 3 {
            result.push_str(&format!("    ... +{}\n", matches.len() - 3));
        }
    }
    if files.len() > 15 {
        result.push_str(&format!("  ... +{} files\n", files.len() - 15));
    }
    result
}

// ─── Docker Filters ──────────────────────────────────────────────

fn docker_ps(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 1 {
        return "no containers\n".to_string();
    }

    let mut result = format!("{} containers:\n", lines.len() - 1);
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let id = &parts[0][..12.min(parts[0].len())];
            let image = parts[1];
            let name = parts.last().unwrap_or(&"?");
            result.push_str(&format!("  {} {} {}\n", name, image, id));
        }
    }
    result
}

fn docker_images(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 1 {
        return "no images\n".to_string();
    }

    let mut result = format!("{} images:\n", lines.len() - 1);
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let size = parts.last().unwrap_or(&"?");
            result.push_str(&format!("  {}:{} {}\n", parts[0], parts[1], size));
        }
    }
    result
}

fn docker_compose_compact(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}{}", stdout, stderr);
    let lines: Vec<&str> = combined.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return "no services\n".to_string();
    }
    if lines.len() <= 10 {
        return combined;
    }
    format!(
        "{} lines (first 10):\n{}\n... +{} more\n",
        lines.len(),
        lines[..10.min(lines.len())].join("\n"),
        lines.len().saturating_sub(10)
    )
}

fn kubectl_get(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 1 {
        return stdout.to_string();
    }

    let header = lines[0];
    let data_lines = &lines[1..];
    let count = data_lines.len();

    if count <= 15 {
        return stdout.to_string();
    }

    let mut result = format!("{}\n", header);
    for line in data_lines.iter().take(10) {
        result.push_str(&format!("{}\n", line));
    }
    result.push_str(&format!("... +{} more ({} total)\n", count - 10, count));
    result
}

fn kubectl_describe(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 50 {
        return stdout.to_string();
    }

    let mut result = Vec::new();
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.ends_with(':') && !trimmed.starts_with(' ') {
            result.push(line.to_string());
        } else if line.starts_with("Name:")
            || line.starts_with("Namespace:")
            || line.starts_with("Status:")
            || line.starts_with("IP:")
            || line.starts_with("Node:")
            || line.starts_with("Start Time:")
            || line.contains("Error")
            || line.contains("Warning")
            || line.contains("Restart Count:")
        {
            result.push(line.to_string());
        }
    }

    if result.len() < lines.len() / 2 {
        result.push(format!("({} lines total, key fields shown)", lines.len()));
        result.join("\n") + "\n"
    } else {
        stdout.to_string()
    }
}

fn ps_compact(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 20 {
        return stdout.to_string();
    }

    let header = lines.first().unwrap_or(&"");
    let procs: Vec<&str> = lines.iter().skip(1).copied().collect();
    format!(
        "{}\n({} processes, showing header only)\n",
        header,
        procs.len()
    )
}

fn df_compact(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    let mut result = Vec::new();

    for line in &lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            let use_pct = parts.get(4).copied().unwrap_or("0%");
            let pct_num: u32 = use_pct.trim_end_matches('%').parse().unwrap_or(0);
            if pct_num >= 70 || result.is_empty() {
                result.push(line.to_string());
            }
        }
    }

    if result.len() < lines.len() {
        result.push(format!(
            "({} filesystems total, showing >70% used)",
            lines.len() - 1
        ));
    }
    result.join("\n") + "\n"
}

// ─── Generic Filters ─────────────────────────────────────────────

fn dedup_lines(stdout: &str) -> String {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut order = Vec::new();

    for line in stdout.lines() {
        let normalized = line.trim().to_string();
        if normalized.is_empty() {
            continue;
        }
        let count = seen.entry(normalized.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            order.push(normalized);
        }
    }

    let mut result = String::new();
    for line in &order {
        let count = seen[line];
        if count > 1 {
            result.push_str(&format!("{} (x{})\n", line, count));
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn env_compact(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    let mut result = format!("{} vars:\n", lines.len());

    let sensitive_patterns = [
        "SECRET", "TOKEN", "PASSWORD", "CREDENTIAL", "PRIVATE",
    ];

    for line in &lines {
        if let Some((key, val)) = line.split_once('=') {
            let k = key.to_uppercase();
            let is_sensitive = sensitive_patterns.iter().any(|p| k.contains(p))
                || (k.ends_with("_KEY") && k != "TERM_SESSION_KEY");
            if is_sensitive {
                result.push_str(&format!("  {}=***\n", key));
            } else if val.len() > 80 {
                result.push_str(&format!("  {}={}...\n", key, &val[..60.min(val.len())]));
            } else {
                result.push_str(&format!("  {}={}\n", key, val));
            }
        }
    }
    result
}

fn truncate(stdout: &str, max: usize) -> String {
    if stdout.len() <= max {
        return stdout.to_string();
    }
    format!(
        "{}...\n[truncated: {} -> {} chars]\n",
        &stdout[..max],
        stdout.len(),
        max
    )
}

fn errors_only(stdout: &str, stderr: &str) -> String {
    let mut errors = Vec::new();

    for line in stderr.lines().chain(stdout.lines()) {
        let lower = line.to_lowercase();
        if lower.contains("error")
            || lower.contains("fail")
            || lower.contains("panic")
            || lower.contains("exception")
            || lower.contains("fatal")
            || lower.contains("traceback")
        {
            errors.push(line.trim().to_string());
        }
    }

    if errors.is_empty() {
        format!("{}{}", stdout, stderr)
    } else {
        let mut result = format!("{} errors:\n", errors.len());
        for e in errors.iter().take(20) {
            result.push_str(&format!("  {}\n", e));
        }
        result
    }
}

fn generic_compact(stdout: &str, stderr: &str) -> String {
    let combined = format!("{}{}", stdout, stderr);
    let lines: Vec<&str> = combined.lines().collect();
    if lines.len() <= 50 {
        return combined;
    }

    let deduped = dedup_lines(&combined);
    let dedup_lines_count: usize = deduped.lines().count();

    if dedup_lines_count < lines.len() / 2 {
        format!(
            "{} lines -> {} unique:\n{}",
            lines.len(),
            dedup_lines_count,
            deduped
        )
    } else if lines.len() > 200 {
        let head: Vec<&str> = lines[..20].to_vec();
        let tail: Vec<&str> = lines[lines.len() - 10..].to_vec();
        format!(
            "{}\n... ({} lines omitted) ...\n{}\n",
            head.join("\n"),
            lines.len() - 30,
            tail.join("\n")
        )
    } else {
        combined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_status_clean() {
        let stdout = "On branch main\nYour branch is up to date with 'origin/main'.\n\nnothing to commit, working tree clean\n";
        let result = git_status(stdout);
        assert_eq!(result, "[main] clean\n");
    }

    #[test]
    fn test_git_status_modified() {
        let stdout = "On branch dev\nChanges not staged for commit:\n  (use \"git add <file>...\" to update)\n\tmodified:   src/main.rs\n\nUntracked files:\n  (use \"git add <file>...\" to include)\n\tREADME.md\n";
        let result = git_status(stdout);
        assert!(result.contains("[dev]"));
        assert!(result.contains("M src/main.rs"));
        assert!(result.contains("? README.md"));
    }

    #[test]
    fn test_git_diff_empty() {
        assert_eq!(git_diff(""), "no changes\n");
        assert_eq!(git_diff("  \n"), "no changes\n");
    }

    #[test]
    fn test_git_diff_basic() {
        let stdout = "diff --git a/src/main.rs b/src/main.rs\nindex abc..def 100644\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -10,3 +10,4 @@ fn main() {\n-    old_line();\n+    new_line();\n+    added_line();\n";
        let result = git_diff(stdout);
        assert!(result.contains("src/main.rs"));
        assert!(result.contains("+2 -1"));
        assert!(result.contains("+    new_line();"));
        assert!(result.contains("-    old_line();"));
    }

    #[test]
    fn test_git_log_compact() {
        let stdout = "commit abc1234567890\nAuthor: John Doe <john@example.com>\nDate:   Mon Jan 1 2024\n\n    Initial commit\n\ncommit def5678901234\nAuthor: Jane <jane@example.com>\nDate:   Tue Jan 2 2024\n\n    Add feature\n";
        let result = git_log(stdout);
        assert!(result.contains("abc1234"));
        assert!(result.contains("Initial commit"));
        assert!(result.contains("John Doe"));
        assert!(result.contains("def5678"));
    }

    #[test]
    fn test_test_compact_pass() {
        let stdout = "running 3 tests\ntest test_a ... ok\ntest test_b ... ok\ntest test_c ... ok\n\ntest result: ok. 3 passed; 0 failed; 0 ignored\n";
        let result = test_compact(stdout, "");
        assert!(result.starts_with("ok"));
        assert!(result.contains("3 passed"));
    }

    #[test]
    fn test_test_compact_fail() {
        let stdout = "test test_a ... ok\ntest test_b ... FAILED\nthread 'test_b' panicked at 'assertion failed'\n\ntest result: ok. 1 passed; 1 failed\n";
        let result = test_compact(stdout, "");
        assert!(result.contains("FAILED"));
        assert!(result.contains("panicked"));
    }

    #[test]
    fn test_build_compact_ok() {
        let stdout = "   Compiling myproject v0.1.0\n    Finished dev [unoptimized + debuginfo] target(s) in 2.5s\n";
        let result = build_compact(stdout, "");
        assert!(result.starts_with("ok"));
    }

    #[test]
    fn test_ls_compact_long() {
        let stdout = "total 16\ndrwxr-xr-x 2 user group 4096 Jan  1 00:00 src\n-rw-r--r-- 1 user group  100 Jan  1 00:00 Cargo.toml\n-rw-r--r-- 1 user group  200 Jan  1 00:00 README.md\n";
        let result = ls_compact(stdout, false);
        assert!(result.contains("1 dirs, 2 files"));
        assert!(result.contains("src/"));
        assert!(result.contains("Cargo.toml"));
    }

    #[test]
    fn test_ls_compact_plain() {
        let stdout = "Cargo.toml  README.md  src/\n";
        let result = ls_compact(stdout, false);
        assert!(result.contains("dirs"));
        assert!(result.contains("files"));
    }

    #[test]
    fn test_env_compact_masking() {
        let stdout = "HOME=/home/user\nSECRET_KEY=abc123\nMONKEY=banana\nAPI_TOKEN=xyz\nPATH=/usr/bin\nPRIVATE_KEY=secret\n";
        let result = env_compact(stdout);
        assert!(result.contains("SECRET_KEY=***"));
        assert!(result.contains("API_TOKEN=***"));
        assert!(result.contains("PRIVATE_KEY=***"));
        assert!(result.contains("MONKEY=banana"));
        assert!(result.contains("HOME=/home/user"));
    }

    #[test]
    fn test_smart_read_small_file() {
        let lines: Vec<String> = (1..=50).map(|i| format!("line {}", i)).collect();
        let stdout = lines.join("\n");
        let result = smart_read(&stdout, Some("test.txt"));
        assert_eq!(result, stdout);
    }

    #[test]
    fn test_kubectl_get_preserves_data() {
        let stdout = "NAME    READY   STATUS    RESTARTS   AGE\npod-1   1/1     Running   0          1d\npod-2   1/1     Running   0          2d\n";
        let result = kubectl_get(stdout);
        assert!(result.contains("pod-1"));
        assert!(result.contains("pod-2"));
    }

    #[test]
    fn test_generic_compact_short() {
        let stdout = "line1\nline2\nline3\n";
        assert_eq!(generic_compact(stdout, ""), format!("{}",stdout));
    }

    #[test]
    fn test_dotnet_restore_ok() {
        let stdout = "  Determining projects to restore...\n  Restored /home/user/project.csproj\n";
        let result = apply_dotnet("restore", stdout, "", 0);
        assert_eq!(result, "ok dotnet restore\n");
    }

    #[test]
    fn test_errors_only_with_errors() {
        let stderr = "error: something went wrong\nfatal: cannot continue\n";
        let result = errors_only("", stderr);
        assert!(result.contains("2 errors"));
        assert!(result.contains("something went wrong"));
    }

    #[test]
    fn test_is_known_command() {
        assert!(is_known_command("git"));
        assert!(is_known_command("dotnet"));
        assert!(is_known_command("terraform"));
        assert!(!is_known_command("unknown_tool"));
    }
}
