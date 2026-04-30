//! Watch `~/.claude/projects/` recursively. On any new or modified `.jsonl` file,
//! touch the index so the summarizer's work-list and the TUI's status counters
//! pick it up.
//!
//! Events are debounced (200 ms) so the per-line writes claude makes during a
//! streaming response coalesce into one index touch per session. Designed to
//! run on a dedicated thread with its own Index connection (rusqlite::Connection
//! is Send not Sync). The TUI re-queries the index on its periodic tick, so the
//! watcher does not need to push events into the TUI.

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, UNIX_EPOCH};

use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;

use crate::error::Result;
use crate::index::Index;
use crate::log;

const DEBOUNCE: Duration = Duration::from_millis(200);

pub fn run(projects_root: &Path, index: Index) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(DEBOUNCE, move |res| {
        let _ = tx.send(res);
    })?;
    debouncer
        .watcher()
        .watch(projects_root, RecursiveMode::Recursive)?;

    for res in rx {
        match res {
            Ok(events) => {
                for event in events {
                    handle_path(&index, &event.path);
                }
            }
            Err(e) => {
                log::error("watcher", format!("{e}"));
            }
        }
    }
    Ok(())
}

fn handle_path(index: &Index, path: &Path) {
    if path.extension().is_none_or(|ext| ext != "jsonl") {
        return;
    }
    let Some(parent) = path.parent() else {
        return;
    };
    let Some(project_dir) = parent.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    let mtime_unix = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if let Err(e) = index.touch_session(project_dir, id, path, mtime_unix) {
        log::error("watcher", format!("touch_session {id}: {e}"));
    }
}
