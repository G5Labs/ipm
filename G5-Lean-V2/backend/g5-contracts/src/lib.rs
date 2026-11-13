//! Dependency-light contracts shared across G5 Rust capability crates.
//!
//! This crate owns data and error contracts plus neutral helpers. It performs
//! no Git, network, live workspace file, parser, ledger, or storage operations.

pub mod attachments;
pub mod claude_models;
pub mod codex_models;
pub mod credential_scrub;
pub mod env;
pub mod error;
pub mod g5_paths;
mod git_author;
pub mod model_catalog;
pub mod model_catalog_gen;
pub mod provenance;
pub mod terminal;
#[cfg(feature = "tracing-compat")]
pub mod tracing_compat;
pub mod types;
pub mod utils;
pub mod workspace_git;

pub use error::{ErrorResponse, G5Error};
pub use git_author::GitAuthor;
