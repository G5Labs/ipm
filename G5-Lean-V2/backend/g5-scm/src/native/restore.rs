//! Restoration of an admitted fresh host or trusted-bootstrap checkout from Git facts.
use super::{
    invalid, observations::full_oid, FailureEffect, FailureKind, NativeGit, NativeGitError,
    NativeObjectKind,
};
use crate::GitAuth;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RestoreSnapshot {
    pub flush_id: Uuid,
    pub local_branch_oid: String,
    pub local_head_oid: String,
    pub snapshot_oid: String,
    /// None requests detached HEAD (including interrupted sequencers).
    pub head_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RestoreCheckout {
    pub repository: PathBuf,
    pub url: String,
    pub workspace_id: Uuid,
    pub repo_id: Uuid,
    pub branch: String,
    pub branch_oid: String,
    pub upstream_branch: Option<String>,
    pub snapshot: Option<RestoreSnapshot>,
}

#[derive(Debug)]
pub struct RestoreCheckoutResult {
    pub fetched_kib: u64,
    pub upstream_failure: Option<NativeGitError>,
}

impl NativeGit {
    /// Fetch local tracking/receipt refs and restore the supplied exact state.
    /// The caller guarantees a fresh root with no prior customer execution and
    /// owns receipt authority. Guest use requires the trusted-bootstrap context.
    /// This never merges a newer fetched tip into the requested local state.
    pub async fn restore_checkout(
        &self,
        request: &RestoreCheckout,
        auth: &GitAuth,
        upstream_timeout: Duration,
    ) -> Result<RestoreCheckoutResult, NativeGitError> {
        let operation = "restore checkout";
        let root = &request.repository;
        let mut completed = Vec::new();
        let result = async {
            super::validate_url(&request.url, auth, operation)?;
            self.validate_branch(Some(root), &request.branch, operation)
                .await?;
            if let Some(upstream) = &request.upstream_branch {
                self.validate_branch(Some(root), upstream, operation)
                    .await?;
            }
            if !full_oid(&request.branch_oid) || upstream_timeout.is_zero() {
                return Err(invalid(
                    operation,
                    "Restore requires a full commit OID and a positive parent-fetch deadline",
                ));
            }
            if let Some(snapshot) = &request.snapshot {
                if [
                    &snapshot.local_branch_oid,
                    &snapshot.local_head_oid,
                    &snapshot.snapshot_oid,
                ]
                .into_iter()
                .any(|oid| !full_oid(oid))
                {
                    return Err(invalid(
                        operation,
                        "Restore receipt requires full commit OIDs",
                    ));
                }
                if let Some(head_ref) = &snapshot.head_ref {
                    if !head_ref.starts_with("refs/heads/") {
                        return Err(invalid(operation, "Restore HEAD must name a local branch"));
                    }
                    self.check_full_ref(head_ref, operation).await?;
                }
            }
            let mut oids = vec![request.branch_oid.as_str()];
            if let Some(snapshot) = &request.snapshot {
                oids.extend([
                    snapshot.local_branch_oid.as_str(),
                    snapshot.local_head_oid.as_str(),
                    snapshot.snapshot_oid.as_str(),
                ]);
            }
            self.check_object_format(root, &oids, operation).await?;
            self.check_expansion(Some(root), &request.url, &request.url, operation)
                .await?;
            let before = self.object_store_kib(root).await?;
            let branch_ref = format!("refs/heads/{}", request.branch);
            let mut refspecs = vec![
                format!("+{branch_ref}:refs/g5/restore/current/{}", request.repo_id),
                format!("+{branch_ref}:refs/remotes/origin/{}", request.branch),
            ];
            refspecs.extend(g5_contracts::workspace_git::provider_fetch_refspecs(
                request.workspace_id,
            ));
            if let Some(snapshot) = &request.snapshot {
                for leaf in ["snapshot", "branch"] {
                    refspecs.push(format!(
                        "+refs/g5/flush/{}/{}/{leaf}:refs/g5/restore/{}/{}/{leaf}",
                        request.workspace_id, snapshot.flush_id, snapshot.flush_id, request.repo_id
                    ));
                }
            }
            self.restore_fetch(root, &request.url, &refspecs, auth, true)
                .await?;
            completed.push("restore tracking refs fetched");
            self.require_restore_commit(root, &request.branch_oid)
                .await?;
            let mut upstream_failure = None;
            if let Some(upstream) = request
                .upstream_branch
                .as_deref()
                .filter(|branch| *branch != request.branch)
            {
                let mut optional = self.clone();
                optional.timeout = optional.timeout.min(upstream_timeout);
                upstream_failure = optional
                    .restore_fetch(
                        root,
                        &request.url,
                        &[format!(
                            "+refs/heads/{upstream}:refs/remotes/origin/{upstream}"
                        )],
                        auth,
                        false,
                    )
                    .await
                    .err();
            }
            if let Some(snapshot) = &request.snapshot {
                for (leaf, expected) in [
                    ("snapshot", &snapshot.snapshot_oid),
                    ("branch", &snapshot.local_branch_oid),
                ] {
                    let reference = format!(
                        "refs/g5/restore/{}/{}/{leaf}",
                        snapshot.flush_id, request.repo_id
                    );
                    if self.ref_tip(root, &reference).await?.as_ref() != Some(expected) {
                        return Err(invalid(
                            operation,
                            "Restore receipt ref does not match immutable spec",
                        ));
                    }
                    self.require_restore_commit(root, expected).await?;
                }
                self.require_restore_commit(root, &snapshot.local_head_oid)
                    .await?;
            }
            let fetched_kib = self.object_store_kib(root).await?.saturating_sub(before);
            clear_sequencer_state(root).await?;
            completed.push("sequencer state cleared");
            let local_branch = request
                .snapshot
                .as_ref()
                .map_or(request.branch_oid.as_str(), |s| &s.local_branch_oid);
            self.restore_step(root, &["update-ref", &branch_ref, local_branch], None)
                .await?;
            completed.push("workspace branch restored");
            match &request.snapshot {
                None => {
                    self.restore_step(root, &["symbolic-ref", "HEAD", &branch_ref], None)
                        .await?;
                    self.restore_step(root, &["reset", "--hard", &request.branch_oid], None)
                        .await?;
                    self.restore_step(root, &["clean", "-ffd"], None).await?;
                }
                Some(snapshot) => {
                    if let Some(head_ref) = &snapshot.head_ref {
                        self.restore_step(
                            root,
                            &["update-ref", head_ref, &snapshot.local_head_oid],
                            None,
                        )
                        .await?;
                        self.restore_step(root, &["symbolic-ref", "HEAD", head_ref], None)
                            .await?;
                        self.restore_step(
                            root,
                            &["reset", "--hard", &snapshot.local_head_oid],
                            None,
                        )
                        .await?;
                    } else {
                        self.restore_step(
                            root,
                            &[
                                "checkout",
                                "--detach",
                                "--force",
                                &snapshot.local_head_oid,
                                "--",
                            ],
                            None,
                        )
                        .await?;
                    }
                    self.restore_step(root, &["clean", "-ffdx"], None).await?;
                    completed.push("HEAD and clean worktree restored");
                    if snapshot.snapshot_oid != snapshot.local_head_oid {
                        let mut diff = self.command(Some(root), operation);
                        diff.args([
                            "diff",
                            "--binary",
                            "--no-ext-diff",
                            "--no-textconv",
                            &snapshot.local_head_oid,
                            &snapshot.snapshot_oid,
                            "--",
                        ])
                        .read_only();
                        diff.protocol_output(); // Binary patch is data; never truncate it as diagnostics.
                        let patch = diff.execute().await?;
                        super::observations::require_success(&patch, operation)?;
                        self.restore_step(
                            root,
                            &["apply", "--binary", "--whitespace=nowarn", "-"],
                            Some(patch.into_protocol_bytes()),
                        )
                        .await?;
                    }
                }
            }
            Ok(RestoreCheckoutResult {
                fetched_kib,
                upstream_failure,
            })
        }
        .await;
        result.map_err(|mut error: NativeGitError| {
            if !completed.is_empty() {
                error.effect = FailureEffect::Partial;
                error.detail = format!(
                    "Completed {}; remaining restore failed: {}",
                    completed.join(", "),
                    error.detail
                );
            }
            error.at_target(
                &format!("{} refs/heads/{}", root.display(), request.branch),
                auth,
            )
        })
    }

    async fn restore_fetch(
        &self,
        root: &Path,
        url: &str,
        refspecs: &[String],
        auth: &GitAuth,
        include_bootstrap_head: bool,
    ) -> Result<(), NativeGitError> {
        let operation = "fetch restore objects";
        let mut command = self.command(Some(root), operation);
        self.authenticate(&mut command, url, auth)?;
        command.args(["fetch", "--no-tags", "--force", "--recurse-submodules=no"]);
        let mut negotiation_ref = None;
        // The required fetch leaves tracking refs for the optional upstream
        // fetch. Only required restoration may create this temporary ref, so
        // cleanup failure cannot be swallowed as an optional upstream failure.
        if include_bootstrap_head && matches!(self.context, super::Context::Bootstrap(_)) {
            // HTTP protocol v0 does not pass --negotiation-tip to fetch-pack.
            // A temporary ref makes the detached seed visible to every transport
            // without changing customer refs or excluding other local tips.
            let mut head = self.command(Some(root), operation);
            head.args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
                .read_only();
            let head = head.execute().await?;
            if !super::observations::negative_observation(&head, operation)? {
                super::observations::require_success(&head, operation)?;
                let oid = head.stdout_text();
                let oid = oid.trim();
                if !full_oid(oid) {
                    return Err(invalid(operation, "Invalid bootstrap negotiation commit"));
                }
                let reference = format!("refs/g5/restore/negotiation/{}", Uuid::new_v4());
                self.restore_step(
                    root,
                    &["update-ref", &reference, oid, &"0".repeat(oid.len())],
                    None,
                )
                .await?;
                negotiation_ref = Some((reference, oid.to_owned()));
            }
        }
        command.args(["--", url]);
        for refspec in refspecs {
            command.arg(refspec);
        }
        let fetched = match command.execute().await {
            Ok(output) => output.success(operation, auth),
            Err(error) => Err(error),
        };
        if let Some((reference, oid)) = negotiation_ref {
            // Cleanup also runs after ordinary fetch failures. A cancelled boot
            // never becomes Ready and its private disk is discarded.
            self.restore_step(root, &["update-ref", "-d", &reference, &oid], None)
                .await?;
        }
        fetched.map_err(|error| error.at_target(url, auth))
    }

    async fn require_restore_commit(&self, root: &Path, oid: &str) -> Result<(), NativeGitError> {
        if self.object_kind(root, oid).await? == Some(NativeObjectKind::Commit) {
            Ok(())
        } else {
            Err(invalid(
                "restore checkout",
                "Required exact restore commit is unavailable",
            ))
        }
    }

    async fn restore_step(
        &self,
        root: &Path,
        args: &[&str],
        input: Option<Vec<u8>>,
    ) -> Result<(), NativeGitError> {
        let operation = "restore local state";
        let mut command = self.command(Some(root), operation);
        for arg in args {
            command.arg(arg);
        }
        if let Some(input) = input {
            command.input(input);
        }
        command.execute().await?.success(operation, &GitAuth::None)
    }

    async fn object_store_kib(&self, root: &Path) -> Result<u64, NativeGitError> {
        let operation = "measure restore object store";
        let mut command = self.command(Some(root), operation);
        command.args(["count-objects", "-v"]).read_only();
        let output = command.execute().await?;
        super::observations::require_success(&output, operation)?;
        let mut sizes = [None, None];
        for line in output.stdout_text().lines() {
            if let Some((key, value)) = line.split_once(':') {
                match key {
                    "size" => sizes[0] = value.trim().parse::<u64>().ok(),
                    "size-pack" => sizes[1] = value.trim().parse::<u64>().ok(),
                    _ => {}
                }
            }
        }
        match sizes {
            [Some(loose), Some(packed)] if !output.truncated => Ok(loose.saturating_add(packed)),
            _ => Err(invalid(operation, "Git object-size report is invalid")),
        }
    }
}

async fn clear_sequencer_state(root: &Path) -> Result<(), NativeGitError> {
    let git = root.join(".git");
    for (name, directory) in [
        ("rebase-merge", true),
        ("rebase-apply", true),
        ("sequencer", true),
        ("MERGE_HEAD", false),
        ("MERGE_MSG", false),
        ("MERGE_MODE", false),
        ("CHERRY_PICK_HEAD", false),
        ("REVERT_HEAD", false),
        ("AUTO_MERGE", false),
    ] {
        let result = if directory {
            tokio::fs::remove_dir_all(git.join(name)).await
        } else {
            tokio::fs::remove_file(git.join(name)).await
        };
        if let Err(error) = result {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(NativeGitError {
                    operation: "clear restore sequencer",
                    target: None,
                    kind: FailureKind::Io,
                    effect: FailureEffect::Unknown,
                    detail: format!("Cannot clear {name}: {error}"),
                });
            }
        }
    }
    Ok(())
}
