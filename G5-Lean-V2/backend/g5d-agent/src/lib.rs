//! One direct provider CLI turn behind the declared `agent.turn` module, with
//! the provider wire, session-file and runtime support it runs on.
//!
//! The dispatcher owns process admission, cancellation, durable event order
//! and reply delivery. This crate owns the provider protocol boundary: frozen
//! input validation, exact policy-to-argv mapping, provider-session restore
//! and harvest, Claude native control requests, and provider-neutral events.
//! Public runner types and port scanning are shared with the dispatcher.

pub mod port_scan;
pub mod runner;

mod codex_auth;
pub mod formal_review;
mod harness;
mod memory;
pub mod ontology_turn;
mod policy;
mod protocol;
mod provider_transport;
mod runtime;
pub mod subscription_usage;
mod types;

pub use policy::{resolve_policy, ResolvedPolicy};
pub use protocol::{ClaudeControl, NativeRequest, NativeRequestKind};
pub use runtime::execute;
pub use runtime::execute_with_status;
pub use types::*;
