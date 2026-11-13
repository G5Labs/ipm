//! Hosts: worker hosts, VM custody and runtime labels, with the host
//! registry job. Depends on identity and git only.
//!
//! The layering is pinned by `backend/g5-api/tests/service_context_layering.rs`.

pub mod host_capacity;
pub mod host_control_state;
pub mod host_provisioning;
pub mod host_registry;
pub mod live_hints;
pub mod runtime_label_resolver;
pub mod worker_host_service;
pub mod workspace_vm_release;

pub mod source_preparation;
