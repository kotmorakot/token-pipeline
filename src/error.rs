use std::fmt;
use std::io;

#[allow(dead_code)]
#[derive(Debug)]
pub enum TpError {
    Io(io::Error),
    CommandNotFound(String),
    CommandFailed { cmd: String, code: i32 },
    ConfigParse(String),
    CacheCorrupt(String),
    SerdeJson(serde_json::Error),
}

impl fmt::Display for TpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::CommandNotFound(cmd) => write!(f, "command not found: {}", cmd),
            Self::CommandFailed { cmd, code } => {
                write!(f, "'{}' exited with code {}", cmd, code)
            }
            Self::ConfigParse(msg) => write!(f, "config error: {}", msg),
            Self::CacheCorrupt(msg) => write!(f, "cache error: {}", msg),
            Self::SerdeJson(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl std::error::Error for TpError {}

impl From<io::Error> for TpError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for TpError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerdeJson(e)
    }
}

#[allow(dead_code)]
pub type TpResult<T> = Result<T, TpError>;
