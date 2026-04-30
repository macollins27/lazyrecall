//! Library error type. The binary keeps `anyhow` for ergonomics; the library
//! exposes typed variants so callers (the TUI today, a future Tauri GUI) can
//! match on what went wrong.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("http: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("notify: {0}")]
    Notify(#[from] notify::Error),

    #[error("system time: {0}")]
    SystemTime(#[from] std::time::SystemTimeError),

    #[error("HOME env var not set")]
    HomeUnset,

    #[error("ANTHROPIC_API_KEY env var not set")]
    ApiKeyUnset,

    #[error("invalid session path: {0}")]
    InvalidSessionPath(String),

    #[error("summarizer returned empty response")]
    EmptyApiResponse,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
