# Distributed Filesystem Migration — Design Plan

> Moves G5 from shared Docker volumes to a distributed filesystem,
> enabling independent-host operation and GCP deployment.

## Current repository layout (#1079)

The sections below are the original plan; this note is authoritative for the
on-disk layout. Every repository-storage path is named by organization and
repository id. With `base` = `G5_STORAGE_BASE_DIR` (Local, `/data/repos`) or
`{mount_point}/{tenant_prefix}` (Filestore, `/mnt/g5-repos/repos`), a bare
repository lives at `{base}/{org_id}/{repo_id}/`, its review artifacts at
`{base}/{org_id}/.review-artifacts/{repo_id}`, and the Filestore writer lock at
`{base}/{org_id}/.{repo_id}.lock`; `{base}/.worktrees` (and the attachment
store's `{base}/.attachments`, once G5-Lean#1053 lands) is not per-repository.
Every repository an organization stores, with its artifacts and lock, lives
under `{base}/{org_id}`. A G5-hosted repository's `repos.git_url` is
`g5-local://{org-slug}/{repo-slug}` and names no disk path.

Only one holder in g5-api-core reaches the store, and only through a key minted
for an organization the caller is scoped to (`ScopedDb`, its reopener, or the
Git route's verified credential); an unscoped handle or an out-of-scope org is
refused as not found, and a static audit forbids raw `RepoStorage` use and
hand-built paths elsewhere. Stores written before this layout were moved once
by `scripts/migrate-repo-layout.py` (see its runbook); governing intent:
`g5.repo.storage-tenancy` and `g5.organization.tenancy`.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current State — Shared-Volume Coupling](#2-current-state)
3. [Work Item 1 — Storage Abstraction Layer](#3-storage-abstraction-layer)
4. [Work Item 2 — Configurable FS Providers](#4-configurable-fs-providers)
5. [Work Item 3 — Indexer Process](#5-indexer-process)
6. [Work Item 4 — Stubbed Operations](#6-stubbed-operations)
7. [Work Item 5 — Manifest Service](#7-manifest-service)
8. [Work Item 6 — Multi-Host Audit](#8-multi-host-audit)
9. [Work Item 7 — GCP Deployment](#9-gcp-deployment)
10. [Implementation Order & Dependencies](#10-implementation-order)
11. [Migration Strategy](#11-migration-strategy)
12. [Risk Register](#12-risk-register)

---

## 1. Executive Summary

G5's API and workers currently share a single Docker volume (`api_repos` → `/data/repos`).
The API reads file trees and content from this path; workers clone, commit, and push to it.
This coupling makes multi-host deployment impossible — both processes must see the same
filesystem.

This plan introduces a **`RepoStorage` trait** that abstracts all filesystem access behind
two provider implementations (local FS, Google Filestore/NFS), adds a **manifest service**
so workers can pull workspace definitions from the API over HTTP, creates an **indexer
process** for async repo maintenance, and concludes with a **GCP deployment** across
dev/staging/prod environments.

**Scope**: 7 work items, estimated 3–4 weeks of focused implementation.

---

## 2. Current State — Shared-Volume Coupling {#2-current-state}

### 2.1 The Coupling Mechanism

g5-api's `TaskService` constructs a **filesystem path** and sends it to workers:

```
task_service.rs L363-368:
  workspace_git_url = "{G5_REPOS_BASE_DIR}/{repo_id}"
```

Workers receive this path in `TaskAssignment.workspace_git_url` via the poll response,
then treat it as a local directory. This only works when both processes share a volume.

### 2.2 All 12 Coupling Points

| # | Component | File | Operation | R/W |
|---|-----------|------|-----------|-----|
| 1 | g5-api | `config.rs:148-150` | Declares `repos_base_dir`, default `/data/repos` | CONFIG |
| 2 | g5-api | `task_service.rs:363-368` | Constructs `workspace_git_url` as FS path for workers | PATH GEN |
| 3 | g5-api | `workspace_service.rs:1723-1731` | `list_file_tree()` — reads dir tree from disk | READ |
| 4 | g5-api | `workspace_service.rs:1754-1759` | `get_file_content()` — reads file bytes | READ |
| 5 | g5-api | `ontology_service.rs:44-82` | `read_file_content()` from repo dir | READ |
| 6 | g5-api | `ontology_service.rs:84-109` | `write_file_content()` to repo dir | WRITE |
| 7 | g5-api | `ontology_service.rs:721` | `get_node_diff()` — runs `git diff` in repo dir | READ |
| 8 | g5-api | `draft_service.rs:684-689` | `commit_draft()` — materializes CRDT files → git | R/W |
| 9 | g5-api | `doc_service.rs:202` | `get_doc()` — reads doc content | READ |
| 10 | g5-api | `search_service.rs:583-584` | `reindex_workspace_branch()` — reads files for indexing | READ |
| 11 | g5d | `worker/mod.rs:195-211` | `resolve_task_workspace_path()` — checks if `workspace_git_url` path exists | READ |
| 12 | g5d | `worker/lifecycle.rs:440-480` | `phase_collect()` — `git add` + `git commit` in shared dir | WRITE |

### 2.3 Docker Volume Layout

```
dev/test compose:
  api_repos:/data/repos     → shared by g5-api + g5d-worker
  worker_workspace:/workspace → g5d-worker only

production compose:
  No volume mounts (deferred to K8s — currently broken)
```

---

## 3. Work Item 1 — Storage Abstraction Layer {#3-storage-abstraction-layer}

### 3.1 Design: `RepoStorage` Trait

Define a trait in `g5-common` that abstracts all repo filesystem operations:

```rust
// g5-common/src/storage/mod.rs

use std::path::{Path, PathBuf};
use async_trait::async_trait;
use bytes::Bytes;

/// Metadata about a file or directory entry.
#[derive(Debug, Clone)]
pub struct StorageEntry {
    pub path: String,          // relative to repo root
    pub is_dir: bool,
    pub size: Option<u64>,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// Abstraction over repository storage backends.
///
/// All paths are relative to the repo root (`{repo_id}/`).
/// Implementations handle the mapping to physical storage.
#[async_trait]
pub trait RepoStorage: Send + Sync + 'static {
    /// List entries in a directory, up to `depth` levels deep.
    async fn list_tree(
        &self,
        repo_id: &str,
        path: &str,
        depth: u32,
    ) -> Result<Vec<StorageEntry>, StorageError>;

    /// Read file content as bytes.
    async fn read_file(
        &self,
        repo_id: &str,
        path: &str,
    ) -> Result<Bytes, StorageError>;

    /// Write file content (creates intermediate dirs).
    async fn write_file(
        &self,
        repo_id: &str,
        path: &str,
        content: &[u8],
    ) -> Result<(), StorageError>;

    /// Check if a path exists.
    async fn exists(
        &self,
        repo_id: &str,
        path: &str,
    ) -> Result<bool, StorageError>;

    /// Delete a file.
    async fn delete_file(
        &self,
        repo_id: &str,
        path: &str,
    ) -> Result<(), StorageError>;

    /// Get the local filesystem path for a repo.
    /// Returns `Some(path)` for local/NFS backends, `None` for object stores.
    /// Used by code that needs direct git access (clone, worktree, diff).
    fn local_path(&self, repo_id: &str) -> Option<PathBuf>;

    /// Initialize storage for a new repo (create directories, etc.)
    async fn init_repo(&self, repo_id: &str) -> Result<PathBuf, StorageError>;

    /// Remove all storage for a repo.
    async fn remove_repo(&self, repo_id: &str) -> Result<(), StorageError>;
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("path traversal blocked: {0}")]
    PathTraversal(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage backend error: {0}")]
    Backend(String),
}
```

### 3.2 Why a Trait, Not object_store

The `object_store` crate (Apache Arrow) is an object-store abstraction (GET/PUT/LIST on
opaque blobs). It does **not** support:
- POSIX filesystem semantics (git needs `open()`, `read()`, `write()`, `stat()`)
- Git worktrees (require a real filesystem)
- `git2::Repository::open()` (requires a path)

G5 workers run `git clone`, `git add`, `git commit` — operations that require a POSIX
filesystem. The abstraction must therefore expose both:
1. Content operations (`read_file`, `write_file`, `list_tree`) for API reads
2. A `local_path()` method for git operations

For a **local FS** or **NFS/Filestore** backend, `local_path()` returns the mount path.
For a hypothetical future **object store** backend, `local_path()` would return `None` and
git operations would need to use a local cache + sync strategy (out of scope for now).

### 3.3 Git Diff and Commit Operations

Several coupling points involve git operations (`git diff`, `git add`, `git commit`).
These cannot be abstracted behind a content API — they need a real working tree.

Strategy: **Both providers expose `local_path()`**. Local FS returns a direct path;
Google Filestore returns the NFS mount path. Git operations use `local_path()` directly.

For the future (object-store only providers), a `GitWorktreeCache` would:
1. Maintain a local checkout per repo
2. Pull on read, push on write
3. This is explicitly out of scope for this migration

### 3.4 Crate Placement

```
g5-common/src/
  storage/
    mod.rs          ← RepoStorage trait, StorageError, StorageEntry
    local.rs        ← LocalRepoStorage implementation
    filestore.rs    ← FilestoreRepoStorage (NFS mount) implementation
    config.rs       ← StorageConfig enum (Local | Filestore)
```

Feature-gated: `storage-local` (default), `storage-filestore`.

---

## 4. Work Item 2 — Configurable FS Providers {#4-configurable-fs-providers}

### 4.1 Provider A: Local Filesystem

```rust
// g5-common/src/storage/local.rs

pub struct LocalRepoStorage {
    base_dir: PathBuf,  // e.g. /data/repos
}

#[async_trait]
impl RepoStorage for LocalRepoStorage {
    async fn list_tree(&self, repo_id: &str, path: &str, depth: u32)
        -> Result<Vec<StorageEntry>, StorageError>
    {
        let root = self.repo_dir(repo_id);
        let target = root.join(path);
        self.validate_path(&root, &target)?;
        self.walk_dir(&root, &target, depth).await
    }

    async fn read_file(&self, repo_id: &str, path: &str)
        -> Result<Bytes, StorageError>
    {
        let full = self.repo_dir(repo_id).join(path);
        self.validate_path(&self.repo_dir(repo_id), &full)?;
        tokio::fs::read(&full).await
            .map(Bytes::from)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => StorageError::NotFound(path.into()),
                _ => StorageError::Io(e),
            })
    }

    fn local_path(&self, repo_id: &str) -> Option<PathBuf> {
        Some(self.base_dir.join(repo_id))
    }

    // ... write_file, exists, delete_file, init_repo, remove_repo
}
```

This is a thin wrapper around the current inline `repos_base_dir` logic, with path
traversal protection extracted into `validate_path()`.

### 4.2 Provider B: Google Filestore (NFS)

Google Filestore provides a managed NFS share. From the application's perspective it
looks identical to a local filesystem — the OS mounts it at a path like `/mnt/g5-repos`.

```rust
// g5-common/src/storage/filestore.rs

pub struct FilestoreRepoStorage {
    /// NFS mount point, e.g. /mnt/g5-repos
    mount_point: PathBuf,
    /// Tenant prefix for multi-tenancy, e.g. "tenants/acme"
    tenant_prefix: Option<String>,
}

#[async_trait]
impl RepoStorage for FilestoreRepoStorage {
    // Identical to LocalRepoStorage but with:
    // - base_dir = mount_point / tenant_prefix
    // - health check verifies NFS mount is live (statfs)
    // - optional file locking via flock() for concurrent writer safety

    fn local_path(&self, repo_id: &str) -> Option<PathBuf> {
        let mut path = self.mount_point.clone();
        if let Some(prefix) = &self.tenant_prefix {
            path = path.join(prefix);
        }
        Some(path.join(repo_id))
    }
    // ...
}
```

Since both providers expose POSIX paths, the implementation is almost identical. The
Filestore provider adds:
- **Health checking**: `statfs()` on mount point at startup and periodically
- **Tenant prefixing**: `{mount_point}/tenants/{tenant_id}/repos/{repo_id}/`
- **NFS advisory locks**: `flock()` on repo dirs when multiple API instances write

### 4.3 Configuration

```rust
// g5-common/src/storage/config.rs

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum StorageConfig {
    /// Local filesystem (dev, single-node).
    Local {
        base_dir: PathBuf,  // default: /data/repos
    },
    /// Google Cloud Filestore (NFS mount).
    Filestore {
        mount_point: PathBuf,       // e.g. /mnt/g5-repos
        tenant_prefix: Option<String>,
    },
}

impl StorageConfig {
    pub fn build(&self) -> Arc<dyn RepoStorage> {
        match self {
            Self::Local { base_dir } =>
                Arc::new(LocalRepoStorage::new(base_dir.clone())),
            Self::Filestore { mount_point, tenant_prefix } =>
                Arc::new(FilestoreRepoStorage::new(
                    mount_point.clone(),
                    tenant_prefix.clone(),
                )),
        }
    }
}
```

Environment variable mapping:

| Env Var | Default | Description |
|---------|---------|-------------|
| `G5_STORAGE_PROVIDER` | `local` | `local` or `filestore` |
| `G5_STORAGE_BASE_DIR` | `/data/repos` | Local provider base dir |
| `G5_STORAGE_MOUNT_POINT` | — | Filestore NFS mount point |
| `G5_STORAGE_TENANT_PREFIX` | — | Multi-tenant path prefix |

### 4.4 Injection into Services

Replace `repos_base_dir: PathBuf` with `storage: Arc<dyn RepoStorage>` in all services:

```rust
// Before (workspace_service.rs)
pub struct WorkspaceService {
    db: PgPool,
    repos_base_dir: PathBuf,
    // ...
}

// After
pub struct WorkspaceService {
    db: PgPool,
    storage: Arc<dyn RepoStorage>,
    // ...
}
```

**Services to update** (8 files, ~40 call sites):

| Service | Current field | Operations to migrate |
|---------|--------------|----------------------|
| `WorkspaceService` | `repos_base_dir` | `list_file_tree`, `get_file_content` |
| `OntologyService` | `repos_base_dir` | `read_file_content`, `write_file_content`, `get_node_diff` |
| `DraftService` | `repos_base_dir` | `commit_draft` (git commit needs `local_path()`) |
| `DocService` | `repos_base_dir` | `get_doc` |
| `SearchService` | `repos_base_dir` | `reindex_workspace_branch` |
| `TaskService` | `repos_base_dir` | `workspace_git_url` construction |
| `services/mod.rs` | config wiring | Build `Arc<dyn RepoStorage>` once, inject everywhere |
| `config.rs` | `ReposConfig` | Replace with `StorageConfig` |

---

## 5. Work Item 3 — Indexer Process {#5-indexer-process}

### 5.1 Purpose

A background process that periodically:
1. Checks for new commits on remotes (fetch)
2. Rebases local repos onto upstream changes
3. Triggers re-indexing (search, ontology refresh)
4. Prunes stale worktrees

### 5.2 Architecture

The indexer runs as a **separate Tokio task set inside g5-api**, not a separate binary.
It uses the existing `MaintenanceService` framework (which already runs CRDT compaction,
stale draft cleanup, etc.)

```rust
// g5-api/src/services/indexer_service.rs

pub struct IndexerService {
    db: PgPool,
    storage: Arc<dyn RepoStorage>,
    redis: RedisClient,
    config: IndexerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexerConfig {
    /// How often to poll remotes for new commits (default: 60s)
    pub poll_interval: Duration,
    /// Max repos to process per tick (default: 10)
    pub batch_size: usize,
    /// Whether to auto-rebase on new upstream commits (default: true)
    pub auto_rebase: bool,
    /// Stale worktree cleanup interval (default: 30min)
    pub worktree_gc_interval: Duration,
}
```

### 5.3 Indexer Loop

```
Every poll_interval:
  1. SELECT repos WHERE last_fetched_at < now() - poll_interval
     ORDER BY last_fetched_at ASC LIMIT batch_size
     FOR UPDATE SKIP LOCKED

  2. For each repo:
     a. git fetch origin (via local_path)
     b. Compare local HEAD with origin/main
     c. If diverged:
        - If auto_rebase: git rebase origin/main
        - Record new commit SHA in workspace_branches table
        - Enqueue affected file paths into search_reindex_queue
        - Emit EventBus notification (WebSocket) for UI refresh
     d. UPDATE repos SET last_fetched_at = now()

  3. Redis pub/sub: notify connected clients of file tree changes

Every worktree_gc_interval:
  4. List all task worktrees on disk
  5. Compare against active tasks in DB
  6. Remove worktrees for completed/failed tasks
```

### 5.4 Schema Additions

```sql
ALTER TABLE workspace_branches
  ADD COLUMN last_fetched_at TIMESTAMPTZ DEFAULT now(),
  ADD COLUMN upstream_head   TEXT,  -- latest known remote HEAD SHA
  ADD COLUMN local_head      TEXT;  -- local HEAD SHA after fetch/rebase
```

### 5.5 Search Queue Producer (Fills Existing Gap)

The indexer also fills the **search indexing gap** identified in §21 of the architecture
discussion — the `search_reindex_queue` table exists but nothing writes to it.

```rust
impl IndexerService {
    /// Called after a fetch detects new commits.
    async fn enqueue_changed_files(
        &self,
        workspace_branch_id: Uuid,
        changed_paths: &[String],
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO search_reindex_queue (workspace_branch_id, file_path, queued_at)
            SELECT $1, unnest($2::text[]), now()
            ON CONFLICT (workspace_branch_id, file_path) DO UPDATE
                SET queued_at = now()
            "#,
            workspace_branch_id,
            changed_paths,
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
```

### 5.6 Leader Election

When running multiple g5-api replicas, only one should run the indexer to avoid
duplicate fetches. Use **Postgres advisory locks**:

```rust
async fn try_become_indexer_leader(db: &PgPool) -> bool {
    sqlx::query_scalar!(
        "SELECT pg_try_advisory_lock(42) AS acquired"  // 42 = indexer lock ID
    )
    .fetch_one(db)
    .await
    .unwrap_or(false)
}
```

The lock is released when the connection drops (on crash/shutdown), allowing another
replica to acquire it.

---

## 6. Work Item 4 — Stubbed Operations {#6-stubbed-operations}

### 6.1 Current State

The codebase has very few `todo!()`/`unimplemented!()` stubs. The "stubbed operations"
are better described as **missing features** — the API surfaces exist but the underlying
workflows are incomplete.

### 6.2 Operations to Implement

#### 6.2.1 Add Repo (`POST /api/v1/workspaces/:id/repos`)

Currently: No endpoint exists. Repos are seeded via direct SQL.

Implementation:
```rust
pub async fn add_repo(
    &self,
    workspace_id: Uuid,
    request: AddRepoRequest,
) -> Result<WorkspaceBranch, G5Error> {
    // 1. Validate git URL is reachable (git ls-remote)
    // 2. INSERT INTO workspace_branches (workspace_id, repo_url, name, ...)
    // 3. storage.init_repo(repo_id) — create directory
    // 4. git clone --mirror {url} → storage.local_path(repo_id)
    // 5. Enqueue initial indexing job (ontology seed + search index)
    // 6. Emit EventBus notification
}

#[derive(Deserialize)]
pub struct AddRepoRequest {
    pub git_url: String,
    pub name: Option<String>,         // default: inferred from URL
    pub default_branch: Option<String>, // default: "main"
    pub credentials: Option<RepoCredentials>,
}
```

#### 6.2.2 Remove Repo (`DELETE /api/v1/workspaces/:id/repos/:repoId`)

```rust
pub async fn remove_repo(
    &self,
    workspace_id: Uuid,
    repo_id: Uuid,
) -> Result<(), G5Error> {
    // 1. Verify no active jobs/tasks reference this repo
    // 2. Cancel any pending jobs for this repo
    // 3. DELETE ontology nodes WHERE workspace_branch_id = branch.id
    // 4. DELETE workspace_branches WHERE id = branch.id
    // 5. storage.remove_repo(repo_id)
    // 6. Remove from search index
}
```

#### 6.2.3 Create Project (`POST /api/v1/orgs/:orgId/workspaces`)

Currently: `WorkspaceService::create_workspace()` exists but is minimal. No repo
initialization, no default ontology, no template support.

Enhancement:
```rust
pub async fn create_workspace(
    &self,
    org_id: Uuid,
    request: CreateWorkspaceRequest,
) -> Result<Workspace, G5Error> {
    // 1. INSERT workspace row
    // 2. For each repo in request.repos:
    //    a. self.add_repo(workspace.id, repo)
    // 3. Create default workspace branch for first repo
    // 4. If request.template is specified:
    //    a. Clone from template workspace (existing clone_service)
    // 5. Generate initial manifest (see §7)
}

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub repos: Vec<AddRepoRequest>,
    pub template_workspace_id: Option<Uuid>,
}
```

#### 6.2.4 Fork Workspace (`POST /api/v1/workspaces/:id/fork`)

Currently: `CloneService::clone_workspace()` copies DB rows but doesn't handle storage.

Enhancement:
```rust
pub async fn fork_workspace(
    &self,
    source_id: Uuid,
    org_id: Uuid,
    name: String,
) -> Result<Workspace, G5Error> {
    // 1. Clone DB rows (existing clone_service logic)
    // 2. For each repo in source workspace:
    //    a. COW clone: storage.local_path(source_repo) → storage.local_path(new_repo)
    //    b. Use cp --reflink=auto for XFS/Btrfs, cp -c for APFS
    //    c. Fallback: full cp for NFS (Filestore doesn't support reflinks)
    // 3. Update git remotes in new repo to point to source repo
}
```

### 6.3 Route Registration

```rust
// g5-api/src/routes/workspace.rs — new endpoints
.route("/api/v1/workspaces/:workspaceId/repos", post(add_repo))
.route("/api/v1/workspaces/:workspaceId/repos/:repoId", delete(remove_repo))
.route("/api/v1/workspaces/:workspaceId/fork", post(fork_workspace))
```

---

## 7. Work Item 5 — Manifest Service {#7-manifest-service}

### 7.1 Purpose

A manifest describes a workspace's complete configuration — repos, branches, ontology
structure, credentials — in a format that workers can consume to bootstrap a local
workspace without needing shared filesystem access.

### 7.2 Manifest Schema

```rust
// g5-common/src/manifest.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub version: u32,                      // schema version (1)
    pub workspace_id: Uuid,
    pub workspace_name: String,
    pub org_id: Uuid,
    pub repos: Vec<RepoManifest>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoManifest {
    pub repo_id: Uuid,
    pub branch_id: Uuid,
    pub name: String,
    pub git_url: String,                   // clone URL (HTTPS)
    pub default_branch: String,            // "main"
    pub head_commit: String,               // current HEAD SHA
    pub credential_ref: Option<String>,    // credential ID for auth
}
```

### 7.3 API Endpoints

```rust
// g5-api/src/routes/manifest.rs

/// Public: get workspace manifest (for g5d clone)
/// GET /api/v1/workspaces/:id/manifest
async fn get_manifest(
    auth: Auth,
    Path(workspace_id): Path<Uuid>,
    State(svc): State<ManifestService>,
) -> Result<Json<WorkspaceManifest>, G5Error> {
    svc.get_manifest(workspace_id).await.map(Json)
}

/// Internal: worker pulls manifest for task workspace
/// GET /api/internal/workspaces/:id/manifest
async fn get_manifest_internal(
    service_auth: ServiceAuth,
    Path(workspace_id): Path<Uuid>,
    State(svc): State<ManifestService>,
) -> Result<Json<WorkspaceManifest>, G5Error> {
    svc.get_manifest(workspace_id).await.map(Json)
}
```

### 7.4 ManifestService Implementation

```rust
// g5-api/src/services/manifest_service.rs

pub struct ManifestService {
    db: PgPool,
}

impl ManifestService {
    pub async fn get_manifest(&self, workspace_id: Uuid) -> Result<WorkspaceManifest> {
        let workspace = sqlx::query_as!(Workspace,
            "SELECT * FROM workspaces WHERE id = $1", workspace_id
        ).fetch_one(&self.db).await?;

        let branches = sqlx::query_as!(WorkspaceBranch,
            "SELECT * FROM workspace_branches WHERE workspace_id = $1", workspace_id
        ).fetch_all(&self.db).await?;

        Ok(WorkspaceManifest {
            version: 1,
            workspace_id,
            workspace_name: workspace.name,
            org_id: workspace.org_id,
            repos: branches.into_iter().map(|b| RepoManifest {
                repo_id: b.id, // repo_id and branch_id currently same
                branch_id: b.id,
                name: b.name,
                git_url: b.repo_url,
                default_branch: b.default_branch,
                head_commit: b.head_commit.unwrap_or_default(),
                credential_ref: b.credential_id.map(|c| c.to_string()),
            }).collect(),
            created_at: workspace.created_at,
            updated_at: workspace.updated_at,
        })
    }
}
```

### 7.5 InternalApiClient Extension

Add manifest fetch to the worker's API client:

```rust
// g5-common/src/internal_api/client.rs

impl InternalApiClient {
    pub async fn get_workspace_manifest(
        &self,
        workspace_id: Uuid,
    ) -> Result<WorkspaceManifest> {
        let url = format!(
            "{}/api/internal/workspaces/{}/manifest",
            self.base_url, workspace_id
        );
        let resp = self.client.get(&url)
            .bearer_auth(&self.service_token)
            .send().await?;
        self.handle_response(resp).await
    }
}
```

### 7.6 Worker Workspace Provisioning (The Key Change)

Currently, `TaskAssignment.workspace_git_url` contains a filesystem path. After this
migration, it will contain either:
- A **git remote URL** (e.g., `https://github.com/org/repo.git`) when workers clone
  independently, OR
- A **filesystem path** to the shared NFS mount (when using Filestore)

The worker's `resolve_task_workspace_path()` logic changes:

```rust
// g5d/src/worker/mod.rs — AFTER migration

async fn resolve_task_workspace_path(
    &self,
    assignment: &TaskAssignment,
) -> Result<PathBuf> {
    let workspace_path = &self.config.workspace_path; // /workspace

    // Check if workspace_git_url is a local path (shared FS mode)
    let url = &assignment.workspace_git_url;
    if Path::new(url).exists() {
        return Ok(PathBuf::from(url));
    }

    // Remote mode: clone from git URL into worker-local storage
    let repo_dir = workspace_path
        .join(&assignment.workspace_id.to_string())
        .join(&assignment.repo_id.to_string());

    if repo_dir.exists() {
        // Already cloned — fetch latest
        let repo = git2::Repository::open(&repo_dir)?;
        self.git_manager.fetch(&repo, "origin").await?;
    } else {
        // Fetch manifest for credential info
        let manifest = self.api_client
            .get_workspace_manifest(assignment.workspace_id)
            .await?;

        let repo_manifest = manifest.repos.iter()
            .find(|r| r.repo_id == assignment.repo_id)
            .ok_or_else(|| anyhow!("repo not in manifest"))?;

        // Clone from git remote
        self.git_manager.clone_repo(
            &repo_manifest.git_url,
            &repo_dir,
            repo_manifest.credential_ref.as_deref(),
        ).await?;
    }

    Ok(repo_dir)
}
```

### 7.7 workspace_git_url Semantics Change

| Field | Before | After |
|-------|--------|-------|
| `workspace_git_url` | FS path: `/data/repos/{repo_id}` | Git URL: `https://...` or NFS path |
| Resolution | Direct path access | Clone if remote, direct if local |

In `task_service.rs`, replace the path construction:

```rust
// Before
let workspace_git_url = format!("{}/{}", repos_base_dir, repo_id);

// After — use the repo's actual git URL from workspace_branches table
let workspace_git_url = branch.repo_url.clone();
```

---

## 8. Work Item 6 — Multi-Host Audit {#8-multi-host-audit}

### 8.1 Shared-FS Assumptions to Eliminate

After implementing work items 1–5, these coupling points are resolved:

| # | Coupling Point | Resolution |
|---|---------------|------------|
| 1 | `config.rs` `ReposConfig` | Replaced by `StorageConfig` |
| 2 | `task_service.rs` FS path in `workspace_git_url` | Now sends git URL instead |
| 3-4 | `workspace_service.rs` file tree/content | Uses `RepoStorage` trait |
| 5-6 | `ontology_service.rs` read/write | Uses `RepoStorage` trait |
| 7 | `ontology_service.rs` git diff | Uses `storage.local_path()` |
| 8 | `draft_service.rs` git commit | Uses `storage.local_path()` |
| 9 | `doc_service.rs` read | Uses `RepoStorage` trait |
| 10 | `search_service.rs` read | Uses `RepoStorage` trait |
| 11 | `worker/mod.rs` path resolution | Clones from remote or uses NFS path |
| 12 | `worker/lifecycle.rs` git commit | Works on worker-local clone |

### 8.2 Remaining Multi-Host Concerns

#### 8.2.1 Worker → API Git Sync (Commit Propagation)

**Problem**: When a worker makes commits on a task branch, the API needs to see them.
Currently this happens via shared volume (instant visibility). With independent hosts:

**Solution**: Workers `git push` to the remote after committing, then notify the API:

```
Worker:                          API:
 1. git commit -m "task result"
 2. git push origin task/{id}
 3. POST /api/internal/tasks/{id}/transition
    { status: "completed", commit_sha: "abc123" }
                                  4. git fetch origin task/{id}
                                  5. Read committed files from local repo
```

The `lifecycle.rs` `phase_collect()` already does `git commit`. It needs to also
`git push` before calling `transition_task()`. The API needs its own independent repo
clone to `git fetch` into.

#### 8.2.2 API Needs Its Own Repo Clones

**Problem**: The API currently reads file content from the shared volume. With
independent hosts, the API needs its own filesystem copies.

**Solution**: Use `RepoStorage` (local or Filestore) on the API side. The indexer
(work item 3) keeps the API's clones up to date via `git fetch`.

#### 8.2.3 CRDT Draft Commit

The `draft_service.rs` materializes CRDT dirty files to the repo working tree and
commits. This requires the API to have a writable repo checkout. With `RepoStorage`,
the API writes via `storage.write_file()` then git commits via `storage.local_path()`.

This works identically for both Local and Filestore providers.

#### 8.2.4 Real-Time File Content After Worker Commits

**Problem**: After a worker completes a task and pushes, the API must surface the
new file content immediately (not wait for the next indexer tick).

**Solution**: The `transition_task(Completed)` handler triggers an immediate
`git fetch origin task/{task_id}` on the API's local clone. Add to `TaskService`:

```rust
async fn on_task_completed(&self, task_id: Uuid, commit_sha: &str) {
    // 1. Fetch the task branch from remote
    let repo_path = self.storage.local_path(&task.repo_id.to_string())
        .expect("local_path required for git fetch");
    git_fetch_branch(&repo_path, &format!("task/{}", task_id)).await?;

    // 2. Invalidate Redis content cache for affected files
    self.redis.del_pattern(&format!("content:{}:*", task.repo_id)).await?;

    // 3. Enqueue search re-index for changed files
    self.indexer.enqueue_changed_files(task.branch_id, &changed_paths).await?;
}
```

### 8.3 Audit Checklist

| Area | Status | Notes |
|------|--------|-------|
| g5-api file tree serving | ✅ via RepoStorage | No shared FS needed |
| g5-api file content serving | ✅ via RepoStorage | No shared FS needed |
| g5-api ontology file reads | ✅ via RepoStorage | No shared FS needed |
| g5-api ontology file writes | ✅ via RepoStorage | No shared FS needed |
| g5-api git diff | ✅ via local_path() | Works on local/NFS |
| g5-api draft commit | ✅ via local_path() | Works on local/NFS |
| g5-api search indexing | ✅ via RepoStorage | No shared FS needed |
| g5d workspace resolution | ✅ clone from remote | Independent host |
| g5d git commit | ✅ local worktree | Then git push |
| g5d → g5-api commit sync | ✅ git push + notify | See §8.2.1 |
| g5-api → client file tree | ✅ via RepoStorage | Immediate after fetch |
| Redis (Memorystore) | ✅ already network | No FS coupling |
| PostgreSQL (Cloud SQL) | ✅ already network | No FS coupling |
| WebSocket pub/sub | ✅ already via Redis | No FS coupling |
| Worker health/heartbeat | ✅ already HTTP | No FS coupling |

---

## 9. Work Item 7 — GCP Deployment {#9-gcp-deployment}

### 9.1 Architecture Overview

```
┌──────────────────────────────────────────────────────┐
│                    GCP Project                        │
│                                                       │
│  ┌──────────┐   ┌──────────────┐   ┌──────────────┐  │
│  │  Cloud   │   │   GKE        │   │  Cloud SQL   │  │
│  │  LB /    │──▶│   Cluster    │──▶│  Postgres 17 │  │
│  │  CDN     │   │              │   │  + pgvector   │  │
│  └──────────┘   │  ┌────────┐  │   └──────────────┘  │
│                  │  │g5-api  │  │                      │
│                  │  │(×3)    │  │   ┌──────────────┐  │
│                  │  └────────┘  │   │ Memorystore  │  │
│                  │  ┌────────┐  │──▶│ Redis 7      │  │
│                  │  │g5d     │  │   └──────────────┘  │
│                  │  │worker  │  │                      │
│                  │  │(×N)    │  │   ┌──────────────┐  │
│                  │  └────────┘  │   │  Filestore   │  │
│                  │  ┌────────┐  │──▶│  (NFS)       │  │
│                  │  │caddy   │  │   │  /repos      │  │
│                  │  │edge    │  │   └──────────────┘  │
│                  │  └────────┘  │                      │
│                  └──────────────┘   ┌──────────────┐  │
│                                     │  GCS         │  │
│                                     │  (caches,    │  │
│                                     │   snapshots) │  │
│                                     └──────────────┘  │
└──────────────────────────────────────────────────────┘
```

### 9.2 Filestore Setup

Google Filestore provides managed NFS with:
- **Basic tier**: 1 TB minimum, up to 63.9 TB, single-zone
- **Enterprise tier**: Regional HA, snapshots, 10 TB–100 TB
- Performance: 100–1200 MB/s read throughput (Basic HDD → Basic SSD)

**For G5 MVP (M0):**
- Basic HDD tier, 1 TB
- Single zone (same zone as GKE nodes)
- Mount at `/mnt/g5-repos` on all GKE nodes via Filestore CSI driver
- Cost: ~$204/mo (Basic HDD 1TB)

```hcl
# infra/modules/filestore/main.tf

resource "google_filestore_instance" "repos" {
  name     = "g5-repos-${var.environment}"
  location = var.zone
  tier     = var.filestore_tier  # BASIC_HDD, BASIC_SSD, ENTERPRISE

  file_shares {
    name       = "repos"
    capacity_gb = var.capacity_gb
  }

  networks {
    network = var.vpc_network
    modes   = ["MODE_IPV4"]
  }
}
```

Kubernetes PV/PVC:

```yaml
# infra/k8s/filestore-pv.yaml
apiVersion: v1
kind: PersistentVolume
metadata:
  name: g5-repos
spec:
  capacity:
    storage: 1Ti
  accessModes:
    - ReadWriteMany  # NFS supports RWX
  nfs:
    server: ${FILESTORE_IP}
    path: /repos
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: g5-repos
spec:
  accessModes:
    - ReadWriteMany
  resources:
    requests:
      storage: 1Ti
  volumeName: g5-repos
```

Both g5-api and g5d-worker pods mount this PVC at `/mnt/g5-repos`.

### 9.3 Deployment Environments

| Aspect | Dev | Staging | Prod |
|--------|-----|---------|------|
| GKE nodes | 1 system, 0–2 worker | 2 system, 0–5 worker | 3 system, 0–20 worker |
| Cloud SQL | db-f1-micro, no HA | db-custom-2-8192, HA | db-custom-4-16384, HA |
| Redis | 1 GB Basic | 5 GB Standard | 10 GB Standard HA |
| Filestore | Basic HDD 1 TB | Basic SSD 1 TB | Enterprise 2 TB |
| GCS | Single bucket | Single bucket | Multi-region bucket |
| g5-api replicas | 1 | 2 | 3 |
| g5d workers | 0–2 | 0–5 | 0–20 (autoscale) |
| Domain | `dev.g5.example.com` | `staging.g5.example.com` | `app.g5.example.com` |
| LLM provider | OpenRouter | OpenRouter | Direct API keys |

### 9.4 Terraform Workspace Strategy

```
infra/
  environments/
    dev.tfvars
    staging.tfvars
    prod.tfvars
  modules/
    gke/
    cloudsql/
    redis/
    filestore/        ← NEW
    gcs/
    networking/
    iam/
  main.tf             ← wires modules together
  variables.tf
  outputs.tf
```

Each environment is a Terraform workspace (`terraform workspace select dev`).
Environment-specific variables in `*.tfvars`.

### 9.5 Helm Chart

```
infra/helm/g5/
  Chart.yaml
  values.yaml
  values-dev.yaml
  values-staging.yaml
  values-prod.yaml
  templates/
    deployment-api.yaml
    deployment-worker.yaml
    deployment-edge.yaml
    service-api.yaml
    ingress.yaml
    configmap.yaml
    secret.yaml
    pvc-repos.yaml
    hpa-worker.yaml
    network-policy.yaml
```

Worker autoscaling:

```yaml
# templates/hpa-worker.yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: g5d-worker
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: g5d-worker
  minReplicas: {{ .Values.worker.minReplicas }}
  maxReplicas: {{ .Values.worker.maxReplicas }}
  metrics:
    - type: External
      external:
        metric:
          name: redis_stream_pending_messages
          selector:
            matchLabels:
              queue: task_queue
        target:
          type: AverageValue
          averageValue: "1"  # 1 pending task per worker
```

### 9.6 CI/CD Pipeline Extension

```yaml
# .github/workflows/deploy.yml
name: Deploy
on:
  push:
    tags: ['v*']
  workflow_dispatch:
    inputs:
      environment:
        type: choice
        options: [dev, staging, prod]

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: ${{ github.event.inputs.environment || 'dev' }}
    steps:
      - uses: google-github-actions/auth@v2
        with:
          workload_identity_provider: ${{ secrets.WIF_PROVIDER }}
          service_account: ${{ secrets.WIF_SA }}

      - uses: google-github-actions/get-gke-credentials@v2
        with:
          cluster_name: g5-${{ env.ENVIRONMENT }}
          location: ${{ vars.GCP_REGION }}

      - run: helm upgrade --install g5 ./infra/helm/g5
          -f ./infra/helm/g5/values-${{ env.ENVIRONMENT }}.yaml
          --set image.tag=${{ github.sha }}
          --wait --timeout 10m
```

### 9.7 Deployment Sequence

```
Phase 1: Infrastructure (Terraform)
  terraform workspace select dev
  terraform apply -var-file=environments/dev.tfvars
  → Creates: GKE, Cloud SQL, Redis, Filestore, GCS, VPC, IAM

Phase 2: Database Migration
  kubectl run migrate --image=ghcr.io/g5labs/g5-api:latest \
    --env="DATABASE_URL=$DB_URL" \
    -- sqlx migrate run

Phase 3: Application (Helm)
  helm install g5 ./infra/helm/g5 -f values-dev.yaml

Phase 4: Verification
  curl https://dev.g5.example.com/healthz
  curl https://dev.g5.example.com/api/v1/health
```

---

## 10. Implementation Order & Dependencies {#10-implementation-order}

```
Week 1: Foundation
  ├─ WI-1: RepoStorage trait + LocalRepoStorage         (2 days)
  ├─ WI-1: Inject into all 8 services                   (2 days)
  └─ WI-2: StorageConfig + env var parsing               (1 day)

Week 2: Services
  ├─ WI-5: ManifestService + endpoints                   (1 day)
  ├─ WI-5: InternalApiClient.get_workspace_manifest()    (0.5 day)
  ├─ WI-5: Worker provisioning (clone from remote)       (1.5 days)
  └─ WI-4: Add repo / Remove repo / Create project      (2 days)

Week 3: Background + Multi-Host
  ├─ WI-3: IndexerService (fetch, rebase, search queue)  (2 days)
  ├─ WI-6: Worker git push after commit                  (1 day)
  ├─ WI-6: API git fetch on task completion              (0.5 day)
  ├─ WI-6: Multi-host integration test                   (1.5 days)
  └─ WI-2: FilestoreRepoStorage implementation           (optional, deferred to deployment)

Week 4: Deployment
  ├─ WI-7: Terraform modules (GKE, Cloud SQL, Redis,
  │         Filestore, GCS, VPC, IAM)                    (2 days)
  ├─ WI-7: Helm chart                                    (1 day)
  ├─ WI-7: Deploy to dev, verify                         (1 day)
  └─ WI-7: Deploy to staging, smoke test                 (1 day)
```

**Critical path**: WI-1 → WI-5 → WI-6 → WI-7

**Parallelizable**: WI-3 and WI-4 can proceed independently after WI-1.

---

## 11. Migration Strategy {#11-migration-strategy}

### 11.1 Backward Compatibility

The `LocalRepoStorage` provider is behaviorally identical to the current `repos_base_dir`
usage. Switching the internal wiring from direct `PathBuf` to `Arc<dyn RepoStorage>` with
`StorageConfig::Local { base_dir: "/data/repos" }` produces zero behavioral change.

**Strategy**: Ship WI-1 first with only `LocalRepoStorage`. All Docker compose
configurations continue to work unchanged. The Filestore provider is only activated
when `G5_STORAGE_PROVIDER=filestore` is set (GCP deployment).

### 11.2 Docker Compose Changes

Dev/test compose files only need one change — add the new env var:

```yaml
environment:
  G5_STORAGE_PROVIDER: local
  G5_STORAGE_BASE_DIR: /data/repos
```

Production compose and K8s use:

```yaml
environment:
  G5_STORAGE_PROVIDER: filestore
  G5_STORAGE_MOUNT_POINT: /mnt/g5-repos
```

### 11.3 Feature Flags

| Feature | Default | When to flip |
|---------|---------|--------------|
| `G5_STORAGE_PROVIDER=local` | ✅ | Always on for dev/test |
| `G5_STORAGE_PROVIDER=filestore` | Off | GCP deployment |
| `G5_INDEXER_ENABLED=true` | Off | After WI-3, enable in prod |
| `G5_WORKER_CLONE_MODE=shared` | ✅ | Dev: use shared volume |
| `G5_WORKER_CLONE_MODE=remote` | Off | Prod: clone from git URL |

---

## 12. Risk Register {#12-risk-register}

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| NFS latency for file tree listing | Medium | Medium | Redis L2 cache for hot file tree snapshots; Filestore SSD tier if needed |
| Git push/fetch overhead per task | Low | Medium | Bare clone cache on worker nodes (`~/.g5/cache/repos`); only fetch changed refs |
| NFS advisory locks insufficient for concurrent commits | Low | High | Serialize git operations per repo via Redis distributed lock |
| Indexer misses commits (race with worker push) | Low | Low | Task completion handler triggers immediate fetch (§8.2.4); indexer is a safety net |
| Filestore 1 TB fills up | Low | High | Monitor via GCP alerts; resize is online (no downtime) |
| Worker clone latency (first task) | Medium | Medium | Pre-warm workers with popular repos; GCS snapshot hydration |
| Terraform state corruption | Low | Critical | Use GCS backend with state locking; separate state per environment |
| Multi-region Filestore cost | High | Medium | Start single-zone Basic HDD; upgrade tier when needed |

---

## Appendix A: File Change Inventory

Files that must be created:

| File | Purpose |
|------|---------|
| `g5-common/src/storage/mod.rs` | `RepoStorage` trait, `StorageError` |
| `g5-common/src/storage/local.rs` | `LocalRepoStorage` |
| `g5-common/src/storage/filestore.rs` | `FilestoreRepoStorage` |
| `g5-common/src/storage/config.rs` | `StorageConfig` |
| `g5-common/src/manifest.rs` | `WorkspaceManifest`, `RepoManifest` |
| `g5-api/src/services/indexer_service.rs` | `IndexerService` |
| `g5-api/src/services/manifest_service.rs` | `ManifestService` |
| `g5-api/src/routes/manifest.rs` | Manifest endpoints |
| `infra/modules/filestore/main.tf` | Filestore Terraform |
| `infra/modules/filestore/variables.tf` | |
| `infra/modules/filestore/outputs.tf` | |
| `infra/helm/g5/` (directory) | Full Helm chart |
| `infra/environments/dev.tfvars` | Dev environment config |
| `infra/environments/staging.tfvars` | Staging config |
| `infra/environments/prod.tfvars` | Production config |
| `.github/workflows/deploy.yml` | Deploy workflow |

Files that must be modified:

| File | Change |
|------|--------|
| `g5-api/src/config.rs` | `ReposConfig` → `StorageConfig` |
| `g5-api/src/services/mod.rs` | Wire `Arc<dyn RepoStorage>` into all services |
| `g5-api/src/services/workspace_service.rs` | `repos_base_dir` → `storage` |
| `g5-api/src/services/ontology_service.rs` | `repos_base_dir` → `storage` |
| `g5-api/src/services/draft_service.rs` | `repos_base_dir` → `storage` |
| `g5-api/src/services/doc_service.rs` | `repos_base_dir` → `storage` |
| `g5-api/src/services/search_service.rs` | `repos_base_dir` → `storage` |
| `g5-api/src/services/task_service.rs` | FS path → git URL in `workspace_git_url` |
| `g5-api/src/routes/workspace.rs` | Add repo/remove repo/fork endpoints |
| `g5-common/src/internal_api/client.rs` | Add `get_workspace_manifest()` |
| `g5-common/src/internal_api/dtos.rs` | Add `WorkspaceManifest` DTO |
| `g5-common/Cargo.toml` | `storage-local`, `storage-filestore` features |
| `g5d/src/worker/mod.rs` | `resolve_task_workspace_path()` rewrite |
| `g5d/src/worker/lifecycle.rs` | `phase_collect()` add git push |
| `docker/docker-compose.dev.yml` | Add `G5_STORAGE_*` env vars |
| `docker/docker-compose.test.yml` | Add `G5_STORAGE_*` env vars |
| `infra/main.tf` | Add filestore module |

---

## Appendix B: Decision Log

| Decision | Rationale | Alternatives Considered |
|----------|-----------|------------------------|
| NFS (Filestore) over GCS FUSE | Git requires POSIX semantics; gcsfuse has 200ms+ latency for metadata ops | gcsfuse, custom object-store sync |
| Trait in g5-common, not g5-api | Workers also need storage access for future features | g5-api only |
| `local_path()` returns `Option<PathBuf>` | Allows future object-store providers to return `None` | Always require path |
| Indexer inside g5-api process | Shares DB pool, Redis, storage; simpler ops than a separate binary | Separate `g5-indexer` binary |
| Postgres advisory locks for leader election | Zero additional infrastructure; auto-release on crash | Redis SETNX, etcd |
| Helm over raw K8s manifests | Templating for multi-env; community standard | Kustomize, raw YAML |
| Basic HDD Filestore for dev | $204/mo vs $680/mo (SSD); sufficient for M0 throughput | SSD, Enterprise |
