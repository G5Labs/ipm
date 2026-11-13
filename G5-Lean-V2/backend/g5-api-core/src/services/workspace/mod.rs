//! Workspace: the workspace, its forks, VM lifecycle, documents,
//! reviews and approvals, with the maintenance jobs that serve them. Depends on
//! identity, git and hosts only; reaches agents through the traits it owns.
//!
//! The layering is pinned by `backend/g5-api/tests/service_context_layering.rs`.

pub mod approval_activity;
pub mod approval_config_service;
pub mod approval_definition_service;
pub mod approval_request_email;
pub mod approval_request_retention;
pub mod base_reverification;
pub mod comment_service;
pub mod doc_reference_service;
pub mod doc_service;
pub mod home_service;
pub mod intent_log_service;
pub mod personal_workspace;
pub mod preview_config;
pub mod preview_run;
pub mod question_set_service;
pub mod question_vote_service;
pub mod review_edits;
pub mod review_source_notifications;
pub mod workspace_activity;
pub mod workspace_file_save;
pub mod workspace_fork_service;
pub mod workspace_kind;
pub mod workspace_lifecycle;
pub mod workspace_owner_email;
pub mod workspace_owner_limit;
pub mod workspace_review_queries;
pub mod workspace_review_service;
pub mod workspace_runtime;
pub mod workspace_service;
pub mod workspace_vm_service;
