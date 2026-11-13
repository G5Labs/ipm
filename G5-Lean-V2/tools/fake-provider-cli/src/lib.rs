//! Fake provider CLIs for the G5-Lean provider runner (plan P3.1, design v6
//! §8.3 and §8.6).
//!
//! Three binaries are built from this crate:
//!
//! * `fake-claude` accepts the argv of the pinned Claude Code CLI in print
//!   mode, reads the prompt from stdin the way the real CLI does, and replays a
//!   scenario file as `stream-json` frames on stdout.
//! * `fake-codex` serves the pinned Codex App Server over stdio JSON-RPC and
//!   replays the same scenarios as notifications. G5 no longer launches
//!   `codex exec`, so the fake refuses it.
//! * `fake-g5-tools` is a scripted MCP stdio server; both fakes speak MCP to
//!   whatever server their configuration names, and this stub is what
//!   `cargo test` and `record.sh` point them at.
//!
//! Without a scenario file the fakes first run the prompt-selected workflows
//! ported from the retired Node fake sidecar (`workflows`), else replay a
//! built-in `hello` turn (`default_scenario`), which is what the deterministic
//! docker-ci lane runs.
//!
//! The scenario format, placeholders, environment variables, and the frame
//! shapes the fakes synthesise are documented in
//! `docs/lean-provider-cli-contract.md`.

pub mod argv;
pub mod claude;
pub mod codex;
pub mod codex_app_server;
pub mod default_scenario;
pub mod emit;
pub mod mcp;
pub mod mcp_config;
pub mod receipt;
pub mod replay;
pub mod resume_fact;
pub mod scenario;
pub mod session_files;
pub mod stub_server;
pub mod version;
pub mod workflows;
