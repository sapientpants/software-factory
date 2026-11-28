//! The AgentDriver trait, Claude driver and event normalisation (SPEC 19.1).
//!
//! M0 builds the part that prepares a session (`session`) and what the agent
//! form of `sf sandbox exec` reads of the stream (`stream`), and no more of
//! the protocol (CLI-6).

pub mod session;
pub mod stream;
