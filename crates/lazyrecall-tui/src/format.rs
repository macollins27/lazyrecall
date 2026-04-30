//! Small formatting helpers used by the UI layer.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use lazyrecall_core::Project;
use ratatui::widgets::ListState;

pub fn oneline(s: impl AsRef<str>) -> String {
    s.as_ref().replace('\n', " ").replace('\r', " ")
}

pub fn truncate_display(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(3)).collect();
        out.push_str("...");
        out
    }
}

pub fn format_relative(unix_seconds: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let delta = now - unix_seconds;
    if delta < 60 {
        format!("{delta}s ago")
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86400)
    }
}

pub fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn display_project(project: &Project) -> String {
    if let Some(name) = &project.display_name {
        return name.clone();
    }
    project
        .encoded_cwd
        .strip_prefix('-')
        .unwrap_or(&project.encoded_cwd)
        .to_string()
}

pub fn move_state(state: &mut ListState, len: usize, delta: i32) {
    if len == 0 {
        state.select(None);
        return;
    }
    let cur = state.selected().unwrap_or(0) as i32;
    let new = (cur + delta).rem_euclid(len as i32);
    state.select(Some(new as usize));
}
