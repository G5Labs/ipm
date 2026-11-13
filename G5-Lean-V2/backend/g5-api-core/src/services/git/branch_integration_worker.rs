//! Maintenance entry point; integration state and lease ownership stay inside
//! the remote integration service.
pub use crate::services::remote_integrations::{
    run_branch_integration_worker, BranchIntegrationWorkerDeps,
};
