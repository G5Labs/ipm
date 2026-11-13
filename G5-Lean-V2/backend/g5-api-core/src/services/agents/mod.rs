//! Agents: dispatch, chats, tasks, workbench and provider
//! configuration, with the maintenance jobs that serve them. Depends on every
//! earlier context.
//!
//! The layering is pinned by `backend/g5-api/tests/service_context_layering.rs`.

pub mod agent_attachment_retention;
pub mod agent_dispatch;
pub mod agent_steering;
pub mod agent_tasks;
pub mod approval_review_service;
pub mod attempt_follower;
pub mod attempt_snapshot;
pub mod chat_activity;
pub mod chat_ontology;
pub mod chat_service;
pub mod chat_session_file_service;
pub mod codex_auth;
pub mod config_service;
pub mod provider_account_service;
pub mod segment_map;
pub(in crate::services) mod session_file_sha256;
mod subscription_usage;
pub mod task_lifecycle;
pub mod task_service;
pub mod timeline_cleanup;
pub mod workbench_config;
pub mod workspace_messages;
