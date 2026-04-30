//! Watch `~/.claude/projects/` recursively. On any new or modified `.jsonl` file,
//! touch the index so the summarizer's work-list and the TUI's status counters
//! pick it up.
//!
//! Designed to run on a dedicated thread with its own Index connection
//! (rusqlite::Connection is Send not Sync). The TUI re-queries the index on its
//! periodic tick, so the watcher does not need to push events into the TUI.

use std::path::Path;
use std::sync::mpsc;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};

use crate::index::Index;

pub fn run(projects_root: &Path, index: Index) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;
    watcher.watch(projects_root, RecursiveMode::Recursive)?;

    for res in rx {
        match res {
            Ok(event) => handle_event(&index, event),
            Err(e) => eprintln!("recall: watcher error: {}", e),
        }
    }
    Ok(())
}

fn handle_event(index: &Index, event: Event) {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
    ) {
        return;
    }
    for path in event.paths {
        if path.extension().is_none_or(|ext| ext != "jsonl") {
            continue;
        }
        let Some(parent) = path.parent() else {
            continue;
        };
        let Some(project_dir) = parent.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let mtime_unix = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let _ = index.touch_session(project_dir, id, &path, mtime_unix);
    }
}
