//! udjat's session model and the [`Harness`] trait.
//!
//! A *harness* is anything that runs or holds LLM sessions: opencode,
//! pi, Claude Code, or a read-only archive of their session files.
//! Every harness implements [`Harness`] and declares the
//! [`Capability`] groups it supports; callers check the declared groups
//! and never need to know how the harness is reached (linked in-process,
//! over HTTP, or over ACP).

mod harness;
mod kind;
mod paths;
mod query;
mod session;
mod transcript;

pub use harness::{Capability, Harness, Unsupported};
pub use kind::{HarnessKind, detect_kind};
pub use paths::file_path_matches;
pub use query::ListQuery;
pub use session::{
    Message, MessageLatencyPair, MessagePartLatencies, ModelAttribution, Session, TokenUsage,
    brand_for_llm_provider, infer_llm_provider_from_model,
};
pub use transcript::{EntryType, Role, TranscriptEntry, merge_transcripts};
