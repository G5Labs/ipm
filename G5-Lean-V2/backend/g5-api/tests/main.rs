//! Single integration-test target for `g5-api`.
//!
//! Every file in this directory used to be its own Cargo test target, so a
//! full test build linked ~140 copies of the g5-api dependency graph (issue
//! #585). This root compiles them all into one binary: one link, one copy of
//! `helpers` and `source_scan`, and a target directory that fits on a
//! workspace VM. nextest still runs each test in its own process, so per-test
//! isolation is unchanged.
//!
//! Test names carry the file name as a module prefix, so select one file's
//! tests with a nextest filter instead of `--test <file>`:
//! `cargo nextest run -p g5-api -E 'test(/^api_workspaces::/)'`.
//!
//! Adding a test file: create it here and add a `mod` line below.

mod chat_ontology_cards;
mod helpers;
mod source_scan;

mod api_agent_approval_evaluation;
mod api_agent_execution;
mod api_approval_activity;
mod api_approval_policy_locks;
mod api_auth;
mod api_automatic_continuation;
mod api_base_reverification;
mod api_change_graph_ontology;
mod api_chat_activity;
mod api_chat_attachment_bounds;
mod api_chat_explicit_permission_mode;
mod api_chat_session_file_bodies;
mod api_chat_session_title;
mod api_chat_stop_before_acceptance;
mod api_chat_turn_stop;
mod api_client_ip;
mod api_comments;
mod api_compute_running;
mod api_config_concurrency;
mod api_config_secret_hints;
mod api_dispatch_ownership;
mod api_explainer_templates;
mod api_explainers;
mod api_external_issues;
mod api_git_push_policy;
mod api_github_app_bindings;
mod api_home;
mod api_host_capacity;
mod api_host_source_preparation;
mod api_intent_log;
mod api_live_relay;
mod api_native_issue_labels;
mod api_native_issues;
mod api_native_requests;
mod api_ontology_baseline;
mod api_ontology_files;
mod api_org_admin_authority;
mod api_org_admin_settings;
mod api_org_approvals;
mod api_orgs;
mod api_owner_limits;
mod api_permissions_e2e;
mod api_personal_start_and_send;
mod api_personal_workspace_runtime;
mod api_personal_workspaces;
mod api_preview_start_and_preview;
mod api_provider_accounts;
mod api_pull_request_conversation;
mod api_pull_request_listing;
mod api_pull_request_reads;
mod api_question_metrics;
mod api_question_sets;
mod api_question_votes;
mod api_runtime_secret_admission;
mod api_self_hosted_license;
mod api_setup_e2e;
mod api_source_control_browser;
mod api_spend;
mod api_standard_features;
mod api_task_lifecycle;
mod api_terminal_activity;
mod api_token_management;
mod api_users;
mod api_view_migration;
mod api_workbench;
mod api_workbench_chat;
mod api_worker_host_runtime_labels;
mod api_worker_host_workspaces;
mod api_worker_hosts;
mod api_worker_hosts_cloud;
mod api_worker_hosts_stale;
mod api_workspace_approvals;
mod api_workspace_deletion_policy;
mod api_workspace_destructive_operations;
mod api_workspace_files_bounds;
mod api_workspace_files_live_tree;
mod api_workspace_flush_receipts;
mod api_workspace_fork_names;
mod api_workspace_fork_owner_access;
mod api_workspace_journey;
mod api_workspace_manifest;
mod api_workspace_member_authz;
mod api_workspace_messages;
mod api_workspace_phase;
mod api_workspace_pull_requests;
mod api_workspace_review_flow;
mod api_workspace_reviews;
mod api_workspace_tasks;
mod api_workspace_timeline;
mod api_workspace_transfer_credentials;
mod api_workspace_transition;
mod api_workspace_vms;
mod api_workspaces;
mod api_ws_revocation;
mod api_ws_tracing;
mod approval_config_projection_static;
mod approval_request_email;
mod approval_request_retention;
mod authoritative_head_writer_guard;
mod authoritative_workspace_reader_guard;
mod authority_branch_scope_migration;
mod authz_membership;
mod branch_commit_landing;
mod branch_integration_remote_pair_lease_migration;
mod core_table_writers_guard;
mod crate_private_fences;
mod crud_lifecycle_integration;
mod current_reader_contract;
mod deploy_worker_image_handoff;
mod email_delivery;
mod git_refs_authoritative;
mod github_request_budget;
mod host_auth_middleware_pg_integration;
mod host_validator_rls_regression;
mod local_issue_removal_guard;
mod maintenance_integration;
mod make_remote_adopt;
mod make_remote_outcomes;
mod migration_037_transfer_pause;
mod migration_045_owner_cleanup;
mod migration_056_base_remote_tracking_parity;
mod migration_057_session_file_bundles;
mod migration_058_segment_session_files;
mod migration_066_approval_phases;
mod migration_069_worker_host_boot_disk;
mod migration_071_workspace_execution_slots;
mod migration_081_base_idle_timeout;
mod migration_082_managed_host_capacity;
mod migration_083_org_admin_settings;
mod migration_084_claude_steering;
mod migration_086_min_free_slots;
mod migration_087_tenant_table_rls;
mod migration_088_approval_reviewer_policy;
mod migration_088_remove_jobs;
mod migration_089_workspace_approval_requests;
mod migration_090_retire_projections;
mod migration_091_remove_projection_storage;
mod migration_092_remove_unused_doc_metadata;
mod migration_097_start_command_sent_at;
mod migration_110_personal_workspaces;
mod migration_149_fixed_workspace_audiences;
mod migration_p4_1_runtime_schema;
mod migration_role_provisioning_concurrency;
mod migration_version_uniqueness;
mod platform_issue_delivery_recovery;
mod platform_issue_deployment_contract;
mod platform_issue_outbox_privileges;
mod platform_issue_reporting;
mod platform_operator;
mod pull_request_adapter_ownership;
mod rate_limit_windows;
mod recovery_status_rollout_contract;
mod remote_advisory_pinger;
mod remote_repo_tracking_integration;
mod remote_tracking_auth_boundary;
mod remote_tracking_background_loop;
mod remote_tracking_merge_active;
mod remote_tracking_merge_direct_clean;
mod remote_tracking_merge_runs_route;
mod remote_tracking_migration;
mod remote_tracking_multi_workspace;
mod remote_tracking_security;
mod remote_tracking_sync_runs_route;
mod repo_security_integration;
mod repo_storage_tenancy;
mod repo_storage_tenancy_guard;
mod route_auth_static;
mod route_sql_guard;
mod seed_codex_explainer;
mod seed_workspace_scripts;
mod selected_org_scope_audit;
mod service_context_layering;
mod shared_shape_reader_guard;
mod startup_fixture_keys;
mod tenant_scope_regressions;
mod test_harness_capacity_contract;
mod tracing_targets_moved_modules;
mod workspace_agent_parity;
mod workspace_branch_resolution_guard;
mod workspace_merge_override;
mod workspace_owner_email;
mod workspace_pull_request_auto_merge;
mod workspace_repository_model;
mod workspace_search_removal_guard;

mod projection_retirement;

mod migration_094_approval_artifacts;

mod api_formal_agent_approvals;

mod formal_review_policy_inventory;
mod system_scope_binding_guard;

mod ordinary_durable_commit_recovery;

mod api_member_activity;

mod api_credential_authority;
mod api_workspace_rename;
mod migration_105_refresh_token_digest;
mod migration_108_workspace_rename;
mod migration_113_remove_agent_review_comments;
mod migration_116_chat_activity_inspection;

mod migration_124_retired_storage;
mod migration_125_drop_base_auto_pull_columns;
mod migration_145_pull_request_numbers;
mod migration_146_host_disk_allocations;
mod migration_150_scoped_local_repo_urls;

mod admin_continuity;
mod migration_153_retire_org_owner;
