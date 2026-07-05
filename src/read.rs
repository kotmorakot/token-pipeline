use std::fs;
use std::path::{Path, PathBuf};

pub fn read_file(path: &str) -> String {
    let p = Path::new(path);

    if !p.exists() {
        return format!("error: file not found: {}\n", path);
    }

    if let Err(msg) = check_path_safety(p) {
        return format!("error: {}\n", msg);
    }

    let metadata = match fs::metadata(p) {
        Ok(m) => m,
        Err(e) => return format!("error: cannot read {}: {}\n", path, e),
    };

    if metadata.is_dir() {
        return read_directory(path);
    }

    let size = metadata.len();
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");

    if is_binary_extension(ext) || size > 10_000_000 {
        return format!(
            "# {} (binary/{}, {} bytes)\n",
            path,
            ext,
            format_size(size)
        );
    }

    let content = match fs::read_to_string(p) {
        Ok(c) => c,
        Err(_) => {
            return format!(
                "# {} (binary, {} bytes)\n",
                path,
                format_size(size)
            );
        }
    };

    let content = redact_sensitive_values(&content, ext);

    let lines: Vec<&str> = content.lines().collect();
    let filename = p.file_name().and_then(|n| n.to_str()).unwrap_or(path);

    if is_config_extension(ext) {
        return format_config(filename, &content, lines.len());
    }

    if is_source_extension(ext) && lines.len() > 100 {
        return extract_signatures(filename, &lines);
    }

    if lines.len() <= 150 {
        return content;
    }

    format_large_file(filename, &lines)
}

fn check_path_safety(p: &Path) -> Result<(), String> {
    let canonical = match fs::canonicalize(p) {
        Ok(c) => c,
        Err(e) => return Err(format!("cannot resolve {}: {}", p.display(), e)),
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let canonical_cwd = fs::canonicalize(&cwd).unwrap_or(cwd);

    if canonical.starts_with(&canonical_cwd) {
        return Ok(());
    }

    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .ok()
        .and_then(|h| fs::canonicalize(h).ok());

    if let Some(home_dir) = home {
        if canonical.starts_with(&home_dir) {
            return Ok(());
        }
    }

    let tmp = fs::canonicalize("/tmp").unwrap_or_else(|_| PathBuf::from("/tmp"));
    if canonical.starts_with(&tmp) {
        return Ok(());
    }

    let sensitive_paths = ["/etc/shadow", "/etc/passwd", "/etc/sudoers"];
    for sp in &sensitive_paths {
        if canonical == PathBuf::from(sp) {
            return Err(format!("access denied: {} is a sensitive system file", canonical.display()));
        }
    }

    if !canonical.starts_with("/home") && !canonical.starts_with(&tmp) {
        return Err(format!(
            "access denied: {} is outside the project directory",
            canonical.display()
        ));
    }

    Ok(())
}

fn redact_sensitive_values(content: &str, ext: &str) -> String {
    if !matches!(ext.to_lowercase().as_str(), "env" | "ini" | "cfg" | "conf" | "properties" | "toml" | "yaml" | "yml") {
        return content.to_string();
    }

    let sensitive = ["SECRET", "TOKEN", "PASSWORD", "CREDENTIAL", "PRIVATE", "API_KEY"];
    content.lines().map(|line| {
        if let Some((key, _val)) = line.split_once('=') {
            let k = key.trim().to_uppercase();
            if sensitive.iter().any(|p| k.contains(p)) || (k.ends_with("_KEY") && k != "TERM_SESSION_KEY") {
                return format!("{}=***", key.trim());
            }
        }
        line.to_string()
    }).collect::<Vec<_>>().join("\n")
}

fn read_directory(path: &str) -> String {
    let mut entries = Vec::new();
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    if let Ok(read_dir) = fs::read_dir(path) {
        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                dirs.push(format!("  {}/", name));
            } else {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                files.push(format!("  {} ({})", name, format_size(size)));
            }
        }
    }

    dirs.sort();
    files.sort();
    entries.push(format!("# {} ({} dirs, {} files)", path, dirs.len(), files.len()));
    entries.extend(dirs);
    entries.extend(files);
    entries.join("\n") + "\n"
}

fn extract_signatures(filename: &str, lines: &[&str]) -> String {
    let mut result = Vec::new();
    result.push(format!("# {} ({} lines, signatures)", filename, lines.len()));

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let is_sig = t.starts_with("pub ")
            || t.starts_with("fn ")
            || t.starts_with("struct ")
            || t.starts_with("enum ")
            || t.starts_with("impl ")
            || t.starts_with("trait ")
            || t.starts_with("mod ")
            || t.starts_with("type ")
            || t.starts_with("class ")
            || t.starts_with("def ")
            || t.starts_with("function ")
            || t.starts_with("export ")
            || t.starts_with("interface ")
            || t.starts_with("const ")
            || (t.starts_with("///") || t.starts_with("//!"))
            || t.starts_with("# ") && i < 5
            || t.starts_with("## ");

        if is_sig {
            result.push(format!("{:4}| {}", i + 1, line));
        }
    }

    if result.len() <= 1 {
        format_large_file(filename, lines)
    } else {
        result.join("\n") + "\n"
    }
}

fn format_config(filename: &str, content: &str, line_count: usize) -> String {
    if line_count <= 100 {
        return content.to_string();
    }

    let mut result = Vec::new();
    result.push(format!("# {} ({} lines)", filename, line_count));

    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("//") {
            continue;
        }
        result.push(format!("  {}", line));
        if result.len() > 80 {
            result.push(format!("  ... (+{} more lines)", line_count - 80));
            break;
        }
    }

    result.join("\n") + "\n"
}

fn format_large_file(filename: &str, lines: &[&str]) -> String {
    let mut result = Vec::new();
    result.push(format!("# {} ({} lines)", filename, lines.len()));

    let head = 20.min(lines.len());
    let tail = 10.min(lines.len());

    for i in 0..head {
        result.push(format!("{:4}| {}", i + 1, lines[i]));
    }

    if lines.len() > head + tail {
        result.push(format!("  ... ({} lines omitted)", lines.len() - head - tail));
    }

    let start = lines.len().saturating_sub(tail);
    if start > head {
        for i in start..lines.len() {
            result.push(format!("{:4}| {}", i + 1, lines[i]));
        }
    }

    result.join("\n") + "\n"
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "exe" | "dll" | "so" | "dylib" | "o" | "a" | "bin" | "class" | "jar" | "war"
            | "zip" | "gz" | "tar" | "bz2" | "xz" | "7z" | "rar"
            | "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "svg" | "webp"
            | "mp3" | "mp4" | "avi" | "mkv" | "wav" | "flac"
            | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
            | "wasm" | "pyc" | "pyo" | "rlib"
    )
}

fn is_config_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf" | "env" | "properties"
            | "xml"
    )
}

fn is_source_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" | "kt" | "scala"
            | "rb" | "php" | "cs" | "cpp" | "c" | "h" | "hpp" | "swift" | "dart"
            | "lua" | "zig" | "nim" | "elixir" | "ex" | "exs" | "erl" | "hrl"
            | "ml" | "mli" | "hs" | "fs" | "fsi" | "fsx" | "vue" | "svelte"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_nonexistent() {
        let result = read_file("/tmp/tp_test_nonexistent_file_xyz");
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_read_small_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("small.txt");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(f, "line 1\nline 2\nline 3").unwrap();

        let result = read_file(file_path.to_str().unwrap());
        assert!(result.contains("line 1"));
    }

    #[test]
    fn test_read_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let result = read_file(dir.path().to_str().unwrap());
        assert!(result.contains("1 dirs"));
        assert!(result.contains("1 files"));
    }

    #[test]
    fn test_is_binary() {
        assert!(is_binary_extension("exe"));
        assert!(is_binary_extension("png"));
        assert!(!is_binary_extension("rs"));
        assert!(!is_binary_extension("txt"));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1536), "1.5KB");
        assert_eq!(format_size(1_500_000), "1.4MB");
    }
}
