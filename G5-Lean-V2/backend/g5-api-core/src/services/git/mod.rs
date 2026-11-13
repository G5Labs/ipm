//! Git: refs, landings, merges, remote integrations, pull requests and
//! provider issues, with the maintenance jobs that serve them. Depends on
//! identity only.
//!
//! The layering is pinned by `backend/g5-api/tests/service_context_layering.rs`.

// LEAN-TODO(P1.5): remote-tracking consumers outlive the projection handoffs.
pub mod advance_consumer;
pub mod branch_commit;
pub mod branch_integration_scheduler;
pub mod branch_integration_service;
pub mod branch_integration_worker;
pub mod branch_sync_runs_service;
pub mod coordinated_repo_publication;
pub mod external_issues;
pub mod git_conflict_resolution;
pub mod git_policy;
#[cfg(feature = "test-helpers")]
pub mod git_test_barrier;
pub mod github_app;
pub mod github_app_binding;
pub mod github_issues;
pub mod github_user_grants;
pub mod issue_comments;
pub mod issue_creations;
pub mod issue_labels;
pub mod merge_override;
pub mod native_issue_query;
pub mod native_issues;
pub mod pull_request_adapters;
pub mod remote_advisory_pinger;
pub mod remote_integrations;
pub mod remote_sync_run_retention;
pub mod remote_tracking_merge;
pub mod repo_gc;
pub mod repo_store;
pub mod source_control_change_graph_service;
pub mod unified_diff;
pub mod workspace_audience;
pub mod workspace_flush_retention;
pub mod workspace_flush_service;
pub mod workspace_merge_service;
pub mod workspace_ops;
pub mod workspace_pull_request_auto_merge;
pub mod workspace_pull_request_create;
pub mod workspace_pull_request_freshness;
pub mod workspace_pull_request_provider_lifecycle;
pub mod workspace_pull_requests;
pub mod workspace_reader;
pub mod workspace_repositories;

pub mod workspace_revision_history;
