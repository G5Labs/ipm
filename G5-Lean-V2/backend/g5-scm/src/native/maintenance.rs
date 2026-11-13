use super::{FailureEffect, FailureKind, NativeGit, NativeGitError};
use crate::GitAuth;
use std::path::Path;

/// Evidence from one object-store maintenance attempt. Scheduling, thresholds
/// and serialization with other operations belong to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectMaintenance {
    NotMaterialized,
    BelowThreshold { loose_objects: u64 },
    Collected { loose_before: u64, loose_after: u64 },
}

impl NativeGit {
    /// Count and collect a materialized bare repository or checkout. This keeps
    /// Git's existing repository-configured GC/retention semantics; it does not
    /// change branch authority, select new refs, or add a retry policy.
    /// A Partial error confirms collection completed but its final count is
    /// unavailable. An Unknown error does not establish collection completion.
    pub async fn collect_garbage(
        &self,
        repository: &Path,
        loose_threshold: u64,
    ) -> Result<ObjectMaintenance, NativeGitError> {
        let result = async {
            let materialized = repository.join(".git").exists()
                || (repository.join("objects").is_dir() && repository.join("HEAD").exists());
            if !materialized {
                return Ok(ObjectMaintenance::NotMaterialized);
            }
            let loose_before = self.loose_object_count(repository).await?;
            if loose_before < loose_threshold {
                return Ok(ObjectMaintenance::BelowThreshold {
                    loose_objects: loose_before,
                });
            }
            let operation = "garbage collection";
            let mut command = self.command(Some(repository), operation);
            command.arg("gc");
            command
                .execute()
                .await?
                .success(operation, &GitAuth::None)?;
            let loose_after = self
                .loose_object_count(repository)
                .await
                .map_err(|mut error| {
                    error.operation = operation;
                    error.effect = FailureEffect::Partial;
                    error.detail = format!(
                    "Git garbage collection completed; final loose-object inspection failed: {}",
                    error.detail
                );
                    error
                })?;
            Ok(ObjectMaintenance::Collected {
                loose_before,
                loose_after,
            })
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&repository.to_string_lossy(), &GitAuth::None)
        })
    }

    async fn loose_object_count(&self, repository: &Path) -> Result<u64, NativeGitError> {
        let operation = "count loose objects";
        let mut command = self.command(Some(repository), operation);
        command.read_only().args(["count-objects", "-v"]);
        let output = command.execute().await?;
        if !output.completed_successfully() {
            return Err(output.failure(
                operation,
                &GitAuth::None,
                FailureKind::Exit,
                FailureEffect::Refused,
            ));
        }
        if !output.truncated {
            if let Some(count) = output
                .stdout_text()
                .lines()
                .find_map(|line| line.strip_prefix("count:"))
            {
                if let Ok(count) = count.trim().parse() {
                    return Ok(count);
                }
            }
        }
        Err(NativeGitError {
            operation,
            target: None,
            kind: FailureKind::InvalidOutput,
            effect: FailureEffect::Refused,
            detail: "Git did not return a valid loose-object count; inspect repository state before retrying".into(),
        })
    }
}
