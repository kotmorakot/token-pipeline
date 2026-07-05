use crate::config::Config;

pub fn rewrite_command(cmd: &str, config: &Config) -> String {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }

    let base_cmd = trimmed.split_whitespace().next().unwrap_or("");
    if should_skip(base_cmd, config) {
        return trimmed.to_string();
    }

    if contains_compound_operator(trimmed) {
        rewrite_compound(trimmed, config)
    } else {
        wrap_single(trimmed, config)
    }
}

fn should_skip(cmd: &str, config: &Config) -> bool {
    let skip_always = [
        "cd", "pushd", "popd", "export", "unset", "alias", "source", ".", "eval", "exec",
        "exit", "return", "set", "shopt", "trap", "umask", "ulimit", "wait", "bg", "fg",
        "jobs", "kill", "disown", "suspend", "tp",
    ];
    skip_always.contains(&cmd) || config.is_excluded(cmd)
}

fn contains_compound_operator(cmd: &str) -> bool {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_char = ' ';
    let mut paren_depth = 0u32;

    for ch in cmd.chars() {
        if ch == '\'' && !in_double_quote && prev_char != '\\' {
            in_single_quote = !in_single_quote;
        } else if ch == '"' && !in_single_quote && prev_char != '\\' {
            in_double_quote = !in_double_quote;
        }

        if !in_single_quote && !in_double_quote {
            if ch == '(' {
                paren_depth += 1;
            } else if ch == ')' {
                paren_depth = paren_depth.saturating_sub(1);
            }

            if paren_depth == 0 {
                if (ch == '&' && prev_char == '&')
                    || (ch == '|' && prev_char == '|')
                    || ch == ';'
                    || (ch == '|' && prev_char != '|' && prev_char != '&')
                {
                    return true;
                }
            }
        }
        prev_char = ch;
    }
    false
}

fn rewrite_compound(cmd: &str, config: &Config) -> String {
    let segments = split_compound(cmd);
    let mut result = Vec::new();
    let mut after_pipe = false;

    for seg in &segments {
        match seg {
            Segment::Command(c) => {
                let trimmed = c.trim();
                if trimmed.is_empty() {
                    result.push(c.to_string());
                } else if after_pipe {
                    result.push(trimmed.to_string());
                } else {
                    result.push(wrap_single(trimmed, config));
                }
            }
            Segment::Operator(op) => {
                if op == "|" {
                    after_pipe = true;
                }
                result.push(format!(" {} ", op.trim()));
            }
        }
    }

    result.join("").trim().to_string()
}

fn wrap_single(cmd: &str, config: &Config) -> String {
    let base = cmd.split_whitespace().next().unwrap_or("");

    if should_skip(base, config) {
        return cmd.to_string();
    }

    if cmd.contains("$(") || cmd.contains('`') {
        return format!("tp run {}", cmd);
    }

    format!("tp run {}", cmd)
}

#[derive(Debug, PartialEq)]
enum Segment {
    Command(String),
    Operator(String),
}

fn split_compound(cmd: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = cmd.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_char = ' ';
    let mut paren_depth = 0u32;

    while let Some(ch) = chars.next() {
        if ch == '\'' && !in_double_quote && prev_char != '\\' {
            in_single_quote = !in_single_quote;
            current.push(ch);
        } else if ch == '"' && !in_single_quote && prev_char != '\\' {
            in_double_quote = !in_double_quote;
            current.push(ch);
        } else if in_single_quote || in_double_quote {
            current.push(ch);
        } else if ch == '(' {
            paren_depth += 1;
            current.push(ch);
        } else if ch == ')' {
            paren_depth = paren_depth.saturating_sub(1);
            current.push(ch);
        } else if paren_depth > 0 {
            current.push(ch);
        } else if ch == ';' {
            segments.push(Segment::Command(current.clone()));
            segments.push(Segment::Operator(";".to_string()));
            current.clear();
        } else if ch == '&' {
            if chars.peek() == Some(&'&') {
                chars.next();
                segments.push(Segment::Command(current.clone()));
                segments.push(Segment::Operator("&&".to_string()));
                current.clear();
            } else {
                current.push(ch);
            }
        } else if ch == '|' {
            if chars.peek() == Some(&'|') {
                chars.next();
                segments.push(Segment::Command(current.clone()));
                segments.push(Segment::Operator("||".to_string()));
                current.clear();
            } else {
                // Pipe: only rewrite the first command, keep rest as-is
                segments.push(Segment::Command(current.clone()));
                segments.push(Segment::Operator("|".to_string()));
                let rest: String = chars.collect();
                segments.push(Segment::Command(rest));
                return segments;
            }
        } else {
            current.push(ch);
        }
        prev_char = ch;
    }

    if !current.is_empty() || !segments.is_empty() {
        segments.push(Segment::Command(current));
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::default()
    }

    #[test]
    fn test_simple_rewrite() {
        assert_eq!(rewrite_command("git status", &cfg()), "tp run git status");
    }

    #[test]
    fn test_compound_and() {
        let result = rewrite_command("cargo fmt && cargo test", &cfg());
        assert_eq!(result, "tp run cargo fmt && tp run cargo test");
    }

    #[test]
    fn test_compound_or() {
        let result = rewrite_command("make build || echo failed", &cfg());
        assert_eq!(result, "tp run make build || tp run echo failed");
    }

    #[test]
    fn test_compound_semicolon() {
        let result = rewrite_command("git add . ; git commit -m 'fix'", &cfg());
        assert_eq!(result, "tp run git add . ; tp run git commit -m 'fix'");
    }

    #[test]
    fn test_pipe_only_first() {
        let result = rewrite_command("ls -la | grep test", &cfg());
        assert_eq!(result, "tp run ls -la | grep test");
    }

    #[test]
    fn test_skip_builtins() {
        assert_eq!(rewrite_command("cd /tmp", &cfg()), "cd /tmp");
        assert_eq!(rewrite_command("export FOO=bar", &cfg()), "export FOO=bar");
        assert_eq!(rewrite_command("tp run git status", &cfg()), "tp run git status");
    }

    #[test]
    fn test_exclude_config() {
        let mut config = Config::default();
        config.exclude_commands = vec!["ssh".to_string()];
        assert_eq!(rewrite_command("ssh server", &config), "ssh server");
    }

    #[test]
    fn test_quoted_strings_not_split() {
        let result = rewrite_command("echo 'hello && world'", &cfg());
        assert_eq!(result, "tp run echo 'hello && world'");
    }

    #[test]
    fn test_double_quoted_not_split() {
        let result = rewrite_command("echo \"foo || bar\"", &cfg());
        assert_eq!(result, "tp run echo \"foo || bar\"");
    }

    #[test]
    fn test_subshell_passthrough() {
        let result = rewrite_command("echo $(date)", &cfg());
        assert_eq!(result, "tp run echo $(date)");
    }

    #[test]
    fn test_empty_command() {
        assert_eq!(rewrite_command("", &cfg()), "");
        assert_eq!(rewrite_command("   ", &cfg()), "");
    }

    #[test]
    fn test_complex_compound() {
        let result = rewrite_command("git add . && git commit -m 'fix' && git push", &cfg());
        assert_eq!(
            result,
            "tp run git add . && tp run git commit -m 'fix' && tp run git push"
        );
    }

    #[test]
    fn test_mixed_operators() {
        let result = rewrite_command("cargo test && echo ok || echo fail", &cfg());
        assert_eq!(
            result,
            "tp run cargo test && tp run echo ok || tp run echo fail"
        );
    }
}
