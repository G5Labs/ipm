# Repo-Native Docs, Views, Comments, and Derived Notifications

Status: Proposed  
Date: 2026-04-13  
Scope: `g5-api`, `g5d`, `g5-common`, client comment/doc flows, reindex/indexer pipeline

## Executive Summary

This document proposes a storage and API refactor that removes the current
hybrid state model for docs, views, and comments.

The target invariant is:

- the authoritative workspace state is the versioned workspace branch tree plus
  the active CRDT draft overlay
- docs, views, specs, and comments all live inside that workspace tree
- SQL stores only projections, indexes, and user-local delivery state
- notifications are derived from draft and committed comment state, not treated
  as canonical content

This is a deliberate tightening of the current architecture. Today:

- specs/code mostly follow the versioned-branch + draft model
- docs/views are hybrid between repo files and `docs.content`
- comments and mention notifications are written directly to SQL as primary
  collaboration state

That split creates semantic drift, clone inconsistencies, and confusing branch
     behaviour.

## Goals

- Make docs and views authoritative in the workspace primary repository, not in
  SQL fallback columns.
- Make comment threads authoritative in repo-native sidecar files attached to
  their document/spec file.
- Preserve immediate mention notifications from CRDT comment activity.
- Make durable notifications derived from authoritative comment state.
- Preserve resolved comments in committed history so provisional notifications
  can reconcile cleanly.
- Reduce clone/merge/reindex logic to branch state plus projection rebuilds.

## Non-Goals

- Replacing Yjs/CRDT drafting.
- Replacing the existing workspace ops / branch advancement model.
- Making notifications themselves versioned content.
- Preserving the current `docs` / `comment_index` / `notifications` schemas as
  primary sources of truth.

## New Invariants

1. File content truth

- Specs, docs, views, and comment threads are authoritative in the workspace
  repository tree or active draft overlay.
- SQL must not contain durable content that can diverge from that tree.

2. Projection-only SQL

- `docs`, `doc_node_links`, `comment_index`, `search_documents`, and mention
  indexes are rebuildable projections.
- Notification content/events are derived from draft or committed comment state.
- Only user-local inbox state remains mutable primary DB state.

3. Resolved comments are committed state

- Resolving a comment does not delete it.
- Resolved threads remain in the committed comment sidecar with status metadata.
- Durable notification reconciliation uses committed thread state, including
  resolved threads.

4. Immediate CRDT mention notifications are provisional

- Creating or editing a draft comment with mentions emits immediate provisional
  notifications.
- Commit-time indexing upgrades, confirms, or withdraws those provisional
  notifications.

5. Ontology must be a projection, not a second source of truth

- The ontology graph must be derived from repo state plus draft overlay state.
- We must not create or mutate durable ontology entities in SQL before the repo
  says those entities exist.
- Public and internal APIs may request reindex/reconciliation, but they must
  not be the canonical birth path for ontology content.

6. Workspace lineage belongs in workspace metadata, not content markdown

- Clone lineage, inheritance rules, and branch ancestry are workspace metadata.
- They must live in workspace and branch metadata surfaces, not in doc/view/spec
  markdown.
- Versioned content-affecting config can live in repo files such as
  `semlift.yaml`, but operational lineage and topology stay outside content
  files.

## Target Storage Model

### 1. Workspace Primary Repository Layout

All durable collaboration content lives in the workspace primary repository.

Recommended layout:

```text
docs/<slug>.md
views/<slug>.md
.semlift/specs/<component_id>.semlift.md
docs/<slug>.md.comments.yaml
views/<slug>.md.comments.yaml
.semlift/specs/<component_id>.semlift.md.comments.yaml
```

If we want a dedicated hidden directory instead of adjacent sidecars, the only
acceptable alternative is a deterministic repo-native path, for example:

```text
.g5/comments/by-path/<escaped_path>.yaml
```

The important part is that comment state versions with the same branch/draft
state as the document it annotates.

### What "Ontology Born In SQL" Means

This phrase refers to the current ability of the system to create ontology
entities directly in the `ontology_nodes` table before a corresponding spec,
view, or other repo-backed artifact exists in the workspace tree.

In plain terms:

- today, a node can appear in the UI because it exists in SQL metadata
- that node may not yet have a committed file or draft-backed file that makes it
  real in workspace state
- that means the database graph can get ahead of the repository graph

That breaks the repo-backed worldview because the user can observe or mutate an
entity whose primary artifact is not yet present in the versioned workspace.

The target model is:

- repo or draft file exists first
- indexing/reconciliation discovers it
- ontology projection reflects it

Not:

- SQL creates the node
- repo catches up later

### Ontology Rows Are Not Draft Nodes Today

The current `ontology_nodes` rows are branch-scoped projection rows, not
draft-scoped entities.

That distinction matters:

- draft content already has a legitimate place in the CRDT overlay
- ontology existence does not
- today, a node can be created in branch-level SQL before any repo-backed or
  draft-backed artifact exists for it

If we ever want "draft ontology nodes" as a concept, they must be explicitly
draft-scoped and reconciled on commit. They should not be represented by normal
branch-level `ontology_nodes` rows that pretend the branch already contains the
entity.

### Workspace Metadata and Lineage

Workspace lineage and projection inheritance should be expressed in workspace
metadata, not in the docs repo markdown itself.

Recommended split:

- workspace and branch lineage in DB-backed workspace metadata
- local materialization in `.g5/workspace.yaml`
- content-affecting semantic config in repo files such as `semlift.yaml`

If clone inheritance or recursive projection fallback is used, it should operate
through workspace metadata such as:

- `workspaces.cloned_from_id`
- a future `workspace_branches.cloned_from_branch_id`

It should not be encoded in markdown frontmatter or document bodies.

### 2. Comment Sidecar Format

Use YAML sidecars for human-readable diffs and straightforward machine updates.

Example:

```yaml
version: 1
document_path: views/auth-roadmap.md
threads:
  - thread_id: thr_01
    anchor_id: block_42
    status: resolved
    created_at: 2026-04-13T13:00:00Z
    created_by: user_123
    resolved_at: 2026-04-13T13:04:00Z
    resolved_by: user_123
    comments:
      - comment_id: cmt_01
        author_id: user_123
        created_at: 2026-04-13T13:00:00Z
        body: |
          Please tighten the definition here.
        mentions:
          - user_456
      - comment_id: cmt_02
        author_id: user_456
        created_at: 2026-04-13T13:02:00Z
        body: |
          Done.
        mentions: []
```

Required fields:

- `thread_id`
- `anchor_id`
- `status`
- `created_at`
- `created_by`
- `comments[].comment_id`
- `comments[].author_id`
- `comments[].created_at`
- `comments[].body`
- `comments[].mentions`

Optional fields:

- `resolved_at`
- `resolved_by`
- `edited_at`
- future reaction metadata

### 3. Anchor IDs

Comments must not anchor to transient SQL node IDs.

Every comment thread must target a stable repo-native anchor:

- `anchor_id` for markdown/spec block anchors
- or `file_path + line + optional surrounding context hash` for code-level
  comments if line comments remain supported outside markdown/spec documents

For docs/views/specs, the preferred path is stable inline anchor IDs embedded in
the markdown/spec AST.

## Target API Contracts

### 1. Docs and Views

`create_view` and `create_doc` should become file-creation operations against
the draft/workspace-op layer.

Required changes:

- `POST /workspaces/:workspaceId/docs/views`
  - creates `views/<slug>.md` in the draft overlay
  - returns `filePath`, `docPath`, and any indexed `docId` if available
  - must not write `docs.content`
- `GET /workspaces/:workspaceId/docs/:docId`
  - resolves the indexed doc entry to a repo path
  - reads content from draft overlay first, then committed branch
  - must not read fallback inline content from SQL
- `PUT /workspaces/:workspaceId/docs/:docId`
  - writes through draft overlay only
  - advances through normal draft commit flow

`docs` becomes an indexed catalog. Public APIs may continue exposing a `docId`,
but that `docId` must be either:

- a stable deterministic doc identifier derived from repo path, or
- a projection id that is never used as the source of truth for content

### 2. Comments

Comments become document-native state.

Required contract changes:

- `POST /workspaces/:workspaceId/docs/:docId/comments`
  - creates or appends to a sidecar thread in draft overlay
  - returns `threadId`, `commentId`, `status`, `provisionalNotificationIds`
- `POST /workspaces/:workspaceId/comments/:threadId/replies`
  - updates the same sidecar
- `POST /workspaces/:workspaceId/comments/:threadId/resolve`
  - sets `status = resolved`
  - does not delete the thread
- `POST /workspaces/:workspaceId/comments/:threadId/reopen`
  - sets `status = open`

Comment reads:

- API reads from the indexed comment projection by default for performance
- if a draft is open and the current user has draft context, draft overlay
  comment state should be merged for read-your-writes semantics

### 3. Notifications

Notifications become a derived event stream plus user receipts.

Required contract changes:

- immediate draft mention notifications are created from CRDT comment ops
- committed notifications are created or reconciled by comment indexing
- `list_notifications` reads derived notification events joined with user-local
  receipt state
- `mark_read` and `dismiss` update only receipt rows

The API should expose notification status as:

- `provisional`
- `committed`
- `withdrawn`

This allows the client to show immediate alerts without pretending they are
already canonical.

## Exact Persistence Changes

### 1. Docs / Views

Current hybrid state to remove:

- `docs.content`
- `docs.mapping_content`
- direct worker writes to docs content in the API

Required schema direction:

- keep `docs` as a projection/catalog table
- add branch/path/version metadata if needed for indexing correctness
- remove any use of `docs.content` as fallback content
- remove any use of `mapping_content` as durable content

Exact code changes:

- [doc_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/doc_service.rs)
  - remove SQL writes to `docs.content`
  - replace repo writes with draft-overlay writes
  - make reads repo/draft-only
- [internal.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/internal.rs)
  - remove `PUT /api/internal/docs/:id/content` as a durable content path
  - replace with reindex trigger or branch advance hook only
- [clone_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/clone_service.rs)
  - stop cloning doc content and link tables as primary state
  - clone branch state, then rebuild doc projections
- [linked_doc_indexer.rs](/Users/lumost/workspace/g5/generation/g5-api/src/maintenance/linked_doc_indexer.rs)
  - become the only `doc_node_links` population path

### 2. Ontology

Current primary-state escape hatches to remove:

- public ontology CRUD creates and updates `ontology_nodes` directly
- internal worker upserts create and mutate `ontology_nodes` directly
- view bootstrap pre-creates ontology nodes for `new_nodes` before a repo-backed
  artifact necessarily exists
- clone can copy ontology rows today without a guaranteed post-clone
  reconciliation contract

Required rule:

- `ontology_nodes` must become a projection over repo files and draft overlay
  content
- if a node corresponds to a spec/view/document artifact, that artifact must be
  the canonical source
- if a node corresponds to a code file, the file itself must be the canonical
  source

Exact code changes:

- [ontology.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/ontology.rs)
  - deprecate public create/delete/update-node metadata APIs as source-of-truth
    mutation paths
  - keep content editing APIs only where they write through the draft overlay
- [ontology_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/ontology_service.rs)
  - narrow to projection helpers and read/query services
  - remove direct create/update/delete semantics as canonical writes
- [internal.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/internal.rs)
  - replace `upsert_ontology_nodes` as a source-of-truth write with a
    reconciliation trigger or draft/repo write path
- [view_task_builder.rs](/Users/lumost/workspace/g5/generation/g5d/src/worker/view_task_builder.rs)
  - stop pre-creating `new_nodes` in SQL before corresponding repo artifacts
    exist
- [ontology_reconciler.rs](/Users/lumost/workspace/g5/generation/g5-api/src/maintenance/ontology_reconciler.rs)
  - become the canonical population/update path for ontology projections
- [clone_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/clone_service.rs)
  - allow copying ontology rows only as a warm cache
  - require reconcile/reindex after clone from branch state
  - never treat copied ontology rows as authoritative state

### 3. Comments

Current primary state to replace:

- `comment_index`
- `comment_replies`
- `comment_mentions`

Target:

- repo-native comment sidecars are authoritative
- SQL comment tables become indexes

Required schema direction:

- keep or rename `comment_index` as projection-only
- optionally add `comment_projection_version` metadata for rebuild/debugging
- add a dedicated `comment_event_cursor` table only if indexers need resume
  checkpoints

Exact code changes:

- [comment_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/comment_service.rs)
  - remove direct inserts as authoritative writes
  - replace with sidecar mutation helpers over draft state
- [comments.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/comments.rs)
  - route handlers call draft/repo mutation path
  - emit live CRDT events
- add a new comment indexer
  - scans changed `*.comments.yaml`
  - projects thread/reply/mention rows
  - resolves anchors to indexed docs/nodes where possible

### 4. Notifications

Current primary state to split:

- `notifications` mixes event content and user inbox state

Target:

- `notification_events` or equivalent derived event table
- `notification_receipts` for user-local mutable state

Recommended schema:

- `notification_events`
  - `id`
  - `workspace_id`
  - `source_type` (`draft_comment`, `commit_comment`, `mention`, `reply`,
    `resolution`)
  - `source_thread_id`
  - `source_comment_id`
  - `status` (`provisional`, `committed`, `withdrawn`)
  - `payload`
  - `created_at`
- `notification_receipts`
  - `notification_event_id`
  - `recipient_user_id`
  - `read_at`
  - `dismissed_at`
  - `last_delivered_at`

Exact code changes:

- [notification_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/notification_service.rs)
  - stop treating notification rows as primary collaboration content
  - split event creation from read/dismiss state
- current mention creation in [comments.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/comments.rs)
  - becomes provisional event creation, not final durable notification write

## CRDT and Reconciliation Model

### 1. Draft-Time Behaviour

On CRDT comment creation or edit:

- mutate the comment sidecar in the draft overlay
- emit live workspace/user WebSocket events
- detect mentions
- create provisional notification events immediately

On draft-time resolution or deletion:

- update the draft sidecar
- emit live events
- mark related provisional notifications as withdrawn or closed

### 2. Commit-Time Behaviour

When a draft commits:

- workspace ops advances branch head
- reindex runs on changed document files and changed `*.comments.yaml`
- comment indexer computes committed delta:
  - new threads/comments
  - new mentions
  - status changes
  - resolved/reopened threads
- notification reconciler:
  - upgrades matching provisional events to `committed`
  - creates committed events if no provisional existed
  - marks stale provisional events as `withdrawn`

### 3. Why Resolved Comments Must Commit

Resolved threads must remain in committed sidecar state because they provide the
canonical record needed to reconcile provisional notifications.

If a comment is created, mentioned, resolved, and then committed in resolved
state:

- the mention still existed
- the user may already have received a provisional notification
- the durable system needs a committed record saying the thread ended resolved

This avoids durable inbox drift and preserves auditability.

## Indexers Required

### 1. Doc Catalog Indexer

Responsibilities:

- discover docs/views/specs from repo paths
- parse frontmatter / metadata
- populate `docs` projection rows
- populate `doc_node_links`
- update doc timestamps/hash metadata

### 2. Comment Indexer

Responsibilities:

- discover changed `*.comments.yaml`
- parse threads/replies/mentions
- populate `comment_index`
- populate mention projection rows
- detect resolution/reopen transitions

### 3. Notification Reconciler

Responsibilities:

- consume draft provisional events and committed index results
- reconcile provisional vs committed events
- materialize user-visible notification rows/views

## Exact Migration Plan

### Phase 1: Read Path Cleanup

- make doc/view reads repo/draft-only
- stop reading `docs.content` as fallback
- leave current writes intact temporarily

### Phase 2: Ontology Projection Cleanup

- remove direct ontology birth paths in SQL
- make ontology reconciler the canonical projection writer
- remove view/bootstrap SQL pre-creation of ontology entities
- allow clone-time copying only as cache warmup with mandatory reconcile

### Phase 3: Repo-Native Comment Sidecars

- add sidecar schema and CRDT mutation helpers
- dual-read comments from sidecars first, SQL as fallback
- add comment indexer

### Phase 4: Notification Split

- introduce provisional vs committed notification events
- keep immediate draft mention notifications
- move read/dismiss state into receipt rows

### Phase 5: Write Path Flip

- switch comment writes to sidecars only
- switch doc/view writes to draft/repo only
- stop authoritative writes to `docs.content`, `comment_index`,
  `notifications`

### Phase 6: Clone/Reindex Simplification

- clone only branch state and operational session state
- allow warm-cache copying of rebuildable projections only when followed by
  reconcile/reindex
- rebuild docs/comments/notifications indexes after clone
- remove any assumption that copied projection rows are authoritative

## Clone and Projection Strategy

Deep-copying rebuildable projection tables is not necessary for correctness.
The preferred long-term model is:

1. clone workspace and branch metadata
2. clone repo refs and committed branch heads
3. optionally warm-copy rebuildable projection rows
4. immediately reconcile/reindex against branch state

This means:

- copy-and-reconcile is acceptable as a performance optimization
- copy-and-trust is not acceptable

Longer-term, we can reduce clone amplification further by using lineage-aware
projection fallback.

Recommended direction:

- keep workspace-level lineage via `cloned_from_id`
- add branch-level lineage such as `cloned_from_branch_id`
- use recursive queries or lineage-aware fallback for cold projection reads
- materialize local projection rows asynchronously after clone

This preserves the repo-backed worldview while avoiding full projection
duplication for every clone.

### Phase 7: Column/Table Retirement

- drop `docs.content`
- drop `docs.mapping_content`
- retire `comment_replies` and `comment_mentions` as primary tables
- retire old `notifications` content semantics

## Other Worldview Exceptions

The broader architecture review found a few other locations worth calling out so
we do not overgeneralize the refactor.

### Real inconsistencies that should be fixed

- Ontology graph direct mutation in SQL
- Treating copied ontology/docs/link projections as if they were primary state
- Docs/views hybrid content persistence
- Comments and notifications as primary collaboration state

### Probably acceptable operational sidecars

- chat sessions and chat messages
- workspace settings
- workspace lineage and operational workspace metadata
- runtime configs and user preferences
- jobs, tasks, spend tracking, worker registry, presence, and draft session
  metadata

These are not repo content. They are operational or user-local state and should
remain outside the repo-backed worldview unless product requirements change.

## Files and Subsystems That Must Change

### API / Services

- [doc_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/doc_service.rs)
- [comment_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/comment_service.rs)
- [notification_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/notification_service.rs)
- [workspace_ops.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/workspace_ops.rs)
- [clone_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/clone_service.rs)

### Routes

- [docs.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/docs.rs)
- [comments.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/comments.rs)
- [internal.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/internal.rs)

### Maintenance / Indexing

- [search_indexer.rs](/Users/lumost/workspace/g5/generation/g5-api/src/maintenance/search_indexer.rs)
- [linked_doc_indexer.rs](/Users/lumost/workspace/g5/generation/g5-api/src/maintenance/linked_doc_indexer.rs)
- new comment/notification indexers under
  `generation/g5-api/src/maintenance/`

### Client

- comment creation/edit/resolve APIs
- notification rendering for provisional vs committed states
- doc/view editor assumptions that currently rely on SQL-backed content

### Migrations

- add comment sidecar projection support tables if needed
- add notification event/receipt split
- later retire `docs.content` and `mapping_content`

## Success Criteria

- Creating or editing a doc/view/spec changes only draft/repo state.
- Creating or resolving a comment changes only draft/repo state.
- Immediate mention notifications still appear before commit.
- Resolving a mentioned comment before commit does not lose reconciliation.
- Cloning a workspace requires no copying of docs/comments primary content from
  SQL.
- If clone warm-copies projections, reconcile/reindex is still the authority.
- Reindex can fully rebuild docs, comment indexes, and durable notifications
  from repo state plus receipt tables.

## Decision Summary

We should make the following explicit:

- comments belong to documents/specs as versioned workspace state
- resolved comments still commit as resolved history
- immediate CRDT mention notifications are allowed, but they are provisional
- durable notifications are derived and reconciled, not canonical content
- docs/views/comments must stop using SQL content as a shadow source of truth
- ontology must stop being able to appear first in SQL before the corresponding
  repo or draft artifact exists
- workspace lineage and clone inheritance belong in workspace metadata, not in
  markdown content
- clone may copy projection rows as cache, but repo state plus reconcile remains
  authoritative
