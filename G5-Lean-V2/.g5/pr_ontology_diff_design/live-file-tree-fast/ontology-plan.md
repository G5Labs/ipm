# Live Files tree in one runtime call

## Ontology changes and governance

**Comparison base:** `main` at `10238048`, branch `worktree-live-file-tree-fast`.
The [chat intent](../../chat_intent/live-file-tree-fast/request.md) holds the request
and the diagnosis.

**Ontology changes** in `features/documents.g5.md`:
- adds `g5.documents.read.live-tree` (one runtime call lists the attached owner's
  subtree; the VM applies ignore rules; the cap and an outdated runtime fall back to the
  committed cut with a reason);
- amends `g5.documents.files-view.refresh`: the owner's live view refreshes on
  file-change notifications *coalesced per workspace*, never cancelling a fetch in
  flight. The amendment and its reason are recorded in that block's metadata.

### Relevant and governing ontology

| Block | Why relevant | Implementation must enforce or consider |
|---|---|---|
| [Label the source being shown](../../ontology/features/documents.g5.md#label-the-source-being-shown) | The tree says which source it is | `source: "live"` only when the VM listed it; `repoOid` is the checkout HEAD the VM read beside the listing, or `null` when it read none, never the durable head; every fallback names its reason in `liveFallback`. |
| [Invalidate live content when authority changes](../../ontology/features/documents.g5.md#invalidate-live-content-when-authority-changes) | The listing is an owner-only live read | The one `fs.tree` call still goes through runtime admission (owner, VM, epoch, lease). |
| [Serve nonowners from durable state](../../ontology/concepts/security.g5.md#serve-nonowners-from-durable-state) | Readers never reach the VM | Unchanged: the live branch is taken only for the attached owner. |
| [Keep FILES reading views current](../../ontology/features/documents.g5.md#keep-files-reading-views-current) | File-change notifications drive refresh | Coalesce per workspace; a change refreshes at once unless a tree or Changes read is in flight, in which case it waits for that read and refreshes after it; a read hung past the bound is cancelled and repeated; later changes within the window refresh once after it; nothing is dropped. TanStack keeps a first read without data even on `cancelRefetch`, so those are cancelled explicitly. |
| [Justify each new mechanism](../../ontology/concepts/system_design_principles.g5.md#justify-each-new-mechanism) | One new RPC and one small browser throttle | No new storage. The outdated-runtime fallback is a labeled read of existing durable state, not a compatibility layer. |
| [Test real boundaries](../../ontology/concepts/system_design_principles.g5.md#test-real-boundaries-and-report-exact-evidence) | Guest, API and browser all change | Guest unit tests on a real directory tree; API tests through the fake dispatcher using the guest's own walker; vitest for coalescing. |

## Requested outcome

With the workspace VM running, the Files tab lists G5-Lean in about the time of one
runtime round trip, independent of the Changes report, and keeps listing it while an
agent or a build writes files in the VM.

## Design

```text
browser ── GET /repos/:id/files?depth=-1 ──▶ API list_file_tree
                                               │ committed tracked paths (storage)
                                               │ fs.watch(root)        ─▶ VM (one watch covers the subtree)
                                               │ fs.tree(root, depth,  ─▶ VM walks its checkout:
                                               │          tracked, cap)    .gitignore layers, tracked kept,
                                               │                           hidden paths skipped, cap → truncated,
                                               │                           + checkout HEAD read from .git (or null)
                                               ▼
                 source=live (repoOid = checkout HEAD | null)
                 | committed + liveFallback ∈ {entry_cap, runtime_outdated}
                 No Changes run is started or awaited for the tree.

VM fs events ─▶ host ─▶ API ─▶ ws workspace.file_changed ─▶ FileChangeCoalescer
                                      no tree/Changes read in flight: refresh now (keep fetches)
                                      one in flight: hold the change, refresh once it returns
                                      burst: one trailing refresh per window
                                      read hung past the bound: cancel it and read again
                                      file bodies and documents: read again at once
```

The ignore rules move from the API into the guest crate (`g5d-dispatch::file_tree`) as a
walker over a small `TreeSource` trait; the guest's confined file service and the API
tests' in-memory dispatcher both implement it, so one implementation defines the live
tree. The walk is bounded in entries (10,000) and in bytes (3 MiB of entries, counted
as the escaped JSON string of each path plus its envelope, under the runtime's 4 MiB
reply limit); a directory wider than one guest listing counts as past the cap. The API
sends the committed tracked paths under the subtree; when they already exceed the cap
the live tree cannot fit and the committed cut is served without a VM call.
`METHOD_NOT_FOUND` from a VM that predates `fs.tree` serves the committed cut labeled
`runtime_outdated`.

Acceptance: the live-tree API tests pass with exactly one `fs.tree` and no `fs.list`,
with the live tree labeled by the HEAD the VM read (distinct from the durable head) and
`null` when it read none, and without starting or awaiting a Changes run; an oversized
checkout and an outdated runtime both serve the committed cut with their reason; the
coalescer's unit tests cover first-event, burst, in-flight and disconnect.

Risks and assumptions: the walk's deadline is the existing 30 s runtime budget, ample
for a 10,000-entry cap on a local filesystem. A VM running the old dispatcher shows
committed files until it restarts. The root watch registers the same pruned directory
set the tree walks (review round 3), so ignored output raises no notifications and the
watch fits the native limit; tracked files inside ignored directories are listed but
not watched.

## Review summary

Implemented as designed. Both reuse assumptions held: `render_file_tree_entries`
takes the walker's entries as `StorageEntry` values, and `committed_tracked_paths`
yields repository-relative paths that the guest matches byte for byte (the API test
seeds `README.md` as tracked and ignored by `*.md`, and it stays listed).

Evidence (2026-10-07, this checkout): `cargo test -p g5d-dispatch --lib tree` (15
passed, including `fs.tree` on a real directory); `cargo test -p g5-api --test
api_workspace_files_live_tree` (3 passed: pruning with one `fs.tree`/one `fs.watch`/no
`fs.list`, cap fallback, outdated-runtime fallback); `--test api_setup_e2e` and
`--test current_reader_contract` (29 and 3 passed); vitest `fileChangeCoalescer.test.ts`
(8), `ws.workspaceAuthority.test.ts` (6), `useDocs.liveChanges.test.tsx` (10).

Not executed: the change against a real VM on QA (needs the deployed dispatcher), and
a browser end-to-end flow of the Files tab during a build. Remaining uncertainty:
whether QA VMs restored from snapshots pick up the new dispatcher without a restart;
until they do, their Files tab shows the committed cut with the generic fallback notice.
