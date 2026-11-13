# Design: Completing Bidirectional Doc Links for Views

## Current State

### What exists

- `doc_node_links(doc_id, node_id)` join table + indexes
- `PUT /docs/:docId/links` endpoint to set links
- `DocDetailDTO` returns `linkedNodes: [{nodeId, nodeName}]`
- `DocListItemDTO` returns `linkedNodeCount`
- Staleness columns on `docs` (`last_uplifted_at`, `last_downlifted_at`, `content_updated_at`)
- Frontend Zod schemas for all of the above
- SyncModal component with uplift/downlift/sync options
- `INTERNAL_WS_PREFIX` split in sidebar → Views vs Project Docs sections

### The gap

The frontmatter `linked_nodes` field is the designed source of truth, but nothing reads it, writes it, or acts on it. SyncModal passes a **doc ID** as `targetNodes[0]`, but `CreateJobRequest.target_nodes` validates against `ontology_nodes` — so it fails.

## Design (6 layers, bottom-up)

### Layer 1: API — Doc-aware job creation

The `POST /jobs` endpoint needs a new optional field so SyncModal can say "sync this doc" without needing to know which nodes:

```rust
CreateJobRequest {
    job_type: String,
    target_nodes: Vec<Uuid>,      // existing — for node-scoped jobs
    target_doc_id: Option<Uuid>,  // NEW — for doc-scoped jobs
    ...
}
```

**Behavior in `job_service::create_job()`:**

- If `target_doc_id` is `Some`, resolve `SELECT node_id FROM doc_node_links WHERE doc_id = $1` → populate `target_nodes` automatically.
- If both are empty → 400 error.
- If both are provided → use explicit `target_nodes` (doc_id is metadata only).
- Store `target_doc_id` on the `jobs` row for provenance (new nullable column).

This is the **critical fix** — it bridges docs to the existing node-based task pipeline.

### Layer 2: API — Staleness computation for docs

Add a `compute_doc_staleness()` function in `doc_service.rs`:

```
A doc is stale when:
  content_updated_at > last_downlifted_at   → doc was edited, specs are behind
  (any linked node).last_uplifted_at > last_uplifted_at  → specs changed, doc is behind
```

Expose via `GET /docs/:docId` response as a computed `staleness: "fresh" | "doc_ahead" | "specs_ahead" | "both"` field. The frontend uses this to show a staleness badge on Views.

### Layer 3: API — Frontmatter → `doc_node_links` sync

Add a reconciliation endpoint or internal function: when a doc's content is saved (CRDT commit or file write), parse the YAML frontmatter, extract `linked_nodes`, resolve component_ids → node UUIDs, and upsert `doc_node_links`.

```
POST /internal/docs/:docId/reconcile-links
  1. Read doc content from git (file_path) or CRDT
  2. Parse YAML frontmatter → extract linked_nodes: ["auth/validator", ...]
  3. Resolve component_id → ontology_nodes.id via workspace_branch_id
  4. DELETE + INSERT into doc_node_links
```

This is called:

- After draft commit (content saved)
- After worker completes an uplift task that touched linked docs
- During bootstrap (Layer 5)

### Layer 4: Worker — Doc context in uplift/downlift

The task agent prompt needs to know about linked docs. Two additions to the worker `prepare` phase in `lifecycle.rs`:

**For downlift tasks** (doc changed → update specs):

- Before the agent runs, fetch the linked doc content via `GET /docs/:docId`
- Inject it into the agent's `TaskContext` as `linked_doc_content`
- The agent prompt says: "The design doc has been updated. Here is the updated doc. Update the spec for node X to reflect…"

**For uplift tasks** (code changed → update doc):

- After the agent finishes the spec uplift, if the node has linked docs, fetch them
- Append a second step: "Update the linked doc at `.g5/docs/auth-adr.md` to reflect the new spec state"
- OR: create a follow-up task for each stale linked doc

**Staleness update** — after a task completes:

- If downlift: set `docs.last_downlifted_at = now()` for the source doc
- If uplift: set `docs.last_uplifted_at = now()` for all linked docs of the uplifted nodes

### Layer 5: Bootstrap — Auto-discover doc links

In `execute_bootstrap()`, after analyzing the project:

1. Scan `.g5/docs/*.md` for YAML frontmatter with `linked_nodes`
2. For each doc, resolve component_ids → node UUIDs
3. Upsert into `doc_node_links`
4. Upsert `docs` rows (create if new, update `file_path`)

This ensures that when a user adds a new doc with `linked_nodes` in the frontmatter and runs bootstrap, the system discovers it.

### Layer 6: Frontend — Wire the remaining UI

| Change | File | Detail |
|--------|------|--------|
| Pass `targetDocId` instead of node ID | `WorkspacePage.tsx` | SyncModal `onSubmit` calls `createJob.mutate({ jobType, targetDocId: syncModalDocId })` instead of `targetNodes: [syncModalDocId]` |
| Add `targetDocId` to `CreateJobInput` | `types.ts` | `targetDocId?: string` field |
| Add staleness badge to doc entries | `TreeNode.tsx` | Show a dot/icon when `staleness !== 'fresh'` |
| Add `useSetDocLinks` mutation | `useDocs.ts` | `PUT /docs/:docId/links` wrapper — used by a future "manage links" UI |
| Show linked nodes in doc detail | SpecEditor / doc viewer | When opening a View doc, show its `linkedNodes` as navigable pills |

## Execution Order

1. **Layer 1** (API job creation) — unblocks the SyncModal immediately
2. **Layer 6** (Frontend `targetDocId` fix) — completes the UI→API path
3. **Layer 2** (Staleness) — gives users visibility into what's out of date
4. **Layer 3** (Frontmatter reconciliation) — makes `linked_nodes` the real source of truth
5. **Layer 4** (Worker doc context) — makes uplift/downlift actually act on docs
6. **Layer 5** (Bootstrap discovery) — auto-populates links on project init

Layers 1+6 together are the minimum to make the Sync button on Views actually work. Layers 2–5 complete the bidirectional contract.
