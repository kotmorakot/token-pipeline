use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default)]
    pub upstream_url: Option<String>,
    #[serde(default = "default_mode")]
    pub compression_mode: String,
    #[serde(default)]
    pub exclude_commands: Vec<String>,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
    #[serde(default = "default_cache_max")]
    pub cache_max_entries: usize,
}

fn default_mode() -> String {
    "full".to_string()
}
fn default_cache_ttl() -> u64 {
    3600
}
fn default_cache_max() -> usize {
    1000
}

impl Default for Config {
    fn default() -> Self {
        Self {
            upstream_url: None,
            compression_mode: default_mode(),
            exclude_commands: Vec::new(),
            cache_ttl_secs: default_cache_ttl(),
            cache_max_entries: default_cache_max(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match fs::read_to_string(&path) {
            Ok(content) => match toml_parse(&content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("tp: warning: failed to parse {}: {}", path.display(), e);
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn is_excluded(&self, cmd: &str) -> bool {
        self.exclude_commands.iter().any(|c| c == cmd)
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(&home).join(".config/tp/config.toml")
}

fn toml_parse(content: &str) -> Result<Config, String> {
    let mut cfg = Config::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');

            match key {
                "upstream_url" => cfg.upstream_url = Some(val.to_string()),
                "compression_mode" => cfg.compression_mode = val.to_string(),
                "cache_ttl_secs" => {
                    cfg.cache_ttl_secs = val.parse().unwrap_or(default_cache_ttl());
                }
                "cache_max_entries" => {
                    cfg.cache_max_entries = val.parse().unwrap_or(default_cache_max());
                }
                "exclude_commands" => {
                    let trimmed = val.trim_start_matches('[').trim_end_matches(']');
                    cfg.exclude_commands = trimmed
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    Ok(cfg)
}

pub fn ensure_config_dir() {
    let path = config_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("tp: warning: cannot create config dir: {}", e);
        }
    }
}

pub fn write_default_config() {
    let path = config_path();
    if path.exists() {
        println!("Config already exists at {}", path.display());
        return;
    }

    ensure_config_dir();

    let default = r#"# tp (token-pipeline) configuration
# Location: ~/.config/tp/config.toml

# Upstream LLM API URL for tp proxy
# upstream_url = "http://localhost:8000"

# Compression mode: lite | full | ultra
compression_mode = "full"

# Commands to exclude from filtering (run raw)
# exclude_commands = ["ssh", "vim", "nano"]

# Response cache TTL in seconds (default: 1 hour)
cache_ttl_secs = 3600

# Maximum cached responses (LRU eviction)
cache_max_entries = 1000
"#;

    if let Err(e) = fs::write(&path, default) {
        eprintln!("tp: failed to write config: {}", e);
    } else {
        println!("Created default config at {}", path.display());
    }
}
