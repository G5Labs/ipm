//! Shared data transfer objects, domain identifiers, enumerations, and
//! context structures used across the entire G5 system.
//!
//! Re-exports all public types from submodules so that consumers can
//! import directly from `g5_contracts::types::*`.

pub mod agent_launch;
pub mod approval_review;
pub mod dto;
pub mod enums;
pub mod explainer_templates;
pub mod grounding;
pub mod ids;
pub mod native_question;
pub mod ontology_markup;
pub mod ontology_resolution;
pub mod provider_account;
pub mod question_set;
pub mod spec_context;
pub mod task_attempt;
pub mod task_context;
pub mod task_env;
pub mod worker_host;

pub use agent_launch::*;
pub use approval_review::*;
pub use dto::*;
pub use enums::*;
pub use explainer_templates::*;
pub use grounding::*;
pub use ids::*;
pub use ontology_markup::*;
pub use ontology_resolution::*;
pub use provider_account::*;
pub use spec_context::*;
pub use task_attempt::*;
pub use task_context::*;
pub use task_env::*;
pub use worker_host::*;

mod approval_artifact;
pub use approval_artifact::*;
mod approval_artifact_limits;

mod approval_execution;
pub use approval_execution::*;
mod approval_identity;
pub use approval_identity::*;
mod approval_policy;
pub use approval_policy::*;
