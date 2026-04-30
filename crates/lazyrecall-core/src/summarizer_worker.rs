//! Background loop that summarizes sessions missing a summary.
//!
//! The TUI spawns this on a dedicated thread (with its own Index connection
//! and tokio runtime). It polls the index for sessions where summary IS NULL,
//! reads the JSONL, truncates to a manageable size, calls Haiku, and writes
//! the result back. The TUI re-reads the index periodically and shows the
//! summary inline.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::index::Index;
use crate::summarizer::Summarizer;

/// Maximum characters of session content sent to Haiku per summary.
/// Sessions can be hundreds of MB; the tail is most representative of
/// what was achieved, so we keep the trailing `MAX_INPUT_CHARS`.
const MAX_INPUT_CHARS: usize = 30_000;

const IDLE_POLL_SECONDS: u64 = 10;
const ERROR_BACKOFF_SECONDS: u64 = 30;

pub async fn run(index: Index, summarizer: Summarizer) -> Result<()> {
    loop {
        match index.missing_summaries() {
            Ok(missing) if missing.is_empty() => {
                tokio::time::sleep(Duration::from_secs(IDLE_POLL_SECONDS)).await;
            }
            Ok(missing) => {
                for (id, path) in missing {
                    if let Err(e) = summarize_one(&index, &summarizer, &id, &path).await {
                        eprintln!("lazyrecall: summarize {} failed: {}", id, e);
                        tokio::time::sleep(Duration::from_secs(ERROR_BACKOFF_SECONDS)).await;
                    }
                }
            }
            Err(e) => {
                eprintln!("lazyrecall: index query failed: {}", e);
                tokio::time::sleep(Duration::from_secs(ERROR_BACKOFF_SECONDS)).await;
            }
        }
    }
}

async fn summarize_one(
    index: &Index,
    summarizer: &Summarizer,
    id: &str,
    path: &str,
) -> Result<()> {
    let content = std::fs::read_to_string(Path::new(path))?;
    let truncated = truncate_tail(&content, MAX_INPUT_CHARS);
    let summary = summarizer.summarize(&truncated).await?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    index.set_summary(id, summary.trim(), now)?;
    Ok(())
}

fn truncate_tail(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        s.to_string()
    } else {
        let skip = total - max_chars;
        s.chars().skip(skip).collect()
    }
}
