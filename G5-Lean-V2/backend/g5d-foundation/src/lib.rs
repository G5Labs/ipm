//! Lower-level runtime utilities and production-neutral runtime contracts
//! shared by g5d composition crates.

pub mod command_execution;
pub mod disk;
pub mod oci;
pub mod ownership;
pub mod process_termination;
pub mod runtime_environment;
pub mod scm_credentials;
pub mod secret_redaction;
pub mod semlift_config;
pub mod terminal;
pub mod workspace;

pub use command_execution::{
    IsolationLevel, TaskCommandExecutor, TaskCommandRequest, TaskCommandResult,
};
pub use secret_redaction::{collect_secret_env_values, redact_secrets, AGENT_CARRIER_TOKEN_ENV};
