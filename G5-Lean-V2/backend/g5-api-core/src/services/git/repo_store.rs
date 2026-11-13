//! Repository storage as G5 reaches it (`g5.repo.storage-guard`).
//!
//! [`TenantRepoStorage`] is the sole holder of the bare-repository store.
//! Every repository path it hands out is named by a [`RepoKey`], and a key
//! exists only for an organization the caller is scoped to
//! ([`TenantRepoStorage::key`]): the path guard, independent of row-level
//! security. A repository id paired with an in-scope organization that does
//! not own it names `{org}/{repo}`, which does not exist.
//!
//! A key, and a path returned for it, may outlive the transaction that
//! minted it: long Git work runs after its scoped transaction closes, and
//! revoking membership mid-operation stays with `MembershipActivityGuard`.
//! `tests/repo_storage_tenancy_guard.rs` keeps every other module from naming
//! the raw store, its constructors or its path methods.

use std::path::PathBuf;
use std::sync::Arc;

use g5_auth::{HostSourceClaims, WorkspaceIncarnationClaims};
use g5_authz::{ScopedDb, ScopedDbReopener};
use g5_contracts::error::G5Error;
use g5_contracts::types::ids::OrgId;
use g5_storage::RepoStorage;
use uuid::Uuid;

mod sealed {
    pub trait Sealed {}
}

/// Who may name an organization's repositories: a tenant transaction (or
/// its reopener) scoped to it, or a verified workspace or host credential
/// that carries it. Sealed, so nothing else can vouch for an organization.
pub trait OrgScope: sealed::Sealed {
    /// The organizations this scope covers; none for an unscoped handle.
    fn scoped_org_ids(&self) -> Vec<Uuid>;

    /// Whether this scope covers `org_id`.
    fn covers_org(&self, org_id: Uuid) -> bool {
        self.scoped_org_ids().contains(&org_id)
    }
}

fn uuids(scope: &[OrgId]) -> Vec<Uuid> {
    scope.iter().map(|org| *org.as_uuid()).collect()
}

impl sealed::Sealed for ScopedDb {}
impl OrgScope for ScopedDb {
    fn scoped_org_ids(&self) -> Vec<Uuid> {
        uuids(self.org_ids())
    }

    fn covers_org(&self, org_id: Uuid) -> bool {
        self.org_ids().iter().any(|org| *org.as_uuid() == org_id)
    }
}

impl sealed::Sealed for ScopedDbReopener {}
impl OrgScope for ScopedDbReopener {
    fn scoped_org_ids(&self) -> Vec<Uuid> {
        uuids(self.org_ids())
    }

    fn covers_org(&self, org_id: Uuid) -> bool {
        self.org_ids().iter().any(|org| *org.as_uuid() == org_id)
    }
}

impl sealed::Sealed for WorkspaceIncarnationClaims {}
impl OrgScope for WorkspaceIncarnationClaims {
    fn scoped_org_ids(&self) -> Vec<Uuid> {
        vec![self.org_id]
    }
}

impl sealed::Sealed for HostSourceClaims {}
impl OrgScope for HostSourceClaims {
    fn scoped_org_ids(&self) -> Vec<Uuid> {
        vec![self.org_id]
    }
}

impl sealed::Sealed for ScopedRepoStorage {}
impl OrgScope for ScopedRepoStorage {
    fn scoped_org_ids(&self) -> Vec<Uuid> {
        self.org_ids.to_vec()
    }

    fn covers_org(&self, org_id: Uuid) -> bool {
        self.org_ids.contains(&org_id)
    }
}

/// A repository named for an organization the minting caller was scoped to.
/// Only [`TenantRepoStorage::key`] makes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepoKey(g5_storage::RepoKey);

impl RepoKey {
    #[must_use]
    pub fn org_id(&self) -> Uuid {
        self.0.org_id
    }

    #[must_use]
    pub fn repo_id(&self) -> Uuid {
        self.0.repo_id
    }
}

/// Told of every repository the store hands a path out for (tests only).
#[cfg(any(test, feature = "test-helpers"))]
pub type KeyObserver = Arc<dyn Fn(&RepoKey) + Send + Sync>;

/// The repository store as every other module reaches it.
#[derive(Clone)]
pub struct TenantRepoStorage {
    backend: Arc<dyn RepoStorage>,
    /// Told of every repository a path is handed out for, so a test can
    /// count lookups or act between them.
    #[cfg(any(test, feature = "test-helpers"))]
    observer: Option<KeyObserver>,
}

impl std::fmt::Debug for TenantRepoStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantRepoStorage").finish_non_exhaustive()
    }
}

impl TenantRepoStorage {
    #[must_use]
    pub fn new(backend: Arc<dyn RepoStorage>) -> Self {
        Self {
            backend,
            #[cfg(any(test, feature = "test-helpers"))]
            observer: None,
        }
    }

    #[cfg(any(test, feature = "test-helpers"))]
    fn observe(&self, key: &RepoKey) {
        if let Some(observer) = &self.observer {
            observer(key);
        }
    }

    #[cfg(not(any(test, feature = "test-helpers")))]
    fn observe(&self, _key: &RepoKey) {}

    /// The deployment's configured store (Local or Filestore).
    pub fn from_config(
        config: &g5_storage::StorageConfig,
    ) -> Result<Self, g5_storage::StorageError> {
        Ok(Self::new(config.build()?))
    }

    /// The key of `org_id`'s repository `repo_id`, refused unless `scope`
    /// covers that organization. An unscoped handle covers none.
    pub fn key(
        scope: &(impl OrgScope + ?Sized),
        org_id: impl Into<Uuid>,
        repo_id: Uuid,
    ) -> Result<RepoKey, G5Error> {
        let org_id = org_id.into();
        if scope.covers_org(org_id) {
            Ok(RepoKey(g5_storage::RepoKey { org_id, repo_id }))
        } else {
            tracing::error!(%org_id, %repo_id, "repository of an organization outside the caller's scope");
            Err(G5Error::NotFound {
                resource: "the repository".to_string(),
            })
        }
    }

    /// This store narrowed to the organizations `scope` covers, for work
    /// that names repositories after its transaction has closed or across
    /// helpers that hold only a connection.
    #[must_use]
    pub fn scoped(&self, scope: &(impl OrgScope + ?Sized)) -> ScopedRepoStorage {
        ScopedRepoStorage {
            storage: self.clone(),
            org_ids: scope.scoped_org_ids().into(),
        }
    }

    /// The bare repository's directory.
    pub fn path(&self, key: &RepoKey) -> Result<PathBuf, G5Error> {
        self.observe(key);
        self.backend
            .local_path(&key.0)
            .ok_or_else(|| G5Error::Internal {
                message: format!(
                    "Repository {} has no materialized Git storage",
                    key.repo_id()
                ),
            })
    }

    /// [`Self::key`] then [`Self::path`].
    pub fn repo_path(
        &self,
        scope: &(impl OrgScope + ?Sized),
        org_id: impl Into<Uuid>,
        repo_id: Uuid,
    ) -> Result<PathBuf, G5Error> {
        self.path(&Self::key(scope, org_id, repo_id)?)
    }

    /// The directory of the repository's review artifacts, outside it.
    #[must_use]
    pub fn artifact_dir(&self, key: &RepoKey) -> Option<PathBuf> {
        self.observe(key);
        self.backend.review_artifact_dir(&key.0)
    }

    /// Create the repository's directory (and its organization's).
    pub async fn init_repo(&self, key: &RepoKey) -> Result<PathBuf, G5Error> {
        self.observe(key);
        self.backend
            .init_repo(&key.0)
            .await
            .map_err(|error| G5Error::Internal {
                message: format!("Failed to initialize repository storage: {error}"),
            })
    }

    /// A test store on a local directory.
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn local_for_test(base_dir: PathBuf) -> Self {
        Self::from_config(&g5_storage::StorageConfig::local(base_dir))
            .expect("local test repository storage")
    }

    /// This store, telling `observer` of every repository it hands a path
    /// out for.
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn observed_for_test(&self, observer: KeyObserver) -> Self {
        Self {
            backend: self.backend.clone(),
            observer: Some(observer),
        }
    }

    /// A test's path to a fixture repository: still keyed through
    /// [`Self::key`], for the organization the fixture names.
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn test_path(&self, org_id: impl Into<Uuid>, repo_id: Uuid) -> PathBuf {
        let org_id = org_id.into();
        self.repo_path(&TestOrgScope(org_id), org_id, repo_id)
            .expect("fixture repository path")
    }

    /// A test's key for a fixture repository, as [`Self::test_path`].
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn test_key(org_id: impl Into<Uuid>, repo_id: Uuid) -> RepoKey {
        let org_id = org_id.into();
        Self::key(&TestOrgScope(org_id), org_id, repo_id).expect("fixture repository key")
    }
}

/// The repository store narrowed to one verified scope's organizations; only
/// [`TenantRepoStorage::scoped`] makes one. It dereferences to the store, so
/// it also serves reads through keys minted earlier.
#[derive(Clone, Debug)]
pub struct ScopedRepoStorage {
    storage: TenantRepoStorage,
    org_ids: Arc<[Uuid]>,
}

impl ScopedRepoStorage {
    /// [`TenantRepoStorage::key`] within this scope.
    pub fn key(&self, org_id: impl Into<Uuid>, repo_id: Uuid) -> Result<RepoKey, G5Error> {
        TenantRepoStorage::key(self, org_id, repo_id)
    }

    /// [`TenantRepoStorage::repo_path`] within this scope.
    pub fn repo_path(&self, org_id: impl Into<Uuid>, repo_id: Uuid) -> Result<PathBuf, G5Error> {
        self.storage.path(&self.key(org_id, repo_id)?)
    }
}

impl std::ops::Deref for ScopedRepoStorage {
    type Target = TenantRepoStorage;

    fn deref(&self) -> &TenantRepoStorage {
        &self.storage
    }
}

/// A test capability for one fixture organization.
#[cfg(any(test, feature = "test-helpers"))]
pub struct TestOrgScope(pub Uuid);

#[cfg(any(test, feature = "test-helpers"))]
impl sealed::Sealed for TestOrgScope {}

#[cfg(any(test, feature = "test-helpers"))]
impl OrgScope for TestOrgScope {
    fn scoped_org_ids(&self) -> Vec<Uuid> {
        vec![self.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage() -> (tempfile::TempDir, TenantRepoStorage) {
        let tmp = tempfile::tempdir().unwrap();
        let storage = TenantRepoStorage::local_for_test(tmp.path().join("repos"));
        (tmp, storage)
    }

    #[test]
    fn refuses_an_organization_outside_the_scope() {
        let mine = Uuid::new_v4();
        let theirs = Uuid::new_v4();
        let repo = Uuid::new_v4();
        let scope = TestOrgScope(mine);
        assert!(TenantRepoStorage::key(&scope, mine, repo).is_ok());
        assert!(matches!(
            TenantRepoStorage::key(&scope, theirs, repo),
            Err(G5Error::NotFound { .. })
        ));
    }

    #[test]
    fn verified_credentials_cover_only_their_organization() {
        let org = Uuid::new_v4();
        let other = Uuid::new_v4();
        let claims = HostSourceClaims {
            org_id: org,
            host_id: Uuid::new_v4(),
            source_workspace_id: Uuid::new_v4(),
            actor_user_id: Uuid::new_v4(),
            repo_ids: Vec::new(),
            membership_digest: String::new(),
            iat: 0,
            exp: 0,
            iss: None,
            aud: String::new(),
            jti: String::new(),
            token_use: String::new(),
        };
        assert!(claims.covers_org(org));
        assert!(!claims.covers_org(other));
    }

    #[tokio::test]
    async fn an_in_scope_organization_does_not_reach_another_tenants_repository() {
        let (_tmp, storage) = storage();
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();
        let repo = Uuid::new_v4();
        let dir = storage
            .init_repo(&TenantRepoStorage::test_key(owner, repo))
            .await
            .unwrap();
        assert!(dir.ends_with(format!("{owner}/{repo}")));

        // `{other}/{repo-of-owner}` is a directory that does not exist.
        let crossed = storage.test_path(other, repo);
        assert_ne!(crossed, dir);
        assert!(!crossed.exists());
    }
}
