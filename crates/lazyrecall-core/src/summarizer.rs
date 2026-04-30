//! Summarize a session transcript using Claude Haiku 4.5.
//!
//! The caller passes a transcript slice (already truncated by `summarizer_worker`
//! to fit context); this module POSTs to the Anthropic Messages API and returns
//! a single-line summary.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const ANTHROPIC_API: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-haiku-4-5-20251001";
const SUMMARY_PROMPT: &str = "Summarize this Claude Code session in exactly 12 words. \
Focus on what was achieved or attempted, not on conversational filler. \
Output only the summary, no preamble.";

#[derive(Clone)]
pub struct Summarizer {
    client: reqwest::Client,
    api_key: String,
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct Response {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

impl Summarizer {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
        }
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| Error::ApiKeyUnset)?;
        Ok(Self::new(api_key))
    }

    pub async fn summarize(&self, transcript: &str) -> Result<String> {
        let body = Request {
            model: MODEL,
            max_tokens: 64,
            system: SUMMARY_PROMPT,
            messages: vec![Message {
                role: "user",
                content: transcript,
            }],
        };

        let resp = self
            .client
            .post(ANTHROPIC_API)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let parsed: Response = resp.json().await?;
        let text = parsed
            .content
            .into_iter()
            .next()
            .map(|c| c.text)
            .ok_or(Error::EmptyApiResponse)?;
        Ok(text.trim().to_string())
    }
}
