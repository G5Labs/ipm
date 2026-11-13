//! G5 End-to-End Test Suite
//!
//! This crate exercises the full G5 system through its external
//! interfaces (HTTP API, WebSocket, CLI). It assumes the Docker Compose
//! test stack is running with all services healthy.
//!
//! Run deterministic E2E with: `./scripts/run-rust-e2e-lanes.sh`

// E2E scenarios are WIP scaffolding — suppress warnings that are expected
// during iterative development of integration tests.
#![allow(
    dead_code,
    unused_variables,
    unused_imports,
    unused_mut,
    clippy::too_many_arguments,
    clippy::must_use_candidate,
    clippy::useless_conversion,
    deprecated
)]

#[cfg(test)]
mod helpers;
#[cfg(test)]
mod scenarios;
