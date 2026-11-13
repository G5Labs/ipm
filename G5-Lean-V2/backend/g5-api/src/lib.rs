//! g5-api — G5 API server library crate.
//!
//! Exposes the server's modules so that integration tests and the binary
//! entry-point can both link against the same code.

// Compile-time guards
// Note: clippy::must_use_candidate is intentionally NOT enabled here —
// it's a pedantic lint that generates excessive noise with `-D warnings`.

pub use g5_api_core::{
    auth, cloud, config, db, errors, grounding_projection, maintenance, middleware, models,
    services, stubs, ws,
};
pub use g5_api_routes::auto_merge;
pub use g5_api_routes::routes;
pub mod http_server;

use g5_contracts::tracing_compat::{LegacyTargetFamily, LegacyTargetMapping};

static LEGACY_TRACING_TARGET_MAPPINGS: &[LegacyTargetMapping] = &[
    // Tranche 2 moved every service into its context module and every
    // maintenance job into the context it serves. The historical
    // `g5_api::services::<name>` and `g5_api::maintenance::<job>` targets keep
    // selecting those modules in `RUST_LOG` and labelling their rendered events;
    // a longer prefix wins over the crate-level entries below. The roots are
    // not mapped: `g5_api_core::services` now physically contains the jobs,
    // so a broad `g5_api::services` directive expands only through these
    // entries, and the two composition roots emit their own events under the
    // logical targets `g5_api::services` and `g5_api::maintenance` directly.
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::agent_attachment_retention",
        physical_prefix: "g5_api_core::services::agents::agent_attachment_retention",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::agent_tasks",
        physical_prefix: "g5_api_core::services::agents::agent_tasks",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::approval_request_retention",
        physical_prefix: "g5_api_core::services::workspace::approval_request_retention",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::branch_integration_scheduler",
        physical_prefix: "g5_api_core::services::git::branch_integration_scheduler",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::branch_integration_worker",
        physical_prefix: "g5_api_core::services::git::branch_integration_worker",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::host_registry",
        physical_prefix: "g5_api_core::services::hosts::host_registry",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::invitation_gc",
        physical_prefix: "g5_api_core::services::identity::invitation_gc",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::platform_issue_outbox",
        physical_prefix: "g5_api_core::services::identity::platform_issue_outbox",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::remote_advisory_pinger",
        physical_prefix: "g5_api_core::services::git::remote_advisory_pinger",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::remote_sync_run_retention",
        physical_prefix: "g5_api_core::services::git::remote_sync_run_retention",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::repo_gc",
        physical_prefix: "g5_api_core::services::git::repo_gc",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::session_pruning",
        physical_prefix: "g5_api_core::services::identity::session_pruning",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::timeline_cleanup",
        physical_prefix: "g5_api_core::services::agents::timeline_cleanup",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::usage_aggregation",
        physical_prefix: "g5_api_core::services::identity::usage_aggregation",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::workspace_flush_retention",
        physical_prefix: "g5_api_core::services::git::workspace_flush_retention",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::workspace_lifecycle",
        physical_prefix: "g5_api_core::services::workspace::workspace_lifecycle",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::maintenance::workspace_pull_requests",
        physical_prefix: "g5_api_core::services::git::workspace_pull_requests",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::advance_consumer",
        physical_prefix: "g5_api_core::services::git::advance_consumer",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::agent_dispatch",
        physical_prefix: "g5_api_core::services::agents::agent_dispatch",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::api_key_service",
        physical_prefix: "g5_api_core::services::identity::api_key_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::approval_config_service",
        physical_prefix: "g5_api_core::services::workspace::approval_config_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::approval_definition_service",
        physical_prefix: "g5_api_core::services::workspace::approval_definition_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::approval_review_service",
        physical_prefix: "g5_api_core::services::agents::approval_review_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::attempt_snapshot",
        physical_prefix: "g5_api_core::services::agents::attempt_snapshot",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::auth_service",
        physical_prefix: "g5_api_core::services::identity::auth_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::branch_commit",
        physical_prefix: "g5_api_core::services::git::branch_commit",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::branch_integration_service",
        physical_prefix: "g5_api_core::services::git::branch_integration_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::branch_sync_runs_service",
        physical_prefix: "g5_api_core::services::git::branch_sync_runs_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::chat_service",
        physical_prefix: "g5_api_core::services::agents::chat_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::chat_session_file_service",
        physical_prefix: "g5_api_core::services::agents::chat_session_file_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::codex_auth",
        physical_prefix: "g5_api_core::services::agents::codex_auth",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::comment_service",
        physical_prefix: "g5_api_core::services::workspace::comment_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::config_service",
        physical_prefix: "g5_api_core::services::agents::config_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::coordinated_repo_publication",
        physical_prefix: "g5_api_core::services::git::coordinated_repo_publication",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::doc_reference_service",
        physical_prefix: "g5_api_core::services::workspace::doc_reference_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::doc_service",
        physical_prefix: "g5_api_core::services::workspace::doc_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::external_issues",
        physical_prefix: "g5_api_core::services::git::external_issues",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::git_conflict_resolution",
        physical_prefix: "g5_api_core::services::git::git_conflict_resolution",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::git_policy",
        physical_prefix: "g5_api_core::services::git::git_policy",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::git_test_barrier",
        physical_prefix: "g5_api_core::services::git::git_test_barrier",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::github_app",
        physical_prefix: "g5_api_core::services::git::github_app",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::github_app_auth",
        physical_prefix: "g5_api_core::services::identity::github_app_auth",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::github_app_binding",
        physical_prefix: "g5_api_core::services::git::github_app_binding",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::github_issues",
        physical_prefix: "g5_api_core::services::git::github_issues",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::github_user_grants",
        physical_prefix: "g5_api_core::services::git::github_user_grants",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::home_service",
        physical_prefix: "g5_api_core::services::workspace::home_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::host_control_state",
        physical_prefix: "g5_api_core::services::hosts::host_control_state",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::intent_log_service",
        physical_prefix: "g5_api_core::services::workspace::intent_log_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::issue_comments",
        physical_prefix: "g5_api_core::services::git::issue_comments",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_merge_service",
        physical_prefix: "g5_api_core::services::git::workspace_merge_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::notification_service",
        physical_prefix: "g5_api_core::services::identity::notification_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::org_service",
        physical_prefix: "g5_api_core::services::identity::org_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::permission_service",
        physical_prefix: "g5_api_core::services::identity::permission_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::platform_issue_github",
        physical_prefix: "g5_api_core::services::identity::platform_issue_github",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::platform_issue_intake",
        physical_prefix: "g5_api_core::services::identity::platform_issue_intake",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::preview_config",
        physical_prefix: "g5_api_core::services::workspace::preview_config",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::provider_account_service",
        physical_prefix: "g5_api_core::services::agents::provider_account_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::provider_quota_service",
        physical_prefix: "g5_api_core::services::identity::provider_quota_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::remote_integrations",
        physical_prefix: "g5_api_core::services::git::remote_integrations",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::remote_integrations",
        physical_prefix: "g5_api_core::services::git::remote_integrations",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::remote_tracking_merge",
        physical_prefix: "g5_api_core::services::git::remote_tracking_merge",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::review_edits",
        physical_prefix: "g5_api_core::services::workspace::review_edits",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::review_source_notifications",
        physical_prefix: "g5_api_core::services::workspace::review_source_notifications",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::runtime_label_resolver",
        physical_prefix: "g5_api_core::services::hosts::runtime_label_resolver",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::self_hosted_license_service",
        physical_prefix: "g5_api_core::services::identity::self_hosted_license_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::session_file_sha256",
        physical_prefix: "g5_api_core::services::agents::session_file_sha256",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::source_control_change_graph_service",
        physical_prefix: "g5_api_core::services::git::source_control_change_graph_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::spend_service",
        physical_prefix: "g5_api_core::services::identity::spend_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::task_lifecycle",
        physical_prefix: "g5_api_core::services::agents::task_lifecycle",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::task_service",
        physical_prefix: "g5_api_core::services::agents::task_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::unified_diff",
        physical_prefix: "g5_api_core::services::git::unified_diff",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::user_service",
        physical_prefix: "g5_api_core::services::identity::user_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workbench_config",
        physical_prefix: "g5_api_core::services::agents::workbench_config",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::worker_host_service",
        physical_prefix: "g5_api_core::services::hosts::worker_host_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_activity",
        physical_prefix: "g5_api_core::services::workspace::workspace_activity",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_flush_service",
        physical_prefix: "g5_api_core::services::git::workspace_flush_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_fork_service",
        physical_prefix: "g5_api_core::services::workspace::workspace_fork_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_incarnation",
        physical_prefix: "g5_api_core::services::identity::workspace_incarnation",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_messages",
        physical_prefix: "g5_api_core::services::agents::workspace_messages",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_mutation_policy",
        physical_prefix: "g5_api_core::services::identity::workspace_mutation_policy",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_ops",
        physical_prefix: "g5_api_core::services::git::workspace_ops",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_pull_request_checks",
        physical_prefix: "g5_api_core::services::git::workspace_pull_request_checks",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_pull_request_create",
        physical_prefix: "g5_api_core::services::git::workspace_pull_request_create",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_pull_request_provider_lifecycle",
        physical_prefix: "g5_api_core::services::git::workspace_pull_request_provider_lifecycle",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_pull_request_read",
        physical_prefix: "g5_api_core::services::git::workspace_pull_request_read",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_reader",
        physical_prefix: "g5_api_core::services::git::workspace_reader",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_repositories",
        physical_prefix: "g5_api_core::services::git::workspace_repositories",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_review_service",
        physical_prefix: "g5_api_core::services::workspace::workspace_review_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_runtime",
        physical_prefix: "g5_api_core::services::workspace::workspace_runtime",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_service",
        physical_prefix: "g5_api_core::services::workspace::workspace_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_vm_release",
        physical_prefix: "g5_api_core::services::hosts::workspace_vm_release",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api::services::workspace_vm_service",
        physical_prefix: "g5_api_core::services::workspace::workspace_vm_service",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api",
        physical_prefix: "g5_api_foundation",
    },
    // The former `g5_api_contracts` targets live under the foundation crate's
    // `contracts` module; this entry keeps `g5_api::<module>` filters and the
    // rendered logical prefix exact for them.
    LegacyTargetMapping {
        legacy_prefix: "g5_api",
        physical_prefix: "g5_api_foundation::contracts",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api",
        physical_prefix: "g5_api_data",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api",
        physical_prefix: "g5_api_core",
    },
    LegacyTargetMapping {
        legacy_prefix: "g5_api",
        physical_prefix: "g5_api_routes",
    },
];

pub static LEGACY_TRACING_TARGET_FAMILIES: &[LegacyTargetFamily] = &[LegacyTargetFamily {
    legacy_root: "g5_api",
    mappings: LEGACY_TRACING_TARGET_MAPPINGS,
}];
