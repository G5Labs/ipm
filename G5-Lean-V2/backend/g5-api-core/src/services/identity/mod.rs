//! Identity: who may do what in an organization, and the leaves every
//! context reads (users, organizations, permissions, notifications, spend,
//! quota, licence, platform issues) with the maintenance jobs that serve them.
//!
//! The layering is pinned by `backend/g5-api/tests/service_context_layering.rs`.

pub mod api_key_service;
pub mod auth_service;
pub(crate) mod github_app_auth;
pub mod invitation_gc;
pub mod notification_service;
pub mod org_admin_settings;
pub mod org_service;
pub mod permission_service;
pub mod platform_issue_github;
pub mod platform_issue_intake;
pub mod platform_issue_outbox;
pub mod platform_operator;
pub mod provider_quota_service;
pub mod self_hosted_license_service;
pub mod session_pruning;
pub mod spend_service;
pub mod usage_aggregation;
pub mod user_service;
pub mod workspace_incarnation;
pub mod workspace_mutation_policy;

pub mod membership_activity;
pub mod workspace_names;
