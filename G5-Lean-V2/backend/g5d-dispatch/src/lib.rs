//! Workspace guest dispatcher: P4.2a's file, port and private lifecycle
//! methods plus P4.2b's declared-module runtime, journal and built-in modules.
//! It is not an isolation backend or a public API.
pub mod config;
pub mod file_tree;
pub mod files;
pub mod git;
pub mod git_transport;
pub mod journal;
pub mod lease;
pub mod modules;
pub mod protocol;
mod server;
pub use server::MAX_RPC_BYTES;
pub mod watcher;
pub mod workspace_flush;
mod workspace_git;

pub use server::run_from_env;
