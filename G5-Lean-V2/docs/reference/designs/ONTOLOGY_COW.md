# Copy-on-Write Ontology & Search Index Design

**Status:** Proposed (future work)
**Author:** auto-generated from architecture audit
**Date:** 2026-02-24

## 1. Problem Statement

When a workspace is cloned (`POST /api/v1/workspaces/:id/clone`), the current
implementation performs a full deep copy of all `ontology_nodes`,
`node_dependencies`, `docs`, `doc_node_links`, and `draft_sessions` within a
single Postgres transaction. The search index (`search_documents`) is populated
asynchronously via the reindex queue after the transaction commits.

For small-to-medium workspaces (hundreds to low thousands of ontology nodes),
this is fast — bulk `INSERT ... SELECT` over a few thousand rows completes in
under a second. However, as workspaces grow (tens of thousands of nodes with
deep dependency graphs, large doc sets, and chunked search documents), the
clone transaction becomes:

1. **Increasingly slow** — `O(N)` row copies with UUID remapping
2. **Lock-heavy** — long-running transaction holds row-level locks
3. **Memory-intensive** — all source rows are loaded into Rust memory for
   UUID remapping before re-insertion

Additionally, workspace **creation** (`POST /api/v1/workspaces`) currently
produces an empty ontology — the indexer pipeline must build everything from
scratch by walking the entire repository tree, which can take minutes for
large codebases.

## 2. Goals

- **Instant clone:** Clone operations should be `O(1)` regardless of workspace
  size, with materialization deferred until writes occur
- **Instant fork from base:** New workspaces that track a base branch should
  inherit the base branch's already-indexed ontology and search data without
  re-processing
- **Correctness:** Mutations in the clone must never affect the source, and
  vice versa
- **Incremental materialization:** Only rows that actually diverge from the
  source need their own storage
- **Minimal migration risk:** The approach should layer on top of existing
  tables rather than replacing them

## 3. Design Options

### 3.1 Option A: Shared Snapshot with Overlay Table (Recommended)

#### Concept

Instead of copying rows, a cloned workspace's branches **share** the source
branch's ontology snapshot at a specific commit, with a per-workspace overlay
table that records local modifications.

#### Schema Changes

```sql
-- Immutable snapshot of ontology state at a specific commit.
-- Rows are never updated — new snapshots are created on commit.
CREATE TABLE ontology_snapshots (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_branch_id UUID NOT NULL REFERENCES workspace_branches(id) ON DELETE CASCADE,
    commit_sha      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_branch_id, commit_sha)
);

-- Individual node state within a snapshot.
-- This is the "base layer" — immutable once written.
CREATE TABLE ontology_snapshot_nodes (
    snapshot_id     UUID NOT NULL REFERENCES ontology_snapshots(id) ON DELETE CASCADE,
    node_id         UUID NOT NULL,  -- logical node identity (stable across snapshots)
    parent_id       UUID,
    name            TEXT NOT NULL,
    node_type       TEXT NOT NULL,
    file_path       TEXT,
    content_hash    TEXT,
    sort_order      INT NOT NULL DEFAULT 0,
    PRIMARY KEY (snapshot_id, node_id)
);

-- Per-workspace overlay: records changes on top of a shared snapshot.
-- Only contains rows that DIFFER from the base snapshot.
CREATE TABLE ontology_overlay (
    workspace_branch_id UUID NOT NULL REFERENCES workspace_branches(id) ON DELETE CASCADE,
    base_snapshot_id    UUID NOT NULL REFERENCES ontology_snapshots(id),
    node_id             UUID NOT NULL,
    -- NULL = node deleted in overlay; otherwise, full node state
    name                TEXT,
    node_type           TEXT,
    file_path           TEXT,
    content_hash        TEXT,
    parent_id           UUID,
    sort_order          INT,
    overlay_op          TEXT NOT NULL CHECK (overlay_op IN ('add', 'modify', 'delete')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_branch_id, node_id)
);

-- Add snapshot reference to workspace_branches
ALTER TABLE workspace_branches
    ADD COLUMN base_snapshot_id UUID REFERENCES ontology_snapshots(id);
```

#### Clone Operation

```sql
-- Clone becomes a single metadata write:
UPDATE workspace_branches
SET base_snapshot_id = (
    SELECT base_snapshot_id FROM workspace_branches WHERE id = $source_branch_id
)
WHERE id = $clone_branch_id;
```

Time complexity: `O(1)`. No ontology rows are copied.

#### Read Path (Merged View)

A database view or query macro merges the snapshot base with the overlay:

```sql
CREATE VIEW ontology_nodes_effective AS
SELECT
    COALESCE(o.node_id, s.node_id) AS id,
    wb.id AS workspace_branch_id,
    COALESCE(o.parent_id, s.parent_id) AS parent_id,
    COALESCE(o.name, s.name) AS name,
    COALESCE(o.node_type, s.node_type) AS node_type,
    COALESCE(o.file_path, s.file_path) AS file_path,
    COALESCE(o.content_hash, s.content_hash) AS content_hash,
    COALESCE(o.sort_order, s.sort_order) AS sort_order
FROM workspace_branches wb
JOIN ontology_snapshot_nodes s
    ON s.snapshot_id = wb.base_snapshot_id
LEFT JOIN ontology_overlay o
    ON o.workspace_branch_id = wb.id
   AND o.node_id = s.node_id
WHERE o.overlay_op IS NULL          -- not in overlay → use snapshot
   OR o.overlay_op != 'delete'      -- in overlay but not deleted
UNION ALL
-- Nodes that exist ONLY in the overlay (added after clone)
SELECT o.node_id, o.workspace_branch_id, o.parent_id, o.name,
       o.node_type, o.file_path, o.content_hash, o.sort_order
FROM ontology_overlay o
WHERE o.overlay_op = 'add';
```

#### Write Path (Copy-on-Write)

When the reconciler processes a change for a cloned branch:
1. Check if `ontology_overlay` already has a row for `(branch_id, node_id)`
2. If not, INSERT into `ontology_overlay` with `overlay_op = 'modify'` (or
   `'delete'` for removals)
3. New files INSERT into `ontology_overlay` with `overlay_op = 'add'`

The original snapshot is never mutated.

#### Snapshot Compaction

When a branch's overlay grows beyond a threshold (e.g., >50% of the base
snapshot size), compact by creating a new snapshot that merges base + overlay:

```sql
-- Create new merged snapshot
INSERT INTO ontology_snapshot_nodes (snapshot_id, node_id, ...)
SELECT $new_snapshot_id, ...
FROM ontology_nodes_effective
WHERE workspace_branch_id = $branch_id;

-- Point branch at new snapshot, clear overlay
UPDATE workspace_branches SET base_snapshot_id = $new_snapshot_id WHERE id = $branch_id;
DELETE FROM ontology_overlay WHERE workspace_branch_id = $branch_id;
```

This can run as a background maintenance task.

### 3.2 Option B: Postgres Row-Level CoW via `INSERT ... SELECT` with Deferred Materialization

Keep the current `ontology_nodes` table but add a `source_branch_id` column.
On clone, set `source_branch_id` on the new branch's row in
`workspace_branches` instead of copying nodes. Reads merge:

```sql
SELECT * FROM ontology_nodes
WHERE workspace_branch_id IN ($branch_id, $source_branch_id)
-- prefer local rows over inherited ones
ORDER BY (workspace_branch_id = $branch_id) DESC
```

**Pros:** Minimal schema change.
**Cons:** Read queries become more complex; chain of `source_branch_id`
references can grow unbounded; harder to reason about correctness.

### 3.3 Option C: Materialized View per Branch

Create a Postgres materialized view per workspace branch that unions the
base snapshot with local changes.

**Pros:** Clean read path via standard `SELECT`.
**Cons:** `CREATE MATERIALIZED VIEW` is DDL (requires schema-level
privileges); per-branch views don't scale; `REFRESH` is expensive.

## 4. Search Index CoW

The same snapshot + overlay pattern can be applied to `search_documents`:

- On clone, the new branch shares the source branch's search snapshot
- The `search_documents` table gains a `base_search_snapshot_id` reference
- Only chunks that change after clone get their own rows
- Compaction merges overlay → new snapshot periodically

This makes search available instantly after clone, without waiting for the
reindex pipeline.

However, this is lower priority than ontology CoW because:
1. The reindex enqueue (now implemented) populates search within ~30 seconds
2. Search documents are simpler (no parent/dependency remapping needed)
3. Search data can be fully regenerated from git content at any time

## 5. Impact on Existing Code

### Modules Affected

| Module | Change |
|--------|--------|
| `ontology_reconciler.rs` | Write to overlay instead of mutating `ontology_nodes` directly |
| `linked_doc_indexer.rs` | Read from `ontology_nodes_effective` view for node resolution |
| `search_indexer.rs` | No change (already reads from ontology, writes to search_documents) |
| `clone_service.rs` | Replace deep copy with `O(1)` snapshot reference |
| `workspace_service.rs` | On create + add repo, set `base_snapshot_id` if base workspace exists |
| `doc_service.rs` | Read from view for doc-node link resolution |
| All read queries | Replace `ontology_nodes` with `ontology_nodes_effective` |

### Migration Strategy

1. **Phase 1:** Add `ontology_snapshots`, `ontology_snapshot_nodes`,
   `ontology_overlay` tables. Add `base_snapshot_id` to `workspace_branches`.
   No behavior change — existing `ontology_nodes` continues to work.

2. **Phase 2:** Background task creates snapshots from current `ontology_nodes`
   rows for all existing branches. Sets `base_snapshot_id`.

3. **Phase 3:** Switch reconciler to write to overlay. Switch reads to use
   the effective view. Clone uses `O(1)` snapshot sharing.

4. **Phase 4:** Drop legacy `ontology_nodes` rows for branches that have
   been migrated to snapshot-based storage. Eventually drop table or repurpose
   it.

## 6. Performance Estimates

| Operation | Current | With CoW |
|-----------|---------|----------|
| Clone (1K nodes) | ~200ms | ~1ms |
| Clone (10K nodes) | ~2s | ~1ms |
| Clone (100K nodes) | ~20s+ | ~1ms |
| Single node read | ~1ms | ~2ms (view join) |
| Bulk tree read | ~5ms | ~8ms (view join) |
| Reconcile (modify 1 file) | ~3ms | ~4ms (overlay insert) |

The read-path overhead is small (one extra LEFT JOIN) and can be mitigated
with proper indexing on `(workspace_branch_id, node_id)` in both tables.

## 7. Decision

**Recommended: Option A (Shared Snapshot with Overlay)**

Rationale:
- Clean separation of immutable base state from mutable workspace changes
- `O(1)` clone regardless of workspace size
- Compaction keeps overlay small for active workspaces
- No unbounded reference chains (unlike Option B)
- No DDL per workspace (unlike Option C)
- Layered migration path that doesn't require big-bang cutover

**Priority: Low-Medium.** The current deep-copy approach works well for
workspaces up to ~10K ontology nodes (sub-second clone). This design
becomes important when:
- Workspaces regularly exceed 10K nodes
- Clone frequency is high (e.g., automated CI workflows creating clones)
- Clone latency impacts user experience

## 8. Open Questions

1. **Garbage collection:** When all branches sharing a snapshot are deleted,
   should the snapshot be GC'd immediately (via reference counting) or lazily
   (via periodic cleanup)?

2. **Cross-org snapshots:** If a public workspace is cloned into a different
   org, should the snapshot be shared across org boundaries or deep-copied?
   (Security implications for RLS.)

3. **Dependency edges:** `node_dependencies` has `from_node_id` / `to_node_id`
   FK references. The overlay approach needs to handle dependency overlays
   as well (or defer dependency remapping to compaction time).

4. **Draft interaction:** When a user edits a file in a cloned workspace with
   an active draft, should the overlay be written immediately or only on
   draft commit?
