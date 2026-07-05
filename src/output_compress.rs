/// Stage 3: Output Compression — Caveman-style response compression
///
/// Compresses LLM response TEXT without touching code blocks, error messages,
/// paths, URLs, or technical terms. Only prose "fluff" is removed.
///
/// Three modes:
///   lite  — Remove filler/hedging, keep full sentences
///   full  — Drop articles, use fragments, short synonyms (default)
///   ultra — Maximum compression, one word when one word enough

use regex::Regex;

pub fn compress_text(text: &str, mode: &str) -> String {
    let segments = split_into_segments(text);
    let mut result = String::new();

    for segment in segments {
        match segment {
            Segment::Code(code) => result.push_str(&code),
            Segment::Prose(prose) => {
                let compressed = match mode {
                    "lite" => compress_lite(&prose),
                    "ultra" => compress_ultra(&prose),
                    _ => compress_full(&prose),
                };
                result.push_str(&compressed);
            }
        }
    }

    result
}

pub fn caveman_system_prompt(mode: &str) -> String {
    match mode {
        "lite" => SYSTEM_LITE.to_string(),
        "ultra" => SYSTEM_ULTRA.to_string(),
        _ => SYSTEM_FULL.to_string(),
    }
}

const SYSTEM_LITE: &str = "Respond concisely. No filler words, no hedging, no pleasantries. \
Keep full sentences but remove unnecessary elaboration. \
Code blocks, error messages, paths, URLs must be exact. \
Technical terms must be precise.";

const SYSTEM_FULL: &str = "Respond terse. Drop articles (a/an/the), filler (just/really/basically), \
pleasantries (sure/certainly/of course). Fragments OK. Short synonyms preferred. \
Code blocks, commands, errors, paths: byte-for-byte exact. \
Technical terms exact. Pattern: [thing] [action] [reason]. [next step].";

const SYSTEM_ULTRA: &str = "Maximum brevity. One word when one word enough. \
Drop articles, conjunctions when unambiguous, filler, hedging. \
State each fact once. Code/errors/paths exact. \
No prose abbreviations. No explanation unless asked.";

// ─── Segment Splitting ──────────────────────────────────────────

enum Segment {
    Code(String),
    Prose(String),
}

fn split_into_segments(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current_prose = String::new();
    let mut in_code_block = false;
    let mut code_buf = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            if in_code_block {
                code_buf.push_str(line);
                code_buf.push('\n');
                segments.push(Segment::Code(code_buf.clone()));
                code_buf.clear();
                in_code_block = false;
            } else {
                if !current_prose.is_empty() {
                    segments.push(Segment::Prose(current_prose.clone()));
                    current_prose.clear();
                }
                code_buf.push_str(line);
                code_buf.push('\n');
                in_code_block = true;
            }
        } else if in_code_block {
            code_buf.push_str(line);
            code_buf.push('\n');
        } else if is_protected_line(trimmed) {
            if !current_prose.is_empty() {
                segments.push(Segment::Prose(current_prose.clone()));
                current_prose.clear();
            }
            segments.push(Segment::Code(format!("{}\n", line)));
        } else {
            current_prose.push_str(line);
            current_prose.push('\n');
        }
    }

    if in_code_block {
        code_buf.push_str("```\n");
        segments.push(Segment::Code(code_buf));
    }
    if !current_prose.is_empty() {
        segments.push(Segment::Prose(current_prose));
    }

    segments
}

fn is_protected_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }

    line.starts_with("$ ")
        || line.starts_with("# ") && line.len() > 2 && line.chars().nth(2).map(|c| !c.is_alphabetic()).unwrap_or(false)
        || line.starts_with("```")
        || line.starts_with("    ") && looks_like_code(line.trim())
        || line.starts_with('\t') && looks_like_code(line.trim())
        || line.contains("error[E")
        || line.contains("Error:")
        || line.contains("FAIL")
        || line.contains("panic")
        || line.starts_with("diff --git")
        || line.starts_with("@@")
        || (line.starts_with('+') || line.starts_with('-')) && line.len() > 1
}

fn looks_like_code(line: &str) -> bool {
    line.ends_with(';')
        || line.ends_with('{')
        || line.ends_with('}')
        || line.ends_with(')')
        || line.starts_with("fn ")
        || line.starts_with("def ")
        || line.starts_with("class ")
        || line.starts_with("import ")
        || line.starts_with("const ")
        || line.starts_with("let ")
        || line.starts_with("var ")
        || line.contains("->")
        || line.contains("=>")
}

// ─── Compression Levels ─────────────────────────────────────────

fn compress_lite(text: &str) -> String {
    let mut result = text.to_string();

    let fillers = [
        "just ", "really ", "basically ", "actually ", "simply ",
        "obviously ", "clearly ", "naturally ", "essentially ",
        "honestly ", "frankly ", "literally ",
    ];
    for filler in &fillers {
        result = case_insensitive_remove(&result, filler);
    }

    let hedges = [
        "I think ", "I believe ", "I would say ", "It seems like ",
        "It appears that ", "It looks like ", "In my opinion, ",
        "From what I can see, ", "As far as I can tell, ",
    ];
    for hedge in &hedges {
        result = case_insensitive_remove(&result, hedge);
    }

    let pleasantries = [
        "Sure! ", "Sure, ", "Certainly! ", "Of course! ", "Of course, ",
        "Absolutely! ", "Great question! ", "Good question! ",
        "Happy to help! ", "I'd be happy to help you with that. ",
        "I'd be happy to help. ", "Let me help you with that. ",
        "No problem! ", "You're welcome! ",
    ];
    for p in &pleasantries {
        result = case_insensitive_remove(&result, p);
    }

    let wordy = [
        ("in order to ", "to "),
        ("due to the fact that ", "because "),
        ("for the purpose of ", "to "),
        ("at this point in time ", "now "),
        ("in the event that ", "if "),
        ("on the other hand, ", "but "),
        ("as a matter of fact, ", ""),
        ("it is important to note that ", ""),
        ("it should be noted that ", ""),
        ("the reason for this is that ", "because "),
    ];
    for (from, to) in &wordy {
        result = case_insensitive_replace(&result, from, to);
    }

    collapse_whitespace(&result)
}

fn compress_full(text: &str) -> String {
    let mut result = compress_lite(text);

    let articles_re = Regex::new(r"(?i)\b(a|an|the)\s+").unwrap();
    result = articles_re.replace_all(&result, "").to_string();

    let extra_fillers = [
        "very ", "quite ", "rather ", "somewhat ", "fairly ",
        "pretty much ", "more or less ", "kind of ", "sort of ",
        "in fact, ", "as well", "also ",
    ];
    for f in &extra_fillers {
        result = case_insensitive_remove(&result, f);
    }

    let shortenings = [
        ("implement", "add"),
        ("implement a solution for", "fix"),
        ("utilize", "use"),
        ("modification", "change"),
        ("functionality", "feature"),
        ("subsequently", "then"),
        ("previously", "before"),
        ("additionally", "also"),
        ("furthermore", "also"),
        ("however", "but"),
        ("therefore", "so"),
        ("consequently", "so"),
        ("approximately", "about"),
        ("configuration", "config"),
        ("application", "app"),
        ("information", "info"),
        ("documentation", "docs"),
        ("repository", "repo"),
        ("directory", "dir"),
        ("parameter", "param"),
        ("environment", "env"),
    ];
    for (from, to) in &shortenings {
        result = case_insensitive_replace(&result, from, to);
    }

    let narration = [
        "Let me explain. ",
        "Let me walk you through this. ",
        "Here's what's happening: ",
        "Here's the thing: ",
        "The key thing to understand is ",
        "What you need to know is ",
        "To summarize, ",
        "In summary, ",
        "To put it simply, ",
    ];
    for n in &narration {
        result = case_insensitive_remove(&result, n);
    }

    collapse_whitespace(&result)
}

fn compress_ultra(text: &str) -> String {
    let mut result = compress_full(text);

    let conjunctions_re = Regex::new(r"(?i)\b(and|but|or|so|yet|for|nor)\b,?\s+").unwrap();
    result = conjunctions_re.replace_all(&result, ". ").to_string();

    let be_verbs_re = Regex::new(r"(?i)\b(is|are|was|were|be|been|being)\s+").unwrap();
    result = be_verbs_re.replace_all(&result, "").to_string();

    let pronouns_re = Regex::new(r"(?i)\b(this|that|these|those|it|they|we|you)\s+").unwrap();
    result = pronouns_re.replace_all(&result, "").to_string();

    result = result.replace(". . ", ". ");
    result = result.replace(".. ", ". ");

    collapse_whitespace(&result)
}

// ─── Helpers ─────────────────────────────────────────────────────

fn case_insensitive_remove(text: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return text.to_string();
    }
    let lower_text = text.to_lowercase();
    let lower_pattern = pattern.to_lowercase();

    let mut result = String::new();
    let mut search_from = 0;

    while let Some(pos) = lower_text[search_from..].find(&lower_pattern) {
        let abs_pos = search_from + pos;
        result.push_str(&text[search_from..abs_pos]);
        search_from = abs_pos + pattern.len();
    }
    result.push_str(&text[search_from..]);

    result
}

fn case_insensitive_replace(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_string();
    }
    let lower_text = text.to_lowercase();
    let lower_from = from.to_lowercase();

    let mut result = String::new();
    let mut search_from = 0;

    while let Some(pos) = lower_text[search_from..].find(&lower_from) {
        let abs_pos = search_from + pos;
        result.push_str(&text[search_from..abs_pos]);
        result.push_str(to);
        search_from = abs_pos + from.len();
    }
    result.push_str(&text[search_from..]);

    result
}

fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_was_space = false;
    let mut last_was_newline = false;

    for ch in text.chars() {
        if ch == '\n' {
            if !last_was_newline {
                result.push('\n');
            }
            last_was_newline = true;
            last_was_space = false;
        } else if ch == ' ' || ch == '\t' {
            if !last_was_space && !last_was_newline {
                result.push(' ');
            }
            last_was_space = true;
        } else {
            result.push(ch);
            last_was_space = false;
            last_was_newline = false;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_lite_removes_fillers() {
        let text = "I think this is basically a just really good solution.";
        let result = compress_lite(text);
        assert!(!result.contains("basically"));
        assert!(!result.contains("just "));
        assert!(!result.contains("really "));
        assert!(!result.contains("I think "));
    }

    #[test]
    fn test_compress_full_removes_articles() {
        let text = "The quick brown fox jumps over a lazy dog in the park.";
        let result = compress_full(text);
        assert!(!result.contains("The "));
        assert!(!result.contains(" a "));
        assert!(!result.contains(" the "));
    }

    #[test]
    fn test_compress_ultra_aggressive() {
        let text = "This is a very long and complicated explanation that we need to understand.";
        let result = compress_ultra(text);
        assert!(result.len() < text.len());
    }

    #[test]
    fn test_code_block_preserved() {
        let text = "Here is code:\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\nThat was basically the solution.";
        let result = compress_text(text, "full");
        assert!(result.contains("fn main()"));
        assert!(result.contains("println!(\"hello\")"));
        assert!(result.contains("```rust"));
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(compress_text("", "lite"), "");
        assert_eq!(compress_text("", "full"), "");
        assert_eq!(compress_text("", "ultra"), "");
    }

    #[test]
    fn test_only_code_blocks() {
        let text = "```python\nprint('hello')\n```\n```bash\necho hi\n```\n";
        let result = compress_text(text, "full");
        assert!(result.contains("print('hello')"));
        assert!(result.contains("echo hi"));
    }

    #[test]
    fn test_mixed_prose_and_code() {
        let text = "Sure! Here's the solution:\n\n```rust\nlet x = 42;\n```\n\nI think this should work.\n";
        let result = compress_text(text, "lite");
        assert!(result.contains("let x = 42;"));
        assert!(!result.contains("Sure!"));
    }

    #[test]
    fn test_caveman_system_prompt() {
        let lite = caveman_system_prompt("lite");
        let full = caveman_system_prompt("full");
        let ultra = caveman_system_prompt("ultra");
        assert!(lite.contains("concisely"));
        assert!(full.contains("terse"));
        assert!(ultra.contains("brevity"));
    }

    #[test]
    fn test_protected_lines_preserved() {
        let text = "Some prose here.\n$ cargo build\nerror[E0308]: mismatched types\nMore prose.\n";
        let result = compress_text(text, "full");
        assert!(result.contains("$ cargo build"));
        assert!(result.contains("error[E0308]"));
    }

    #[test]
    fn test_wordy_phrases_shortened() {
        let text = "In order to fix this, due to the fact that it failed.";
        let result = compress_lite(text);
        assert!(result.contains("to fix"));
        assert!(result.contains("because"));
        assert!(!result.contains("In order to"));
        assert!(!result.contains("due to the fact that"));
    }
}
