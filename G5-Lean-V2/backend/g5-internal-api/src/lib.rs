//! Shared internal API contracts for the G5 system.
//!
//! This module provides:
//! - [`InternalApiClient`] — a typed reqwest wrapper for calling g5-api's
//!   `/api/internal/*` endpoints with workspace-incarnation and host auth
//!   plus retry logic
//!   (gated behind the `http-client` feature)
//! - Shared DTO structs for retained internal task-graph, host, and workspace
//!   runtime endpoints
//!
//! By placing the client and DTOs in `g5-internal-api`, we avoid
//! crate-level dependency cycles: g5d and g5-api depend on this neutral shared
//! boundary, never on each other.
//!
//! # Feature Flags
//!
//! - `http-client`: Enables the [`InternalApiClient`] and the `reqwest` dependency.
//!   Without this feature, the DTO types are available.

pub mod codex_auth;
pub mod dtos;
pub mod host_ctl;
#[cfg(feature = "test-support")]
pub mod host_ctl_test_support;
pub mod session_files;
pub mod subscription_usage;
pub mod workspace_flush;

#[cfg(feature = "stream-mux")]
pub mod stream_mux;

#[cfg(feature = "http-client")]
pub mod client;

// Re-export all DTOs for convenience
pub use dtos::*;

// Re-export the client when the feature is enabled
#[cfg(feature = "http-client")]
pub use client::InternalApiClient;
