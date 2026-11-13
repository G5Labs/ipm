//! Optional preparation owned by the host. No VM root or provider credentials
//! enter this path; current read authority is checked again before publication.
use anyhow::{ensure, Context, Result};
use futures_util::{
    future::{BoxFuture, Shared},
    FutureExt,
};
use g5_contracts::types::worker_host::WORKER_HOST_SOURCE_PREPARATION_DISKS;
use g5_internal_api::host_ctl::HostSourcePreparation;
use g5d_foundation::workspace::WorkspaceCheckout;
use g5d_isolation::{
    microvm::{self, SourcePreparationScope},
    WorkloadRepoMember, WorkloadRepoScope,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(crate) struct SourcePreparation {
    job: Mutex<Option<Job>>,
    foreground: Arc<AtomicU32>,
}
struct Job {
    key: String,
    cancel: CancellationToken,
    task: Shared<BoxFuture<'static, ()>>,
}
pub(crate) struct Foreground(Arc<AtomicU32>);
impl Drop for Foreground {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
impl SourcePreparation {
    /// Cancellation is signalled before foreground Git acquires cache locks.
    /// The existing cache owner still waits for its child to exit before unlock.
    pub async fn foreground(&self) -> Foreground {
        self.foreground.fetch_add(1, Ordering::AcqRel);
        let guard = Foreground(self.foreground.clone());
        if let Some(job) = self.job.lock().await.as_ref() {
            job.cancel.cancel();
        }
        guard
    }
    pub async fn stop(&self) {
        let done = self.job.lock().await.as_ref().map(|job| {
            job.cancel.cancel();
            job.task.clone()
        });
        if let Some(done) = done {
            done.await;
        }
    }
    pub async fn schedule(
        &self,
        assignment: HostSourcePreparation,
        checkout: Arc<WorkspaceCheckout>,
        size: u32,
        slots: u32,
    ) {
        let mut state = self.job.lock().await;
        if self.foreground.load(Ordering::Acquire) != 0 {
            return;
        }
        let key = format!(
            "{}:{}:{:?}",
            assignment.image_ref,
            assignment.membership_digest,
            assignment
                .repositories
                .iter()
                .map(|r| (&r.repo_id, &r.branch_oid))
                .collect::<Vec<_>>()
        );
        if state
            .as_ref()
            .is_some_and(|job| job.key == key && !job.cancel.is_cancelled())
        {
            return;
        }
        let previous = state.take();
        if let Some(old) = previous.as_ref() {
            old.cancel.cancel();
        }
        let cancel = CancellationToken::new();
        let stopped = cancel.clone();
        let foreground = self.foreground.clone();
        let task = tokio::spawn(async move {
            // Serial cleanup belongs to the job, never the admission mutex.
            // A foreground Start can cancel the replacement immediately while
            // the previous job proves its helper processes have exited.
            if let Some(old) = previous {
                let _ = old.task.await;
            }
            if stopped.is_cancelled() || foreground.load(Ordering::Acquire) != 0 {
                stopped.cancel();
                return;
            }
            let result = prepare(assignment, checkout, size, slots, &stopped).await;
            if let Err(error) = result {
                // Never include Git transport diagnostics or the read token.
                tracing::info!(error_kind = %error.root_cause(), "optional repository preparation skipped");
                stopped.cancel(); // A later live assignment may retry.
            }
        }).map(|_| ()).boxed().shared();
        *state = Some(Job { key, cancel, task });
    }
}

impl Drop for SourcePreparation {
    fn drop(&mut self) {
        if let Some(job) = self.job.get_mut().as_ref() {
            job.cancel.cancel();
        }
    }
}

async fn prepare(
    assignment: HostSourcePreparation,
    checkout: Arc<WorkspaceCheckout>,
    size: u32,
    slots: u32,
    cancel: &CancellationToken,
) -> Result<()> {
    ensure!(
        !assignment.repositories.is_empty() && assignment.repositories.len() <= 64,
        "invalid seed member count"
    );
    ensure!(
        assignment.expires_at > chrono::Utc::now().timestamp() + 30,
        "source grant expired"
    );
    tokio::fs::create_dir_all(checkout.cache_root()).await?;
    microvm::check_workload_capacity(
        checkout.cache_root(),
        size,
        slots.saturating_add(WORKER_HOST_SOURCE_PREPARATION_DISKS),
    )
    .context("insufficient space for source staging and seed preparation")?;
    let processes = SourcePreparationScope::new(checkout.cache_root())
        .context("source resource limits unavailable")?;
    let staging = tempfile::Builder::new()
        .prefix(".seed-input-")
        .tempdir_in(checkout.cache_root())?;
    let scope = WorkloadRepoScope {
        org_id: assignment.org_id,
        repos: assignment
            .repositories
            .iter()
            .map(|repo| WorkloadRepoMember {
                repo_id: repo.repo_id,
                root_path: repo.root_path.clone(),
                git_url: repo.git_url.clone(),
            })
            .collect(),
    };
    let budget = Duration::from_secs(
        (assignment.expires_at - chrono::Utc::now().timestamp()).clamp(1, 600) as u64,
    );
    let work = async {
        // The required environment was prepared before registration. Resolution
        // uses that same immutable OCI cache; a changed image is not selected here.
        let template = g5d_foundation::oci::OciExtractor::from_env()
            .prepare_rootfs_template(
                &assignment.image_ref,
                &microvm::rootfs_state_dir().join("templates"),
            )
            .await?;
        // Cancellation on an absent control-plane assignment remains conservative.
        // A subsequent live assignment can reuse its still-current pristine seed
        // after fresh authorization, without exporting or formatting it again.
        if microvm::repository_template_is_current(
            template.clone(),
            size,
            scope.clone(),
            assignment
                .repositories
                .iter()
                .map(|r| (r.repo_id, r.branch_oid.clone()))
                .collect(),
        )
        .await?
        {
            for repo in &assignment.repositories {
                checkout
                    .refresh_source_in_scope(
                        &repo.git_url,
                        &assignment.read_token,
                        processes.path(),
                    )
                    .await
                    .map_err(|_| anyhow::anyhow!("source authorization no longer valid"))?;
            }
            ensure!(
                assignment.expires_at > chrono::Utc::now().timestamp(),
                "source grant expired before reuse"
            );
            ensure!(
                !cancel.is_cancelled(),
                "foreground startup cancelled preparation"
            );
            return Ok(());
        }
        let mut bundles = Vec::new();
        let mut remaining = u64::from(size) * 1024 * 1024;
        for repo in &assignment.repositories {
            let bundle = staging.path().join(format!("{}.bundle", repo.repo_id));
            checkout
                .export_seed_bundle(
                    &repo.git_url,
                    &assignment.read_token,
                    g5d_foundation::workspace::SeedBundleExport {
                        branch: &repo.branch,
                        commit: &repo.branch_oid,
                        output: bundle.clone(),
                        max_bytes: remaining,
                    },
                    processes.path(),
                )
                .await
                .map_err(|_| anyhow::anyhow!("source fetch or committed bundle export failed"))?;
            remaining = remaining
                .checked_sub(tokio::fs::metadata(&bundle).await?.len())
                .context("source bundle capacity exceeded")?;
            bundles.push(serde_json::json!({"repo_id":repo.repo_id,"bundle":bundle,"commit":repo.branch_oid}));
        }
        let manifest = staging.path().join("input.json");
        tokio::fs::write(
            &manifest,
            serde_json::to_vec(&serde_json::json!({"scope":scope,"repositories":bundles}))?,
        )
        .await?;
        microvm::prepare_workload_templates_in_scope(
            &template,
            size,
            slots,
            Some(&manifest),
            Some(processes.path()),
        )
        .await?;
        for repo in &assignment.repositories {
            checkout
                .refresh_source_in_scope(&repo.git_url, &assignment.read_token, processes.path())
                .await
                .map_err(|_| anyhow::anyhow!("source authorization no longer valid"))?;
        }
        ensure!(
            assignment.expires_at > chrono::Utc::now().timestamp(),
            "source grant expired before publication"
        );
        ensure!(
            !cancel.is_cancelled(),
            "foreground startup cancelled preparation"
        );
        microvm::publish_repository_template(
            template,
            size,
            scope,
            assignment
                .repositories
                .iter()
                .map(|r| (r.repo_id, r.branch_oid.clone()))
                .collect(),
        )
        .await?;
        tracing::info!(
            members = assignment.repositories.len(),
            "published repository seed after fresh source authorization"
        );
        Ok(())
    };
    // Re-check while fetches write the existing bare cache too. Space is an
    // admission budget, not a reservation against unrelated host writers.
    let pressure = async {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            microvm::check_workload_capacity(checkout.cache_root(), size, slots)?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    };
    let result = tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(anyhow::anyhow!("foreground startup cancelled preparation")),
        value = pressure => value.context("source preparation exhausted host headroom"),
        value = tokio::time::timeout(budget, work) => value.unwrap_or_else(|_| Err(anyhow::anyhow!("source preparation budget elapsed"))),
    };
    // Never remove staging while Git/formatter descendants can still write it.
    if let Err(error) = processes.finish().await {
        let retained: PathBuf = staging.keep();
        tracing::warn!(path=%retained.display(), "retaining source staging until process exit can be proven");
        return Err(error);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn foreground_cancels_replacement_without_waiting_for_old_cleanup() {
        let preparation = Arc::new(SourcePreparation::default());
        let old_cancel = CancellationToken::new();
        let (release, blocked) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let _ = blocked.await;
        })
        .map(|_| ())
        .boxed()
        .shared();
        *preparation.job.lock().await = Some(Job {
            key: "old".into(),
            cancel: old_cancel.clone(),
            task,
        });
        let stopping = {
            let preparation = preparation.clone();
            tokio::spawn(async move { preparation.stop().await })
        };
        tokio::time::timeout(Duration::from_secs(1), old_cancel.cancelled())
            .await
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let checkout = Arc::new(WorkspaceCheckout::new(
            g5d_foundation::workspace::RepoCache::new(temp.path().join("cache")),
            temp.path().join("workspaces"),
        ));
        let assignment = HostSourcePreparation {
            org_id: Uuid::new_v4(),
            host_id: Uuid::new_v4(),
            source_workspace_id: Uuid::new_v4(),
            image_ref: "unused".into(),
            membership_digest: "new".into(),
            repositories: vec![],
            expires_at: chrono::Utc::now().timestamp() + 60,
            read_token: "unused".into(),
        };
        tokio::time::timeout(
            Duration::from_secs(1),
            preparation.schedule(assignment, checkout, 1024, 1),
        )
        .await
        .unwrap();
        assert!(
            !stopping.is_finished(),
            "cleanup is deliberately still blocked"
        );
        let foreground = tokio::time::timeout(Duration::from_secs(1), preparation.foreground())
            .await
            .unwrap();
        assert!(preparation
            .job
            .lock()
            .await
            .as_ref()
            .unwrap()
            .cancel
            .is_cancelled());
        assert!(
            !temp.path().join("cache").exists(),
            "replacement cannot start before old cleanup"
        );
        release.send(()).unwrap();
        stopping.await.unwrap();
        preparation.stop().await;
        drop(foreground);
        assert_eq!(preparation.foreground.load(Ordering::Acquire), 0);
        assert!(
            !temp.path().join("cache").exists(),
            "cancelled replacement must not prepare sources"
        );
    }
}
