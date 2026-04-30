//! Persistent index over discovered sessions, stored at `~/.lazyrecall/index.db`.
//!
//! Schema is versioned from day 1 so V2+ migrations don't paint us into a corner.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::error::{Error, Result};
use crate::parser::SessionMetadata;

pub struct Index {
    conn: Connection,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IndexStats {
    pub total: usize,
    pub summarized: usize,
}

const SCHEMA_VERSION: i32 = 2;

/// Sessions whose summarizer has failed this many times stop being retried.
/// The TUI surfaces them with the recorded error in the preview pane.
pub const MAX_SUMMARY_ATTEMPTS: i64 = 3;

const SCHEMA_BOOTSTRAP: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
);
"#;

const SCHEMA_V1: &str = r#"
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

/// V2: track summarizer failures so a malformed session can't wedge the worker.
const SCHEMA_V2: &str = r#"
ALTER TABLE sessions ADD COLUMN summary_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN summary_last_error TEXT;
"#;

impl Index {
    pub fn data_dir() -> Result<PathBuf> {
        let home = std::env::var("HOME").map_err(|_| Error::HomeUnset)?;
        Ok(PathBuf::from(home).join(".lazyrecall"))
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
        self.conn.execute_batch(SCHEMA_BOOTSTRAP)?;
        let current: i32 = self
            .conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap_or(0);

        // V1 is idempotent (CREATE TABLE/INDEX IF NOT EXISTS), always safe to run.
        self.conn.execute_batch(SCHEMA_V1)?;

        // V2 uses ALTER TABLE which is not idempotent; only run once.
        if current < 2 {
            self.conn.execute_batch(SCHEMA_V2)?;
        }

        if current == 0 {
            self.conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![SCHEMA_VERSION],
            )?;
        } else if current < SCHEMA_VERSION {
            self.conn.execute(
                "UPDATE schema_version SET version = ?1",
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
        let total: i64 = self
            .conn
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

    pub fn set_summary(
        &self,
        session_id: &str,
        summary: &str,
        generated_at_unix: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions
             SET summary = ?1, summary_generated_at = ?2, summary_last_error = NULL
             WHERE id = ?3",
            params![summary, generated_at_unix, session_id],
        )?;
        Ok(())
    }

    /// Record a failed summarization attempt. Increments `summary_attempts` and
    /// stores the error message so the worker can skip sessions that have hit
    /// `MAX_SUMMARY_ATTEMPTS` and the TUI can surface why.
    pub fn record_summary_failure(&self, session_id: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions
             SET summary_attempts = summary_attempts + 1,
                 summary_last_error = ?1
             WHERE id = ?2",
            params![error, session_id],
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

    /// Sessions that still need a summary. Skips sessions that have already
    /// failed `MAX_SUMMARY_ATTEMPTS` times so the worker can't wedge on them.
    pub fn missing_summaries(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path FROM sessions
             WHERE summary IS NULL
               AND summary_attempts < ?1
             ORDER BY mtime DESC",
        )?;
        let rows = stmt
            .query_map(params![MAX_SUMMARY_ATTEMPTS], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fresh_index() -> (tempfile::TempDir, Index) {
        let dir = tempdir().unwrap();
        let idx = Index::open(&dir.path().join("index.db")).unwrap();
        (dir, idx)
    }

    fn fake_meta(id: &str, mtime: i64) -> SessionMetadata {
        SessionMetadata {
            id: id.to_string(),
            cwd: Some("/tmp/proj".to_string()),
            message_count: 5,
            last_text_preview: "preview".to_string(),
            last_modified_unix: mtime,
        }
    }

    #[test]
    fn migrate_creates_schema_at_current_version() {
        let (_dir, idx) = fresh_index();
        let v: i32 = idx
            .conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn upsert_then_set_summary_then_query() {
        let (_dir, idx) = fresh_index();
        let path = std::path::Path::new("/tmp/proj/abc.jsonl");
        idx.upsert_session("encoded-proj", path, &fake_meta("abc", 1000))
            .unwrap();

        idx.set_summary("abc", "did some stuff", 2000).unwrap();

        let summaries = idx.project_summaries("encoded-proj").unwrap();
        assert_eq!(
            summaries.get("abc").map(String::as_str),
            Some("did some stuff")
        );

        let stats = idx.stats().unwrap();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.summarized, 1);
    }

    #[test]
    fn missing_summaries_excludes_after_max_attempts() {
        let (_dir, idx) = fresh_index();
        let path = std::path::Path::new("/tmp/proj/abc.jsonl");
        idx.upsert_session("encoded-proj", path, &fake_meta("abc", 1000))
            .unwrap();

        // Brand new session: shows up in missing_summaries.
        assert_eq!(idx.missing_summaries().unwrap().len(), 1);

        // Record MAX_SUMMARY_ATTEMPTS failures.
        for _ in 0..MAX_SUMMARY_ATTEMPTS {
            idx.record_summary_failure("abc", "fake error").unwrap();
        }

        // Now excluded — worker won't wedge on it.
        assert_eq!(idx.missing_summaries().unwrap().len(), 0);
    }

    #[test]
    fn touch_session_is_idempotent() {
        let (_dir, idx) = fresh_index();
        let path = std::path::Path::new("/tmp/proj/abc.jsonl");
        idx.touch_session("encoded-proj", "abc", path, 1000)
            .unwrap();
        idx.touch_session("encoded-proj", "abc", path, 2000)
            .unwrap();
        let stats = idx.stats().unwrap();
        assert_eq!(stats.total, 1);
    }
}
