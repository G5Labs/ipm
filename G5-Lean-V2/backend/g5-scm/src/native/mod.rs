//! Complete native Git operations. Arguments, credentials and process ownership
//! are private; callers retain workspace admission and publication policy.

mod cancellation;
mod checkout_queries;
pub mod configuration;
mod maintenance;
mod materialization;
mod member;
pub use member::{
    MemberAdopt, MemberAdoptFailure, MemberAdoptResult, MemberDivergence, MemberGit,
    MemberGitEnvironment, MemberParentUpdate, MemberPush, MemberRebase, MemberRebaseFailure,
    MemberRebaseResult, MemberStatusEntry, MemberTrackingRefresh, MemberTrackingResult,
    MemberUpdateFailure,
};
#[cfg(unix)]
pub use member::{
    MemberCheckpoint, MemberCheckpointResult, MemberFileInventory, MemberFlushCapture,
    MemberFlushOutcome, MemberFlushPush, MemberFlushSnapshot, MemberObjectOwnership,
    MemberPathSync, MemberPathSyncOutcome,
};
mod observations;
mod process;
mod protocol;
mod remote;
mod restore;
mod worktree_snapshot;
pub use cancellation::NativeCancellation;
pub use maintenance::ObjectMaintenance;
pub use observations::{NativeObjectKind, ReviewSnapshotRead};
pub use process::NativeGitStream;
pub use protocol::{GitService, ReceivePackResult, ReceiveRefStatus, ReceiveRefUpdate};
pub use restore::{RestoreCheckout, RestoreCheckoutResult, RestoreSnapshot};
pub use worktree_snapshot::FileSelection;

use crate::GitAuth;
use g5_contracts::{credential_scrub::scrub, G5Error};
use process::{Invocation, Output};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

pub use crate::FailureEffect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    InvalidInput,
    Destination,
    /// The remote refused the supplied credential (or required one).
    Unauthorized,
    Spawn,
    Rejected,
    /// The remote refused because the credential lacks a provider permission
    /// the pushed content needs. Retrying with the same credential cannot
    /// succeed.
    ProviderPermission(ProviderPermission),
    Exit,
    Deadline,
    Cancelled,
    Io,
    InvalidOutput,
}

/// A provider permission whose absence makes the remote refuse a push.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPermission {
    /// A GitHub App installation or user token without `workflows`.
    GithubAppWorkflows,
    /// A GitHub OAuth or personal token without the `workflow` scope.
    GithubWorkflowScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeGitError {
    pub operation: &'static str,
    pub target: Option<String>,
    pub kind: FailureKind,
    pub effect: FailureEffect,
    pub detail: String,
}

impl std::fmt::Display for NativeGitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Git {} failed", self.operation)?;
        if let Some(target) = &self.target {
            write!(f, " for {target}")?;
        }
        write!(f, ": {} ({})", self.detail, self.effect_guidance())
    }
}
impl std::error::Error for NativeGitError {}
impl NativeGitError {
    /// Public outcome guidance without filesystem targets or subprocess diagnostics.
    /// Callers may append independently confirmed, authorized effect evidence.
    pub fn public_message(&self) -> String {
        format!("Git {} failed; {}", self.operation, self.effect_guidance())
    }

    fn effect_guidance(&self) -> &'static str {
        match self.effect {
            FailureEffect::Refused => "this invocation made no Git changes",
            FailureEffect::Partial => "Git confirmed an effect, but completion failed; inspect current state before retrying",
            FailureEffect::Unknown => "Git changes may have occurred; inspect current state before retrying",
        }
    }

    fn at_target(mut self, target: &str, auth: &GitAuth) -> Self {
        let secrets = match auth {
            GitAuth::Token(token) | GitAuth::ScmToken { token, .. } => vec![token.clone()],
            GitAuth::SshKey {
                private_key,
                passphrase,
            } => std::iter::once(private_key.clone())
                .chain(passphrase.clone())
                .collect(),
            _ => Vec::new(),
        };
        self.target = Some(process::redact(target, &secrets));
        self
    }
}
impl From<NativeGitError> for G5Error {
    fn from(error: NativeGitError) -> Self {
        if let FailureKind::ProviderPermission(permission) = error.kind {
            return provider_permission_refusal(permission, &error);
        }
        Self::GitError {
            message: error.to_string(),
        }
    }
}

/// An expected, client-visible refusal (`g5.git.workflow-files.refusal`): it names the
/// refused file and the missing permission, says nothing was published and
/// gives the recovery. The target and detail are already redacted.
fn provider_permission_refusal(permission: ProviderPermission, error: &NativeGitError) -> G5Error {
    // `at_target` formats pushes as "<url> refs/heads/<branch> (<update>)".
    let (repository, branch) = error
        .target
        .as_deref()
        .and_then(|target| target.split_once(" refs/heads/"))
        .map(|(url, rest)| {
            (
                url,
                rest.rsplit_once(" (").map_or(rest, |(branch, _)| branch),
            )
        })
        .unzip();
    let path = error
        .detail
        .split_once("workflow `")
        .and_then(|(_, rest)| rest.split_once('`'))
        .map(|(path, _)| path);
    let (permission_name, credential, missing, recovery) = match permission {
        ProviderPermission::GithubAppWorkflows => (
            "workflows",
            "github_app",
            "the G5 GitHub App credential lacks the Workflows permission",
            "To publish workflow changes, an organization admin must add the \"Workflows: Read and write\" repository permission to the G5 GitHub App, approve the updated permissions on the organization's installation, then choose Refresh from GitHub in the organization's GitHub App settings in G5. Retry afterwards.",
        ),
        ProviderPermission::GithubWorkflowScope => (
            "workflow",
            "personal",
            "the GitHub credential G5 used lacks the workflow scope",
            "Reconnect GitHub in Account settings with a credential that has the workflow scope, or publish through an organization GitHub App granted the Workflows permission. Retry afterwards.",
        ),
    };
    let file = path.map_or_else(
        || "files under .github/workflows".to_owned(),
        |path| format!("the workflow file {path}"),
    );
    let destination = match (branch, repository) {
        (Some(branch), Some(repository)) => format!(" branch {branch} to {repository}"),
        _ => String::new(),
    };
    G5Error::ConflictDetails {
        reason: format!(
            "GitHub refused to publish{destination} because its new commits change {file} and {missing}. Nothing was published to that branch. {recovery}"
        ),
        details: serde_json::json!({
            "code": "PROVIDER_PERMISSION_REQUIRED",
            "provider": "github",
            "permission": permission_name,
            "credential": credential,
            "path": path,
            "repository": repository,
            "remoteBranch": branch,
            "effect": "refused",
        }),
    }
}

#[derive(Debug, Clone)]
pub enum PushSource {
    Branch(String),
    Commit(String),
}

#[derive(Debug, Clone)]
pub enum PushUpdate {
    FastForward,
    /// `None` requires the destination to be absent. A concrete old OID must
    /// match exactly. The caller owns permission to replace non-fast-forward.
    Lease {
        expected: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct BranchPush {
    pub repository: PathBuf,
    pub remote: String,
    pub expected_url: String,
    pub source: PushSource,
    pub branch: String,
    pub update: PushUpdate,
}

#[derive(Debug, Clone)]
pub struct BranchDelete {
    pub repository: PathBuf,
    pub remote: String,
    pub expected_url: String,
    pub branch: String,
    pub expected: String,
}

struct BranchChange {
    repository: PathBuf,
    remote: Option<String>,
    expected_url: String,
    source: Option<PushSource>,
    branch: String,
    update: PushUpdate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Updated,
    Unchanged,
}

/// Selected by the trusted host composition boundary. No arbitrary environment
/// passthrough: repository controls and credentials are always operation-owned.
#[derive(Debug, Clone)]
pub struct HostGitEnvironment {
    pub search_path: OsString,
    pub home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub global_config: Option<PathBuf>,
    pub system_config_path: Option<PathBuf>,
    pub system_config: bool,
    pub transport: HostTransportEnvironment,
    pub ssh_agent_socket: Option<PathBuf>,
    pub library_path: Option<OsString>,
    pub dyld_library_path: Option<OsString>,
    pub ca_bundle: Option<PathBuf>,
    pub ca_directory: Option<PathBuf>,
}

/// Host-selected transport tools and proxy settings. Never inherit Git's
/// repository/config injection variables. Debug omits potentially secret values.
#[derive(Clone, Default)]
pub struct HostTransportEnvironment {
    pub http_proxy: Option<OsString>,
    pub https_proxy: Option<OsString>,
    pub all_proxy: Option<OsString>,
    pub no_proxy: Option<OsString>,
    pub ssh_command: Option<OsString>,
    pub ssh_program: Option<PathBuf>,
}
impl std::fmt::Debug for HostTransportEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostTransportEnvironment")
            .finish_non_exhaustive()
    }
}

/// Native execution in the selected API or CLI context. Resolve the binary once
/// per handle and retain the context's environment/configuration policy. The
/// timeout covers each native invocation, including output collection.
#[derive(Debug, Clone)]
pub struct NativeGit {
    binary: PathBuf,
    timeout: Duration,
    deadline: Option<std::time::Instant>,
    cancellation: NativeCancellation,
    context: Context,
    process_scope: Option<std::sync::Arc<std::fs::File>>,
    hidden_protocol_refs: Vec<String>,
    // Exercise the real bootstrap entrypoint in an isolated temporary namespace.
    // Shipped runners always require /work; no runtime override exists.
    #[cfg(test)]
    bootstrap_test_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
enum Context {
    Cli,
    WorkspaceCli(PathBuf),
    Api,
    Host(HostGitEnvironment),
    Bootstrap(HostGitEnvironment),
    Member(MemberGitEnvironment),
}

/// Locate Git on the owner's selected executable search path without spawning
/// a shell or consulting an ambient path. Invocation contexts remain separate.
pub fn locate_executable(search_path: &std::ffi::OsStr) -> Result<PathBuf, NativeGitError> {
    process::resolve_host_binary(search_path).map_err(|detail| NativeGitError {
        operation: "locate executable",
        target: None,
        kind: FailureKind::Spawn,
        effect: FailureEffect::Refused,
        detail,
    })
}

impl NativeGit {
    pub fn cli(timeout: Duration) -> Result<Self, NativeGitError> {
        if timeout.is_zero() {
            return Err(invalid("setup", "Git deadline must be positive"));
        }
        let binary = process::resolve_binary().map_err(|detail| NativeGitError {
            target: None,
            operation: "setup",
            kind: FailureKind::Spawn,
            effect: FailureEffect::Refused,
            detail,
        })?;
        Ok(Self {
            binary,
            timeout,
            deadline: None,
            cancellation: NativeCancellation::default(),
            context: Context::Cli,
            process_scope: None,
            hidden_protocol_refs: Vec::new(),
            #[cfg(test)]
            bootstrap_test_root: None,
        })
    }

    pub fn host(
        environment: HostGitEnvironment,
        timeout: Duration,
    ) -> Result<Self, NativeGitError> {
        if timeout.is_zero() {
            return Err(invalid("setup", "Git deadline must be positive"));
        }
        let binary = process::resolve_host_binary(&environment.search_path).map_err(|detail| {
            NativeGitError {
                target: None,
                operation: "setup",
                kind: FailureKind::Spawn,
                effect: FailureEffect::Refused,
                detail,
            }
        })?;
        Ok(Self {
            binary,
            timeout,
            deadline: None,
            cancellation: NativeCancellation::default(),
            context: Context::Host(environment),
            process_scope: None,
            hidden_protocol_refs: Vec::new(),
            #[cfg(test)]
            bootstrap_test_root: None,
        })
    }

    /// Platform bootstrap before customer code starts. Its executable, loader,
    /// transport helpers and CA roots are supplied by the read-only runtime.
    /// This capability is not for ordinary commands in a running workspace.
    pub fn trusted_bootstrap(timeout: Duration) -> Result<Self, NativeGitError> {
        let environment = HostGitEnvironment {
            search_path: "/opt/g5/bootstrap:/opt/g5/bin".into(),
            home: Some("/run/g5-bootstrap".into()),
            xdg_config_home: None,
            global_config: Some("/dev/null".into()),
            system_config_path: Some("/dev/null".into()),
            system_config: false,
            transport: HostTransportEnvironment::default(),
            ssh_agent_socket: None,
            library_path: None,
            dyld_library_path: None,
            ca_bundle: Some("/opt/g5/bootstrap/ca-certificates.crt".into()),
            ca_directory: None,
        };
        let mut runner = Self::host(environment.clone(), timeout)?;
        if runner.binary != Path::new("/opt/g5/bootstrap/git") {
            return Err(invalid("bootstrap setup", "Trusted runtime Git is absent"));
        }
        runner.context = Context::Bootstrap(environment);
        Ok(runner)
    }

    /// The API owner selects the executable; a bare name uses the API's fixed
    /// subprocess PATH, and relative paths are resolved once. Inherited
    /// environment variables are cleared, preserving repository/system config.
    pub fn api(binary: &Path, timeout: Duration) -> Result<Self, NativeGitError> {
        if timeout.is_zero() || binary.as_os_str().is_empty() {
            return Err(invalid(
                "setup",
                "Git executable and a positive deadline are required",
            ));
        }
        let binary = process::resolve_api_binary(binary).map_err(|detail| NativeGitError {
            operation: "setup",
            target: None,
            kind: FailureKind::Spawn,
            effect: FailureEffect::Refused,
            detail,
        })?;
        Ok(Self {
            binary,
            timeout,
            deadline: None,
            cancellation: NativeCancellation::default(),
            context: Context::Api,
            process_scope: None,
            hidden_protocol_refs: Vec::new(),
            #[cfg(test)]
            bootstrap_test_root: None,
        })
    }

    /// Use the member relay installed by the trusted guest runtime. This
    /// context never hands platform credentials to repository Git.
    pub fn workspace_cli(
        timeout: Duration,
        runtime_config: PathBuf,
    ) -> Result<Self, NativeGitError> {
        if !runtime_config.is_absolute() {
            return Err(invalid(
                "setup",
                "Runtime Git config must be an absolute owner-supplied path",
            ));
        }
        let mut runner = Self::cli(timeout)?;
        runner.context = Context::WorkspaceCli(runtime_config);
        Ok(runner)
    }

    fn runtime_config(&self) -> Option<&Path> {
        match &self.context {
            Context::WorkspaceCli(path) => Some(path),
            _ => None,
        }
    }

    /// Private refs owned by a disposable workspace, including server receipts.
    pub fn private_workspace_refs(branch: &str, workspace: uuid::Uuid) -> Vec<String> {
        vec![
            format!("refs/heads/{branch}"),
            format!("refs/g5/forks/{workspace}"),
            format!("refs/g5/flush/{workspace}/"),
            format!("refs/g5-tracking/{branch}/"),
        ]
    }

    /// Exact refs hidden from this authorized smart-HTTP audience. Callers
    /// derive names from durable resource identities; no Git options escape.
    pub fn with_hidden_protocol_refs(
        mut self,
        references: Vec<String>,
    ) -> Result<Self, NativeGitError> {
        if references.iter().any(|reference| {
            !reference.starts_with("refs/")
                || reference.chars().any(|c| {
                    c.is_control()
                        || c == ' '
                        || c == '^'
                        || c == '!'
                        || c == '~'
                        || c == ':'
                        || c == '?'
                        || c == '*'
                        || c == '['
                        || c == '\\'
                })
        }) {
            return Err(invalid(
                "git audience",
                "Expected confined ref names or prefixes",
            ));
        }
        self.hidden_protocol_refs = references;
        Ok(self)
    }

    /// Bound every invocation of one multi-step operation by a shared
    /// deadline, in addition to the per-invocation timeout.
    pub fn with_deadline(mut self, deadline: std::time::Instant) -> Self {
        self.deadline = Some(
            self.deadline
                .map_or(deadline, |current| current.min(deadline)),
        );
        self
    }

    pub fn with_cancellation(mut self, cancellation: NativeCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Trusted host preparation may place only its subprocesses in a bounded
    /// cgroup. The caller keeps cache locks and cancellation ownership outside it.
    #[cfg(target_os = "linux")]
    pub fn with_host_process_scope(mut self, group: &Path) -> Result<Self, NativeGitError> {
        use std::os::unix::fs::OpenOptionsExt;
        if !matches!(self.context, Context::Host(_)) || !group.starts_with("/sys/fs/cgroup") {
            return Err(invalid(
                "setup",
                "Process resource placement requires a host cgroup",
            ));
        }
        let file = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(group.join("cgroup.procs"))
            .map_err(|_| invalid("setup", "Cannot open host process scope"))?;
        self.process_scope = Some(std::sync::Arc::new(file));
        Ok(self)
    }
    #[cfg(not(target_os = "linux"))]
    pub fn with_host_process_scope(self, _: &Path) -> Result<Self, NativeGitError> {
        Err(invalid(
            "setup",
            "Process resource placement requires Linux",
        ))
    }

    fn command(&self, repository: Option<&Path>, operation: &'static str) -> Invocation {
        let timeout = self.deadline.map_or(self.timeout, |deadline| {
            self.timeout
                .min(deadline.saturating_duration_since(std::time::Instant::now()))
        });
        let mut command = Invocation::new(&self.binary, repository, operation, timeout);
        command.set_cancellation(self.cancellation.clone());
        command.set_process_scope(self.process_scope.clone());
        match &self.context {
            Context::Api => command.api_context(),
            Context::Host(environment) => {
                command.host_context(environment);
                explicit_git_dir(&mut command, repository);
            }
            Context::Bootstrap(environment) => {
                command.host_context(environment);
                command.config("credential.helper", "");
                command.config("core.fsmonitor", "false");
                command.config("core.attributesFile", "/dev/null");
                explicit_git_dir(&mut command, repository);
            }
            Context::Member(environment) => {
                command.member_context(environment);
                explicit_git_dir(&mut command, repository);
            }
            Context::WorkspaceCli(config) => command.runtime_config(config),
            Context::Cli => {}
        }
        command
    }

    /// Clone a checkout of `branch` (or the remote default) that tracks only
    /// that branch (`--single-branch`).
    pub async fn clone_checkout(
        &self,
        url: &str,
        destination: &Path,
        branch: Option<&str>,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        self.clone_worktree(url, destination, branch, true, auth)
            .await
    }

    /// Clone a checkout of `branch` that keeps tracking every remote branch,
    /// as a person's own working clone expects.
    pub async fn clone_full_checkout(
        &self,
        url: &str,
        destination: &Path,
        branch: Option<&str>,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        self.clone_worktree(url, destination, branch, false, auth)
            .await
    }

    async fn clone_worktree(
        &self,
        url: &str,
        destination: &Path,
        branch: Option<&str>,
        single_branch: bool,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let result = async {
            let operation = "clone checkout";
            validate_url(url, auth, operation)?;
            if let Some(branch) = branch {
                self.validate_branch(None, branch, operation).await?;
            }
            let transport = self.transport_destination(None, url, operation).await?;
            self.check_expansion(None, url, &transport, operation)
                .await?;
            let mut command = self.command(None, operation);
            if self.runtime_config().is_some() {
                command.member_rewrite(url, &transport);
            }
            self.authenticate(&mut command, &transport, auth)?;
            command.arg("clone");
            if let Some(branch) = branch {
                command.args(["--branch", branch]);
                if single_branch {
                    command.arg("--single-branch");
                }
            }
            let redirected = test_transport(url);
            command
                .arg("--")
                .arg(redirected.as_deref().unwrap_or(url))
                .arg(destination);
            command.execute().await?.success(operation, auth)?;
            if redirected.is_some() {
                let mut origin = self.command(Some(destination), operation);
                origin.args(["remote", "set-url", "origin", url]);
                origin.execute().await?.success(operation, auth)?;
            }
            Ok(())
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&format!("{url} -> {}", destination.display()), auth)
        })
    }

    /// Update only the named remote's URL. This is persistent member config,
    /// not global/system config; it does not mutate refs or fetch objects.
    pub async fn set_remote_url(
        &self,
        repository: &Path,
        remote: &str,
        url: &str,
    ) -> Result<(), NativeGitError> {
        let result = async {
            let operation = "set checkout remote";
            validate_remote(remote, operation)?;
            validate_url(url, &GitAuth::None, operation)?;
            let mut command = self.command(Some(repository), operation);
            command.args(["remote", "set-url", remote, url]);
            command.execute().await?.success(operation, &GitAuth::None)
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(
                &format!("{} remote {remote}", repository.display()),
                &GitAuth::None,
            )
        })
    }

    /// Make `remote` fetch `branch` into its tracking namespace, so a
    /// single-branch checkout can later switch to it. Idempotent: an existing
    /// wildcard or exact refspec is left as is. Writes only that checkout's
    /// remote configuration.
    pub async fn track_remote_branch(
        &self,
        repository: &Path,
        remote: &str,
        branch: &str,
    ) -> Result<(), NativeGitError> {
        let operation = "track remote branch";
        let result = async {
            validate_remote(remote, operation)?;
            self.validate_branch(Some(repository), branch, operation)
                .await?;
            let mut read = self.command(Some(repository), operation);
            read.args(["config", "--get-all"])
                .arg(format!("remote.{remote}.fetch"))
                .read_only();
            let output = read.execute().await?;
            if !output.completed_successfully()
                && output.status.and_then(|status| status.code()) != Some(1)
            {
                return Err(output.failure(
                    operation,
                    &GitAuth::None,
                    FailureKind::Exit,
                    FailureEffect::Refused,
                ));
            }
            let wildcard = format!("+refs/heads/*:refs/remotes/{remote}/*");
            let exact = format!("+refs/heads/{branch}:refs/remotes/{remote}/{branch}");
            if output
                .stdout_text()
                .lines()
                .any(|line| line == wildcard || line == exact)
            {
                return Ok(());
            }
            let mut add = self.command(Some(repository), operation);
            add.args(["remote", "set-branches", "--add", "--", remote, branch]);
            add.execute().await?.success(operation, &GitAuth::None)
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(
                &format!("{} remote {remote} branch {branch}", repository.display()),
                &GitAuth::None,
            )
        })
    }

    /// Fetch the named remote's configured refspecs and prune its tracking refs.
    /// The expected URL comes from the policy owner, not repository config.
    pub async fn fetch_checkout(
        &self,
        repository: &Path,
        remote: &str,
        expected_url: &str,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let result = async {
            let operation = "fetch checkout";
            validate_url(expected_url, auth, operation)?;
            let transport = self
                .check_remote(repository, remote, expected_url, false, operation)
                .await?;
            let mut command = self.command(Some(repository), operation);
            if self.runtime_config().is_some() {
                command.member_rewrite(expected_url, &transport);
            }
            self.authenticate(&mut command, &transport, auth)?;
            command.args(["fetch", "--prune", "--recurse-submodules=no", "--", remote]);
            command.execute().await?.success(operation, auth)
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(&format!("{expected_url} remote {remote}"), auth)
        })
    }

    pub async fn push_branch(
        &self,
        push: BranchPush,
        auth: &GitAuth,
    ) -> Result<PushOutcome, NativeGitError> {
        self.change_branch(
            BranchChange {
                repository: push.repository,
                remote: Some(push.remote),
                expected_url: push.expected_url,
                source: Some(push.source),
                branch: push.branch,
                update: push.update,
            },
            auth,
        )
        .await
    }

    /// Fast-forward one branch into an existing local dev mirror. No remote
    /// configuration is installed or changed in the source checkout.
    pub fn push_local_mirror_blocking(
        &self,
        repository: &Path,
        destination: &Path,
        branch: &str,
    ) -> Result<PushOutcome, NativeGitError> {
        let destination = if destination.is_absolute() {
            destination.to_path_buf()
        } else {
            repository.join(destination)
        };
        let destination = destination
            .canonicalize()
            .map_err(|_| invalid("push local mirror", "Local mirror must exist"))?;
        let destination = destination
            .to_str()
            .ok_or_else(|| invalid("push local mirror", "Local mirror path must be UTF-8"))?
            .to_owned();
        let push = BranchChange {
            repository: repository.to_path_buf(),
            remote: None,
            expected_url: destination,
            source: Some(PushSource::Branch(branch.to_owned())),
            branch: branch.to_owned(),
            update: PushUpdate::FastForward,
        };
        let runner = self.clone();
        run_blocking("push local mirror", FailureEffect::Unknown, async move {
            runner.change_branch(push, &GitAuth::None).await
        })
    }

    /// Remove one branch only while it still names the supplied old head.
    pub async fn delete_branch(
        &self,
        delete: BranchDelete,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        self.change_branch(
            BranchChange {
                repository: delete.repository,
                remote: Some(delete.remote),
                expected_url: delete.expected_url,
                source: None,
                branch: delete.branch,
                update: PushUpdate::Lease {
                    expected: Some(delete.expected),
                },
            },
            auth,
        )
        .await
        .map(|_| ())
    }

    async fn change_branch(
        &self,
        push: BranchChange,
        auth: &GitAuth,
    ) -> Result<PushOutcome, NativeGitError> {
        let result = async {
            let operation = if push.source.is_some() {
                "push branch"
            } else {
                "delete branch"
            };
            validate_url(&auth.transport_url(&push.expected_url), auth, operation)?;
            self.validate_branch(Some(&push.repository), &push.branch, operation)
                .await?;
            let source = match &push.source {
                Some(PushSource::Branch(branch)) => {
                    self.validate_branch(Some(&push.repository), branch, operation)
                        .await?;
                    format!("refs/heads/{branch}")
                }
                Some(PushSource::Commit(oid)) => {
                    validate_oid(oid, operation)?;
                    oid.clone()
                }
                None => String::new(),
            };
            if let PushUpdate::Lease {
                expected: Some(oid),
            } = &push.update
            {
                validate_oid(oid, operation)?;
            }
            let transport = if let Some(remote) = &push.remote {
                self.check_remote(
                    &push.repository,
                    remote,
                    &push.expected_url,
                    true,
                    operation,
                )
                .await?
            } else {
                let transport = self
                    .transport_destination(Some(&push.repository), &push.expected_url, operation)
                    .await?;
                self.check_expansion(
                    Some(&push.repository),
                    &push.expected_url,
                    &transport,
                    operation,
                )
                .await?;
                self.check_push_expansion(&push.repository, &push.expected_url, operation)
                    .await?;
                transport
            };
            let transport = if self.runtime_config().is_none() {
                auth.transport_url(&transport).into_owned()
            } else {
                transport
            };
            if self.runtime_config().is_none() && transport != push.expected_url {
                self.check_push_expansion(&push.repository, &transport, operation)
                    .await?;
            }
            let target = format!("refs/heads/{}", push.branch);
            let mut command = self.command(Some(&push.repository), operation);
            if self.runtime_config().is_some() {
                command.member_rewrite(&push.expected_url, &transport);
            }
            self.authenticate(&mut command, &transport, auth)?;
            command.args([
                "push",
                "--porcelain",
                "--no-follow-tags",
                "--recurse-submodules=no",
            ]);
            if let PushUpdate::Lease { expected } = &push.update {
                command.arg(format!(
                    "--force-with-lease={target}:{}",
                    expected.as_deref().unwrap_or("")
                ));
            }
            command
                .arg("--")
                .arg(if let Some(local) = test_transport(&transport) {
                    local
                } else if self.runtime_config().is_none() && transport != push.expected_url {
                    transport.clone()
                } else {
                    push.remote
                        .as_deref()
                        .unwrap_or(&push.expected_url)
                        .to_owned()
                })
                .arg(format!("{source}:{target}"));
            let output = command.execute_captured().await?;
            // A success exit alone is insufficient evidence of the requested ref.
            let status = output.stdout_text().split_inclusive('\n').find_map(|line| {
                if !line.ends_with('\n') {
                    return None;
                }
                let mut fields = line.split('\t');
                let flag = fields.next()?;
                let refs = fields.next()?;
                let summary = fields.next().unwrap_or("");
                (refs.rsplit_once(':')?.1 == target).then(|| (flag.to_owned(), summary.to_owned()))
            });
            let flag = status.as_ref().map(|(flag, _)| flag.as_str());
            let summary = status
                .as_ref()
                .map(|(_, summary)| summary.as_str())
                .unwrap_or("");
            match flag {
                Some("=") if output.completed_successfully() => Ok(PushOutcome::Unchanged),
                Some(" " | "+" | "*" | "-") if output.completed_successfully() => {
                    Ok(PushOutcome::Updated)
                }
                Some("!") => {
                    // Both forms leave the destination unchanged, but only a
                    // moved/stale destination is a ref conflict the caller can
                    // resolve by refreshing; hook, policy or unpack refusals
                    // are not.
                    let (kind, effect) = match push_refusal(summary) {
                        Some(PushRefusal::RefConflict) => {
                            (FailureKind::Rejected, FailureEffect::Refused)
                        }
                        Some(PushRefusal::ProviderPermission(permission)) => (
                            FailureKind::ProviderPermission(permission),
                            FailureEffect::Refused,
                        ),
                        Some(PushRefusal::Policy) => (FailureKind::Exit, FailureEffect::Refused),
                        None => (FailureKind::Exit, FailureEffect::Unknown),
                    };
                    let mut error = output.failure(operation, auth, kind, effect);
                    error.detail = format!("{}; {}", output.redact(summary), error.detail);
                    Err(error)
                }
                Some("=" | " " | "+" | "*" | "-") => {
                    let mut error =
                        output.failure(operation, auth, FailureKind::Exit, FailureEffect::Partial);
                    error.detail =
                        format!("Git confirmed the requested remote ref; {}", error.detail);
                    Err(error)
                }
                _ if !output.completed_successfully() => {
                    let detail = output.detail(auth);
                    let rejected = detail.contains("HTTP 409") || detail.contains("error: 409");
                    Err(output.failure(
                        operation,
                        auth,
                        if rejected {
                            FailureKind::Rejected
                        } else {
                            FailureKind::Exit
                        },
                        if rejected {
                            FailureEffect::Refused
                        } else {
                            FailureEffect::Unknown
                        },
                    ))
                }
                _ => Err(NativeGitError {
                    target: None,
                    operation,
                    kind: FailureKind::Io,
                    effect: FailureEffect::Unknown,
                    detail: "Git did not confirm the requested destination ref".into(),
                }),
            }
        }
        .await;
        result.map_err(|error: NativeGitError| {
            error.at_target(
                &format!(
                    "{} refs/heads/{} ({:?})",
                    push.expected_url, push.branch, push.update
                ),
                auth,
            )
        })
    }

    /// Synchronous consumers execute exactly the same operation/supervisor.
    pub fn push_branch_blocking(
        &self,
        push: BranchPush,
        auth: &GitAuth,
    ) -> Result<PushOutcome, NativeGitError> {
        let runner = self.clone();
        let auth = auth.clone();
        run_blocking("push branch", FailureEffect::Unknown, async move {
            runner.push_branch(push, &auth).await
        })
    }

    pub fn clone_checkout_blocking(
        &self,
        url: &str,
        destination: &Path,
        branch: Option<&str>,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let (runner, url, destination, branch, auth) = (
            self.clone(),
            url.to_owned(),
            destination.to_path_buf(),
            branch.map(str::to_owned),
            auth.clone(),
        );
        run_blocking("clone checkout", FailureEffect::Unknown, async move {
            runner
                .clone_checkout(&url, &destination, branch.as_deref(), &auth)
                .await
        })
    }

    pub fn fetch_checkout_blocking(
        &self,
        repository: &Path,
        remote: &str,
        expected_url: &str,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let (runner, repository, remote, expected_url, auth) = (
            self.clone(),
            repository.to_path_buf(),
            remote.to_owned(),
            expected_url.to_owned(),
            auth.clone(),
        );
        run_blocking("fetch checkout", FailureEffect::Unknown, async move {
            runner
                .fetch_checkout(&repository, &remote, &expected_url, &auth)
                .await
        })
    }

    pub fn delete_branch_blocking(
        &self,
        delete: BranchDelete,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        let runner = self.clone();
        let auth = auth.clone();
        run_blocking("delete branch", FailureEffect::Unknown, async move {
            runner.delete_branch(delete, &auth).await
        })
    }

    async fn validate_branch(
        &self,
        repository: Option<&Path>,
        branch: &str,
        operation: &'static str,
    ) -> Result<(), NativeGitError> {
        let mut command = self.command(repository, operation);
        command
            .args(["check-ref-format", &format!("refs/heads/{branch}")])
            .read_only();
        let output = command.execute().await?;
        if output.completed_successfully() {
            Ok(())
        } else {
            Err(invalid(operation, "Invalid branch name"))
        }
    }

    fn authenticate(
        &self,
        command: &mut Invocation,
        url: &str,
        auth: &GitAuth,
    ) -> Result<(), NativeGitError> {
        if matches!(self.context, Context::Member(_)) {
            command.relay(url);
            return Ok(());
        }
        if self.runtime_config().is_some() {
            command.relay(url);
            Ok(())
        } else {
            command.authenticate(url, auth)
        }
    }

    async fn transport_destination(
        &self,
        repository: Option<&Path>,
        canonical: &str,
        operation: &'static str,
    ) -> Result<String, NativeGitError> {
        if self.runtime_config().is_none() {
            return Ok(canonical.to_owned());
        }
        // --global excludes repository-controlled mappings, while --includes
        // evaluates the runtime's existing per-member gitdir conditions.
        let mut command = self.command(repository, operation);
        command
            .args([
                "config",
                "--global",
                "--includes",
                "--null",
                "--get-regexp",
                "^url\\..*\\.insteadof$",
            ])
            .read_only();
        let output = command.execute().await?;
        let mut routes = std::collections::BTreeSet::new();
        collect_runtime_routes(&output, canonical, &mut routes);
        {
            // A fresh clone and its subsequent pinned fetch are outside the
            // original checkout's gitdir condition. Resolve member identity
            // from files named by the trusted runtime config; do not rebuild
            // runtime paths or consult repository-controlled mappings.
            let mut includes = self.command(None, operation);
            includes
                .args([
                    "config",
                    "--global",
                    "--includes",
                    "--null",
                    "--get-regexp",
                    "^includeif\\.gitdir:.*\\.path$",
                ])
                .read_only();
            let includes = includes.execute().await?;
            if includes.truncated {
                return Err(invalid(
                    operation,
                    "Runtime member configuration exceeds the read limit",
                ));
            }
            for entry in includes.stdout_text().split('\0') {
                let Some((_, path)) = entry.split_once('\n') else {
                    continue;
                };
                if !Path::new(path).is_absolute() {
                    return Err(invalid(
                        operation,
                        "Runtime member config must use owner-supplied absolute paths",
                    ));
                }
                let mut member = self.command(None, operation);
                member
                    .args([
                        "config",
                        "--file",
                        path,
                        "--includes",
                        "--null",
                        "--get-regexp",
                        "^url\\..*\\.insteadof$",
                    ])
                    .read_only();
                let output = member.execute().await?;
                if !output.completed_successfully() || output.truncated {
                    return Err(invalid(
                        operation,
                        "Cannot read the runtime member relay configuration",
                    ));
                }
                collect_runtime_routes(&output, canonical, &mut routes);
            }
        }
        if !output.truncated && routes.len() == 1 {
            let route = routes.first().expect("one runtime route");
            if let Ok(url) = url::Url::parse(route) {
                if url.scheme() == "http"
                    && url.host_str() == Some("127.0.0.1")
                    && url.port().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
                {
                    return Ok(route.to_owned());
                }
            }
        }
        Err(NativeGitError { target: None, operation, kind: FailureKind::Destination, effect: FailureEffect::Refused,
            detail: "No unique runtime-owned relay is configured for this member; inspect the workspace runtime before retrying".into() })
    }

    async fn check_expansion(
        &self,
        repository: Option<&Path>,
        url: &str,
        expected: &str,
        operation: &'static str,
    ) -> Result<(), NativeGitError> {
        let mut command = self.command(repository, operation);
        if self.runtime_config().is_some() {
            command.member_rewrite(url, expected);
        }
        command
            .args(["ls-remote", "--get-url", "--", url])
            .read_only();
        self.check_destination(command.execute().await?, expected, operation)
    }

    async fn check_push_expansion(
        &self,
        repository: &Path,
        url: &str,
        operation: &'static str,
    ) -> Result<(), NativeGitError> {
        self.check_expansion(Some(repository), url, url, operation)
            .await?;
        // ls-remote expands insteadOf, but a direct-URL push additionally uses
        // the longest matching pushInsteadOf prefix. Refuse a changed identity
        // without contacting it or altering the configured remote.
        let mut command = self.command(Some(repository), operation);
        command
            .args([
                "config",
                "--null",
                "--get-regexp",
                "^url\\..*\\.pushinsteadof$",
            ])
            .read_only();
        let output = command.execute().await?;
        if output.truncated
            || (!output.completed_successfully()
                && output.status.and_then(|status| status.code()) != Some(1))
        {
            return Err(destination_error(operation));
        }
        let mut longest: Option<(usize, bool)> = None;
        let text = output.stdout_text();
        for entry in text.split('\0').filter(|entry| !entry.is_empty()) {
            let Some((key, prefix)) = entry.split_once('\n') else {
                return Err(destination_error(operation));
            };
            let Some(base) = key
                .strip_prefix("url.")
                .and_then(|key| key.strip_suffix(".pushinsteadof"))
            else {
                return Err(destination_error(operation));
            };
            if url.starts_with(prefix) {
                match longest.as_mut() {
                    Some((length, changes_identity)) if *length == prefix.len() => {
                        // Git groups rules by URL base, so flat config order
                        // cannot settle equal-prefix ties. Refuse any unsafe
                        // candidate at the maximum length.
                        *changes_identity |= base != prefix;
                    }
                    Some((length, _)) if *length > prefix.len() => {}
                    _ => longest = Some((prefix.len(), base != prefix)),
                }
            }
        }
        if longest.is_some_and(|(_, changes_identity)| changes_identity) {
            return Err(destination_error(operation));
        }
        Ok(())
    }

    async fn check_remote(
        &self,
        repository: &Path,
        remote: &str,
        url: &str,
        push: bool,
        operation: &'static str,
    ) -> Result<String, NativeGitError> {
        validate_remote(remote, operation)?;
        let transport = self
            .transport_destination(Some(repository), url, operation)
            .await?;
        let mut command = self.command(Some(repository), operation);
        if self.runtime_config().is_some() {
            command.member_rewrite(url, &transport);
        }
        command.args(["remote", "get-url"]);
        if push {
            command.args(["--push", "--all"]);
        }
        command.arg(remote).read_only();
        self.check_destination(command.execute().await?, &transport, operation)?;
        Ok(transport)
    }

    fn check_destination(
        &self,
        output: Output,
        expected: &str,
        operation: &'static str,
    ) -> Result<(), NativeGitError> {
        if !output.completed_successfully()
            || output.truncated
            || output.stdout_text().trim_end() != expected
        {
            return Err(destination_error(operation));
        }
        Ok(())
    }
}

/// Fixture-only transport redirection for API integration tests that model a
/// provider repository with a local one. Validation, destination checks and
/// credential binding still use the canonical URL; only the final transport
/// argument changes. Compiled out of production builds.
#[cfg(feature = "test-helpers")]
pub fn redirect_transport_for_tests(canonical: &str, local: &Path) {
    test_transports()
        .lock()
        .unwrap()
        .insert(canonical.to_owned(), format!("file://{}", local.display()));
}

/// Where [`emulate_github_workflow_permission_for_tests`] reads the
/// permissions of the token "presented" to a fixture remote. Tests write the
/// most recently minted installation-token permissions here.
#[cfg(any(test, feature = "test-helpers"))]
pub const TEST_TOKEN_PERMISSIONS_FILE: &str = "g5-test-token-permissions.json";

/// Fixture-only model of GitHub's workflow-file rule for a local bare
/// repository. A branch update whose new commits change `.github/workflows/`
/// is refused with GitHub's App wording unless [`TEST_TOKEN_PERMISSIONS_FILE`]
/// grants `"workflows": "write"`; other updates apply with receive-pack's
/// old-OID check. Implemented as a `proc-receive` hook, the only hook whose
/// refusal reason reaches the pusher verbatim as `[remote rejected] (...)`.
#[cfg(any(test, feature = "test-helpers"))]
pub fn emulate_github_workflow_permission_for_tests(bare: &Path) {
    const HOOK: &str = r#"#!/usr/bin/perl
use strict;
use warnings;
binmode STDIN;
binmode STDOUT;
$| = 1;
sub pkt_read {
    my $got = read(STDIN, my $len, 4);
    die "short pkt-line length\n" unless defined $got && $got == 4;
    return undef if $len eq '0000';
    my $size = hex($len) - 4;
    my $data = '';
    while (length($data) < $size) {
        my $n = read(STDIN, $data, $size - length($data), length($data));
        die "short pkt-line\n" unless $n;
    }
    $data =~ s/\n\z//;
    return $data;
}
sub pkt_write { my ($data) = @_; printf '%04x%s', length($data) + 5, "$data\n"; }
1 while defined pkt_read();
pkt_write('version=1');
print '0000';
my @commands;
while (defined(my $line = pkt_read())) { push @commands, $line; }
my $granted = 0;
if (open(my $fh, '<', 'g5-test-token-permissions.json')) {
    local $/;
    my $permissions = <$fh>;
    $granted = $permissions =~ /"workflows"\s*:\s*"write"/;
}
for my $command (@commands) {
    my ($old, $new, $ref) = split / /, $command;
    my @workflows = grep { m{^\.github/workflows/} }
        split /\n/, `git log --format= --name-only $new --not --all`;
    if (@workflows && !$granted) {
        pkt_write("ng $ref refusing to allow a GitHub App to create or update workflow `$workflows[0]` without `workflows` permission");
    } elsif (system('git', 'update-ref', $ref, $new, $old) == 0) {
        pkt_write("ok $ref");
    } else {
        pkt_write("ng $ref failed to update ref");
    }
}
print '0000';
"#;
    use std::os::unix::fs::PermissionsExt;
    let hook = bare.join("hooks/proc-receive");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, HOOK).unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(bare.join("config"))
        .unwrap()
        .write_all(b"[receive]\n\tprocReceiveRefs = refs/heads\n")
        .unwrap();
}

#[cfg(feature = "test-helpers")]
fn test_transports() -> &'static std::sync::Mutex<std::collections::BTreeMap<String, String>> {
    static MAP: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, String>>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(Default::default)
}

fn test_transport(url: &str) -> Option<String> {
    #[cfg(feature = "test-helpers")]
    {
        test_transports().lock().unwrap().get(url).cloned()
    }
    #[cfg(not(feature = "test-helpers"))]
    {
        let _ = url;
        None
    }
}

/// Name the repository's Git directory explicitly, even for a damaged
/// checkout, so Git never discovers an ancestor repository.
fn explicit_git_dir(command: &mut Invocation, repository: Option<&Path>) {
    if let Some(repository) = repository {
        command.args([
            "--git-dir",
            if repository.join(".git").exists() {
                ".git"
            } else {
                "."
            },
        ]);
    }
}

fn run_blocking<T: Send + 'static>(
    operation: &'static str,
    effect: FailureEffect,
    work: impl std::future::Future<Output = Result<T, NativeGitError>> + Send + 'static,
) -> Result<T, NativeGitError> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| invalid(operation, "Cannot start Git runtime"))?;
        runtime.block_on(work)
    })
    .join()
    .map_err(|_| NativeGitError {
        target: None,
        operation,
        kind: FailureKind::Io,
        effect,
        detail: "Git execution supervisor stopped unexpectedly".into(),
    })?
}

fn collect_runtime_routes(
    output: &Output,
    canonical: &str,
    routes: &mut std::collections::BTreeSet<String>,
) {
    if !output.completed_successfully() || output.truncated {
        return;
    }
    for entry in output.stdout_text().split('\0') {
        let Some((key, value)) = entry.split_once('\n') else {
            continue;
        };
        if value == canonical {
            if let Some(route) = key
                .strip_prefix("url.")
                .and_then(|key| key.strip_suffix(".insteadof"))
            {
                routes.insert(route.to_owned());
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum PushRefusal {
    /// The destination moved or failed its lease/fast-forward check.
    RefConflict,
    /// The credential lacks a permission the pushed content requires.
    ProviderPermission(ProviderPermission),
    /// The remote refused for another reason (hook, policy, unpack, checkout).
    Policy,
}

/// Classify a `git push --porcelain` refusal summary. Client-side `[rejected]`
/// is always a destination-state refusal (non-fast-forward, fetch first, stale
/// lease, already exists). A remote refusal is a ref conflict only when
/// receive-pack failed its own locked old-OID update.
fn push_refusal(summary: &str) -> Option<PushRefusal> {
    if summary.starts_with("[rejected]") {
        return Some(PushRefusal::RefConflict);
    }
    let reason = summary.strip_prefix("[remote rejected]")?;
    // GitHub refuses new commits that create or update `.github/workflows/*`
    // unless the token may write workflows; the wording names the token kind.
    if reason.contains("to create or update workflow") {
        if reason.contains("without `workflows` permission") {
            return Some(PushRefusal::ProviderPermission(
                ProviderPermission::GithubAppWorkflows,
            ));
        }
        if reason.contains("without `workflow` scope") {
            return Some(PushRefusal::ProviderPermission(
                ProviderPermission::GithubWorkflowScope,
            ));
        }
    }
    Some(
        if [
            "failed to update ref",
            "cannot lock ref",
            "atomic transaction failed",
        ]
        .iter()
        .any(|conflict| reason.contains(conflict))
        {
            PushRefusal::RefConflict
        } else {
            PushRefusal::Policy
        },
    )
}

/// In-process `git check-ref-format refs/heads/<branch>` for remote reads,
/// which need no repository and would otherwise spawn a process per read.
fn valid_branch_name(branch: &str) -> bool {
    !branch.is_empty()
        && !branch.starts_with(['/', '.', '-'])
        && !branch.ends_with(['/', '.'])
        && !branch.ends_with(".lock")
        && !branch.contains("..")
        && !branch.contains("//")
        && !branch.contains("@{")
        && branch != "@"
        && !branch.contains("/.")
        && !branch.split('/').any(|part| part.ends_with(".lock"))
        && !branch
            .chars()
            .any(|c| c.is_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
}

fn validate_remote(remote: &str, operation: &'static str) -> Result<(), NativeGitError> {
    if remote.is_empty() || remote.starts_with('-') || remote.contains(['\0', '\n', '\r']) {
        Err(invalid(operation, "Invalid remote name"))
    } else {
        Ok(())
    }
}
fn validate_oid(oid: &str, operation: &'static str) -> Result<(), NativeGitError> {
    if oid.len() != 40 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        Err(invalid(operation, "Expected a full commit OID"))
    } else {
        Ok(())
    }
}
fn validate_url(
    value: &str,
    auth: &GitAuth,
    operation: &'static str,
) -> Result<(), NativeGitError> {
    if value.contains(['\0', '\n', '\r']) {
        return Err(invalid(operation, "Invalid repository URL"));
    }
    if let GitAuth::ScmToken { provider, .. } = auth {
        return if provider.allows(value) {
            Ok(())
        } else {
            Err(invalid(operation, "SCM push destination is not authorized"))
        };
    }
    if matches!(auth, GitAuth::SshKey { .. } | GitAuth::None) {
        if let Some((host, path)) = value.split_once(':') {
            // Git's SCP syntax is a transport identity, not a URL rewrite.
            if !value.contains("://")
                && !host.contains('/')
                && !host.starts_with('-')
                && !host.is_empty()
                && !path.is_empty()
                && !path.starts_with(':')
                && !value.chars().any(char::is_whitespace)
            {
                return Ok(());
            }
        }
        if let Ok(url) = url::Url::parse(value) {
            if url.scheme() == "ssh"
                && url.host_str().is_some()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
            {
                return Ok(());
            }
        }
    }
    // Git treats a colon before the first slash as a transport separator;
    // otherwise a nonempty relative path is a valid local repository too.
    let local_path = Path::new(value).is_absolute()
        || (!value.is_empty()
            && !value.starts_with('-')
            && !value.split('/').next().unwrap_or("").contains(':'));
    if local_path && matches!(auth, GitAuth::None | GitAuth::SshKey { .. }) {
        return Ok(());
    }
    let url = url::Url::parse(value).map_err(|_| invalid(operation, "Invalid repository URL"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || !(matches!(url.scheme(), "http" | "https")
            || (url.scheme() == "file" && matches!(auth, GitAuth::None | GitAuth::SshKey { .. }))
            || (url.scheme() == "git" && url.host_str().is_some() && matches!(auth, GitAuth::None)))
    {
        return Err(invalid(operation, "Repository URL must not contain credentials and must use HTTP(S), or an unauthenticated local file"));
    }
    Ok(())
}
fn destination_error(operation: &'static str) -> NativeGitError {
    NativeGitError {
        target: None,
        operation,
        kind: FailureKind::Destination,
        effect: FailureEffect::Refused,
        detail: "Git destination differs from the requested repository; inspect remote URLs and URL rewrites before retrying".into(),
    }
}

fn invalid(operation: &'static str, detail: &str) -> NativeGitError {
    NativeGitError {
        target: None,
        operation,
        kind: FailureKind::InvalidInput,
        effect: FailureEffect::Refused,
        detail: scrub(detail),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod protocol_tests;

#[cfg(test)]
mod materialization_tests;
