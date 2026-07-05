use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

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
    access_order: Vec<String>,
    pub hits: u64,
    pub misses: u64,
    pub ttl_secs: u64,
    pub max_entries: usize,
}

impl ResponseCache {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_limits(3600, 1000)
    }

    pub fn with_limits(ttl_secs: u64, max_entries: usize) -> Self {
        let entries = load_cache_from_disk();
        let access_order: Vec<String> = entries.keys().cloned().collect();
        Self {
            entries,
            access_order,
            hits: 0,
            misses: 0,
            ttl_secs,
            max_entries,
        }
    }

    #[allow(dead_code)]
    pub fn cache_key(messages_json: &str, model: &str) -> String {
        Self::cache_key_with_mode(messages_json, model, "full")
    }

    pub fn cache_key_with_mode(messages_json: &str, model: &str, mode: &str) -> String {
        let input = format!("{}:{}:{}", mode, model, messages_json);
        blake3::hash(input.as_bytes()).to_hex().to_string()
    }

    pub fn get(&mut self, key: &str) -> Option<&CachedResponse> {
        if let Some(entry) = self.entries.get(key) {
            let now = now_secs();
            if self.ttl_secs > 0 && now.saturating_sub(entry.timestamp) > self.ttl_secs {
                self.entries.remove(key);
                self.access_order.retain(|k| k != key);
                self.misses += 1;
                return None;
            }
            self.hits += 1;
            self.touch(key);
            self.entries.get(key)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn put(&mut self, key: String, response: CachedResponse) {
        if self.entries.len() >= self.max_entries {
            self.evict_oldest();
        }
        save_entry_to_disk(&key, &response);
        self.entries.insert(key.clone(), response);
        self.touch(&key);
    }

    pub fn stats(&self) -> (u64, u64, usize) {
        (self.hits, self.misses, self.entries.len())
    }

    fn touch(&mut self, key: &str) {
        self.access_order.retain(|k| k != key);
        self.access_order.push(key.to_string());
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self.access_order.first().cloned() {
            self.entries.remove(&oldest_key);
            self.access_order.remove(0);
            remove_entry_from_disk(&oldest_key);
        }
    }
}

// ─── Prompt Compression ─────────────────────────────────────────

pub fn compress_prompt(content: &str) -> String {
    compress_prompt_level(content, "full")
}

pub fn compress_prompt_level(content: &str, mode: &str) -> String {
    if content.len() < 80 {
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

    if mode == "ultra" {
        let ultra_phrases = [
            "basically ",
            "actually ",
            "essentially ",
            "literally ",
            "honestly ",
            "frankly ",
            "just ",
            "simply ",
            "I think ",
            "I believe ",
            "I'd say ",
            "In my opinion, ",
            "From my perspective, ",
            "It is worth noting that ",
            "It is worth mentioning that ",
            "Let me preface this by saying ",
            "Before we begin, ",
            "First and foremost, ",
        ];
        for p in &ultra_phrases {
            result = result.replace(p, "");
        }
    }

    result = result.replace("  ", " ");
    result.trim().to_string()
}

// ─── Disk Cache Management ──────────────────────────────────────

fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(&home).join(".local/share/token-pipeline/cache");
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("tp: warning: cannot create cache dir: {}", e);
    }
    dir
}

fn load_cache_from_disk() -> HashMap<String, CachedResponse> {
    let dir = cache_dir();
    let mut entries = HashMap::new();

    if let Ok(read_dir) = fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            if entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
            {
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
    let path = cache_dir().join(format!("{}.json", key));
    match serde_json::to_string(entry) {
        Ok(json) => {
            if let Err(e) = fs::write(&path, json) {
                eprintln!("tp: warning: cache write failed: {}", e);
            }
        }
        Err(e) => eprintln!("tp: warning: cache serialize failed: {}", e),
    }
}

fn remove_entry_from_disk(key: &str) {
    let path = cache_dir().join(format!("{}.json", key));
    let _ = fs::remove_file(path);
}

pub fn clear_disk_cache() {
    let dir = cache_dir();
    if let Ok(read_dir) = fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            if entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
            {
                if let Err(e) = fs::remove_file(entry.path()) {
                    eprintln!("tp: warning: failed to remove cache entry: {}", e);
                }
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
            if entry
                .path()
                .extension()
                .map(|e| e == "json")
                .unwrap_or(false)
            {
                count += 1;
                total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    println!(
        "Cache: {} entries, {:.1} KB",
        count,
        total_size as f64 / 1024.0
    );
    println!("Location: {}", dir.display());
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_deterministic() {
        let key1 = ResponseCache::cache_key("hello", "gpt-4");
        let key2 = ResponseCache::cache_key("hello", "gpt-4");
        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 64);
    }

    #[test]
    fn test_cache_key_different_models() {
        let key1 = ResponseCache::cache_key("hello", "gpt-4");
        let key2 = ResponseCache::cache_key("hello", "gpt-3.5");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_cache_put_get() {
        let mut cache = ResponseCache::with_limits(3600, 100);
        let key = "test_key".to_string();
        cache.put(
            key.clone(),
            CachedResponse {
                content: "cached content".to_string(),
                model: "gpt-4".to_string(),
                prompt_tokens: 10,
                completion_tokens: 20,
                timestamp: now_secs(),
            },
        );

        let result = cache.get(&key);
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "cached content");
        assert_eq!(cache.hits, 1);
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = ResponseCache::with_limits(3600, 100);
        assert!(cache.get("nonexistent").is_none());
        assert_eq!(cache.misses, 1);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let mut cache = ResponseCache::with_limits(3600, 1000);
        let initial = cache.entries.len();
        let limit = initial + 2;
        cache.max_entries = limit;

        for i in 0..4 {
            cache.put(
                format!("evict_test_{}", i),
                CachedResponse {
                    content: format!("content_{}", i),
                    model: "test".to_string(),
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    timestamp: now_secs(),
                },
            );
        }

        assert!(cache.entries.len() <= limit);
    }

    #[test]
    fn test_compress_prompt_short_passthrough() {
        let short = "Fix the bug.";
        assert_eq!(compress_prompt(short), short);
    }

    #[test]
    fn test_compress_prompt_removes_filler() {
        let verbose = "Could you please help me with this task? I would like you to please fix the bug. Thank you very much for your help with this task.";
        let result = compress_prompt(verbose);
        assert!(!result.contains("Could you please"));
        assert!(!result.contains("Thank you very much"));
        assert!(result.len() < verbose.len());
    }

    #[test]
    fn test_compress_prompt_ultra() {
        let verbose = "I think we should basically just fix the bug. In my opinion, it's essentially a simple issue. First and foremost, let me preface this by saying it needs attention.";
        let result = compress_prompt_level(verbose, "ultra");
        assert!(!result.contains("basically"));
        assert!(!result.contains("First and foremost"));
        assert!(result.len() < verbose.len());
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = ResponseCache::with_limits(3600, 1000);
        let initial_entries = cache.entries.len();
        let unique_key = format!("stats_test_{}", now_secs());
        cache.put(
            unique_key.clone(),
            CachedResponse {
                content: "c1".to_string(),
                model: "m".to_string(),
                prompt_tokens: 0,
                completion_tokens: 0,
                timestamp: now_secs(),
            },
        );
        cache.get(&unique_key);
        cache.get("absolutely_missing_key");

        let (hits, misses, entries) = cache.stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
        assert!(entries >= initial_entries + 1);
    }
}
