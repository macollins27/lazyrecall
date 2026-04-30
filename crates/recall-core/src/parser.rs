//! Parse Claude Code session JSONL files into structured metadata.
//!
//! Each line is one event: user message, assistant message, tool use, tool result.
//! V1 keeps event shape opaque (loose JSON) and only extracts metadata for the index.
//! V1.5+ replaces this with a typed event model and stream parsing.

use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub message_count: usize,
    pub last_message_preview: String,
    pub last_modified_unix: i64,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub metadata: SessionMetadata,
    pub recent_events: Vec<RawEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawEvent(pub serde_json::Value);

pub fn parse_metadata(path: &Path) -> Result<SessionMetadata> {
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("session path has no file stem")?
        .to_string();

    let modified = std::fs::metadata(path)?
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_secs() as i64;

    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let message_count = lines.len();

    let last_message_preview = lines
        .last()
        .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .as_ref()
        .and_then(extract_text_preview)
        .unwrap_or_default();

    Ok(SessionMetadata {
        id,
        message_count,
        last_message_preview,
        last_modified_unix: modified,
    })
}

fn extract_text_preview(v: &serde_json::Value) -> Option<String> {
    fn walk(v: &serde_json::Value, depth: u32) -> Option<String> {
        if depth > 6 {
            return None;
        }
        match v {
            serde_json::Value::String(s) if s.len() > 8 => Some(s.chars().take(120).collect()),
            serde_json::Value::Array(arr) => arr.iter().find_map(|x| walk(x, depth + 1)),
            serde_json::Value::Object(obj) => {
                for key in ["text", "content", "message"] {
                    if let Some(found) = obj.get(key).and_then(|x| walk(x, depth + 1)) {
                        return Some(found);
                    }
                }
                obj.values().find_map(|x| walk(x, depth + 1))
            }
            _ => None,
        }
    }
    walk(v, 0)
}
