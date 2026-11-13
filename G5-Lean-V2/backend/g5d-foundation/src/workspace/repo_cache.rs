//! Warm bare-repo cache for workspace VM materialization.
//!
//! See `g5/g5d/workspace` ontology D4 / D12 for the contract: first touch
//! runs `git clone --bare`; subsequent touches run `git fetch --prune
//! --quiet origin`.
//!
//! Concurrency model (two complementary locks, mirroring the OCI cache in
//! PR #122):
//!
//! 1. **In-process**: a `tokio::sync::Mutex<HashMap<CacheKey, Arc<Mutex<()>>>>`
//!    deduplicates same-key calls within a single daemon process.
//! 2. **Cross-process**: a per-key `.<hex>.lock` directory adjacent to the
//!    final cache entry, claimed atomically via `std::fs::create_dir`.
//!    Losers poll with `tokio::time::sleep` for up to `LOCK_MAX_WAIT`,
//!    with stale-lock recovery if the holder crashed mid-operation.
//!    `ensure_warm` always claims this lock BEFORE deciding clone-vs-fetch,
//!    so cache-hit fetches and cache-miss clones are equally serialised
//!    against concurrent peer writes (codex round 3 of PR-rev7-6).
//!
//! Repo URL guard (opus47-of-opus46 review B2 + codex round 2 of PR-rev7-6):
//! every URL is checked against an allowlist (`https://`, `ssh://`,
//! `git@…`, `file://…`, absolute `/path`) and must NOT begin with `-`.
//! The `git clone` command additionally uses a `--` end-of-options
//! separator so a hostile URL cannot be reinterpreted by git as a flag.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::error::WorkspaceError;
use g5_scm::{
    native::{HostGitEnvironment, NativeCancellation, NativeGit},
    GitAuth,
};

/// Cross-process lock poll interval. Mirrors `oci::cache::LOCK_POLL_INTERVAL`.
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Maximum total wait on the cross-process cache lock before declaring it
/// stale and surfacing `WorkspaceError::CacheLockTimeout`. It must exceed
/// the holder's whole execution budget (see `CACHE_EXECUTION_BUDGET`), so a
/// cold clone of a large repository over a slow link is waited for rather
/// than failed.
const LOCK_MAX_WAIT: Duration = Duration::from_secs(1920);

/// Stale-lock recovery threshold. If the lock dir mtime is older than this,
/// the lock is considered abandoned and we attempt to take it over.
///
/// **Codex round 2 / opus47 B-NEW-3 of PR-rev7-6:** the prior 300s value
/// raced legitimate slow clones — a cold `git clone --bare` of a large
/// monorepo can exceed 5 minutes, and a concurrent waiter would then
/// classify the live holder's lock as stale, remove it, and stomp the
/// in-progress clone. Bumping the threshold to `LOCK_MAX_WAIT - 60s`
/// closes the window: a waiter will only attempt recovery when the
/// holder is already within 60s of timing out, by which point the
/// holder's own bounded wait is also about to surface
/// `CacheLockTimeout` rather than continuing to write a half-clone.
///
/// **Codex round 3 of PR-rev7-6:** the prior implementation also gated
/// stale-recovery on `!is_bare_repo(cache_dir)` so it would not recover
/// "during a clone". That gate was removed because cache state is no
/// longer used as a peer-liveness signal — both clones and fetches now
/// mutate `cache_dir` under the lock, and the threshold exceeds the
/// holder's bounded execution budget, so a live holder is never stale.
const LOCK_STALE_AGE: Duration = Duration::from_secs(1860);

// `const_assert!` style invariant: stale must always be strictly less than
// max-wait, with at least 60s of safety margin so a future maintainer
// flipping the constants in the wrong direction trips compilation rather
// than silently disabling recovery.
const _LOCK_STALE_VS_MAX_INVARIANT: () = {
    assert!(
        LOCK_STALE_AGE.as_secs() + 60 <= LOCK_MAX_WAIT.as_secs(),
        "LOCK_STALE_AGE must leave at least 60s of headroom under \
         LOCK_MAX_WAIT so stale-recovery cannot race a still-live holder \
         (codex round 2 / opus47 B-NEW-3 of PR-rev7-6)"
    );
};

/// Filesystem-safe cache key derived from a `repo_url`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey(String);

/// Exact committed seed contents and the admitted export size limit.
pub struct SeedBundleExport<'a> {
    pub branch: &'a str,
    pub commit: &'a str,
    pub output: PathBuf,
    pub max_bytes: u64,
}

impl CacheKey {
    /// Compute the cache key for a `repo_url`. See ontology D3 for the
    /// canonicalization rules.
    pub fn from_repo_url(repo_url: &str) -> Self {
        let canonical = canonicalize_repo_url(repo_url);
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let digest = hasher.finalize();
        let hex = digest.iter().map(|b| format!("{:02x}", b)).collect();
        CacheKey(hex)
    }

    /// Return the key as a borrowed string slice (a 64-hex sha256).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn canonicalize_repo_url(repo_url: &str) -> String {
    let trimmed = repo_url.trim();
    trimmed.strip_suffix('/').unwrap_or(trimmed).to_string()
}

/// Reject repo URLs that don't match the allowlist or that begin with `-`
/// (would otherwise be reparsed by git as a flag even with the `--`
/// separator on some legacy paths).
///
/// **Allowed URL shapes** (codex round 2 of PR-rev7-6: documenting the
/// trust model explicitly):
///
/// * `https://…` — public clone over HTTPS (production)
/// * `ssh://…` — SSH transport (production, when worker hosts hold an SSH key)
/// * `git@host:path` — scp-style SSH (production)
/// * `file://…` — local file URL (local-dev + integration tests cloning
///   from a temp bare repo)
/// * `/absolute/path` — bare local filesystem path (same use case as
///   `file://…`; some git versions and tools strip the `file://` prefix
///   so both forms must be accepted to avoid regressions across test
///   environments)
/// * `http://<loopback>…` — loopback hosts ONLY (`localhost`, `127.0.0.1`,
///   `[::1]`; userinfo `@` forms rejected). The local microVM dev flip
///   (workspace D17 / `g5/g5d/host` D16): the native macOS host
///   materializes from the dev stack's TLS-less smart-HTTP endpoint
///   `http://localhost:3000/git/:repoId`. Cleartext HTTP to any
///   non-loopback host stays rejected by default.
/// * `http://<allowlisted-host>…` — operator opt-in ONLY, via
///   `G5D_GIT_HTTP_ALLOW_HOSTS` (workspace D17 opt-in arm). `http_allow_hosts`
///   is the parsed list of exact `host[:port]` authorities; a URL matches
///   only when its authority equals an entry exactly (host case-insensitive,
///   explicit `:port` must match — no default-port aliasing, no wildcards),
///   and userinfo `@` forms are rejected before matching. Empty list ⇒ the
///   loopback arm above is the only `http://` shape accepted (default).
///
/// **Trust model:** `TaskAssignment.workspace_git_url` is operator-controlled
/// via the g5-api control plane — the host trusts that the URL was vetted
/// at task-creation time. This validation is **defense in depth** (closing
/// the git-CLI argument-injection class), not the primary permission
/// boundary. Operators who restrict task assignments to remote-only repos
/// can tighten the allowlist by deleting the `file://` / `/` arms.
pub fn validate_repo_url(
    repo_url: &str,
    http_allow_hosts: &[String],
) -> Result<(), WorkspaceError> {
    let trimmed = repo_url.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        return Err(WorkspaceError::RepoUrlRejected {
            repo_url: "[rejected repository URL]".into(),
        });
    }
    let allowed = trimmed.starts_with("https://")
        || trimmed.starts_with("ssh://")
        || trimmed.starts_with("git@")
        || trimmed.starts_with("file://")
        || trimmed.starts_with('/')
        || is_loopback_http_url(trimmed)
        || is_allowlisted_http_url(trimmed, http_allow_hosts);
    if !allowed {
        return Err(WorkspaceError::RepoUrlRejected {
            repo_url: "[rejected repository URL]".into(),
        });
    }
    Ok(())
}

/// Parse the `G5D_GIT_HTTP_ALLOW_HOSTS` raw value into the exact-match
/// `host[:port]` allowlist (workspace D17 opt-in arm). Comma-separated;
/// per-entry surrounding whitespace is trimmed and empty entries dropped, so
/// an absent/empty/whitespace-only value yields an empty list (loopback-only
/// default preserved byte-for-byte). No lowercasing or other normalization is
/// applied — `is_allowlisted_http_url` compares the host case-insensitively at
/// match time.
pub fn parse_http_allow_hosts(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// `http://` is acceptable for a non-loopback host ONLY when the operator
/// explicitly allowlisted its exact `host[:port]` authority via
/// `G5D_GIT_HTTP_ALLOW_HOSTS` (workspace D17 opt-in arm). The authority is
/// parsed with the same strictness as the loopback arm: any userinfo (`@`) is
/// rejected before matching so `http://g5-api@evil.example/…` cannot spoof the
/// host check. The whole authority is compared case-insensitively against each
/// allowlist entry — the host is thereby case-insensitive while a port, being
/// digits-only, matches exactly; a URL with an explicit port does not match a
/// port-less entry (no default-port aliasing) and there is no wildcard or
/// prefix matching.
fn is_allowlisted_http_url(trimmed: &str, http_allow_hosts: &[String]) -> bool {
    if http_allow_hosts.is_empty() {
        return false;
    }
    let Some(rest) = trimmed.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    http_allow_hosts
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(authority))
}

/// `http://` is acceptable ONLY for loopback hosts (workspace D17): the local
/// dev stack's smart-HTTP endpoint has no TLS, but cleartext git traffic must
/// never leave the machine. The authority is parsed strictly: any userinfo
/// (`@`) is rejected outright so `http://localhost@evil.example/…` cannot
/// spoof the host check, and only an optional `:port` may follow the host.
fn is_loopback_http_url(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    // `[::1]` needs bracket-aware port stripping; plain hosts split on ':'.
    let host = if let Some(after_bracket) = authority.strip_prefix('[') {
        match after_bracket.split_once(']') {
            Some((h, port)) if port.is_empty() || port.starts_with(':') => {
                return h == "::1";
            }
            _ => return false,
        }
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1"
}

/// Warm bare-repo cache rooted at a configurable directory.
///
/// One instance per supervisor process. Cloned freely via `Arc` for the
/// per-VM spawn future.
pub struct RepoCache {
    root: PathBuf,
    locks: Mutex<HashMap<CacheKey, Arc<Mutex<()>>>>,
    /// Optional live worker session-JWT handle for authenticating the
    /// smart-HTTP clone/fetch (ontology D20). `None` is the public-clone
    /// path used by `file://` test remotes and public HTTPS; `Some` injects
    /// invocation-scoped credentials read fresh under the cache lock
    /// so a heartbeat-rotated token is honoured.
    git_auth: Option<Arc<RwLock<String>>>,
    /// Operator opt-in allowlist of exact `host[:port]` authorities that may
    /// be cloned over cleartext `http://` even though they are not loopback
    /// (workspace ontology D17 opt-in arm). Empty by default — the loopback
    /// arm is then the only `http://` shape accepted, byte-identical to the
    /// pre-opt-in behavior. Populated once at host startup from
    /// `G5D_GIT_HTTP_ALLOW_HOSTS` and held as explicit configuration so
    /// `validate_repo_url` never reads the environment ambiently.
    http_allow_hosts: Vec<String>,
    git_environment: HostGitEnvironment,
}

impl RepoCache {
    /// Construct a new cache rooted at `root`. The directory is NOT created
    /// here; it is created lazily on the first `ensure_warm`. No git auth is
    /// configured — see [`RepoCache::with_git_auth`].
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            git_auth: None,
            locks: Mutex::new(HashMap::new()),
            http_allow_hosts: Vec::new(),
            git_environment: host_git_environment(),
        }
    }

    /// Set the operator opt-in cleartext-`http://` host allowlist (workspace
    /// ontology D17 opt-in arm). `hosts` is the already-parsed list of exact
    /// `host[:port]` authorities (see [`parse_http_allow_hosts`]); an empty
    /// list keeps the loopback-only default. Builder-style so the supervisor
    /// wiring stays a single expression.
    pub fn with_http_allow_hosts(mut self, hosts: Vec<String>) -> Self {
        self.http_allow_hosts = hosts;
        self
    }

    /// Attach a live worker session-token handle so the warm-cache
    /// clone/fetch authenticate against the g5-api smart-HTTP transport
    /// (ontology D20). The token is read fresh at each command build, so a
    /// heartbeat/register rotation is
    /// picked up without rebuilding the cache. Builder-style so the supervisor
    /// wiring stays a single expression.
    pub fn with_git_auth(mut self, token: Arc<RwLock<String>>) -> Self {
        self.git_auth = Some(token);
        self
    }

    /// Override the trusted host execution context, for composition and isolated tests.
    pub fn with_git_environment(mut self, environment: HostGitEnvironment) -> Self {
        self.git_environment = environment;
        self
    }

    pub(crate) fn native_git(&self) -> Result<NativeGit, WorkspaceError> {
        NativeGit::host(self.git_environment.clone(), CACHE_EXECUTION_BUDGET)
            .map_err(|error| WorkspaceError::GitExecution { source: error })
    }

    fn git_auth(&self, token_override: Option<&str>) -> GitAuth {
        let token = token_override.map(str::to_owned).or_else(|| {
            self.git_auth
                .as_ref()
                .map(|handle| handle.read().unwrap_or_else(|e| e.into_inner()).clone())
        });
        match token.filter(|token| !token.is_empty()) {
            Some(token) => GitAuth::Token(token),
            None => GitAuth::None,
        }
    }

    /// The root the cache writes under.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Compute the on-disk cache directory for a `repo_url` without I/O.
    pub fn cache_dir_for(&self, repo_url: &str) -> PathBuf {
        self.root.join(CacheKey::from_repo_url(repo_url).as_str())
    }

    /// Ensure the warm bare-repo for `repo_url` is present and refreshed.
    ///
    /// Returns the absolute path to the cache directory. See ontology D4
    /// for the deterministic sequence.
    ///
    /// **Cross-process lock discipline** (codex round 3 of PR-rev7-6):
    /// `ensure_warm` ALWAYS claims the cross-process `.<hex>.lock`
    /// directory BEFORE inspecting the cache or deciding between clone
    /// and fetch. The earlier version short-circuited on
    /// `is_bare_repo(cache_dir)` — clone-completeness (HEAD present)
    /// was misread as "no peer is writing", but a peer mid-`git fetch`
    /// also has HEAD present and is concurrently mutating
    /// `.git/refs/...lock` / `.git/objects/`. Two concurrent
    /// supervisors hitting that path then raced on the same packfile /
    /// ref-lock paths. With the lock taken first, clone-vs-fetch is
    /// decided under the lock and ref/packfile mutations are
    /// serialised across processes. The fast path (no peer writing)
    /// is one extra `create_dir` + `remove_dir_all` — negligible
    /// against the cost of `git fetch` itself.
    pub async fn ensure_warm(&self, repo_url: &str) -> Result<PathBuf, WorkspaceError> {
        self.ensure_warm_inner(repo_url, None, None, None).await
    }

    /// Refresh using an operation-scoped workspace-incarnation credential.
    /// This avoids replacing the host's shared worker-token handle while
    /// concurrent legacy task materializations are in flight.
    pub async fn ensure_warm_with_token(
        &self,
        repo_url: &str,
        token: &str,
    ) -> Result<PathBuf, WorkspaceError> {
        self.ensure_warm_inner(repo_url, Some(token), None, None)
            .await
    }

    pub async fn refresh_in_scope(
        &self,
        repo_url: &str,
        token: &str,
        process_scope: &Path,
    ) -> Result<(), WorkspaceError> {
        self.ensure_warm_inner(repo_url, Some(token), None, Some(process_scope))
            .await?;
        Ok(())
    }

    /// Fetch and export under the same in-process and cross-process cache locks.
    pub async fn export_seed_bundle(
        &self,
        repo_url: &str,
        token: &str,
        export: SeedBundleExport<'_>,
        process_scope: &Path,
    ) -> Result<(), WorkspaceError> {
        self.ensure_warm_inner(
            repo_url,
            Some(token),
            Some((
                export.branch.to_owned(),
                export.commit.to_owned(),
                export.output,
                export.max_bytes,
            )),
            Some(process_scope),
        )
        .await?;
        Ok(())
    }

    async fn ensure_warm_inner(
        &self,
        repo_url: &str,
        token_override: Option<&str>,
        seed_export: Option<(String, String, PathBuf, u64)>,
        process_scope: Option<&Path>,
    ) -> Result<PathBuf, WorkspaceError> {
        validate_repo_url(repo_url, &self.http_allow_hosts)?;
        let key = CacheKey::from_repo_url(repo_url);
        let cache_dir = self.root.join(key.as_str());

        // In-process dedup first — common case is one supervisor process
        // serving concurrent assignments for the same repo URL.
        let per_key = self.lock_for(&key).await;
        let guard = per_key.lock_owned().await;

        ensure_dir(&self.root).await?;
        let lock_dir = self.root.join(format!(".{}.lock", key.as_str()));

        // Always take the cross-process lock first. Decide clone-vs-fetch
        // under the lock so a peer process mid-write cannot race us on
        // refs/objects. See doc-comment above for the round-3 rationale.
        // The returned RAII guard releases the lock on EVERY exit from this
        // scope — including a caller-dropped future at any await point below
        // (D23 drop-safety): the host supervisor races materialization
        // against task cancellation, and a post-op release statement would
        // be skipped on drop, stranding the lock until LOCK_STALE_AGE.
        let lock_guard = self.claim_lock(&lock_dir).await?;

        // Snapshot after admission/locks. Local/SSH cache sources never receive
        // the HTTP incarnation token, matching their existing unauthenticated path.
        let auth = if repo_url.starts_with("http://") || repo_url.starts_with("https://") {
            self.git_auth(token_override)
        } else {
            GitAuth::None
        };
        let cancellation = NativeCancellation::default();
        let mut git = self.native_git()?.with_cancellation(cancellation.clone());
        if let Some(scope) = process_scope {
            git = git
                .with_host_process_scope(scope)
                .map_err(|source| WorkspaceError::GitExecution { source })?;
        }
        let repo_url = repo_url.to_owned();
        let (keep_alive, caller_dropped) = tokio::sync::oneshot::channel::<()>();
        // This is an in-memory owner task, not a retry worker. Both cache locks
        // outlive the awaiting caller until the shared runner proves cleanup.
        let task = tokio::spawn(async move {
            let was_complete = is_bare_repo(&cache_dir);
            let result = tokio::select! {
                result = tokio::time::timeout(CACHE_EXECUTION_BUDGET,
                    async {
                        refresh_under_lock(&git, &repo_url, &cache_dir, &auth).await?;
                        if let Some((branch, commit, output, max_bytes)) = &seed_export {
                            git.export_seed_bundle(&cache_dir, branch, commit, output, *max_bytes).await
                                .map_err(|source| WorkspaceError::GitExecution { source })?;
                        }
                        Ok(())
                    }) => {
                    result.unwrap_or_else(|_| Err(WorkspaceError::GitExecution {
                        source: cache_interruption(g5_scm::native::FailureKind::Deadline, &cache_dir),
                    }))
                }
                _ = caller_dropped => Err(WorkspaceError::GitExecution {
                    source: cache_interruption(g5_scm::native::FailureKind::Cancelled, &cache_dir),
                }),
            };
            cancellation.cancel();
            cancellation.wait_idle().await;
            // Probe only this trusted local cache, after the process owner has
            // proven exit and before discarding a failed clone. Git's diagnostic
            // text is not evidence of host capacity, nor can a denied credential
            // or invalid source request be converted into a capacity retry.
            let result = result.map_err(|error| cache_write_failure(error, &cache_dir));
            if result.is_err() && !was_complete {
                let _ = tokio::fs::remove_dir_all(&cache_dir).await;
            }
            drop(lock_guard);
            drop(guard);
            result.map(|()| cache_dir)
        });
        let result = task.await.map_err(|error| WorkspaceError::Io {
            path: self.root.clone(),
            source: std::io::Error::other(format!("Cache owner failed: {error}")),
        })?;
        drop(keep_alive);
        result
    }

    /// Wait until we own the cross-process lock at `lock_dir`. Returns
    /// `Ok(())` once acquired; errors with `CacheLockTimeout` after
    /// `LOCK_MAX_WAIT`. The caller is responsible for the matching
    /// `remove_dir_all(lock_dir)` to release.
    ///
    /// **Cache-state independence (codex round 3 of PR-rev7-6):** earlier
    /// versions of this function consulted `is_bare_repo(cache_dir)` to
    /// short-circuit out of the wait when a peer's clone had completed
    /// (HEAD present). That conflated two distinct states: "peer just
    /// finished cloning" and "peer is mid-fetch on a clone-complete
    /// repo". During a peer fetch HEAD is present the whole time, so the
    /// short-circuit let a second process skip the wait and race the
    /// fetch on ref-lock / packfile paths. The lock now always serialises
    /// access to `cache_dir`; the clone-vs-fetch decision happens in
    /// `ensure_warm` AFTER acquisition.
    ///
    /// **Concurrent-loser race (opus47 B-NEW-4 of PR-rev7-6):** when two
    /// processes simultaneously observe a stale lock, both can call
    /// `remove_dir(lock_dir)` and both can then `create_dir(lock_dir)`
    /// successfully — at which point both think they own the lock and
    /// would both proceed to wipe + re-clone `cache_dir`. The
    /// `mkdir`-as-lock primitive doesn't natively distinguish "I just
    /// created this lock" from "I see a stale lock created moments ago
    /// by a peer". The mitigation here is a two-phase commit:
    ///
    /// 1. After successful `create_dir`, write a `.owner` token file
    ///    inside the lock dir whose contents are a per-process random
    ///    nonce + PID.
    /// 2. Sleep briefly to let any peer's `remove_dir` + `create_dir`
    ///    race complete.
    /// 3. Re-read `.owner`. If the contents still match our token, we
    ///    own the lock. If they differ (or the file is missing), a
    ///    peer stomped us — back off and re-poll.
    ///
    /// This makes the claim atomic in the practical sense: any peer
    /// that races us into the recovery branch will either have its
    /// `.owner` overwritten by ours (we win) or overwrite our `.owner`
    /// (we lose, retry, eventually win once the peer releases).
    async fn claim_lock(&self, lock_dir: &Path) -> Result<LockReleaseGuard, WorkspaceError> {
        let lock_start = std::time::Instant::now();
        loop {
            // The claim must be cancellation-atomic (D23): create the lock
            // dir synchronously so no await point separates the on-disk
            // side effect from arming the release guard below. The earlier
            // spawn_blocking shape awaited the join handle in between; an
            // abort landing on that await left the dir created with the
            // guard never armed, stranding the lock until LOCK_STALE_AGE
            // (issue #1854). A single create_dir on the async thread is the
            // same negligible-cost budget as the guard's own synchronous
            // remove_dir_all.
            let create_dir = std::fs::create_dir(lock_dir);
            match create_dir {
                Ok(()) => {
                    // Arm the release guard IMMEDIATELY after the create_dir
                    // claim, before the owner-token confirmation awaits — a
                    // caller-dropped future at any later await point must
                    // still release the lock (D23 drop-safety; PR #385
                    // review round 3: a Stop/TTL cancel racing `materialize`
                    // previously stranded the lock until LOCK_STALE_AGE).
                    let mut guard = LockReleaseGuard::armed(lock_dir.to_path_buf());
                    // Two-phase commit: confirm no concurrent recoverer
                    // stomped our claim. See doc-comment above.
                    if claim_owner_token(lock_dir).await? {
                        return Ok(guard);
                    }
                    // A peer overwrote our token: the lock dir is THEIRS now
                    // — disarm so our guard does not remove it out from
                    // under them. Re-poll: the next create_dir will see
                    // AlreadyExists (the peer's lock_dir is still there)
                    // and we'll wait for the peer to release it under the
                    // normal poll loop.
                    guard.disarm();
                    tokio::time::sleep(LOCK_POLL_INTERVAL).await;
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if lock_start.elapsed() >= LOCK_MAX_WAIT {
                        return Err(WorkspaceError::CacheLockTimeout {
                            lock_dir: lock_dir.to_path_buf(),
                            waited_secs: LOCK_MAX_WAIT.as_secs(),
                        });
                    }
                    // Stale-lock recovery: if the lock dir's mtime is
                    // older than LOCK_STALE_AGE, the holder almost
                    // certainly crashed. The threshold exceeds the holder's
                    // bounded execution budget (CACHE_EXECUTION_BUDGET), so
                    // recovery cannot race a still-live holder
                    // (codex round 2 / opus47 B-NEW-3 of PR-rev7-6).
                    if let Ok(meta) = tokio::fs::metadata(lock_dir).await {
                        if let Ok(modified) = meta.modified() {
                            if let Ok(age) = modified.elapsed() {
                                if age >= LOCK_STALE_AGE {
                                    tracing::warn!(
                                        lock_dir = %lock_dir.display(),
                                        age_secs = age.as_secs(),
                                        "Stale workspace cache lock detected; \
                                         previous holder appears to have crashed \
                                         mid-operation. Removing + retrying."
                                    );
                                    // Best-effort recursive remove so a
                                    // dead holder's `.owner` token doesn't
                                    // block the recovery `mkdir`.
                                    let _ = tokio::fs::remove_dir_all(lock_dir).await;
                                    continue;
                                }
                            }
                        }
                    }
                    tokio::time::sleep(LOCK_POLL_INTERVAL).await;
                }
                Err(source) => {
                    return Err(WorkspaceError::Io {
                        path: lock_dir.to_path_buf(),
                        source,
                    });
                }
            }
        }
    }

    async fn lock_for(&self, key: &CacheKey) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        if let Some(existing) = locks.get(key) {
            return Arc::clone(existing);
        }
        let new_lock = Arc::new(Mutex::new(()));
        locks.insert(key.clone(), Arc::clone(&new_lock));
        new_lock
    }
}

/// True iff `dir` looks like the destination of a previous successful
/// `git clone --bare`. `HEAD` is the clone-complete sentinel (git writes
/// it at the end of clone). A directory that exists but lacks `HEAD` is
/// treated as a half-initialised cache and re-cloned.
fn is_bare_repo(dir: &Path) -> bool {
    dir.join("HEAD").is_file()
}

/// Length of the per-claim random nonce in bytes (lowered to a small fixed
/// size so the resulting hex token fits comfortably in one ext4/XFS block).
const OWNER_TOKEN_RANDOM_BYTES: usize = 16;

/// Write a per-process owner-token into `lock_dir/.owner`, sleep briefly to
/// let any racing recoverer commit their own claim, then re-read the token.
/// Returns `true` iff our token survived (we own the lock).
///
/// See [`RepoCache::claim_lock`] for the concurrent-loser race this guards
/// against. Opus47 B-NEW-4 of PR-rev7-6.
async fn claim_owner_token(lock_dir: &Path) -> Result<bool, WorkspaceError> {
    let token = write_owner_token(lock_dir).await?;
    // Brief settling delay so a peer racing through the recovery branch
    // has a chance to commit its own write before we re-read. 100ms is
    // > 2x the poll interval so any peer entering the loop after we did
    // will reach the `create_dir`/`write` pair before our re-read.
    tokio::time::sleep(Duration::from_millis(100)).await;
    owner_token_survived(lock_dir, &token).await
}

/// Write-phase of [`claim_owner_token`]: stamp a fresh per-process token into
/// `lock_dir/.owner` and return it.
async fn write_owner_token(lock_dir: &Path) -> Result<String, WorkspaceError> {
    let token = generate_owner_token();
    let owner_path = lock_dir.join(".owner");
    tokio::fs::write(&owner_path, token.as_bytes())
        .await
        .map_err(|source| WorkspaceError::Io {
            path: owner_path.clone(),
            source,
        })?;
    Ok(token)
}

/// Re-read phase of [`claim_owner_token`]: `true` iff our token survived the
/// settling window (we own the lock). A missing/unreadable token file means a
/// peer recovered the lock dir out from under us — not an error.
async fn owner_token_survived(lock_dir: &Path, token: &str) -> Result<bool, WorkspaceError> {
    let owner_path = lock_dir.join(".owner");
    let observed = match tokio::fs::read(&owner_path).await {
        Ok(b) => b,
        Err(_) => return Ok(false),
    };
    Ok(observed == token.as_bytes())
}

fn generate_owner_token() -> String {
    use sha2::Digest;
    // Compose a unique-enough nonce without pulling in a `rand` dep:
    //  * PID — distinguishes processes on the same host
    //  * monotonic nanoseconds since UNIX_EPOCH — distinguishes claims
    //    within a single process
    //  * the lock_dir's own path address (Pin-free pointer identity)
    //    isn't load-bearing for uniqueness but makes the value
    //    non-predictable across builds.
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut hasher = sha2::Sha256::new();
    hasher.update(pid.to_ne_bytes());
    hasher.update(nanos.to_ne_bytes());
    // Stir in a per-call counter so two claims back-to-back within the
    // same nanosecond tick still differ. AtomicUsize starts at 0 on
    // process start, which is fine — it's combined with the PID and
    // nanos above.
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    hasher.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_ne_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .take(OWNER_TOKEN_RANDOM_BYTES)
        .map(|b| format!("{:02x}", b))
        .collect()
}

async fn ensure_dir(dir: &Path) -> Result<(), WorkspaceError> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|source| WorkspaceError::Io {
            path: dir.to_path_buf(),
            source,
        })
}

/// RAII release for the cross-process `.<hex>.lock` dir (D23 drop-safety).
///
/// `Drop` removes the lock dir synchronously (`std::fs::remove_dir_all` of a
/// dir holding one token file — a trivial blocking op, safe on an async
/// thread), so the lock is released even when the owning future is dropped at
/// an await point (the host supervisor races `materialize` against task
/// cancellation). `disarm` is for the owner-token *lost* path: the lock dir
/// then belongs to the peer that stomped the claim and must not be removed.
#[derive(Debug)]
struct LockReleaseGuard {
    lock_dir: Option<PathBuf>,
}

impl LockReleaseGuard {
    fn armed(lock_dir: PathBuf) -> Self {
        Self {
            lock_dir: Some(lock_dir),
        }
    }

    fn disarm(&mut self) {
        self.lock_dir = None;
    }
}

impl Drop for LockReleaseGuard {
    fn drop(&mut self) {
        if let Some(lock_dir) = self.lock_dir.take() {
            if let Err(e) = std::fs::remove_dir_all(&lock_dir) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        lock_dir = %lock_dir.display(),
                        error = %e,
                        "Failed to release workspace cache lock on drop; a peer \
                         will recover it after LOCK_STALE_AGE"
                    );
                }
            }
        }
    }
}

// The complete effect budget (including existing recovery backoff) expires
// before a peer can classify our live cross-process lock as stale.
// Generous enough for a cold clone of a large repository over a slow link.
const CACHE_EXECUTION_BUDGET: Duration = Duration::from_secs(1800);
const _: () = assert!(CACHE_EXECUTION_BUDGET.as_secs() < LOCK_STALE_AGE.as_secs());

fn system_config_enabled(no_system: Option<&std::ffi::OsStr>) -> bool {
    no_system.is_none_or(|value| {
        matches!(
            value.to_string_lossy().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        )
    })
}

/// Capture the host transport/configuration policy shared by cache and restore.
pub fn host_git_environment() -> HostGitEnvironment {
    let path = |name| std::env::var_os(name).map(PathBuf::from);
    HostGitEnvironment {
        search_path: std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        home: path("HOME"),
        xdg_config_home: path("XDG_CONFIG_HOME"),
        global_config: path("GIT_CONFIG_GLOBAL"),
        system_config_path: path("GIT_CONFIG_SYSTEM"),
        transport: g5_scm::native::HostTransportEnvironment {
            // curl deliberately ignores uppercase HTTP_PROXY. Lowercase wins
            // for the variables whose uppercase forms it does support.
            http_proxy: std::env::var_os("http_proxy"),
            https_proxy: std::env::var_os("https_proxy")
                .or_else(|| std::env::var_os("HTTPS_PROXY")),
            all_proxy: std::env::var_os("all_proxy").or_else(|| std::env::var_os("ALL_PROXY")),
            no_proxy: std::env::var_os("no_proxy").or_else(|| std::env::var_os("NO_PROXY")),
            ssh_command: std::env::var_os("GIT_SSH_COMMAND"),
            ssh_program: path("GIT_SSH"),
        },
        system_config: system_config_enabled(std::env::var_os("GIT_CONFIG_NOSYSTEM").as_deref()),
        ssh_agent_socket: path("SSH_AUTH_SOCK"),
        library_path: std::env::var_os("LD_LIBRARY_PATH"),
        dyld_library_path: std::env::var_os("DYLD_LIBRARY_PATH"),
        ca_bundle: path("GIT_SSL_CAINFO"),
        ca_directory: path("SSL_CERT_DIR"),
    }
}

fn cache_interruption(
    kind: g5_scm::native::FailureKind,
    cache: &Path,
) -> g5_scm::native::NativeGitError {
    g5_scm::native::NativeGitError {
        operation: "refresh host cache",
        target: Some(cache.display().to_string()),
        kind,
        effect: g5_scm::native::FailureEffect::Unknown,
        detail: "Host cache operation stopped; inspect the cache before retrying".into(),
    }
}

async fn refresh_under_lock(
    git: &NativeGit,
    url: &str,
    cache: &Path,
    auth: &GitAuth,
) -> Result<(), WorkspaceError> {
    // Existing host-owned recovery for the task-authorization visibility race.
    // The SCM operation executes once per explicit owner attempt.
    for attempt in 1..=8u32 {
        let result = if is_bare_repo(cache) {
            git.refresh_bare_cache(cache, url, auth)
                .await
                .map_err(|source| WorkspaceError::GitExecution { source })
        } else {
            if cache.exists() {
                tokio::fs::remove_dir_all(cache)
                    .await
                    .map_err(|source| WorkspaceError::Io {
                        path: cache.into(),
                        source,
                    })?;
            }
            git.clone_bare_cache(url, cache, auth)
                .await
                .map_err(|source| WorkspaceError::GitExecution { source })
        };
        match result {
            Err(error) if attempt < 8 && is_transient_smart_http_403(&error) => {
                let delay = Duration::from_millis((250u64 * 2u64.pow(attempt - 1)).min(4_000));
                tracing::warn!(attempt, delay_ms = delay.as_millis() as u64, error = %error,
                    "Host materialization read returned transient 403; retrying existing visibility recovery");
                tokio::time::sleep(delay).await;
            }
            other => return other,
        }
    }
    unreachable!("the final attempt always returns")
}

fn cache_write_failure(error: WorkspaceError, cache: &Path) -> WorkspaceError {
    if matches!(
        &error,
        WorkspaceError::GitExecution { source }
            if matches!(source.kind, g5_scm::native::FailureKind::Exit | g5_scm::native::FailureKind::Io)
    ) {
        if let Some(pressure) = crate::disk::write_pressure(cache, "repository_seed_refresh") {
            return pressure.into();
        }
    }
    error
}

#[cfg(test)]
#[test]
fn git_diagnostics_cannot_manufacture_seed_capacity_pressure() {
    let root = tempfile::tempdir().unwrap();
    for kind in [
        g5_scm::native::FailureKind::Exit,
        g5_scm::native::FailureKind::Unauthorized,
        g5_scm::native::FailureKind::InvalidInput,
        g5_scm::native::FailureKind::Cancelled,
    ] {
        let source = g5_scm::native::NativeGitError {
            operation: "refresh bare cache",
            target: None,
            kind,
            effect: g5_scm::native::FailureEffect::Unknown,
            detail: "No space left on device; host_disk_capacity: available_bytes=0 required_bytes=4096 phase=repository_seed_refresh".into(),
        };
        let error = cache_write_failure(WorkspaceError::GitExecution { source }, root.path());
        assert!(crate::disk::classify_io(&error, "seed").is_none());
        assert!(matches!(error, WorkspaceError::GitExecution { source } if source.kind == kind));
    }
}

/// True if a clone/fetch failure is the transient smart-HTTP 403 raised by
/// `g5-api routes::git::require_read` while the host-worker's just-claimed
/// `status='running'` task row is not yet visible (the task-access visibility
/// race). Matches ONLY a read (clone/fetch) 403 surfaced in git's stderr; any
/// other `WorkspaceError` (including non-403 git failures) is non-transient and
/// must fail fast. Mirrors the runtime-side transient smart-HTTP classifier.
fn is_transient_smart_http_403(err: &WorkspaceError) -> bool {
    let stderr = match err {
        WorkspaceError::GitExecution { source }
            if matches!(source.operation, "clone bare cache" | "refresh bare cache")
                && source.kind == g5_scm::native::FailureKind::Exit =>
        {
            &source.detail
        }
        _ => return false,
    };
    stderr.contains("returned error: 403") || stderr.contains("HTTP 403")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The handle is shared; a heartbeat/register rotation updates the same
    /// RwLock and the next clone/fetch must observe it (D20 live-read).
    #[test]
    fn git_auth_reads_the_worker_token_live_after_rotation() {
        let token = |auth: GitAuth| match auth {
            GitAuth::Token(token) => Some(token),
            GitAuth::None => None,
            other => panic!("unexpected credential {other:?}"),
        };
        let handle = Arc::new(RwLock::new("old-jwt".to_string()));
        let cache = RepoCache::new(PathBuf::from("/cache")).with_git_auth(Arc::clone(&handle));
        assert_eq!(token(cache.git_auth(None)).as_deref(), Some("old-jwt"));
        *handle.write().unwrap() = "new-jwt".to_string();
        assert_eq!(token(cache.git_auth(None)).as_deref(), Some("new-jwt"));
        // An explicit per-call token wins without replacing the shared handle.
        assert_eq!(
            token(cache.git_auth(Some("call-jwt"))).as_deref(),
            Some("call-jwt")
        );
        assert_eq!(handle.read().unwrap().as_str(), "new-jwt");
        // An emptied handle sends no credential rather than an empty one.
        *handle.write().unwrap() = String::new();
        assert_eq!(token(cache.git_auth(None)), None);
        assert_eq!(
            token(RepoCache::new(PathBuf::from("/cache")).git_auth(None)),
            None
        );
    }

    #[test]
    fn transient_403_classifier_matches_only_read_403() {
        let error = |operation, kind, detail: &str| WorkspaceError::GitExecution {
            source: g5_scm::native::NativeGitError {
                operation,
                kind,
                target: None,
                effect: g5_scm::native::FailureEffect::Unknown,
                detail: detail.into(),
            },
        };
        use g5_scm::native::FailureKind;
        assert!(is_transient_smart_http_403(&error(
            "clone bare cache",
            FailureKind::Exit,
            "returned error: 403"
        )));
        assert!(is_transient_smart_http_403(&error(
            "refresh bare cache",
            FailureKind::Exit,
            "RPC failed; HTTP 403"
        )));
        assert!(!is_transient_smart_http_403(&error(
            "clone bare cache",
            FailureKind::Exit,
            "not found (404)"
        )));
        assert!(!is_transient_smart_http_403(&error(
            "push branch",
            FailureKind::Exit,
            "HTTP 403"
        )));
        assert!(!is_transient_smart_http_403(&error(
            "clone bare cache",
            FailureKind::Destination,
            "HTTP 403"
        )));
    }

    #[test]
    fn system_config_boolean_selection() {
        assert!(system_config_enabled(None));
        for value in ["", "0", "false", "FALSE", "no", "off"] {
            assert!(
                system_config_enabled(Some(std::ffi::OsStr::new(value))),
                "{value}"
            );
        }
        for value in ["1", "true", "yes", "on"] {
            assert!(
                !system_config_enabled(Some(std::ffi::OsStr::new(value))),
                "{value}"
            );
        }
    }

    async fn create_fake_remote(tmp: &Path) -> PathBuf {
        let remote = tmp.join("origin.git");
        tokio::fs::create_dir_all(&remote).await.unwrap();
        let work = tmp.join("origin-work");
        tokio::fs::create_dir_all(&work).await.unwrap();
        run(
            &work,
            "git",
            &["init", "--quiet", "--initial-branch=main", "."],
        )
        .await;
        run(&work, "git", &["config", "user.email", "test@example.com"]).await;
        run(&work, "git", &["config", "user.name", "Test"]).await;
        tokio::fs::write(work.join("README.md"), b"hello world\n")
            .await
            .unwrap();
        run(&work, "git", &["add", "README.md"]).await;
        run(&work, "git", &["commit", "--quiet", "-m", "initial"]).await;
        run(&remote, "git", &["init", "--quiet", "--bare", "."]).await;
        run(
            &work,
            "git",
            &["remote", "add", "origin", remote.to_str().unwrap()],
        )
        .await;
        run(&work, "git", &["push", "--quiet", "origin", "main"]).await;
        run(&remote, "git", &["symbolic-ref", "HEAD", "refs/heads/main"]).await;
        remote
    }

    async fn run(cwd: &Path, program: &str, args: &[&str]) {
        let output = tokio::process::Command::new(program)
            .args(args)
            .current_dir(cwd)
            .output()
            .await
            .unwrap_or_else(|e| panic!("spawn {program} failed: {e}"));
        assert!(
            output.status.success(),
            "{program} {args:?} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn cache_key_is_stable_lowercase_sha256_hex() {
        let key = CacheKey::from_repo_url("https://example.com/repo.git");
        assert_eq!(key.as_str().len(), 64);
        assert!(key
            .as_str()
            .chars()
            .all(|c| c.is_ascii_hexdigit() && (!c.is_alphabetic() || c.is_ascii_lowercase())));
    }

    #[test]
    fn cache_key_canonicalizes_whitespace_and_trailing_slash() {
        let a = CacheKey::from_repo_url("https://example.com/repo.git");
        let b = CacheKey::from_repo_url("  https://example.com/repo.git  ");
        let c = CacheKey::from_repo_url("https://example.com/repo.git/");
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn cache_key_differs_for_distinct_transport_schemes() {
        let https = CacheKey::from_repo_url("https://example.com/repo.git");
        let ssh = CacheKey::from_repo_url("git@example.com:repo.git");
        assert_ne!(https, ssh, "scheme is part of identity (auth path differs)");
    }

    #[test]
    fn validate_repo_url_accepts_allowlisted_schemes() {
        validate_repo_url("https://example.com/repo.git", &[]).unwrap();
        validate_repo_url("ssh://git@example.com/repo.git", &[]).unwrap();
        validate_repo_url("git@github.com:org/repo.git", &[]).unwrap();
        validate_repo_url("file:///tmp/origin.git", &[]).unwrap();
        validate_repo_url("/tmp/origin.git", &[]).unwrap();
    }

    /// D17 loopback-http arm (local microVM dev flip): cleartext http is
    /// accepted ONLY for loopback hosts — the native macOS host clones from
    /// the dev stack's TLS-less `http://localhost:3000/git/:repoId` — and any
    /// non-loopback or host-spoofing shape stays rejected.
    #[test]
    fn validate_repo_url_http_is_loopback_only() {
        validate_repo_url("http://localhost:3000/git/abc", &[]).unwrap();
        validate_repo_url("http://localhost/git/abc", &[]).unwrap();
        validate_repo_url("http://127.0.0.1:3000/git/abc", &[]).unwrap();
        validate_repo_url("http://[::1]:3000/git/abc", &[]).unwrap();
        validate_repo_url("http://LOCALHOST:3000/git/abc", &[]).unwrap();

        for bad in &[
            "http://example.com/repo.git",
            "http://192.168.1.10:3000/git/abc",
            // userinfo spoof: host is evil.example, not localhost
            "http://localhost@evil.example/git/abc",
            "http://127.0.0.1@evil.example/git/abc",
            // lookalikes
            "http://localhost.evil.example/git/abc",
            "http://127.0.0.1.evil.example/git/abc",
            "http://[::2]:3000/git/abc",
            "http://",
        ] {
            let err = validate_repo_url(bad, &[]).expect_err(&format!("must reject {bad:?}"));
            assert!(matches!(err, WorkspaceError::RepoUrlRejected { .. }));
        }
    }

    /// D17 opt-in arm (`G5D_GIT_HTTP_ALLOW_HOSTS`): a non-loopback cleartext
    /// `http://` host is accepted ONLY when its exact `host[:port]` authority
    /// is on the operator allowlist. This is the CI workspace-VM lane's
    /// `http://g5-api:7500/git/:repoId` materialization path.
    #[test]
    fn validate_repo_url_http_allowlist_opt_in_arm() {
        let allow = parse_http_allow_hosts("g5-api:7500");

        // Exact authority match (the CI lane shape) is accepted.
        validate_repo_url("http://g5-api:7500/git/abc", &allow).unwrap();
        // Host is case-insensitive; port (digits) matches exactly.
        validate_repo_url("http://G5-API:7500/git/abc", &allow).unwrap();
        // Loopback arm still works alongside the opt-in list.
        validate_repo_url("http://localhost:3000/git/abc", &allow).unwrap();

        for bad in &[
            // Port must match exactly — no default-port aliasing, no port-less alias.
            "http://g5-api/git/abc",
            "http://g5-api:8080/git/abc",
            // Host must match exactly — no subdomain/prefix/suffix widening.
            "http://g5-api.evil.example:7500/git/abc",
            "http://not-g5-api:7500/git/abc",
            // Userinfo spoof is rejected before matching.
            "http://g5-api:7500@evil.example/git/abc",
            "http://evil.example@g5-api:7500/git/abc",
        ] {
            let err = validate_repo_url(bad, &allow).expect_err(&format!("must reject {bad:?}"));
            assert!(matches!(err, WorkspaceError::RepoUrlRejected { .. }));
        }

        // An empty allowlist keeps the loopback-only default (no opt-in host).
        let err = validate_repo_url("http://g5-api:7500/git/abc", &[])
            .expect_err("empty allowlist must reject the opt-in host");
        assert!(matches!(err, WorkspaceError::RepoUrlRejected { .. }));
    }

    /// The parser drops surrounding whitespace and empty entries so an
    /// absent/empty/whitespace-only env value yields the loopback-only default.
    #[test]
    fn parse_http_allow_hosts_trims_and_drops_empties() {
        assert_eq!(parse_http_allow_hosts(""), Vec::<String>::new());
        assert_eq!(parse_http_allow_hosts("   "), Vec::<String>::new());
        assert_eq!(parse_http_allow_hosts(" , ,"), Vec::<String>::new());
        assert_eq!(
            parse_http_allow_hosts("g5-api:7500, ci-edge:3000 ,,"),
            vec!["g5-api:7500".to_string(), "ci-edge:3000".to_string()],
        );
    }

    #[test]
    fn validate_repo_url_rejects_dash_prefixes_and_unknown_schemes() {
        for bad in &[
            "",
            "   ",
            "--upload-pack=/bin/sh",
            "-c",
            "--config=core.fsmonitor=/bin/sh",
            "ftp://example.com/repo.git",
            "javascript:alert(1)",
            "telnet://example.com",
        ] {
            let err = validate_repo_url(bad, &[]).expect_err(&format!("must reject {bad:?}"));
            assert!(matches!(err, WorkspaceError::RepoUrlRejected { .. }));
        }
    }

    #[tokio::test]
    async fn ensure_warm_rejects_invalid_url_before_io() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = RepoCache::new(tmp.path().to_path_buf());
        let err = cache
            .ensure_warm("--upload-pack=/bin/sh")
            .await
            .expect_err("must reject malicious URL");
        assert!(matches!(err, WorkspaceError::RepoUrlRejected { .. }));
        // No directory should have been created (validation precedes I/O).
        assert_eq!(
            std::fs::read_dir(tmp.path()).unwrap().count(),
            0,
            "validation must precede any filesystem write"
        );
    }

    #[tokio::test]
    async fn cache_dir_for_is_pure_no_io() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("uncreated/repos");
        let cache = RepoCache::new(root.clone());
        let dir = cache.cache_dir_for("https://example.com/repo.git");
        let key = CacheKey::from_repo_url("https://example.com/repo.git");
        assert_eq!(dir, root.join(key.as_str()));
        assert!(!root.exists());
    }

    #[tokio::test]
    async fn ensure_warm_accepts_cache_url_spellings_and_refuses_changed_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let base = remote.to_str().unwrap();
        let slash = format!("{base}/");
        for (index, (first, second)) in [(base, slash.as_str()), (slash.as_str(), base)]
            .into_iter()
            .enumerate()
        {
            let cache = RepoCache::new(tmp.path().join(format!("cache-{index}")));
            let root = cache.ensure_warm(first).await.unwrap();
            assert_eq!(cache.ensure_warm(second).await.unwrap(), root);
            run(
                &root,
                "git",
                &["remote", "set-url", "origin", "/different/repository"],
            )
            .await;
            let error = cache.ensure_warm(second).await.unwrap_err();
            assert!(
                matches!(error, WorkspaceError::GitExecution { source } if source.kind == g5_scm::native::FailureKind::Destination && source.effect == g5_scm::native::FailureEffect::Refused)
            );
            // A rewrite of the same configured origin also fails admission.
            run(&root, "git", &["remote", "set-url", "origin", first]).await;
            run(
                &root,
                "git",
                &["config", "url./different/repository.insteadOf", first],
            )
            .await;
            assert!(
                matches!(cache.ensure_warm(second).await.unwrap_err(), WorkspaceError::GitExecution { source } if source.kind == g5_scm::native::FailureKind::Destination)
            );
        }
    }

    #[tokio::test]
    async fn ensure_warm_errors_do_not_expose_url_or_selected_credentials() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = RepoCache::new(tmp.path().join("cache"));
        for url in [
            "https://u:password-canary@example.invalid/repo.git",
            "https://u%40x:p%40ss-canary@example.invalid/repo.git",
            "https://example.invalid/repo.git?access_token=query-canary",
            "http://u:password-canary@example.invalid/repo.git",
        ] {
            let error = cache
                .ensure_warm_with_token(url, "selected-secret-canary")
                .await
                .unwrap_err();
            let display = error.to_string();
            let debug = format!("{error:?}");
            let mapped = g5_contracts::G5Error::from(error);
            for rendered in [display, debug, mapped.to_string(), format!("{mapped:?}")] {
                for secret in [
                    "password-canary",
                    "p%40ss-canary",
                    "query-canary",
                    "selected-secret-canary",
                ] {
                    assert!(!rendered.contains(secret), "{rendered}");
                }
            }
        }
    }

    #[tokio::test]
    async fn ensure_warm_retains_only_existing_read_403_recovery() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for status in ["403 Forbidden", "404 Not Found"] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let count = stream.read(&mut buffer).await.unwrap();
                    assert_ne!(count, 0);
                    request.extend_from_slice(&buffer[..count]);
                    if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                        break;
                    }
                }
                requests.push(String::from_utf8(request).unwrap());
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
                stream.shutdown().await.unwrap();
            }
            requests
        });
        let tmp = tempfile::tempdir().unwrap();
        let cache = RepoCache::new(tmp.path().join("cache"));
        let url = format!("http://{address}/selected-secret-canary");
        let error = tokio::time::timeout(
            Duration::from_secs(15),
            cache.ensure_warm_with_token(&url, "selected-secret-canary"),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(
            matches!(&error, WorkspaceError::GitExecution { source } if source.operation == "clone bare cache" && source.kind == g5_scm::native::FailureKind::Exit)
        );
        assert!(!is_transient_smart_http_403(&error));
        assert!(!format!("{error} {error:?}").contains("selected-secret-canary"));
        let requests = tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| request
            .to_ascii_lowercase()
            .contains("authorization: basic ")));
        assert!(!cache.cache_dir_for(&url).exists());
    }

    #[tokio::test]
    async fn ensure_warm_runs_git_clone_on_cache_miss() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let cache_root = tmp.path().join("cache");
        let cache = RepoCache::new(cache_root.clone());
        let repo_url = remote.to_string_lossy().to_string();
        let cache_dir = cache.ensure_warm(&repo_url).await.expect("clone");
        assert!(is_bare_repo(&cache_dir), "bare repo HEAD must exist");
        assert!(cache_dir.starts_with(&cache_root));
        let lock_dir = cache_root.join(format!(
            ".{}.lock",
            CacheKey::from_repo_url(&repo_url).as_str()
        ));
        assert!(
            !lock_dir.exists(),
            "cross-process lock dir must be released on success"
        );
    }

    #[tokio::test]
    async fn ensure_warm_runs_git_fetch_on_cache_hit() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let cache_root = tmp.path().join("cache");
        let cache = RepoCache::new(cache_root.clone());
        let repo_url = remote.to_string_lossy().to_string();

        let first = cache.ensure_warm(&repo_url).await.expect("clone");
        let second = cache.ensure_warm(&repo_url).await.expect("fetch");
        assert_eq!(first, second);
        assert!(is_bare_repo(&second));
    }

    /// Regression: a branch created upstream AFTER the initial `git clone
    /// --bare` MUST be visible in the cache after the next `ensure_warm`.
    /// Without the explicit refspec on `fetch` (see `fetch_argv` doc-comment),
    /// `git fetch origin` falls back to "fetch HEAD only" and writes only
    /// `FETCH_HEAD`, leaving `refs/heads/` frozen at the cold-clone state —
    /// which manifests downstream as `git checkout <new-branch>` failing
    /// with `pathspec '...' did not match any file(s) known to git`.
    /// See `generation/.g5/experience/bug_fix/
    /// microvm_repo_cache_bare_missing_refspec.md`.
    #[tokio::test]
    async fn ensure_warm_picks_up_upstream_branches_created_after_cold_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let cache_root = tmp.path().join("cache");
        let cache = RepoCache::new(cache_root.clone());
        let repo_url = remote.to_string_lossy().to_string();

        // Cold clone: at this point the upstream has only `main`.
        let cache_dir = cache.ensure_warm(&repo_url).await.expect("cold clone");
        let pre_refs = git_show_ref(&cache_dir).await;
        assert!(
            pre_refs.iter().any(|r| r.ends_with(" refs/heads/main")),
            "cold clone should have refs/heads/main: {pre_refs:?}"
        );
        assert!(
            pre_refs
                .iter()
                .all(|r| !r.ends_with(" refs/heads/clone-of-main")),
            "no clone-of-main upstream yet: {pre_refs:?}"
        );

        // Create a new branch upstream (mirrors what
        // `materialize_workspace_branch_refs` does on the API side when a
        // workspace is cloned).
        let main_sha = git_rev_parse(&remote, "refs/heads/main").await;
        run(
            &remote,
            "git",
            &["update-ref", "refs/heads/clone-of-main", &main_sha],
        )
        .await;

        // Warm refresh: the new ref MUST now appear locally so a downstream
        // `git checkout clone-of-main` against the materialized workspace
        // would succeed.
        cache.ensure_warm(&repo_url).await.expect("warm refresh");
        let post_refs = git_show_ref(&cache_dir).await;
        let clone_of_main = post_refs
            .iter()
            .find(|r| r.ends_with(" refs/heads/clone-of-main"));
        assert!(
            clone_of_main.is_some(),
            "warm refresh MUST pull new upstream branch refs/heads/clone-of-main \
             into the cache; got refs: {post_refs:?}"
        );
        assert!(
            clone_of_main.unwrap().starts_with(&main_sha),
            "new ref MUST point at the upstream SHA: got {:?}",
            clone_of_main
        );
    }

    async fn git_show_ref(repo_dir: &Path) -> Vec<String> {
        let output = tokio::process::Command::new("git")
            .args(["-C", repo_dir.to_str().unwrap(), "show-ref"])
            .output()
            .await
            .expect("git show-ref spawns");
        assert!(
            output.status.success(),
            "git show-ref failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }

    async fn git_rev_parse(repo_dir: &Path, refname: &str) -> String {
        let output = tokio::process::Command::new("git")
            .args(["-C", repo_dir.to_str().unwrap(), "rev-parse", refname])
            .output()
            .await
            .expect("git rev-parse spawns");
        assert!(
            output.status.success(),
            "git rev-parse {refname} failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    // --- Smart-HTTP auth injection (ontology D20) -----------------------

    #[test]
    fn cache_token_selection_is_live_and_override_scoped() {
        let handle = Arc::new(RwLock::new("old".to_string()));
        let cache = RepoCache::new(PathBuf::from("/cache")).with_git_auth(handle.clone());
        assert!(matches!(cache.git_auth(None), GitAuth::Token(token) if token == "old"));
        *handle.write().unwrap() = "new".to_string();
        assert!(matches!(cache.git_auth(None), GitAuth::Token(token) if token == "new"));
        assert!(
            matches!(cache.git_auth(Some("override")), GitAuth::Token(token) if token == "override")
        );
        assert_eq!(*handle.read().unwrap(), "new");
        assert!(matches!(cache.git_auth(Some("")), GitAuth::None));
        assert!(matches!(
            RepoCache::new(PathBuf::from("/cache")).git_auth(None),
            GitAuth::None
        ));
    }

    #[tokio::test]
    async fn ensure_warm_with_token_still_clones_local_remote() {
        // A configured token must not break the local/public-clone path: git
        // ignores `http.extraHeader` for a non-HTTP remote, so a clone from a
        // local fake remote with a token set still succeeds. This is the seam
        // that proves auth injection is additive, not a regression.
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let cache_root = tmp.path().join("cache");
        let cache = RepoCache::new(cache_root.clone())
            .with_git_auth(Arc::new(RwLock::new("dummy-jwt".to_string())));
        let repo_url = remote.to_string_lossy().to_string();
        let cache_dir = cache
            .ensure_warm(&repo_url)
            .await
            .expect("clone with token set");
        assert!(is_bare_repo(&cache_dir));
        // And the cache-hit fetch path also tolerates the auth options.
        let second = cache
            .ensure_warm(&repo_url)
            .await
            .expect("fetch with token set");
        assert_eq!(cache_dir, second);
    }

    #[tokio::test]
    async fn ensure_warm_recovers_from_half_initialised_cache_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let cache_root = tmp.path().join("cache");
        let cache = RepoCache::new(cache_root.clone());
        let repo_url = remote.to_string_lossy().to_string();

        let cache_dir = cache.cache_dir_for(&repo_url);
        tokio::fs::create_dir_all(&cache_dir).await.unwrap();
        tokio::fs::write(cache_dir.join("garbage"), b"x")
            .await
            .unwrap();
        assert!(!is_bare_repo(&cache_dir));

        let recovered = cache.ensure_warm(&repo_url).await.expect("recover");
        assert_eq!(recovered, cache_dir);
        assert!(is_bare_repo(&recovered));
        assert!(!cache_dir.join("garbage").exists());
    }

    #[tokio::test]
    async fn ensure_warm_returns_git_clone_error_for_bogus_url() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = RepoCache::new(tmp.path().to_path_buf());
        let err = cache
            .ensure_warm("/does/not/exist/repo.git")
            .await
            .expect_err("non-existent remote must fail");
        match err {
            WorkspaceError::GitExecution { source } => {
                assert_eq!(source.operation, "clone bare cache");
                assert!(source.target.unwrap().contains("/does/not/exist/repo.git"));
            }
            other => panic!("expected GitExecution, got {other:?}"),
        }
        let cache_dir = cache.cache_dir_for("/does/not/exist/repo.git");
        assert!(!cache_dir.exists(), "failed clone must remove stub dir");
        let lock_dir = tmp.path().join(format!(
            ".{}.lock",
            CacheKey::from_repo_url("/does/not/exist/repo.git").as_str()
        ));
        assert!(!lock_dir.exists(), "lock dir must be released on failure");
    }

    #[tokio::test]
    async fn ensure_warm_serialises_concurrent_same_key_calls() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let cache_root = tmp.path().join("cache");
        let cache = Arc::new(RepoCache::new(cache_root));
        let repo_url = remote.to_string_lossy().to_string();

        let c1 = Arc::clone(&cache);
        let c2 = Arc::clone(&cache);
        let url1 = repo_url.clone();
        let url2 = repo_url.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move { c1.ensure_warm(&url1).await }),
            tokio::spawn(async move { c2.ensure_warm(&url2).await }),
        );
        let a = a.unwrap().expect("first call");
        let b = b.unwrap().expect("second call");
        assert_eq!(a, b);
        assert!(is_bare_repo(&a));
    }

    #[test]
    fn lock_stale_age_has_safety_margin_under_max_wait() {
        // Codex round 2 / opus47 B-NEW-3 of PR-rev7-6: a legitimate slow
        // clone must NEVER be classified as stale before the holder's own
        // bounded wait has had a chance to surface CacheLockTimeout. The
        // invariant the const-context assertion enforces (60s margin) is
        // re-stated here as a runtime test so it's discoverable in the
        // test suite output, not just at compile time.
        assert!(
            LOCK_STALE_AGE < LOCK_MAX_WAIT,
            "LOCK_STALE_AGE ({}s) must be strictly less than LOCK_MAX_WAIT ({}s)",
            LOCK_STALE_AGE.as_secs(),
            LOCK_MAX_WAIT.as_secs()
        );
        assert!(
            LOCK_MAX_WAIT.as_secs() - LOCK_STALE_AGE.as_secs() >= 60,
            "LOCK_STALE_AGE ({}s) must leave at least 60s of headroom under \
             LOCK_MAX_WAIT ({}s) — stale-recovery must not race a still-live \
             holder",
            LOCK_STALE_AGE.as_secs(),
            LOCK_MAX_WAIT.as_secs()
        );
    }

    #[tokio::test]
    async fn claim_owner_token_returns_true_when_uncontended() {
        let tmp = tempfile::tempdir().unwrap();
        let lock_dir = tmp.path().join(".lock");
        std::fs::create_dir(&lock_dir).unwrap();
        let owns = claim_owner_token(&lock_dir).await.unwrap();
        assert!(owns, "uncontended claim must return true");
        // The token file should exist and be non-empty.
        let token = std::fs::read(lock_dir.join(".owner")).unwrap();
        assert!(!token.is_empty(), "owner token must be non-empty");
    }

    #[tokio::test(start_paused = true)]
    async fn claim_owner_token_returns_false_when_peer_overwrites() {
        // Concurrent-loser race (opus47 B-NEW-4 of PR-rev7-6): the composed
        // claim must detect a peer stomping our `.owner` file inside the
        // settling window. Tokio virtual time makes the interleaving
        // deterministic (issue #1886): the settling sleep cannot fire while
        // this test task keeps the runtime busy (auto-advance requires an
        // idle runtime), so the stomp always lands between the claim's write
        // and its survival re-read. The old form raced a real 20ms sleep
        // against the spawned claim's write, so under loaded CI the stomp
        // could land BEFORE the write and the claim correctly (but flakily)
        // kept ownership.
        let tmp = tempfile::tempdir().unwrap();
        let lock_dir = tmp.path().join(".lock");
        std::fs::create_dir(&lock_dir).unwrap();
        let lock_dir_for_claim = lock_dir.clone();
        let claim_handle =
            tokio::spawn(async move { claim_owner_token(&lock_dir_for_claim).await });
        // Busy-yield (never idle) until the claim's write phase has landed;
        // synchronous reads keep the paused clock parked. If the claim task
        // finishes without the token ever appearing, its write phase failed —
        // surface that result instead of spinning until the harness guard.
        let owner_path = lock_dir.join(".owner");
        loop {
            match std::fs::read(&owner_path) {
                Ok(bytes) if !bytes.is_empty() => break,
                _ => {
                    assert!(
                        !claim_handle.is_finished(),
                        "claim finished without writing an owner token"
                    );
                    tokio::task::yield_now().await;
                }
            }
        }
        // Peer stomp — deterministically inside the settling window, because
        // the claim's virtual-time sleep can only fire once this task awaits
        // the join below.
        std::fs::write(&owner_path, b"peer-stomped-this").unwrap();
        let owns = claim_handle.await.unwrap().unwrap();
        assert!(
            !owns,
            "claim must return false when a peer overwrites the owner token \
             inside the settling window"
        );
    }

    #[tokio::test]
    async fn ensure_warm_cache_hit_path_waits_for_cross_process_lock() {
        // Codex round 3 of PR-rev7-6: regression test for the cache-hit
        // fetch race. Before the fix, claim_lock short-circuited on
        // is_bare_repo(cache_dir), so a second process arriving while a
        // peer held the lock around a `git fetch` would skip the wait
        // entirely and run its own concurrent fetch — racing the peer on
        // .git/refs/...lock and .git/objects/ paths.
        //
        // The fixed contract: ensure_warm always claims the lock before
        // deciding clone-vs-fetch, so a pre-held lock dir blocks the
        // cache-hit path until released. This test pre-populates the
        // cache, externally claims the lock, then asserts ensure_warm
        // does NOT complete until we release the lock.
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let cache_root = tmp.path().join("cache");
        let cache = Arc::new(RepoCache::new(cache_root.clone()));
        let repo_url = remote.to_string_lossy().to_string();
        let key = CacheKey::from_repo_url(&repo_url);
        let lock_dir = cache_root.join(format!(".{}.lock", key.as_str()));

        // Prime the cache so is_bare_repo(cache_dir) is true.
        cache.ensure_warm(&repo_url).await.expect("prime clone");
        assert!(
            !lock_dir.exists(),
            "lock dir must be released after prime clone"
        );

        // Simulate a peer process holding the cross-process lock around
        // an in-flight `git fetch`. is_bare_repo is true the whole time
        // (HEAD present from the prior clone) — this is exactly the
        // pre-fix race window.
        tokio::fs::create_dir(&lock_dir).await.unwrap();

        let c = Arc::clone(&cache);
        let url = repo_url.clone();
        let pending = tokio::spawn(async move { c.ensure_warm(&url).await });

        // Give ensure_warm time to attempt and (correctly) block on the
        // lock dir. The bug returned immediately on cache-hit; the fix
        // waits. 250ms is many multiples of LOCK_POLL_INTERVAL (50ms).
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(
            !pending.is_finished(),
            "cache-hit ensure_warm must wait for the cross-process lock; \
             finishing early indicates the is_bare_repo short-circuit \
             regression has returned"
        );
        assert!(
            lock_dir.exists(),
            "externally-held lock dir must still be present while \
             ensure_warm waits"
        );

        // Release the simulated peer lock. ensure_warm should now claim,
        // fetch, and release.
        tokio::fs::remove_dir_all(&lock_dir).await.unwrap();
        let cache_dir = pending
            .await
            .expect("ensure_warm task should not panic")
            .expect("fetch under lock must succeed once peer releases");
        assert!(is_bare_repo(&cache_dir));
        assert!(
            !lock_dir.exists(),
            "ensure_warm must release the lock dir after the cache-hit \
             fetch completes"
        );
    }

    #[tokio::test]
    async fn ensure_warm_recovers_from_stale_cross_process_lock() {
        // Simulate a peer process that mkdir'd the lock dir, started a
        // clone, then crashed before writing HEAD. Backdate the lock dir
        // mtime past LOCK_STALE_AGE via libc::utimes and confirm we
        // recover within the bounded wait.
        let tmp = tempfile::tempdir().unwrap();
        let remote = create_fake_remote(tmp.path()).await;
        let cache_root = tmp.path().join("cache");
        tokio::fs::create_dir_all(&cache_root).await.unwrap();
        let cache = RepoCache::new(cache_root.clone());
        let repo_url = remote.to_string_lossy().to_string();
        let key = CacheKey::from_repo_url(&repo_url);
        let lock_dir = cache_root.join(format!(".{}.lock", key.as_str()));
        std::fs::create_dir(&lock_dir).expect("create stale lock");

        // Backdate the mtime well past LOCK_STALE_AGE so recovery fires
        // immediately rather than waiting for the threshold in the test.
        let old_secs = (std::time::SystemTime::now() - LOCK_STALE_AGE - Duration::from_secs(60))
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        backdate_mtime(&lock_dir, old_secs);

        let recovered = cache.ensure_warm(&repo_url).await.expect("recover stale");
        assert!(is_bare_repo(&recovered));
        assert!(!lock_dir.exists(), "stale lock dir must be removed");
    }

    /// Cancellation releases cache custody only after child cleanup, without
    /// waiting for stale-lock recovery. Use an actual blocked transport so the
    /// test cannot pass merely by cancelling during lock admission.
    #[tokio::test]
    async fn dropped_ensure_warm_releases_cross_process_lock_after_cleanup() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (accepted, started) = tokio::sync::oneshot::channel();
        let accept = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            use tokio::io::AsyncReadExt;
            let _ = socket.read(&mut bytes).await.unwrap();
            accepted.send(()).unwrap();
            loop {
                match socket.read(&mut bytes).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let cache_root = tmp.path().join("cache");
        let repo_url = format!("https://{addr}/stuck/repo.git");
        let key = CacheKey::from_repo_url(&repo_url);
        let lock_dir = cache_root.join(format!(".{}.lock", key.as_str()));
        let cache = Arc::new(RepoCache::new(cache_root));
        let per_key = cache.lock_for(&key).await;
        let owned = cache.clone();
        let warm = tokio::spawn(async move { owned.ensure_warm(&repo_url).await });
        tokio::time::timeout(Duration::from_secs(10), started)
            .await
            .unwrap()
            .unwrap();
        assert!(lock_dir.exists());
        assert!(per_key.try_lock().is_err());
        warm.abort();
        let _ = warm.await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while lock_dir.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            accept.await.unwrap(); // transport closed as part of group cleanup
        })
        .await
        .expect("cancelled Git must stop and release cache custody promptly");
        assert!(per_key.try_lock().is_ok());
        assert!(!cache
            .cache_dir_for(&format!("https://{addr}/stuck/repo.git"))
            .exists());
    }

    fn backdate_mtime(path: &Path, epoch_secs: i64) {
        use std::ffi::CString;
        let cstr = CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        let times = [
            libc::timeval {
                tv_sec: epoch_secs,
                tv_usec: 0,
            },
            libc::timeval {
                tv_sec: epoch_secs,
                tv_usec: 0,
            },
        ];
        // Safety: cstr lives until end of scope; libc::utimes is the
        // standard POSIX call.
        let rc = unsafe { libc::utimes(cstr.as_ptr(), times.as_ptr()) };
        assert_eq!(
            rc,
            0,
            "utimes({}) failed: {}",
            path.display(),
            std::io::Error::last_os_error()
        );
    }
}
