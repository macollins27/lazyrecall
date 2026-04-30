//! Discover Claude Code projects and sessions on the local filesystem.
//!
//! Claude Code stores transcripts at `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl`,
//! where `encoded-cwd` is the original cwd with `/` replaced by `-`. The encoding is lossy
//! (real path may have contained `-`), so for human display we recover the real cwd by
//! reading the first conversational event from the most recent session — every event in
//! the JSONL carries an authoritative `cwd` field.

use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Project {
    pub encoded_cwd: String,
    pub session_count: usize,
    /// Real cwd recovered from session JSONL, e.g. "/Users/maxwell/Developer/recall".
    pub real_cwd: Option<String>,
    /// Last segment of `real_cwd`, e.g. "recall". Used for the projects pane label.
    pub display_name: Option<String>,
    /// Newest session mtime in this project, unix seconds. Drives the sort order
    /// so projects with the most recent activity appear at the top.
    pub latest_mtime_unix: Option<i64>,
}

pub fn projects_root() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME env var not set")?;
    Ok(PathBuf::from(home).join(".claude").join("projects"))
}

pub fn list_projects() -> Result<Vec<Project>> {
    let root = projects_root()?;
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut projects = Vec::new();
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let encoded_cwd = entry.file_name().to_string_lossy().into_owned();
        let session_count = count_sessions(&entry.path())?;
        let (real_cwd, latest_mtime_unix) =
            inspect_newest_session(&entry.path()).unwrap_or((None, None));
        let display_name = real_cwd.as_deref().and_then(|p| {
            Path::new(p)
                .file_name()
                .and_then(|s| s.to_str())
                .map(String::from)
        });
        projects.push(Project {
            encoded_cwd,
            session_count,
            real_cwd,
            display_name,
            latest_mtime_unix,
        });
    }
    // Sort by recency: most-recently-active project first. Projects with no
    // recoverable mtime fall to the bottom in alphabetical order.
    projects.sort_by(|a, b| match (a.latest_mtime_unix, b.latest_mtime_unix) {
        (Some(t1), Some(t2)) => t2.cmp(&t1),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => {
            let a_key = a.display_name.as_deref().unwrap_or(&a.encoded_cwd);
            let b_key = b.display_name.as_deref().unwrap_or(&b.encoded_cwd);
            a_key.to_lowercase().cmp(&b_key.to_lowercase())
        }
    });
    Ok(projects)
}

pub fn list_sessions(project: &Project) -> Result<Vec<PathBuf>> {
    let dir = projects_root()?.join(&project.encoded_cwd);
    let mut sessions = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            sessions.push(path);
        }
    }
    sessions.sort();
    Ok(sessions)
}

fn count_sessions(dir: &Path) -> Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|ext| ext == "jsonl") {
            count += 1;
        }
    }
    Ok(count)
}

/// Find the most-recently-modified session in a project dir, return its `cwd`
/// (peeking ~10 lines into the JSONL) and its mtime as unix seconds. Both fields
/// are independently optional. Cheap: BufReader::lines reads on demand, so huge
/// sessions are not fully read.
fn inspect_newest_session(dir: &Path) -> Result<(Option<String>, Option<i64>)> {
    let mut newest: Option<(PathBuf, SystemTime)> = None;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.extension().is_some_and(|e| e == "jsonl") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(mt) = meta.modified() {
                if newest.as_ref().is_none_or(|(_, prev)| mt > *prev) {
                    newest = Some((path, mt));
                }
            }
        }
    }
    let Some((path, mtime)) = newest else {
        return Ok((None, None));
    };
    let mtime_unix = mtime
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64);

    let f = fs::File::open(&path)?;
    let reader = BufReader::new(f);
    let mut cwd: Option<String> = None;
    for (i, line_result) in reader.lines().enumerate() {
        if i > 10 {
            break;
        }
        let Ok(line) = line_result else { continue };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                cwd = Some(c.to_string());
                break;
            }
        }
    }
    Ok((cwd, mtime_unix))
}
