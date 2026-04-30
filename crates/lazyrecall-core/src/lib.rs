//! lazyrecall-core: discovery, parsing, indexing, and summarization of Claude
//! Code session transcripts.
//!
//! The `lazyrecall` binary (in the sibling `lazyrecall-tui` crate) is a thin
//! UI over this crate. The split exists so a future GUI (Tauri, web, anything)
//! can reuse the core without a rewrite.

pub mod discovery;
pub mod error;
pub mod index;
pub mod log;
pub mod parser;
pub mod summarizer;
pub mod summarizer_worker;
pub mod watcher;

pub use discovery::Project;
pub use error::{Error, Result};
pub use index::{Index, IndexStats, MAX_SUMMARY_ATTEMPTS};
pub use parser::{Event, EventKind, SessionMetadata};
pub use summarizer::Summarizer;
