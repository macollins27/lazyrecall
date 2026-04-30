//! Application state: the `App` struct, focus + pane management, and the
//! caches the UI reads from.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use lazyrecall_core::{discovery, parser, Event as SessionEvent, Index, IndexStats, Project};

use ratatui::widgets::ListState;

use crate::format::{move_state, mtime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Projects,
    Sessions,
    Preview,
}

pub struct App {
    pub projects: Vec<Project>,
    pub project_state: ListState,
    pub sessions: Vec<PathBuf>,
    pub session_state: ListState,
    pub metadata_cache: HashMap<PathBuf, parser::SessionMetadata>,
    pub recent_cache: HashMap<PathBuf, Vec<SessionEvent>>,
    pub summary_cache: HashMap<String, String>,
    pub focus: Pane,
    pub resume_request: Option<(String, Option<String>)>,
    pub last_loaded_project_idx: Option<usize>,
    pub index: Index,
    pub stats: IndexStats,
    pub api_key_set: bool,
}

impl App {
    pub fn new(projects: Vec<Project>, index: Index, api_key_set: bool) -> Self {
        let mut project_state = ListState::default();
        if !projects.is_empty() {
            project_state.select(Some(0));
        }
        let stats = index.stats().unwrap_or_default();
        Self {
            projects,
            project_state,
            sessions: Vec::new(),
            session_state: ListState::default(),
            metadata_cache: HashMap::new(),
            recent_cache: HashMap::new(),
            summary_cache: HashMap::new(),
            focus: Pane::Projects,
            resume_request: None,
            last_loaded_project_idx: None,
            index,
            stats,
            api_key_set,
        }
    }

    pub fn refresh_sessions(&mut self) {
        let Some(idx) = self.project_state.selected() else {
            return;
        };
        if self.last_loaded_project_idx == Some(idx) {
            return;
        }
        let project = self.projects[idx].clone();
        self.sessions = discovery::list_sessions(&project).unwrap_or_default();
        self.sessions.sort_by(|a, b| mtime(b).cmp(&mtime(a)));
        self.session_state = ListState::default();
        if !self.sessions.is_empty() {
            self.session_state.select(Some(0));
        }
        self.last_loaded_project_idx = Some(idx);
        self.refresh_index_state();
    }

    pub fn refresh_index_state(&mut self) {
        if let Ok(stats) = self.index.stats() {
            self.stats = stats;
        }
        let Some(idx) = self.last_loaded_project_idx else {
            return;
        };
        let Some(project) = self.projects.get(idx) else {
            return;
        };
        if let Ok(summaries) = self.index.project_summaries(&project.encoded_cwd) {
            self.summary_cache = summaries;
        }
    }

    pub fn current_session(&self) -> Option<&PathBuf> {
        self.session_state
            .selected()
            .and_then(|i| self.sessions.get(i))
    }

    pub fn current_session_metadata(&mut self) -> Option<&parser::SessionMetadata> {
        let path = self.current_session()?.clone();
        if !self.metadata_cache.contains_key(&path) {
            if let Ok(meta) = parser::parse_metadata(&path) {
                let project_key = self
                    .last_loaded_project_idx
                    .and_then(|i| self.projects.get(i))
                    .map(|p| p.encoded_cwd.clone())
                    .unwrap_or_default();
                let _ = self.index.upsert_session(&project_key, &path, &meta);
                self.metadata_cache.insert(path.clone(), meta);
            }
        }
        self.metadata_cache.get(&path)
    }

    pub fn current_recent_events(&mut self) -> Option<&Vec<SessionEvent>> {
        let path = self.current_session()?.clone();
        if !self.recent_cache.contains_key(&path) {
            let events = parser::parse_recent(&path, 6).unwrap_or_default();
            self.recent_cache.insert(path.clone(), events);
        }
        self.recent_cache.get(&path)
    }

    pub fn move_down(&mut self) {
        match self.focus {
            Pane::Projects => {
                move_state(&mut self.project_state, self.projects.len(), 1);
                self.refresh_sessions();
            }
            Pane::Sessions => move_state(&mut self.session_state, self.sessions.len(), 1),
            Pane::Preview => {}
        }
    }

    pub fn move_up(&mut self) {
        match self.focus {
            Pane::Projects => {
                move_state(&mut self.project_state, self.projects.len(), -1);
                self.refresh_sessions();
            }
            Pane::Sessions => move_state(&mut self.session_state, self.sessions.len(), -1),
            Pane::Preview => {}
        }
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Pane::Projects => Pane::Sessions,
            Pane::Sessions => Pane::Preview,
            Pane::Preview => Pane::Projects,
        };
    }

    pub fn cycle_focus_back(&mut self) {
        self.focus = match self.focus {
            Pane::Projects => Pane::Preview,
            Pane::Sessions => Pane::Projects,
            Pane::Preview => Pane::Sessions,
        };
    }

    pub fn request_resume(&mut self) {
        let stem = self
            .current_session()
            .and_then(|p| p.file_stem().and_then(|s| s.to_str()))
            .map(|s| s.to_string());
        let Some(stem) = stem else { return };
        // claude --resume scopes lookup to the cwd it runs in. Capture the
        // session's recorded cwd so main() can chdir before exec — otherwise
        // claude searches the wrong project dir and reports "transcript does
        // not exist".
        let metadata_cwd = self.current_session_metadata().and_then(|m| m.cwd.clone());
        let cwd = metadata_cwd.or_else(|| {
            self.last_loaded_project_idx
                .and_then(|i| self.projects.get(i))
                .and_then(|p| p.real_cwd.clone())
        });
        self.resume_request = Some((stem, cwd));
    }
}

/// Load the Anthropic API key. Prefers `ANTHROPIC_API_KEY` in the environment;
/// falls back to a key stored at `~/.lazyrecall/api-key` (one line, mode 0600
/// recommended). The file-based path keeps the secret out of shell history
/// and shell config.
pub fn load_api_key() -> Option<String> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        let trimmed = k.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home)
        .join(".lazyrecall")
        .join("api-key");
    let content = std::fs::read_to_string(&path).ok()?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub fn seed_index(index: &Index, projects: &[Project]) {
    for project in projects {
        let Ok(sessions) = discovery::list_sessions(project) else {
            continue;
        };
        for path in sessions {
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let mtime_unix = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let _ = index.touch_session(&project.encoded_cwd, id, &path, mtime_unix);
        }
    }
}
