//! g5d — workspace runtime, host, and local tooling facade.
//!
//! This library crate owns the `g5d` command-line interface (`cli`) and
//! re-exports the extracted runtime crates so that integration tests (in
//! `tests/`) can access them via `g5d::module_name`.

// Compile-time guards
// Note: clippy::must_use_candidate is intentionally NOT enabled here —
// it's a pedantic lint that generates excessive noise with `-D warnings`.

pub mod cli;
#[cfg(feature = "test-helpers")]
pub mod dispatch_test_support;
pub mod host;
pub use cli::config;
pub use cli::git;
pub use g5d_foundation::oci;
pub use g5d_foundation::terminal;
pub use g5d_isolation::isolation;

pub use g5d_foundation::workspace;

/// Route one parsed command through the CLI dispatcher.
pub use cli::dispatch as run;
