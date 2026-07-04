/// Stage 2: Optimization — KatGPT-RS-inspired caching and validation
///
/// BLAKE3-based response caching: identical prompts get cached results instantly.
/// Constraint validation: check structured output without re-calling LLM.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ─── BLAKE3 Response Cache ───────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct CachedResponse {
    pub content: String,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub timestamp: u64,
}

pub struct ResponseCache {
    entries: HashMap<String, CachedResponse>,
    pub hits: u64,
    pub misses: u64,
}

impl ResponseCache {
    pub fn new() -> Self {
        let entries = load_cache_from_disk();
        Self {
            entries,
            hits: 0,
            misses: 0,
        }
    }

    pub fn cache_key(messages_json: &str, model: &str) -> String {
        let input = format!("{}:{}", model, messages_json);
        blake3::hash(input.as_bytes()).to_hex().to_string()
    }

    pub fn get(&mut self, key: &str) -> Option<&CachedResponse> {
        if self.entries.contains_key(key) {
            self.hits += 1;
            self.entries.get(key)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn put(&mut self, key: String, response: CachedResponse) {
        self.entries.insert(key.clone(), response);
        save_entry_to_disk(&key, &self.entries[&key]);
    }

    pub fn stats(&self) -> (u64, u64, usize) {
        (self.hits, self.misses, self.entries.len())
    }
}

// ─── Constraint Validator ────────────────────────────────────────

pub struct ConstraintValidator;

impl ConstraintValidator {
    pub fn validate_json(text: &str) -> ValidationResult {
        let clean = extract_json_from_response(text);
        match serde_json::from_str::<serde_json::Value>(&clean) {
            Ok(val) => ValidationResult::Valid(serde_json::to_string(&val).unwrap_or_default()),
            Err(e) => ValidationResult::Invalid(e.to_string()),
        }
    }

    pub fn validate_code_block(text: &str) -> ValidationResult {
        if text.contains("```") {
            let blocks: Vec<&str> = text.split("```").collect();
            if blocks.len() % 2 == 1 {
                ValidationResult::Valid(text.to_string())
            } else {
                ValidationResult::Invalid("Unclosed code block".to_string())
            }
        } else {
            ValidationResult::Valid(text.to_string())
        }
    }

    pub fn try_fix_json(text: &str) -> Option<String> {
        let clean = extract_json_from_response(text);

        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&clean) {
            return Some(serde_json::to_string_pretty(&val).unwrap_or_default());
        }

        if let Some(start) = clean.find('{') {
            if let Some(end) = clean.rfind('}') {
                let slice = &clean[start..=end];
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(slice) {
                    return Some(serde_json::to_string_pretty(&val).unwrap_or_default());
                }
            }
        }

        if let Some(start) = clean.find('[') {
            if let Some(end) = clean.rfind(']') {
                let slice = &clean[start..=end];
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(slice) {
                    return Some(serde_json::to_string_pretty(&val).unwrap_or_default());
                }
            }
        }

        None
    }
}

pub enum ValidationResult {
    Valid(String),
    Invalid(String),
}

fn extract_json_from_response(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```json") {
        trimmed
            .strip_prefix("```json")
            .unwrap_or(trimmed)
            .strip_suffix("```")
            .unwrap_or(trimmed)
            .trim()
            .to_string()
    } else if trimmed.starts_with("```") && (trimmed.contains('{') || trimmed.contains('[')) {
        trimmed
            .strip_prefix("```")
            .unwrap_or(trimmed)
            .strip_suffix("```")
            .unwrap_or(trimmed)
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    }
}

// ─── Prompt Compression ─────────────────────────────────────────

pub fn compress_prompt(content: &str) -> String {
    if content.len() < 100 {
        return content.to_string();
    }

    let mut result = content.to_string();

    let removable_phrases = [
        "I would like you to please ",
        "Could you please ",
        "I'd appreciate if you could ",
        "Please help me ",
        "Can you help me ",
        "I need you to ",
        "Would you be able to ",
        "I was wondering if you could ",
        "It would be great if you could ",
        "Thank you very much for your help with this task.",
        "Thanks in advance.",
        "Thank you!",
        "Please and thank you.",
        "I really appreciate your help.",
        "Please make sure to ",
        "Make sure that ",
        "Please ensure that ",
        "Keep in mind that ",
        "Note that ",
        "It's important to note that ",
        "As you may know, ",
        "As we all know, ",
    ];

    for phrase in &removable_phrases {
        result = result.replace(phrase, "");
    }

    result = result.replace("  ", " ");
    result.trim().to_string()
}

// ─── Disk Cache Management ──────────────────────────────────────

fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(&home).join(".local/share/token-pipeline/cache");
    fs::create_dir_all(&dir).ok();
    dir
}

fn load_cache_from_disk() -> HashMap<String, CachedResponse> {
    let dir = cache_dir();
    let mut entries = HashMap::new();

    if let Ok(read_dir) = fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(data) = fs::read_to_string(entry.path()) {
                    if let Ok(cached) = serde_json::from_str::<CachedResponse>(&data) {
                        let key = entry
                            .path()
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        entries.insert(key, cached);
                    }
                }
            }
        }
    }

    entries
}

fn save_entry_to_disk(key: &str, entry: &CachedResponse) {
    let path = cache_dir().join(format!("{}.json", &key[..32.min(key.len())]));
    if let Ok(json) = serde_json::to_string(entry) {
        fs::write(path, json).ok();
    }
}

pub fn clear_disk_cache() {
    let dir = cache_dir();
    if let Ok(read_dir) = fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                fs::remove_file(entry.path()).ok();
            }
        }
    }
}

pub fn show_cache_info() {
    let dir = cache_dir();
    let mut count = 0;
    let mut total_size = 0u64;

    if let Ok(read_dir) = fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                count += 1;
                total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    println!("Cache: {} entries, {:.1} KB", count, total_size as f64 / 1024.0);
    println!("Location: {}", dir.display());
}
