//! Retained end-to-end scenarios for the workspace-VM architecture.
//!
//! Per-task worker registration, polling, assignment, sticky-runtime and
//! preview-job scenarios were removed with the worker execution path. Runtime
//! qualification now lives in the workspace-VM API, host and Playwright
//! suites; this crate retains independent control-plane and repository tests.

/// Workspace WebSocket subscription contract.
pub mod browser_interaction;

/// Opt-in production block-root layout, user flows and Docker qualification.
pub mod block_workload;

/// Stale Cloud Build cancellation helper contract.
pub mod ci_stale_build_cancellation;

/// Cloud Build workspace-host image and lane isolation guards.
pub mod ci_e2e_lane_isolation;

/// Retained local `g5d` CLI operations.
pub mod cli_ops;

/// Provider-control state persistence independent of a running agent.
pub mod codex_control_frames;

/// Comment CRUD, reply threading, mention notifications and resolution.
pub mod comment_notifications;

/// Host boot qualification: cold, warm and new-child timings on the bound
/// workspace-VM host.
pub mod host_boot_timings;

/// Failed live-fork flush and post-restart recovery on the real workspace VM.
pub mod fork_recovery;

/// Verified receipt restore and Preview fencing after a real bound-host loss.
pub mod host_loss_recovery;

/// Ontology lifecycle through the retained workspace ontology route.
pub mod ontology_lifecycle;

/// R2 guest PTY reconnect, teardown, and lifecycle fencing.
pub mod r2_guest_lifecycle;

/// Two live chat sessions and a queued turn on one candidate VM.
pub mod shared_vm_chats;

/// Executable package-to-validation catalog for the workspace MVP.
pub mod value_loop_matrix;

/// Formal approvals through isolated workspace VM execution and durable history.
pub mod formal_agent_approvals;
