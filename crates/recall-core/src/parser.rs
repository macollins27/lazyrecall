//! Parse Claude Code session JSONL files into structured metadata and event streams.
//!
//! Each line is one event. We care about a small subset:
//! - `type: "user"` with `message.content` either a string or block array (text + tool_result)
//! - `type: "assistant"` with `message.content` block array (text + tool_use)
//! - `type: "system"` for system messages (slash command output, etc.)
//! - other types (`file-history-snapshot`, `last-prompt`) are ignored
//!
//! Conversational events carry a `cwd` field that lets us recover the real project path
//! that the encoded directory name only approximates.

use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub cwd: Option<String>,
    pub message_count: usize,
    pub last_text_preview: String,
    pub last_modified_unix: i64,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventKind,
    pub timestamp: String,
    pub is_sidechain: bool,
    pub is_meta: bool,
}

#[derive(Debug, Clone)]
pub enum EventKind {
    UserText(String),
    UserToolResult { tool_id: String, content: String },
    AssistantText(String),
    AssistantToolUse { name: String, input: String },
    System(String),
}

const TEXT_PREVIEW_CHARS: usize = 200;
const EVENT_TEXT_CHARS: usize = 500;
const TOOL_RESULT_CHARS: usize = 300;
const TOOL_INPUT_CHARS: usize = 120;

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
    let mut message_count = 0usize;
    let mut cwd: Option<String> = None;
    let mut last_text_preview = String::new();

    for line in content.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                cwd = Some(c.to_string());
            }
        }

        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        if ty != "user" && ty != "assistant" {
            continue;
        }
        if v.get("isSidechain").and_then(|x| x.as_bool()).unwrap_or(false) {
            continue;
        }
        if v.get("isMeta").and_then(|x| x.as_bool()).unwrap_or(false) {
            continue;
        }

        message_count += 1;

        let text = first_text_block(&v);
        if !text.is_empty() {
            last_text_preview = take_chars(&text, TEXT_PREVIEW_CHARS);
        }
    }

    Ok(SessionMetadata {
        id,
        cwd,
        message_count,
        last_text_preview,
        last_modified_unix: modified,
    })
}

/// Parse the last `n` user/assistant/system events from the JSONL.
/// Sidechain (subagent) and meta events are skipped.
pub fn parse_recent(path: &Path, n: usize) -> Result<Vec<Event>> {
    let content = std::fs::read_to_string(path)?;
    let mut events: Vec<Event> = Vec::new();

    for line in content.lines() {
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        let is_sidechain = v.get("isSidechain").and_then(|x| x.as_bool()).unwrap_or(false);
        let is_meta = v.get("isMeta").and_then(|x| x.as_bool()).unwrap_or(false);
        if is_sidechain || is_meta {
            continue;
        }
        let timestamp = v
            .get("timestamp")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();

        let kinds = match ty {
            "user" => extract_user(&v),
            "assistant" => extract_assistant(&v),
            "system" => extract_system(&v),
            _ => continue,
        };

        for kind in kinds {
            events.push(Event {
                kind,
                timestamp: timestamp.clone(),
                is_sidechain,
                is_meta,
            });
        }
    }

    let total = events.len();
    Ok(if total > n {
        events.split_off(total - n)
    } else {
        events
    })
}

fn extract_user(v: &serde_json::Value) -> Vec<EventKind> {
    let Some(content) = v.get("message").and_then(|m| m.get("content")) else {
        return vec![];
    };
    if let Some(s) = content.as_str() {
        return vec![EventKind::UserText(take_chars(s, EVENT_TEXT_CHARS))];
    }
    let Some(arr) = content.as_array() else {
        return vec![];
    };
    let mut out = Vec::new();
    for block in arr {
        let block_type = block.get("type").and_then(|x| x.as_str()).unwrap_or("");
        match block_type {
            "text" => {
                if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                    out.push(EventKind::UserText(take_chars(t, EVENT_TEXT_CHARS)));
                }
            }
            "tool_result" => {
                let tool_id = block
                    .get("tool_use_id")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let content = tool_result_text(block.get("content"));
                out.push(EventKind::UserToolResult {
                    tool_id,
                    content: take_chars(&content, TOOL_RESULT_CHARS),
                });
            }
            _ => {}
        }
    }
    out
}

fn extract_assistant(v: &serde_json::Value) -> Vec<EventKind> {
    let Some(arr) = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    else {
        return vec![];
    };
    let mut out = Vec::new();
    for block in arr {
        let block_type = block.get("type").and_then(|x| x.as_str()).unwrap_or("");
        match block_type {
            "text" => {
                if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                    out.push(EventKind::AssistantText(take_chars(t, EVENT_TEXT_CHARS)));
                }
            }
            "tool_use" => {
                let name = block
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = block
                    .get("input")
                    .map(|i| serde_json::to_string(i).unwrap_or_default())
                    .unwrap_or_default();
                out.push(EventKind::AssistantToolUse {
                    name,
                    input: take_chars(&input, TOOL_INPUT_CHARS),
                });
            }
            _ => {}
        }
    }
    out
}

fn extract_system(v: &serde_json::Value) -> Vec<EventKind> {
    let body = v.get("content").and_then(|x| x.as_str()).unwrap_or("");
    if body.is_empty() {
        return vec![];
    }
    vec![EventKind::System(take_chars(body, EVENT_TEXT_CHARS))]
}

fn first_text_block(v: &serde_json::Value) -> String {
    let Some(content) = v.get("message").and_then(|m| m.get("content")) else {
        return String::new();
    };
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        for block in arr {
            let bt = block.get("type").and_then(|x| x.as_str()).unwrap_or("");
            if bt == "text" {
                if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                    return t.to_string();
                }
            }
        }
    }
    String::new()
}

/// Tool results carry their content either as a plain string or as a list of text blocks.
fn tool_result_text(v: Option<&serde_json::Value>) -> String {
    let Some(v) = v else {
        return String::new();
    };
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(arr) = v.as_array() {
        return arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

fn take_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
