# G5 Development Flows

## Overview

G5 uses a **Base Commit + CRDT Draft Overlay** model.

- **g5-api** serves as the CRDT authority (Yjs + Postgres).
- **g5d** is a local daemon responsible for git checkout, filesystem synchronization, and agent execution.
- All edits accumulate in a CRDT-backed draft layer.
- A commit materializes the draft into a Git checkpoint.

This architecture enables real-time collaboration, deterministic commits, and seamless integration with existing developer tooling.

---

## 1. CLI: Workspace and Orchestration

### Workspace Initialization

Create a new workspace:

```
g5d init ~/work/streaming
g5d add git@github.com:acme/backend
g5d serve ~/work/streaming
```

Clone an existing workspace:

```
g5d clone ws_123 ~/work/streaming
g5d serve ~/work/streaming
```

---

### Agent Control

```
g5 run uplift src/auth/
g5 run downlift auth/validator
g5 tasks --watch
g5 chat "refactor cache layer"
g5 diff
g5 status
```

---

### Forking and Reverting

Create a new workspace fork (including draft state):

```
g5d fork --name "experiment"
```

Discard all draft changes:

```
g5d revert --all
```

---

### Commit and Push

```
g5d commit -m "Refactor auth" --all
# Publish from the workspace root with the helper: it plans every member
# (mirror head, ancestry, lease, no URL rewrite), pushes one refspec per
# member with the bearer token scoped to the API, then refreshes the
# projections. Chain the steps: a failed plan must publish nothing.
cd ~/work/streaming
source "$G5_LEAN/scripts/lib/native-publish.sh" \
  && native_publish_configure "$API" "$TOKEN" \
  && native_publish_plan code "$PWD/repos/backend" "$CODE_REPO" "$BRANCH" \
  && native_publish_plan docs "$PWD/repos/docs" "$DOCS_REPO" "$BRANCH" \
  && native_publish_manifest_members_exist "$WS" \
  && native_publish_apply \
  && native_publish_sync_projections "$WS"
```

The mirror refuses the obsolete `X-G5-Force-Push` header and branch creation,
and compare-and-swaps against the head it advertised; an authorized rewrite
with a matching `old` is accepted, so the helper's ancestry check is what keeps
publication fast-forward only. The plain-Git form of one member's push, with
the protections the helper reproduces, is under "Publishing" in G5D_CLI.md.

#### Commit Guarantees

- The CRDT draft is fully materialized.
- A coordinated Git commit is created across repositories and documentation.
- The base commit advances.
- The draft resets to a clean state.

---

## 2. Browser Interface

The browser interface provides:

- Real-time collaborative editing (CRDT-based)
- Code review (diffs and comments)
- Workflow management (tasks, DAGs, monitoring)
- Hosted agents and conversational interfaces

The draft model is identical to the CLI.
A commit represents the same materialization boundary.

---

## 3. CLI + Third-Party Tools

Developers may use any editor or AI tool, including:

Claude, Cursor, Vim, JetBrains, Emacs, and others.

### Filesystem Bridge

Specifications:

```
.g5/specs/
```

Source repositories:

```
repos/<repo>/
```

The `g5d` daemon synchronizes the filesystem and CRDT state bidirectionally.

---

### Edit Loop

1. A tool saves a file.
2. The filesystem change is detected.
3. The change is converted to a CRDT delta.
4. The delta is synchronized to the server.

Conversely:

1. A browser edit produces a CRDT delta.
2. The delta is delivered to `g5d`.
3. The local file is updated.
4. The editor reloads.

Typical convergence latency: 50–200 ms.

---

### Example Workflow

```
g5d clone ws_123 ~/work/streaming
g5d serve ~/work/streaming

cd ~/work/streaming/repos/backend
claude "Implement RS256 verification"

g5d commit -m "Implement RS256" --all

# Publish every changed member and refresh its projections from the
# workspace root (see "Commit and Push" above): the receive endpoint only
# moves refs, so the helper plans each member, pushes one refspec per
# member, then re-projects specs for every manifest code member and the docs.
cd ~/work/streaming
source "$G5_LEAN/scripts/lib/native-publish.sh" \
  && native_publish_configure "$API" "$TOKEN" \
  && native_publish_plan code "$PWD/repos/backend" "$CODE_REPO" "$BRANCH" \
  && native_publish_plan docs "$PWD/repos/docs" "$DOCS_REPO" "$BRANCH" \
  && native_publish_manifest_members_exist ws_123 \
  && native_publish_apply \
  && native_publish_sync_projections ws_123
```

The plain-Git form of one member's push is in
[G5D_CLI.md](G5D_CLI.md#publishing-native-git-push-g5d-push-is-retired).

---

## Architecture

```
Developer Tool (Claude, Vim, IDE)
        ↕ Filesystem
       g5d
        ↕ CRDT
      g5-api
        ↕
     Postgres
```

- Editing may occur in any tool.
- Orchestration occurs via CLI or browser.
- Shared state is maintained by the CRDT authority.

---

## Capability Comparison

| Capability              | CLI | Browser     | CLI + Tool |
|-------------------------|-----|-------------|------------|
| Workspace lifecycle     | Yes | No          | Yes        |
| File editing            | No  | Real-time   | Save-level |
| Uplift / Downlift       | Yes | Yes         | Yes        |
| Agent chat              | Yes | Yes         | Yes        |
| Real-time co-editing    | No  | Yes         | Save-level |
| Task monitoring         | Yes | Yes         | Yes        |
| IntelliSense            | No  | IDE-native  | Native     |
| Offline support         | Git-level | No  | Git-level  |
| Installation required   | g5d | None        | g5d + tool |
