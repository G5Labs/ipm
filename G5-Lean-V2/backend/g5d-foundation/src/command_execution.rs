//! Production-neutral port for task-scoped command execution.

use std::path::Path;

use async_trait::async_trait;
use g5_contracts::error::G5Error;

/// Isolation strength reported by a concrete command executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IsolationLevel {
    /// Process-level only (weakest). No VM or container boundary.
    Process = 0,
    /// Container-level. Linux namespaces and cgroups.
    Container = 1,
    /// VM-level (strongest). Separate kernel, hardware-enforced.
    Vm = 2,
}

impl std::fmt::Display for IsolationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process => write!(f, "process"),
            Self::Container => write!(f, "container"),
            Self::Vm => write!(f, "vm"),
        }
    }
}

/// One task-scoped shell command and its resource/environment overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCommandRequest {
    pub command: String,
    pub extra_env: Vec<(String, String)>,
    pub cpus: u8,
    pub mem_mib: u32,
}

/// Implementation-neutral result of one task command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Port consumed by tooling and implemented by a concrete isolation context.
#[async_trait]
pub trait TaskCommandExecutor: Send + Sync {
    fn isolation_level(&self) -> IsolationLevel;
    fn host_workspace(&self) -> &Path;
    fn guest_workspace(&self) -> &str;
    async fn execute_command(
        &self,
        request: TaskCommandRequest,
    ) -> Result<TaskCommandResult, G5Error>;
}

#[cfg(test)]
mod tests {
    use super::IsolationLevel;

    #[test]
    fn isolation_levels_are_ordered_by_strength() {
        assert!(IsolationLevel::Process < IsolationLevel::Container);
        assert!(IsolationLevel::Container < IsolationLevel::Vm);
        assert_eq!(IsolationLevel::Vm.to_string(), "vm");
    }
}
