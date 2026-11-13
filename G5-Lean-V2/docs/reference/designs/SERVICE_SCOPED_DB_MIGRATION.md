# Service Layer ScopedDb Migration Guide

## Overview

This document describes how to migrate service methods from direct `PgPool`
access to `ScopedDb` transactions that enforce Row-Level Security (RLS).

**Status**: Infrastructure ready (M2). Mandatory migration begins with M3
(when RLS policies are added to tenant tables).

## Architecture

### Current Pattern (pre-M3)

```
Route Handler → state.service.method(args) → &self.db (PgPool)
```

Services store `PgPool` at construction time. All queries run with full
database access — no per-org isolation.

### Target Pattern (M3+)

```
Route Handler → mint capability → ScopedDb::from_capability()
                → service.method(&mut db, args)
                → db.tx() (RLS-scoped transaction)
                → db.commit()
```

The ScopedDb wraps a transaction with RLS session context set via
`authz.set_ctx()`. All queries within the transaction are automatically
filtered to the org(s) in the capability.

## Migration Steps

### Step 1: Update Service Method Signature

**Before:**
```rust
impl WorkspaceService {
    pub async fn list_workspaces(
        &self,
        org_id: Uuid,
        filters: WorkspaceListFilters,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<WorkspaceListItem>, G5Error> {
        let total = sqlx::query_scalar("...")
            .fetch_one(&self.db)  // ← PgPool
            .await?;
```

**After:**
```rust
use g5_authz::ScopedDb;

impl WorkspaceService {
    pub async fn list_workspaces(
        &self,
        db: &mut ScopedDb,  // ← ScopedDb parameter
        org_id: Uuid,
        filters: WorkspaceListFilters,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<WorkspaceListItem>, G5Error> {
        let total = sqlx::query_scalar("...")
            .fetch_one(db.tx())   // ← use transaction
            .await?;
```

### Step 2: Update Route Handler

**Before:**
```rust
async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Query(filters): Query<WorkspaceListFilters>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<ResponseEnvelope<PaginatedResult<WorkspaceListItem>>>, AppError> {
    let result = state.workspace_service
        .list_workspaces(params.org_id, filters, &pagination)
        .await?;
```

**After:**
```rust
use crate::db::scoped::scoped_db_for_org;

async fn list_workspaces(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(params): Path<OrgPathParams>,
    Query(filters): Query<WorkspaceListFilters>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<ResponseEnvelope<PaginatedResult<WorkspaceListItem>>>, AppError> {
    let mut db = scoped_db_for_org(
        &state, &auth, params.org_id, &["workspaces:read"],
    ).await?;

    let result = state.workspace_service
        .list_workspaces(&mut db, params.org_id, filters, &pagination)
        .await?;

    db.commit().await?;
```

### Step 3: Multiple Service Calls in One Handler

When a handler calls multiple services, share the same `ScopedDb`:

```rust
async fn get_workspace_detail(...) -> ... {
    let mut db = scoped_db_for_org(&state, &auth, org_id, &["workspaces:read"]).await?;

    let workspace = state.workspace_service
        .get_workspace(&mut db, workspace_id).await?;
    let jobs = state.job_service
        .list_jobs(&mut db, workspace_id, &pagination).await?;

    db.commit().await?;
    Ok(Json(WorkspaceDetail { workspace, jobs }))
}
```

## Service Classification

### Must Migrate (org-scoped, RLS-enforced)
| Service | Methods | Query Sites |
|---------|---------|-------------|
| WorkspaceService | 21 | 41 |
| OntologyService | 16 | 29 |
| JobService | 9 | 23 |
| TaskService | 20 | 71 |
| ChatService | 22 | 40 |
| DraftService | 9 | 24 |
| CommentService | 9 | 16 |
| NotificationService | 6 | 7 |
| DocService | 5 | 12 |
| SpendService | 7 | 13 |
| CloneService | 1 | 22 |
| SearchService | 6 | 13 |
| PermissionService | 11 | 17 |

### Keep PgPool (no org context / privileged operations)
| Service | Reason |
|---------|--------|
| AuthService | Authentication — needs full DB access |
| OrgService | Creates orgs, manages memberships — privileged |
| UserService | Global user operations — no per-org filtering |
| WaitlistService | Global waitlist — no org context |

## Query Pattern Replacements

| Before | After |
|--------|-------|
| `.fetch_one(&self.db)` | `.fetch_one(db.tx())` |
| `.fetch_all(&self.db)` | `.fetch_all(db.tx())` |
| `.fetch_optional(&self.db)` | `.fetch_optional(db.tx())` |
| `.execute(&self.db)` | `.execute(db.tx())` |
| `self.db.begin()` | *Remove* — ScopedDb already wraps a tx |

## Helper Functions

The `crate::db::scoped` module provides:

| Function | Use Case |
|----------|----------|
| `scoped_db_for_org(state, auth, org_id, scopes)` | Single-org routes (most routes) |
| `scoped_db_for_orgs(state, auth, org_ids, scopes)` | Multi-org routes |
| `scoped_db_for_user_orgs(state, auth, scopes)` | Cross-org routes (dashboard, notifications) |

## Transaction Boundaries

**Before**: Some services called `self.db.begin()` for multi-step
operations. After migration, ScopedDb already IS a transaction, so
these become unnecessary:

```rust
// Before (self manages transaction)
let mut tx = self.db.begin().await?;
sqlx::query("...").execute(&mut *tx).await?;
sqlx::query("...").execute(&mut *tx).await?;
tx.commit().await?;

// After (ScopedDb IS the transaction)
sqlx::query("...").execute(db.tx()).await?;
sqlx::query("...").execute(db.tx()).await?;
// Caller commits via db.commit()
```

## Error Handling

`AppError` has `From` implementations for:
- `AuthzError` → maps to 401/403/500
- `ScopedDbError` → maps to 401/500

The `?` operator works seamlessly for both capability minting
and ScopedDb operations in route handlers.
