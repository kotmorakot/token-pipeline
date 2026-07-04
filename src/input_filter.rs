/// Stage 1: Input Filtering — RTK-style command output compression
///
/// Each filter acts as a ConstraintPruner (KatGPT-RS concept):
/// removes information the LLM does NOT need, keeps everything it DOES.
///
/// Safety guarantee: code, errors, paths, and data are preserved exactly.
/// Only formatting, decoration, and redundant prose are removed.

use std::collections::HashMap;

pub fn apply(cmd: &str, stdout: &str, stderr: &str, exit_code: i32) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return format!("{}{}", stdout, stderr);
    }

    match parts[0] {
        "git" => apply_git(parts.get(1).copied().unwrap_or(""), stdout, stderr, exit_code),
        "ls" | "dir" | "exa" | "eza" => ls_compact(stdout),
        "cat" | "bat" | "head" | "tail" | "less" | "more" => smart_read(stdout, parts.get(1).copied()),
        "cargo" => apply_cargo(parts.get(1).copied().unwrap_or(""), stdout, stderr),
        "npm" | "pnpm" | "yarn" | "bun" => apply_js_runner(&parts, stdout, stderr),
        "pytest" | "jest" | "vitest" | "rspec" | "go" => test_compact(stdout, stderr),
        "grep" | "rg" | "ag" => grep_compact(stdout),
        "find" | "fd" => find_compact(stdout),
        "docker" | "podman" => apply_docker(parts.get(1).copied().unwrap_or(""), stdout, stderr),
        "kubectl" | "oc" | "helm" => dedup_lines(stdout),
        "env" | "printenv" => env_compact(stdout),
        "curl" | "wget" | "httpie" => truncate(stdout, 2000),
        "tree" => tree_compact(stdout),
        "ps" => ps_compact(stdout),
        "df" => df_compact(stdout),
        "make" | "cmake" | "ninja" => build_compact(stdout, stderr),
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

fn apply_docker(sub: &str, stdout: &str, stderr: &str) -> String {
    match sub {
        "ps" => docker_ps(stdout),
        "images" => docker_images(stdout),
        "logs" => dedup_lines(stdout),
        "build" => build_compact(stdout, stderr),
        _ => format!("{}{}", stdout, stderr),
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
        format!("[{}] {}\n", branch, stdout.lines().last().unwrap_or("unknown state"))
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
    let mut stats = (0usize, 0usize);

    for line in stdout.lines() {
        if line.starts_with("diff --git") {
            if !current_file.is_empty() && (stats.0 > 0 || stats.1 > 0) {
                result.push(format!(
                    "--- {} (+{} -{}) ---",
                    current_file, stats.0, stats.1
                ));
                stats = (0, 0);
            }
            current_file = line
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_start_matches("b/")
                .to_string();
        } else if line.starts_with("@@") {
            let hunk_header = line
                .split("@@")
                .nth(2)
                .unwrap_or("")
                .trim();
            if !hunk_header.is_empty() {
                result.push(format!("  @@ {} @@", line.split("@@").nth(1).unwrap_or("").trim()));
                result.push(format!("  // {}", hunk_header));
            } else {
                result.push(format!("  @@ {} @@", line.split("@@").nth(1).unwrap_or("").trim()));
            }
        } else if line.starts_with('+') && !line.starts_with("+++") {
            result.push(format!("  {}", line));
            stats.0 += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            result.push(format!("  {}", line));
            stats.1 += 1;
        }
    }

    if !current_file.is_empty() {
        result.insert(
            result
                .iter()
                .rposition(|l| l.starts_with("---"))
                .map(|i| i + 1)
                .unwrap_or(0),
            format!("--- {} (+{} -{}) ---", current_file, stats.0, stats.1),
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
        } else if !line.starts_with("Date:") && !line.trim().is_empty() && !hash.is_empty() && msg.is_empty()
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
        } else if !trimmed.is_empty() && (line.starts_with("    ") || line.starts_with('\t')) {
            if result.last().map(|l: &&str| l.starts_with("Date:")).unwrap_or(false) || result.is_empty() {
                result.push(*line);
            }
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

    let current = lines.iter().find(|l| l.starts_with("* ")).copied().unwrap_or("* ?");
    let others: Vec<&str> = lines
        .iter()
        .filter(|l| !l.starts_with("* "))
        .copied()
        .collect();
    format!(
        "{}\n({} other branches)\n",
        current.trim(),
        others.len()
    )
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
        format!("FAIL: git {} (exit {})\n{}{}", sub, exit_code, stdout, stderr)
    }
}

// ─── File/Directory Filters ──────────────────────────────────────

fn ls_compact(stdout: &str) -> String {
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("total ") {
            continue;
        }

        let name = trimmed.split_whitespace().last().unwrap_or(trimmed);
        if name == "." || name == ".." {
            continue;
        }

        if trimmed.starts_with('d') || name.ends_with('/') {
            dirs.push(name.trim_end_matches('/'));
        } else {
            files.push(name);
        }
    }

    if dirs.is_empty() && files.is_empty() {
        return stdout.to_string();
    }

    let mut result = format!("{} dirs, {} files\n", dirs.len(), files.len());
    for d in &dirs {
        result.push_str(&format!("  {}/\n", d));
    }
    for f in &files {
        result.push_str(&format!("  {}\n", f));
    }
    result
}

fn smart_read(stdout: &str, filename: Option<&str>) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 80 {
        return stdout.to_string();
    }

    let mut result = Vec::new();
    let ext = filename.and_then(|f| f.rsplit('.').next()).unwrap_or("");
    result.push(format!(
        "# {} ({} lines, key parts shown)",
        filename.unwrap_or("file"),
        lines.len()
    ));

    let mut consecutive_blank = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            consecutive_blank += 1;
            if consecutive_blank <= 1 {
                result.push(String::new());
            }
            continue;
        }
        consecutive_blank = 0;

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
            || t.starts_with("async def ")
            || t.starts_with("function ")
            || t.starts_with("export ")
            || t.starts_with("import ")
            || t.starts_with("from ")
            || t.starts_with("const ")
            || t.starts_with("let ")
            || t.starts_with("var ")
            || t.starts_with("interface ")
            || t.starts_with("#[")
            || t.starts_with("@")
            || t.starts_with("///")
            || t.starts_with("//!")
            || t.starts_with("# ")
            || t.starts_with("## ")
            || i < 10
            || i >= lines.len() - 5;

        let _ = ext;

        if is_key {
            result.push(format!("{:4}| {}", i + 1, line));
        }
    }

    result.push(format!("# total {} lines", lines.len()));
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
    let mut depth_counts: HashMap<usize, usize> = HashMap::new();

    for line in &lines {
        let depth = line.len() - line.trim_start_matches(|c: char| c == ' ' || c == '│' || c == '├' || c == '└' || c == '─' || c == '|').len();
        *depth_counts.entry(depth / 4).or_insert(0) += 1;

        if depth / 4 <= 2 {
            result.push(*line);
        }
    }

    if result.len() < lines.len() {
        result.push(&"");
        let omitted = lines.len() - result.len();
        let summary = format!("({} deeper entries omitted, {} total)", omitted, lines.len());
        result.push(Box::leak(summary.into_boxed_str()));
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

        if t.contains("FAILED") || t.contains("FAIL ") || t.contains("panicked at") || t.contains("AssertionError") || t.contains("error[E") {
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

        if t.contains("test result:") || t.contains("Tests:") || (t.contains("passed") && t.contains("failed")) {
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
            let last_meaningful = lines.iter().rev().find(|l| !l.trim().is_empty()).unwrap_or(&"done");
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
        .filter(|l| l.contains("Finished") || l.contains("error:") || l.contains("warning:") || l.contains("Error:"))
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

fn ps_compact(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() <= 20 {
        return stdout.to_string();
    }

    let header = lines.first().unwrap_or(&"");
    let procs: Vec<&str> = lines.iter().skip(1).copied().collect();
    format!("{}\n({} processes, showing header only)\n", header, procs.len())
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
                result.push(*line);
            }
        }
    }

    if result.len() < lines.len() {
        result.push(Box::leak(format!("({} filesystems total, showing >70% used)", lines.len() - 1).into_boxed_str()));
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

    for line in &lines {
        if let Some((key, val)) = line.split_once('=') {
            let k = key.to_uppercase();
            if k.contains("SECRET")
                || k.contains("TOKEN")
                || k.contains("PASSWORD")
                || k.contains("KEY")
                || k.contains("CREDENTIAL")
                || k.contains("AUTH")
            {
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
        "{}...\n[truncated: {} → {} chars]\n",
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
            "{} lines → {} unique:\n{}",
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
