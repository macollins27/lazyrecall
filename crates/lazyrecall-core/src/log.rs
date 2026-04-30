//! Append-only error log at `~/.lazyrecall/log`.
//!
//! The TUI runs fullscreen, so `eprintln!` from spawned threads is invisible
//! to the user. We write structured error lines to a file instead. Best-effort:
//! if the log itself can't be written, we silently drop the message — there's
//! no useful place to surface a logging failure.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Append `[{unix_ts}] {scope}: {message}` to `~/.lazyrecall/log`.
pub fn error(scope: &str, message: impl AsRef<str>) {
    let _ = write_line(scope, message.as_ref());
}

fn write_line(scope: &str, message: &str) -> std::io::Result<()> {
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    writeln!(f, "[{ts}] {scope}: {message}")
}

fn log_path() -> std::io::Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME not set"))?;
    Ok(PathBuf::from(home).join(".lazyrecall").join("log"))
}
