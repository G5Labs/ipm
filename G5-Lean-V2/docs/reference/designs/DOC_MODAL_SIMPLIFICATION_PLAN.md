# Doc Modal Simplification Plan

## Goal

Simplify the current "view/project docs" creation flow so the user chooses between only two document kinds:

1. Plain doc
2. Linked doc

The default should be **Plain doc**.

As part of the same cleanup, hide the inline up/down sync arrows currently shown in the sidebar tree row for docs/views.

This change should preserve the newer view/bootstrap model:

- plain docs are just docs
- linked docs are docs with persona + instructions + reference ontology nodes
- first sync/bootstrap creates the linked-doc/view scaffolding and links, not creation time

## Desired UX

### Plain Doc

The user sees a minimal form:

- doc kind: `Plain doc` selected by default
- name
- optional subtype if we want to preserve current doc categories:
  - `design`
  - `adr`
  - `plan`
  - `guide`
  - `note`

Submit behavior:

- create a regular document immediately
- no persona
- no ontology node picker
- no bootstrap job

### Linked Doc

The user sees a richer form:

- doc kind: `Linked doc`
- name
- persona
- instructions
- reference ontology nodes

Reference ontology nodes should be selected through search, not by forcing the user to browse the whole tree.

Submit behavior:

- create a linked doc using the current view-backed document model
- store persona/instructions on the doc
- store the selected ontology references as initial `doc_node_links`
- do **not** bootstrap during creation
- first sync handles the existing bootstrap flow

## Product Decision

We should treat **Linked doc** as the friendly product label for what is currently implemented through the view-backed doc flow.

That keeps the backend semantics aligned with the current architecture:

- create a linked doc now
- sync later
- first sync bootstraps
- later syncs become narrower uplift/downlift operations

## Current State

Today the modal is split into two mixed concepts:

- project docs created directly through `POST /docs`
- persona-based future views delegated to `CreateViewModal`

This creates a few problems:

- the modal is conceptually heavier than it needs to be
- "project doc" and "view" are mixed in one chooser
- linked-doc creation is indirect
- reference ontology nodes are not part of the create flow

Relevant files:

- [CreateProjectDocModal.tsx](/Users/lumost/workspace/g5/generation/client/src/components/views/CreateProjectDocModal.tsx)
- [CreateViewModal.tsx](/Users/lumost/workspace/g5/generation/client/src/components/views/CreateViewModal.tsx)
- [WorkspacePage.tsx](/Users/lumost/workspace/g5/generation/client/src/pages/WorkspacePage.tsx)
- [TreeNode.tsx](/Users/lumost/workspace/g5/generation/client/src/components/tree/TreeNode.tsx)
- [useViews.ts](/Users/lumost/workspace/g5/generation/client/src/hooks/useViews.ts)
- [view.rs](/Users/lumost/workspace/g5/generation/g5-common/src/types/view.rs)
- [doc_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/doc_service.rs)

## Recommended Implementation

## 1. Simplify the Modal Contract

Refactor the current modal into a single create-doc experience with two modes:

- `plain`
- `linked`

Recommended rename:

- `CreateProjectDocModal` -> `CreateDocModal`

This is optional for the first patch, but it would better reflect the new mental model.

## 2. Keep Plain Docs on the Existing `create_doc` Path

Plain docs should continue to use:

- `POST /api/v1/workspaces/:workspaceId/docs`

Payload shape can remain essentially unchanged:

- `name`
- `docType`
- `filePath`
- `content`

No backend contract change is required for the plain-doc path.

## 3. Use the View-Backed Path for Linked Docs

Linked docs need:

- persona
- instructions
- initial ontology references

The current `create_view` path already stores persona/instruction semantics correctly, so it is the right base path.

Recommended direction:

- keep using `POST /api/v1/workspaces/:workspaceId/docs/views` for linked docs
- extend its request contract to accept initial linked ontology node ids

### Required shared type change

Extend [CreateViewRequest](/Users/lumost/workspace/g5/generation/g5-common/src/types/view.rs) with something like:

```rust
pub linked_node_ids: Option<Vec<String>>
```

Wire format:

- `linkedNodeIds`

This keeps the linked-doc create path aligned with the current sync behavior, which already reads `doc_node_links`.

## 4. Persist Reference Ontology Nodes at Create Time

`DocService::create_view_impl(...)` should insert `doc_node_links` for any submitted `linked_node_ids`.

This should mirror the existing behavior already used by `create_doc(...)` for `linked_node_ids`.

Desired result after linked-doc creation:

- doc row exists
- persona/instruction metadata exists
- selected ontology references exist in `doc_node_links`
- starter markdown exists
- no bootstrap job is created yet

## 5. Make the Modal Searchable for Ontology References

The linked-doc form should include:

- search box
- results list
- selected-node chips/list

Recommended frontend hook:

- [useSearch.ts](/Users/lumost/workspace/g5/generation/client/src/hooks/useSearch.ts)

Recommended query mode:

- search type filter: `["node"]`

Selection behavior:

- user types at least 2 chars
- matching ontology nodes appear
- selecting a result adds it to `linkedNodeIds`
- user can remove selected nodes before submit

We should avoid requiring tree navigation inside the modal.

## 6. Make Plain Doc the Default

On modal open:

- default mode = `plain`
- no persona fields shown
- no reference node search shown

This should reduce friction for the common case.

## 7. Remove the Current Future-View Relay from This Modal

Today the modal delegates persona-based creation through `onCreateFutureView(...)`, which opens `CreateViewModal`.

That indirection should be removed from this modal.

Instead:

- plain doc mode creates immediately
- linked doc mode creates immediately through the linked-doc create endpoint

This keeps the modal self-contained.

`CreateViewModal` can remain in the codebase for other entry points if we still want a dedicated view-creation surface elsewhere.

## 8. Hide Sidebar Sync Arrows

The current sidebar tree row shows inline hover actions for:

- uplift
- downlift

Those are the up/down arrow buttons rendered in:

- [TreeNode.tsx](/Users/lumost/workspace/g5/generation/client/src/components/tree/TreeNode.tsx)

Recommendation:

- remove these inline arrow buttons from the sidebar row
- keep sync actions accessible from the more explicit doc/view surfaces and sync modal flow

Why:

- they add noise to the sidebar
- they make the row feel more operational than navigational
- they are easy to click accidentally
- this cleanup matches the simpler mental model for linked docs

## Exact Change Surface

### Frontend

#### Modal UI

- [CreateProjectDocModal.tsx](/Users/lumost/workspace/g5/generation/client/src/components/views/CreateProjectDocModal.tsx)

Changes:

- replace current mixed type grid with two top-level choices:
  - Plain doc
  - Linked doc
- default to Plain doc
- keep filename input
- keep a lightweight subtype selector for plain docs if we want to preserve current categories
- add linked-doc-only fields:
  - persona
  - instructions
  - ontology-node search and selection
- remove future-view delegation from submit path

#### Sidebar row cleanup

- [TreeNode.tsx](/Users/lumost/workspace/g5/generation/client/src/components/tree/TreeNode.tsx)

Changes:

- remove the hover-revealed `ArrowUp` / `ArrowDown` sync buttons for doc/view rows
- retain any non-sync actions we still want, such as future-view implementation if that remains part of the product

#### Page wiring

- [WorkspacePage.tsx](/Users/lumost/workspace/g5/generation/client/src/pages/WorkspacePage.tsx)

Changes:

- simplify modal state/props
- remove `onCreateFutureView(...)` relay from the project-doc modal path
- keep `CreateViewModal` only if still needed by separate explicit "Create View" entry points

#### View hook / request mapping

- [useViews.ts](/Users/lumost/workspace/g5/generation/client/src/hooks/useViews.ts)

Changes:

- extend `CreateViewInput`
- include `linkedNodeIds`
- map them into the new shared request contract

#### Search integration

- [useSearch.ts](/Users/lumost/workspace/g5/generation/client/src/hooks/useSearch.ts)
- [types.ts](/Users/lumost/workspace/g5/generation/client/src/lib/types.ts)

Likely no core hook changes needed, but the modal will need to consume:

- `SearchResult`
- `type === "node"`
- `id`
- `title`
- `breadcrumb`

### Shared Types

- [view.rs](/Users/lumost/workspace/g5/generation/g5-common/src/types/view.rs)

Changes:

- add `linked_node_ids` to `CreateViewRequest`

### Backend

- [docs.rs](/Users/lumost/workspace/g5/generation/g5-api/src/routes/docs.rs)
- [doc_service.rs](/Users/lumost/workspace/g5/generation/g5-api/src/services/doc_service.rs)

Changes:

- plumb `linked_node_ids` through the create-view route and service
- insert initial `doc_node_links` during linked-doc creation
- do not create bootstrap work at create time

This keeps the backend aligned with the already-correct "bootstrap on first sync" flow.

## Testing Plan

### Frontend tests

- [CreateProjectDocModal.test.tsx](/Users/lumost/workspace/g5/generation/client/src/components/CreateProjectDocModal.test.tsx)

Update to verify:

- plain doc is the default selection
- plain doc submit calls `POST /docs`
- linked doc mode reveals persona/instructions/reference search
- linked doc submit calls the linked-doc create path with `linkedNodeIds`

Likely add or update tests in:

- [WorkspacePageV2.test.tsx](/Users/lumost/workspace/g5/generation/client/src/pages/WorkspacePageV2.test.tsx)
- [useViews.ts](/Users/lumost/workspace/g5/generation/client/src/hooks/useViews.ts) test surface if present

### Backend tests

Add or update tests to verify:

- `create_view` accepts `linkedNodeIds`
- linked-doc creation inserts `doc_node_links`
- linked-doc creation still returns no bootstrap job
- first sync still bootstraps when no prior links exist, or uses submitted links if they already exist

Relevant backend suites:

- [api_views.rs](/Users/lumost/workspace/g5/generation/g5-api/tests/api_views.rs)
- any linked-doc / sync wiring E2E coverage

## Suggested Rollout

### Phase 1

Backend contract support:

- add `linkedNodeIds` to create-view request
- store `doc_node_links` during linked-doc creation

### Phase 2

Frontend modal simplification:

- new two-mode modal
- plain default
- linked-doc search picker
- hide sidebar sync arrows

### Phase 3

Cleanup:

- remove obsolete future-view relay from this modal
- rename modal/component if desired
- trim stale tests and copy

## Non-Goals

This change should **not** try to:

- redesign the separate dedicated `CreateViewModal` unless we explicitly choose to collapse both surfaces later
- change sync semantics
- create jobs during linked-doc creation
- rework ontology search itself beyond consuming the existing search API

## Open Decision

One small product choice remains:

- should Plain doc preserve the current subtype selector (`design`, `adr`, `plan`, `guide`, `note`)?

Recommendation:

- yes, keep it for now
- but keep it visually secondary so the main user decision is still just:
  - Plain doc
  - Linked doc

## Summary

The cleanest implementation is:

- Plain doc -> existing `create_doc`
- Linked doc -> current view-backed create path, extended with `linkedNodeIds`
- First sync/bootstrap remains the point where linked-doc scaffolding is fully initialized

That gives us the simpler UX you want without fighting the current backend model.
