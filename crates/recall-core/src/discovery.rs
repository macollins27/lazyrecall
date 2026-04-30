//! Discover Claude Code projects and sessions on the local filesystem.
//!
//! Claude Code stores transcripts at `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl`,
//! where `encoded-cwd` is the original cwd with `/` replaced by `-`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Project {
    pub encoded_cwd: String,
    pub session_count: usize,
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
        projects.push(Project { encoded_cwd, session_count });
    }
    projects.sort_by(|a, b| a.encoded_cwd.cmp(&b.encoded_cwd));
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
