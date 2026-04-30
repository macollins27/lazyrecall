//! Persistent index over discovered sessions, stored at `~/.recall/index.db`.
//!
//! Schema is versioned from day 1 so V2+ migrations don't paint us into a corner.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::parser::SessionMetadata;

pub struct Index {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IndexStats {
    pub total: usize,
    pub summarized: usize,
}

const SCHEMA_VERSION: i32 = 1;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL,
    path TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    message_count INTEGER NOT NULL,
    last_message_preview TEXT NOT NULL DEFAULT '',
    summary TEXT,
    summary_generated_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project);
CREATE INDEX IF NOT EXISTS idx_sessions_mtime ON sessions(mtime DESC);
"#;

impl Index {
    pub fn data_dir() -> Result<PathBuf> {
        let home = std::env::var("HOME").context("HOME env var not set")?;
        Ok(PathBuf::from(home).join(".recall"))
    }

    pub fn default_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("index.db"))
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let index = Self { conn };
        index.migrate()?;
        Ok(index)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA_V1)?;
        let current: Option<i32> = self
            .conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| r.get(0))
            .ok();
        if current.is_none() {
            self.conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![SCHEMA_VERSION],
            )?;
        }
        Ok(())
    }

    /// Insert a row for a session if it doesn't already exist. No-op if it does.
    /// Used by the startup scan to seed the work list for the summarizer without
    /// reading or parsing every JSONL file (which is slow for large transcripts).
    pub fn touch_session(&self, project: &str, id: &str, path: &Path, mtime: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions (id, project, path, mtime, message_count)
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![id, project, path.to_string_lossy(), mtime],
        )?;
        Ok(())
    }

    pub fn stats(&self) -> Result<IndexStats> {
        let total: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
        let summarized: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE summary IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(IndexStats {
            total: total as usize,
            summarized: summarized as usize,
        })
    }

    pub fn upsert_session(&self, project: &str, path: &Path, meta: &SessionMetadata) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (id, project, path, mtime, message_count, last_message_preview)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                project = excluded.project,
                path = excluded.path,
                mtime = excluded.mtime,
                message_count = excluded.message_count,
                last_message_preview = excluded.last_message_preview",
            params![
                meta.id,
                project,
                path.to_string_lossy(),
                meta.last_modified_unix,
                meta.message_count as i64,
                meta.last_text_preview,
            ],
        )?;
        Ok(())
    }

    pub fn set_summary(&self, session_id: &str, summary: &str, generated_at_unix: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET summary = ?1, summary_generated_at = ?2 WHERE id = ?3",
            params![summary, generated_at_unix, session_id],
        )?;
        Ok(())
    }

    /// All non-null summaries for sessions in a given project, keyed by session id.
    /// Used by the TUI to render summary text inline in the sessions list.
    pub fn project_summaries(&self, project: &str) -> Result<HashMap<String, String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, summary FROM sessions WHERE project = ?1 AND summary IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![project], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (id, summary) = row?;
            out.insert(id, summary);
        }
        Ok(out)
    }

    pub fn missing_summaries(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path FROM sessions WHERE summary IS NULL ORDER BY mtime DESC",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
