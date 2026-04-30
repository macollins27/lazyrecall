//! recall-core: discovery, parsing, indexing, and summarization of Claude Code session transcripts.

pub mod discovery;
pub mod index;
pub mod parser;
pub mod summarizer;
pub mod summarizer_worker;

pub use discovery::Project;
pub use index::Index;
pub use parser::{Event, EventKind, SessionMetadata};
pub use summarizer::Summarizer;
