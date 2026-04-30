//! Background loop that summarizes sessions missing a summary.
//!
//! The TUI spawns this on a dedicated thread (with its own Index connection
//! and tokio runtime). It polls the index for sessions where `summary IS NULL`
//! and `summary_attempts < MAX_SUMMARY_ATTEMPTS`, reads the JSONL, truncates
//! to a manageable size, calls Haiku in parallel (bounded), and writes the
//! result back. Failed sessions are recorded so a malformed one can't wedge
//! the worker.
//!
//! The TUI re-reads the index periodically and shows the summary inline.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::stream::{self, StreamExt};

use crate::error::Result;
use crate::index::Index;
use crate::log;
use crate::summarizer::Summarizer;

/// Maximum characters of session content sent to Haiku per summary.
/// Sessions can be hundreds of MB; the tail is most representative of
/// what was achieved, so we keep the trailing `MAX_INPUT_CHARS`.
const MAX_INPUT_CHARS: usize = 30_000;

/// Concurrent in-flight Haiku requests. Anthropic's per-key rate limit is
/// generous; the bottleneck is end-to-end latency. 6 strikes a safe balance.
const MAX_CONCURRENT: usize = 6;

const IDLE_POLL: Duration = Duration::from_secs(10);
const ERROR_BACKOFF: Duration = Duration::from_secs(30);

pub async fn run(index: Index, summarizer: Summarizer) -> Result<()> {
    loop {
        match index.missing_summaries() {
            Ok(missing) if missing.is_empty() => {
                tokio::time::sleep(IDLE_POLL).await;
            }
            Ok(missing) => {
                process_batch(&index, &summarizer, missing).await;
            }
            Err(e) => {
                log::error("summarizer", format!("index query: {e}"));
                tokio::time::sleep(ERROR_BACKOFF).await;
            }
        }
    }
}

async fn process_batch(index: &Index, summarizer: &Summarizer, missing: Vec<(String, String)>) {
    let stream = stream::iter(missing)
        .map(|(id, path)| async move {
            let result = summarize_one(summarizer, &path).await;
            (id, result)
        })
        .buffer_unordered(MAX_CONCURRENT);

    stream
        .for_each(|(id, result)| async move {
            match result {
                Ok(summary) => {
                    let now = unix_now();
                    if let Err(e) = index.set_summary(&id, summary.trim(), now) {
                        log::error("summarizer", format!("set_summary {id}: {e}"));
                    }
                }
                Err(e) => {
                    let msg = format!("summarize {id} failed: {e}");
                    log::error("summarizer", &msg);
                    let _ = index.record_summary_failure(&id, &e.to_string());
                }
            }
        })
        .await;
}

async fn summarize_one(summarizer: &Summarizer, path: &str) -> Result<String> {
    let content = std::fs::read_to_string(Path::new(path))?;
    let truncated = truncate_tail(&content, MAX_INPUT_CHARS);
    summarizer.summarize(&truncated).await
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

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::truncate_tail;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn truncate_tail_never_exceeds_max(s in ".{0,2000}", max in 0usize..2000) {
            let out = truncate_tail(&s, max);
            prop_assert!(out.chars().count() <= max);
        }

        #[test]
        fn truncate_tail_idempotent_below_max(s in ".{0,200}", max in 200usize..400) {
            let out = truncate_tail(&s, max);
            prop_assert_eq!(out, s);
        }

        #[test]
        fn truncate_tail_keeps_suffix(s in ".{500,1000}", max in 50usize..200) {
            let out = truncate_tail(&s, max);
            // The output should be a suffix of the input.
            prop_assert!(s.ends_with(&out));
        }
    }
}
