# G5 Backend API & Postgres Storage Design

> Companion document to WORKER_INFRASTRUCTURE.md. This document specifies the
> concrete Postgres schema, REST/WebSocket API surface, and data flow for the
> G5 API server (`g5-api`). It is derived from:
>
> 1. The React prototype mock types (`client/src/data/mock.ts`)
> 2. The architectural decisions in WORKER_INFRASTRUCTURE.md (§0–§14)

---

## 0. Design-Level Constraints

Refer to WORKER_INFRASTRUCTURE.md §0 for the four optimization criteria. The
storage and API design follows from them:

| Principle | Storage / API implication |
|-----------|--------------------------|
| **P1 – Native editing** | CRDT state (Yjs) is persisted server-side in Postgres. Every draft session row holds a serialized Yjs document so that closing a browser tab never loses work. |
| **P2 – Real FS + compute** | Worker state (heartbeat, task assignment) lives in Redis with a Postgres audit trail. The API surface for workers is a thin REST layer; heavy interaction goes through the Yjs WebSocket and Redis pub/sub. |
| **P3 – Isolated, committed** | Workspaces map 1:1 to git branches. Tasks create sub-branches. Postgres stores metadata *about* these branches, not the file content itself (git is canonical for file content). |
| **P4 – Collaborative editing** | Draft sessions, comment indexes, and chat history all live in Postgres. Cross-branch comment discovery is powered by the comment index table. |

**What Postgres stores:** metadata, relationships, CRDT blobs, chat history,
comment indexes, cost accounting, session history, user/org identity.

**What Postgres does NOT store:** file content (git), live CRDT awareness
(in-memory Yjs + WebSocket), ephemeral worker health (Redis), job queues
(Redis streams).

---

## 1. Entity Relationship Overview

```
┌──────────────┐       ┌──────────────┐       ┌──────────────────┐
│   org        │──1:N──│   user       │──N:M──│  org_membership  │
└──────┬───────┘       └──────┬───────┘       └──────────────────┘
       │                      │
       │ 1:N                  │ created_by
       ▼                      ▼
┌──────────────┐       ┌──────────────┐
│   repo       │       │  workspace   │ ◄──1:1── workspace_settings
└──────────────┘       └──────┬───────┘
       ▲                      │
       │ FK                   │ 1:N
       │                      ▼
       │               ┌──────────────────┐
       └───────────────│ workspace_branch │──1:1──┐
                       │ (workspace×repo) │       │
                       │ + head_commit   │       │
                       └──────┬──────────┘       │
                              │                  │
                        ┌─────┴──────┐    ┌──────┴──────────┐
                        │  ontology  │    │ draft_session   │
                        │  _node     │    │ (per branch)    │
                        └─────┬──────┘    └──────┬──────────┘
                              │ self-ref         │
                              │                  │ participants
                              │ (via node_id)    ▼
                              │           ┌──────────────┐
              ┌──────────┐    │           │ds_participant│
              │  job      │   │           └──────────────┘
              └────┬─────┘   │
                   │         │
              ┌────┴────┐    │
              │  task    │   └───────┐
              └────┬────┘           │
                   │         ┌──────┴──────────┐
              ┌────┴────┐    │  comment_index  │
              │ timeline │   └─────────────────┘
              │ _entry   │          ▲
              └──────────┘          │
                              ┌─────┴────────────┐
              ┌──────────────────┐  comment_reply │
              │  chat_session    │  └─────────────┘
              └──────┬───────────┘
                     │ 1:N
              ┌──────┴───────────┐
              │  chat_message    │
              └──────────────────┘

              ┌──────────────────┐       ┌──────────────────┐
              │  feature         │       │  plan             │
              └──────────────────┘       └──────────────────┘

              ┌──────────────────┐       ┌──────────────────┐
              │  node_dependency │       │  workspace_dep   │
              └──────────────────┘       └──────────────────┘
```

---

## 2. Postgres Schema

All tables use `gen_random_uuid()` for internal primary keys (Postgres 14+).
However, the **API surface uses path-based `component_id` identifiers** —
deterministic, human-readable strings derived from the filesystem or
organizational hierarchy (e.g., `auth/validator`, `task-graph-rework`).
Each entity table has a unique `component_id TEXT` column alongside its
UUID PK. The API accepts and returns `component_id`; UUIDs are internal
to Postgres and never exposed to clients.

Timestamps are `TIMESTAMPTZ`. Text enums use `CHECK` constraints rather
than Postgres `ENUM` types to allow zero-downtime migration.

### 2.1 Identity & Access

```sql
-- ─── Organisations ───────────────────────────────────────────────────────
CREATE TABLE orgs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    slug        TEXT NOT NULL UNIQUE,              -- URL-safe identifier
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── Users ───────────────────────────────────────────────────────────────
CREATE TABLE users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email       TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    avatar_url  TEXT,
    oauth_provider TEXT,                            -- 'github', 'google', etc.
    oauth_id    TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (oauth_provider, oauth_id)
);

-- ─── Org Membership ──────────────────────────────────────────────────────
CREATE TABLE org_memberships (
    org_id      UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role        TEXT NOT NULL DEFAULT 'member'
                CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (org_id, user_id)
);

-- ─── SCM Connections (GitHub / GitLab tokens) ────────────────────────────
-- Stores encrypted OAuth tokens for source-code providers. A user may have
-- one connection per provider. These tokens are used to:
--   1. List repos the user can access on the provider (import flow)
--   2. Clone private repos in workers and editors
--   3. Push branches / open PRs on behalf of the user
--
-- Separate from the login OAuth — a user may log in with Google but
-- connect GitHub for repo access.
CREATE TABLE user_scm_connections (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider          TEXT NOT NULL
                      CHECK (provider IN ('github', 'gitlab', 'bitbucket')),
    provider_user_id  TEXT NOT NULL,                -- Provider's numeric user ID
    provider_username TEXT,                         -- e.g. GitHub login, GitLab username
    access_token_enc  BYTEA NOT NULL,              -- AES-256-GCM encrypted
    refresh_token_enc BYTEA,                       -- Encrypted; NULL for non-expiring tokens
    token_expires_at  TIMESTAMPTZ,                 -- NULL = non-expiring (classic GitHub OAuth)
    scopes            TEXT[] NOT NULL DEFAULT '{}', -- Granted OAuth scopes
    installation_id   BIGINT,                      -- GitHub App installation ID (if applicable)
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, provider)
);
CREATE INDEX idx_scm_conn_provider ON user_scm_connections(provider, provider_user_id);
```

### 2.2 Repos & Workspaces

A **workspace** is the unit of work a user (or agent) opens in G5. It
owns one or more **workspace branches**, each pinned to a specific git
**commit** with an optional **draft** (CRDT) overlay on top. When an
agent "checks out a workspace", it is cloning/fetching each repo,
checking out the branch at its `head_commit`, and applying the draft
overlay.

A workspace is scoped to an **org** (not to a separate entity).
There is no N:M join table — the workspace itself declares which repos
it touches via `workspace_branches`.

**Tracking vs. pinned branches:**

Each `workspace_branch` has a `tracking_mode`:

| Mode | Writable | Drafts | Behavior |
|------|----------|--------|----------|
| `pinned` (default) | Yes | Yes | Working branches (e.g., `feat/streaming-api`) forked from base; `head_commit` fixed until explicitly advanced |
| `tracking` | Yes | Yes | Branch auto-advances `head_commit` to remote HEAD on push (via webhook or periodic poll) |

Tracking mode is useful for keeping a workspace in sync with a repo's
default branch — browsing the ontology and reading code at the latest
commit. When the user creates new working branches (via
`POST /api/v1/workspaces/:id/branches`), those branches default to
`pinned` mode.

**Cloning:** Any workspace can be **cloned** via
`POST /api/v1/workspaces/:id/clone`. The clone is a deep copy of the
source — workspace branches, ontology nodes, docs, settings, and
critically, **any active draft sessions** (including the serialized Yjs
document with all uncommitted edits). Cloning a dirty workspace produces
a clone with the same dirty state. See §3.1 for API details.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                               Workspace                                      │
│                                                                              │
│  Org: acme                                                                  │
│  ├── Repo: acme/frontend       (React app)                                 │
│  ├── Repo: acme/backend        (Rust API server)                           │
│  └── Repo: acme/shared-types   (Protobuf definitions)                      │
│                                                                              │
│  Workspace "add-streaming-api"                                              │
│  ├── workspace_branch: acme/backend                                        │
│  │   ├── branch: feat/streaming-api                                        │
│  │   ├── base:   main                                                      │
│  │   ├── head_commit: abc1234   ← pinned commit                           │
│  │   └── draft_session: { yjs_state, dirty_files }                         │
│  │                                                                          │
│  ├── workspace_branch: acme/frontend                                       │
│  │   ├── branch: feat/streaming-ui                                         │
│  │   ├── base:   main                                                      │
│  │   ├── head_commit: def5678                                              │
│  │   └── draft_session: { yjs_state, dirty_files }                         │
│  │                                                                          │
│  └── workspace_branch: acme/shared-types                                   │
│      ├── branch: feat/streaming-types                                      │
│      ├── base:   main                                                      │
│      ├── head_commit: 789abcd                                              │
│      └── draft_session: (none — no uncommitted edits)                      │
│                                                                              │
│  Each repo has its own ontology root(s):                                    │
│  ├── frontend ontology:   UI Components → Pages → Design System            │
│  ├── backend ontology:    Core → Execution Engine → Storage                │
│  └── shared ontology:     Types → Messages → Enums                         │
│                                                                              │
│  Agent checkout = for each workspace_branch:                                │
│    git fetch <repo> && git checkout <head_commit>                           │
│    apply draft overlay (Yjs → files)                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

```sql
-- ─── Repos ───────────────────────────────────────────────────────────────
-- A repo corresponds to a single git repository.
CREATE TABLE repos (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    slug            TEXT NOT NULL,                      -- URL-safe: <org>/<slug>
    git_url         TEXT NOT NULL,                      -- Clone URL (HTTPS)
    default_branch  TEXT NOT NULL DEFAULT 'main',
    provider        TEXT,                               -- 'github', 'gitlab', NULL = manual
    provider_repo_id TEXT,                              -- Provider's numeric repo ID
    provider_full_name TEXT,                            -- e.g. 'acme/backend' on GitHub
    webhook_secret_enc BYTEA,                          -- Encrypted; for push/PR webhooks
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (org_id, slug),
    UNIQUE (provider, provider_repo_id)                 -- At most one row per provider repo
);

-- ─── Workspaces ─────────────────────────────────────────────────────────
-- A workspace is the top-level unit a user or agent opens. It lives directly
-- under an org. It declares which repos it touches via workspace_branches.
--
-- Every org has exactly one **base workspace** (is_base = true). The base
-- workspace includes all repos at their default branches and the full
-- docs repo. All other workspaces are subsets of the base — forked from
-- it (or from another workspace forked from it) and intended to merge back.
--
-- cloned_from_id tracks provenance when a workspace is cloned. The clone
-- gets a deep copy of the source's workspace_branches (with new branch names),
-- ontology_nodes, and — critically — any active draft_sessions (including
-- the serialized Yjs document). This means cloning a dirty workspace
-- produces a clone with the same uncommitted edits.
--
-- docs_repo_id / docs_branch_name point to the org's docs repository.
-- On disk, g5d materializes this as an independent repository checkout
-- at .g5/docs/; the parent repository does not stage or pin it.
-- The base workspace tracks the default branch; child workspaces may
-- branch it for edits or keep it in tracking mode.
CREATE TABLE workspaces (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    is_base         BOOLEAN NOT NULL DEFAULT false,     -- Exactly one per org
    status          TEXT NOT NULL DEFAULT 'idea'
                    CHECK (status IN (
                        'idea', 'active', 'blocked', 'review', 'integrated'
                    )),
    description     TEXT,
    cloned_from_id  UUID REFERENCES workspaces(id) ON DELETE SET NULL,
    docs_repo_id    UUID REFERENCES repos(id),          -- Org's docs repository
    docs_branch_name TEXT,                              -- Branch in docs repo for this workspace
    docs_tracking_mode TEXT NOT NULL DEFAULT 'pinned'
                    CHECK (docs_tracking_mode IN ('tracking', 'pinned')),
    visibility      TEXT NOT NULL DEFAULT 'org'
                    CHECK (visibility IN ('org', 'invite_only')),
                                                    -- 'org' = visible to all org
                                                    -- members. 'invite_only' =
                                                    -- visible only to creator and
                                                    -- explicit workspace_members.
    created_by      UUID REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── Workspace Branches (per-repo: commit pin + branch) ─────────────────
-- One row per (workspace, repo) pair. This is what the agent checks out:
-- the branch at head_commit, with the draft overlay on top.
CREATE TABLE workspace_branches (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    repo_id     UUID NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    branch_name TEXT NOT NULL,
    base_branch TEXT NOT NULL DEFAULT 'main',       -- Parent branch in this repo
    head_commit TEXT NOT NULL,                      -- Pinned SHA — the commit the
                                                    -- draft is based on. Updated
                                                    -- on commit, pull, rebase.
    tracking_mode TEXT NOT NULL DEFAULT 'pinned'
                  CHECK (tracking_mode IN ('tracking', 'pinned')),
    materialization_status TEXT NOT NULL DEFAULT 'materialized'
                  CHECK (materialization_status IN ('materialized', 'unborn', 'missing_ref', 'error')),
                                                    -- 'tracking' = auto-advance
                                                    -- head_commit to remote HEAD
                                                    -- on push. 'pinned' = fixed
                                                    -- to a specific commit.
    UNIQUE (workspace_id, repo_id),
    UNIQUE (repo_id, branch_name)                   -- One workspace per branch per repo
);

-- ─── Workspace Dependencies (DAG) ───────────────────────────────────────
-- Models the mock's `dependencies: string[]` as an explicit join table.
CREATE TABLE workspace_dependencies (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    depends_on   UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (workspace_id, depends_on),
    CHECK (workspace_id <> depends_on)
);
CREATE INDEX idx_ws_dep_reverse ON workspace_dependencies(depends_on);

-- ─── Workspace Members (access control for invite-only workspaces) ────────
-- When a workspace has visibility = 'invite_only', only users listed here
-- (plus the workspace creator) can see or interact with it.
-- For visibility = 'org', this table is unused — all org members have access.
CREATE TABLE workspace_members (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role         TEXT NOT NULL DEFAULT 'editor'
                 CHECK (role IN ('admin', 'editor', 'viewer')),
    invited_by   UUID REFERENCES users(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);

-- ─── Org Invitations (Pending Membership) ───────────────────────────────
-- When `POST /api/v1/orgs/:orgId/members` is called with an email
-- that does not match an existing G5 user, a pending invitation is
-- created here. On the invited user's first login (matching email),
-- the invitation is auto-accepted and an org_memberships row is
-- created. Invitations expire after 7 days and are garbage-collected
-- by a background job.
--
-- State machine:
--   pending ───┬───▶ accepted   (user signs up / logs in with matching email,
--              │                  OR calls POST /api/v1/invitations/:id/accept)
--              ├───▶ declined   (user calls POST /api/v1/invitations/:id/decline)
--              └───▶ expired    (background job: expires_at < now())
--
--   Transitions from accepted/declined/expired are terminal.
CREATE TABLE org_invitations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    email       TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'member'
                CHECK (role IN ('admin', 'member', 'viewer')),
    invited_by  UUID NOT NULL REFERENCES users(id),
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'accepted', 'declined', 'expired')),
    accepted_by UUID REFERENCES users(id),           -- Set on acceptance
    expires_at  TIMESTAMPTZ NOT NULL
                DEFAULT (now() + interval '7 days'),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (org_id, email, status)                   -- One pending invite per email per org
);

CREATE INDEX idx_invitation_org ON org_invitations(org_id, status);
CREATE INDEX idx_invitation_email ON org_invitations(email)
    WHERE status = 'pending';                        -- Fast lookup on login
CREATE INDEX idx_invitation_gc ON org_invitations(expires_at)
    WHERE status = 'pending';                        -- GC sweep
```

### 2.3 Ontology Nodes

The ontology tree is the core structural artifact. Each **repo** has its own
ontology tree(s) — root nodes have `parent_id IS NULL`. A workspace that spans
multiple repos sees a **forest** of ontology trees (one per repo). The
`workspace_branch_id` links each node to the specific (workspace, repo) context.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Ontology Forest (multi-repo workspace)                    │
│                                                                              │
│  Workspace: "add-streaming-api"                                              │
│                                                                              │
│  ┌─ acme/backend (feat/streaming-api) ──────────────────────────────┐      │
│  │  Core                                                              │      │
│  │  ├── Execution Engine                                              │      │
│  │  │   ├── hashtable_aggregate                                      │      │
│  │  │   └── streaming_operator  ← NEW                                │      │
│  │  └── Storage                                                       │      │
│  └────────────────────────────────────────────────────────────────────┘      │
│                                                                              │
│  ┌─ acme/frontend (feat/streaming-ui) ──────────────────────────────┐      │
│  │  UI Components                                                     │      │
│  │  ├── Pages                                                         │      │
│  │  │   └── StreamingDashboard  ← NEW                                │      │
│  │  └── Design System                                                 │      │
│  └────────────────────────────────────────────────────────────────────┘      │
│                                                                              │
│  ┌─ acme/shared-types (feat/streaming-types) ───────────────────────┐      │
│  │  Types                                                              │      │
│  │  └── Messages                                                       │      │
│  │      └── StreamEvent  ← NEW                                        │      │
│  └────────────────────────────────────────────────────────────────────┘      │
│                                                                              │
│  Cross-repo dependencies are expressed via node_dependencies:               │
│    streaming_operator → StreamEvent (backend depends on shared types)      │
│    StreamingDashboard → StreamEvent (frontend depends on shared types)     │
│                                                                              │
│  Compile walks the full cross-repo DAG. The scheduler creates tasks        │
│  for each node regardless of which repo it lives in.                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

```sql
-- ─── Ontology Nodes ──────────────────────────────────────────────────────
-- Maps the mock OntologyNode { id, parentId, name, type, filePath, content,
-- children }. Children are expressed via parent_id self-reference.
--
-- Each ontology node belongs to a specific (workspace, repo) via
-- workspace_branch_id. Root nodes have parent_id IS NULL. A multi-repo
-- workspace will have multiple root nodes (one per repo ontology root).
CREATE TABLE ontology_nodes (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_branch_id UUID NOT NULL REFERENCES workspace_branches(id)
                       ON DELETE CASCADE,
    parent_id          UUID REFERENCES ontology_nodes(id) ON DELETE CASCADE,

    name        TEXT NOT NULL,
    node_type   TEXT NOT NULL
                CHECK (node_type IN ('node', 'document', 'code')),
    file_path   TEXT,                               -- For code nodes
    content_hash TEXT,                              -- SHA-256 of content

    -- Ordering within siblings
    sort_order  INT NOT NULL DEFAULT 0,

    -- Sync metadata (relative to base branch)
    sync_status TEXT NOT NULL DEFAULT 'synced'
                CHECK (sync_status IN (
                    'synced', 'modified', 'added', 'deleted', 'conflict'
                )),
    ahead_of_base INT NOT NULL DEFAULT 0,
    behind_base   INT NOT NULL DEFAULT 0,

    -- Staleness tracking for uplift/downlift (AGENTS.md §3.7)
    last_uplifted_at    TIMESTAMPTZ,              -- When spec was last produced/refreshed
    last_downlifted_at  TIMESTAMPTZ,              -- When code was last produced/refreshed
    content_updated_at  TIMESTAMPTZ,              -- When content was last modified (any source)

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (workspace_branch_id, parent_id, name)
);

-- Efficient tree queries: find children, find root nodes
CREATE INDEX idx_onto_parent ON ontology_nodes(workspace_branch_id, parent_id);
CREATE INDEX idx_onto_workspace_branch ON ontology_nodes(workspace_branch_id);

-- Note: node *content* lives in git (markdown files, source files).
-- Postgres stores the tree structure and metadata only. Content is served
-- by reading from the editor's git checkout or the worker's worktree.
--
-- Ontology ↔ Git sync: When a task completes (AGENTS.md §6.5), the
-- completion hook commits and pushes spec files to .g5/specs/ on the task branch.
-- A post-push reconciliation pass reads the YAML frontmatter of each
-- changed spec file and upserts ontology_nodes metadata (content_hash,
-- sync_status, last_uplifted_at / last_downlifted_at). The SpecContext
-- (component_id, src_files, parent_spec, child_specs) stored in the spec
-- frontmatter is the source of truth; the DB mirrors it for query
-- performance and graph traversal.

-- ─── Node Dependencies ──────────────────────────────────────────────────
-- Directed dependency edges: from_node_id depends on to_node_id.
-- (Docs use bidirectional 'links' instead — see doc_node_links table.)
CREATE TABLE node_dependencies (
    from_node_id UUID NOT NULL REFERENCES ontology_nodes(id) ON DELETE CASCADE,
    to_node_id   UUID NOT NULL REFERENCES ontology_nodes(id) ON DELETE CASCADE,
    dep_type     TEXT NOT NULL DEFAULT 'direct'
                 CHECK (dep_type IN ('direct', 'inherited')),
    PRIMARY KEY (from_node_id, to_node_id)
);
CREATE INDEX idx_node_dep_to ON node_dependencies(to_node_id);
CREATE INDEX idx_node_dep_from ON node_dependencies(from_node_id);
```

### 2.4 Docs

Docs are markdown documents that capture cross-cutting design intent.
A doc may be standalone or linked to ontology nodes (participating in
uplift/downlift workflows). Linkedness is determined by the presence of
rows in `doc_node_links` — which is a queryable index of the doc's YAML
frontmatter `linked_nodes` field.

```sql
-- ─── Docs ────────────────────────────────────────────────────────────────
CREATE TABLE docs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    doc_type    TEXT NOT NULL DEFAULT 'design'
                CHECK (doc_type IN ('spec', 'design', 'api', 'guide', 'adr', 'note')),
    file_path   TEXT,                               -- .g5/docs/<name>.md

    -- Staleness tracking (for docs linked to ontology nodes;
    -- NULL for standalone docs that have no node links)
    last_uplifted_at    TIMESTAMPTZ,              -- When this doc was last re-synced from specs
    last_downlifted_at  TIMESTAMPTZ,              -- When linked specs were last updated from this doc
    content_updated_at  TIMESTAMPTZ,              -- When doc content was last modified

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── Doc ↔ Node Links ───────────────────────────────────────────────────
-- Queryable index of the YAML frontmatter `linked_nodes` field in
-- each doc's markdown file. Same pattern as how ontology_nodes index
-- spec metadata: the YAML frontmatter in git is the source of truth;
-- this table mirrors it for fast queries and graph traversal.
CREATE TABLE doc_node_links (
    doc_id      UUID NOT NULL REFERENCES docs(id) ON DELETE CASCADE,
    node_id     UUID NOT NULL REFERENCES ontology_nodes(id) ON DELETE CASCADE,
    PRIMARY KEY (doc_id, node_id)
);
CREATE INDEX idx_doc_links_node ON doc_node_links(node_id);
```

### 2.5 Draft Sessions (CRDT)

Per WORKER_INFRASTRUCTURE.md §11.7 — server-authoritative Yjs drafts.

Each draft session is scoped to a **workspace branch** (= one repo within
a workspace). A multi-repo workspace with 3 repos = up to 3 independent
draft sessions. The draft's base commit is `workspace_branches.head_commit`
— the draft records edits *on top of* that pinned SHA. When the user
commits, `head_commit` advances and the draft resets.

**Persistence strategy: update-log + compaction.**

Rather than overwriting a single large `BYTEA` blob on every persist (which
causes TOAST rewrite and VACUUM pressure), we use an append-only update log:

1. **Write path:** Every 2–5s of editing activity, the server appends a small
   Yjs incremental update (~100B–2KB) as an INSERT to `draft_updates`.
   No row contention, no blob rewrite.
2. **Read path:** Load the compacted snapshot from `draft_sessions.yjs_state`,
   then replay any `draft_updates` rows created after `compacted_at`.
   Typically <10 updates to replay.
3. **Compaction:** A background job runs every 50 updates or 5 minutes
   (whichever comes first). It merges all pending updates into the snapshot
   via `Y.mergeUpdates()`, writes the result to `yjs_state`, advances
   `compacted_at`, and deletes the compacted update rows.

This mirrors the approach used by Hocuspocus (Tiptap's production Yjs
server) and similar systems. The snapshot column provides fast cold-start
reads; the update log provides efficient writes.

```sql
-- ─── Draft Sessions ─────────────────────────────────────────────────────
-- One active draft session per workspace_branch (= per repo within a workspace).
-- Contains the compacted Yjs snapshot that tracks uncommitted edits on
-- top of workspace_branches.head_commit. Incremental updates are appended
-- to draft_updates (see below) and periodically compacted back into
-- yjs_state.
CREATE TABLE draft_sessions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_branch_id UUID NOT NULL REFERENCES workspace_branches(id)
                        ON DELETE CASCADE,

    -- Compacted Yjs document snapshot (binary). Updated by compaction job.
    -- On cold start: load this + replay draft_updates where seq > compacted_seq.
    yjs_state   BYTEA NOT NULL DEFAULT '\x'::bytea,

    -- Compaction bookkeeping
    compacted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),  -- When yjs_state was last compacted
    compacted_seq BIGINT NOT NULL DEFAULT 0,           -- Highest draft_updates.seq included in snapshot
    pending_updates INT NOT NULL DEFAULT 0,            -- Count of un-compacted updates (denormalized)

    -- Computed diff summary (denormalized for fast display)
    diff_summary JSONB NOT NULL DEFAULT '{"files":[],"totalAdditions":0,"totalDeletions":0}',

    -- Dirty tracking
    dirty_files  TEXT[] NOT NULL DEFAULT '{}',
    dirty_count  INT NOT NULL DEFAULT 0,

    -- Lifecycle
    status       TEXT NOT NULL DEFAULT 'active'
                 CHECK (status IN ('active', 'committing', 'discarding')),
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    persisted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (workspace_branch_id)                      -- At most one draft per branch
);

-- ─── Draft Updates (Append-Only Yjs Update Log) ────────────────────────
-- Incremental Yjs updates appended during editing. Each row is a small
-- binary delta (~100B–2KB) produced by Y.encodeStateAsUpdate(). The
-- server appends an INSERT every 2–5s of activity — no contention with
-- other writers.
--
-- Compaction job: when pending_updates >= 50 OR age of oldest pending
-- update > 5 min, the compaction job:
--   1. SELECT update_data FROM draft_updates
--      WHERE session_id = $1 AND seq > (SELECT compacted_seq FROM draft_sessions WHERE id = $1)
--      ORDER BY seq;
--   2. Merge all updates + existing yjs_state via Y.mergeUpdates()
--   3. UPDATE draft_sessions SET yjs_state = $merged, compacted_seq = $max_seq,
--          compacted_at = now(), pending_updates = 0 WHERE id = $1;
--   4. DELETE FROM draft_updates WHERE session_id = $1 AND seq <= $max_seq;
CREATE TABLE draft_updates (
    seq         BIGSERIAL PRIMARY KEY,              -- Monotonic, used for compaction cursor
    session_id  UUID NOT NULL REFERENCES draft_sessions(id) ON DELETE CASCADE,
    update_data BYTEA NOT NULL,                     -- Y.encodeStateAsUpdate() delta
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_draft_updates_session ON draft_updates(session_id, seq);

-- ─── Draft Participants ─────────────────────────────────────────────────
-- Who is currently connected to a draft session (live presence).
-- Rows are inserted on WebSocket connect, deleted on disconnect.
-- Required for the Yjs awareness protocol: shows collaborator cursors
-- and presence indicators in the Monaco editor. Agents also appear as
-- participants when they join a task's draft session (AGENTS.md §4).
CREATE TABLE draft_participants (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id   UUID NOT NULL REFERENCES draft_sessions(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id),
    role         TEXT NOT NULL DEFAULT 'editor'
                 CHECK (role IN ('editor', 'reviewer')),
    connected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    cursor_data  JSONB                              -- { file, line, col }
);

CREATE INDEX idx_dp_session ON draft_participants(session_id);
```

### 2.6 Comments (Cross-Branch Index)

Per WORKER_INFRASTRUCTURE.md §11.7 — all comments indexed in editor DB.

```sql
-- ─── Comment Index ──────────────────────────────────────────────────────
-- Every comment (line-level and node-level) is indexed here for
-- cross-branch discovery. Line-level comments are additionally stored
-- in git (notes or .g5/comments), but the index enables queries like
-- "show all comments on src/auth.rs across all branches".
CREATE TABLE comment_index (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    author_id   UUID NOT NULL REFERENCES users(id),

    -- Scope (determines primary storage and default visibility)
    scope       TEXT NOT NULL CHECK (scope IN ('line', 'node')),

    -- Node-level anchor (always set for node-scope, optional for line-scope)
    node_id     UUID REFERENCES ontology_nodes(id) ON DELETE SET NULL,

    -- Line-level anchor (set when scope = 'line')
    file_path   TEXT,
    line_number INT,
    commit_sha  TEXT,                               -- SHA at creation time
    commit_range_base TEXT,                         -- Diff base SHA
    commit_range_head TEXT,                         -- Diff head SHA

    -- Content
    content     TEXT NOT NULL,
    resolved    BOOLEAN NOT NULL DEFAULT FALSE,
    resolved_at TIMESTAMPTZ,

    -- Provenance
    origin_branch TEXT NOT NULL,                    -- Branch where comment was created
    branch_deleted BOOLEAN NOT NULL DEFAULT FALSE,  -- Set when branch is deleted

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Cross-branch queries: "all comments on this file across branches"
CREATE INDEX idx_comment_file ON comment_index(workspace_id, file_path)
    WHERE scope = 'line';

-- Node-scoped queries: "all comments on this ontology node"
CREATE INDEX idx_comment_node ON comment_index(node_id)
    WHERE scope = 'node';

-- Branch-scoped queries: "all comments on this branch"
CREATE INDEX idx_comment_branch ON comment_index(workspace_id, origin_branch);

-- GC stale comments: branch_deleted = true AND updated_at < (now - 90d)
CREATE INDEX idx_comment_gc ON comment_index(branch_deleted, updated_at)
    WHERE branch_deleted = TRUE;

-- ─── Comment Replies ────────────────────────────────────────────────────
CREATE TABLE comment_replies (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    comment_id  UUID NOT NULL REFERENCES comment_index(id) ON DELETE CASCADE,
    author_id   UUID NOT NULL REFERENCES users(id),
    content     TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_reply_comment ON comment_replies(comment_id, created_at);
```

### 2.7 Jobs & Tasks (Execution)

Jobs are the server-side unit of orchestrated work. A user (or agent)
requests an operation — compile, uplift, test, etc. — targeting one or more
ontology nodes. The server creates a **bootstrap task** that a worker
executes to analyze the codebase and produce the task DAG. Once bootstrap
completes, workers poll for regular tasks and the scheduler allocates them
using locality-aware scoring.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Job Lifecycle (server-managed)                             │
│                                                                              │
│  1. Client calls POST /api/v1/workspaces/:workspaceId/jobs                      │
│     → specifies job_type (compile | uplift), start_node, and force         │
│                                                                              │
│  2. Server resolves job ordering using **lanes**:                            │
│     → Batch lane (compile, uplift, downlift, test, custom):                │
│       depends_on_job = most recent batch job in this workspace             │
│       Batch jobs chain sequentially — uplift waits for compile, etc.       │
│     → Interactive lane (chat, review):                                     │
│       depends_on_job = NULL (runs immediately, never queued)               │
│       Chat and review jobs always start without waiting.                   │
│     → Override: client can set depends_on_job explicitly to force          │
│       specific ordering or set NULL to bypass lane defaults                │
│                                                                              │
│  3. Server creates a single bootstrap task (task_type = 'bootstrap'):      │
│     → task.node_id = job.start_node                                        │
│     → task.depends_on = [] (no dependencies — it runs first)               │
│     → Job enters 'bootstrapping' status                                    │
│                                                                              │
│  4. A worker polls and picks up the bootstrap task:                         │
│     → Clones repo / sets up worktree                                       │
│     → For compile/uplift: runs codebase analysis (ANALYSIS_DESIGN.md)      │
│       Resolves the ontology dependency graph, checks staleness              │
│     → For downlift: reads the start node's spec tree                       │
│     → Submits the task graph back to the server via                         │
│       POST /api/internal/jobs/:id/tasks                                     │
│     → Server validates acyclicity, creates task rows                       │
│     → If bootstrap fails: job enters 'failed'                              │
│                                                                              │
│  5. Job transitions to 'running', regular tasks become available:          │
│     → Workers poll for tasks via POST /api/internal/workers/:id/poll       │
│     → Server scores ready tasks by locality (ontological closeness to the  │
│       worker's last completed task) and assigns the best match             │
│     → Async scaling process provisions workers when queue depth exceeds    │
│       configurable threshold                                                │
│                                                                              │
│  6. Worker executes task:                                                   │
│     → Creates task branch, runs agent, streams timeline events             │
│     → On completion: pushes branch, reports result to server               │
│                                                                              │
│  7. Scheduler advances:                                                     │
│     → Marks task completed → unblocks downstream tasks                     │
│     → If any task fails: mark dependents as 'blocked'                      │
│     → When all tasks done: mark job as 'completed'                         │
│                                                                              │
│  8. Task failure / unblock (user-driven):                                   │
│     → User opens the blocked/failed task's worker in draft state           │
│       (attaches to the task session — Mode 2 from §10.9)                   │
│     → User edits files in the shared CRDT draft alongside the worker       │
│     → User marks the task as completed via POST /api/v1/tasks/:id/complete │
│     → Scheduler unblocks downstream tasks normally                          │
│                                                                              │
│  9. Cost is accumulated per-task and rolled up to the job level            │
└─────────────────────────────────────────────────────────────────────────────┘
```

```sql
-- ─── Jobs ────────────────────────────────────────────────────────────────
CREATE TABLE jobs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    job_type    TEXT NOT NULL
                CHECK (job_type IN (
                    'compile', 'uplift', 'downlift', 'test', 'review',
                    'chat', 'custom'
                )),
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN (
                    'pending', 'bootstrapping', 'running', 'paused',
                    'completed', 'failed', 'cancelled', 'rolled_back'
                )),

    -- Target scope: the ontology node that roots this operation.
    -- Bootstrap walks this node's subtree to create tasks (AGENTS.md §3.7).
    start_node    UUID REFERENCES ontology_nodes(id) ON DELETE SET NULL,
    force         BOOLEAN NOT NULL DEFAULT false,   -- Skip staleness checks

    -- Job ordering via lanes:
    --   Batch lane (compile, uplift, downlift, test, custom): each new
    --     batch job depends on the previous batch job in this workspace.
    --   Interactive lane (chat, review): depends_on_job = NULL (immediate).
    -- Client can override explicitly. NULL = run immediately.
    depends_on_job  UUID REFERENCES jobs(id) ON DELETE SET NULL,

    -- Scheduling policy
    max_concurrency INT NOT NULL DEFAULT 4,         -- Max parallel tasks
    priority        INT NOT NULL DEFAULT 0,         -- Higher = scheduled first

    -- Progress (denormalized for fast display)
    total_tasks     INT NOT NULL DEFAULT 0,
    completed_tasks INT NOT NULL DEFAULT 0,
    failed_tasks    INT NOT NULL DEFAULT 0,

    -- Cost tracking (rolled up from tasks)
    input_tokens  BIGINT NOT NULL DEFAULT 0,
    output_tokens BIGINT NOT NULL DEFAULT 0,
    cost_usd      NUMERIC(10, 4) NOT NULL DEFAULT 0,

    -- Requester
    created_by    UUID REFERENCES users(id),

    started_at    TIMESTAMPTZ,
    completed_at  TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_job_workspace ON jobs(workspace_id, status);
CREATE INDEX idx_job_scheduling ON jobs(status, priority DESC, created_at)
    WHERE status IN ('pending', 'bootstrapping', 'running');
CREATE INDEX idx_job_predecessor ON jobs(depends_on_job)
    WHERE status = 'pending';

-- ─── Tasks ───────────────────────────────────────────────────────────────
-- Each job starts with a single bootstrap task. On completion the bootstrap
-- worker submits regular tasks (one per ontology node). The dependency
-- edges mirror the ontology node_dependencies graph, scoped to this job.
CREATE TABLE tasks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id      UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    node_id     UUID REFERENCES ontology_nodes(id) ON DELETE SET NULL,
    name        TEXT NOT NULL,
    task_type   TEXT NOT NULL DEFAULT 'regular'
                CHECK (task_type IN ('bootstrap', 'regular')),
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN (
                    'pending', 'running', 'completed', 'failed',
                    'blocked', 'cancelled'
                )),

    -- Git isolation
    branch_name   TEXT,                             -- task/<task-id>
    base_commit   TEXT,                             -- SHA we branched from
    workspace_ref TEXT,                             -- Container workspace path

    -- Worker assignment (set by scheduler)
    worker_id     TEXT,                             -- Assigned worker instance ID
    assigned_at   TIMESTAMPTZ,

    -- Retry policy: on failure, auto-retry up to max_retries.
    -- When exhausted, task escalates to 'blocked' (human intervention).
    retry_count   INT NOT NULL DEFAULT 0,
    max_retries   INT NOT NULL DEFAULT 3,

    -- Dependency ordering (task IDs within this job)
    depends_on    UUID[] NOT NULL DEFAULT '{}',

    -- Cost tracking (per-task)
    input_tokens  BIGINT NOT NULL DEFAULT 0,
    output_tokens BIGINT NOT NULL DEFAULT 0,
    cost_usd      NUMERIC(10, 4) NOT NULL DEFAULT 0,

    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_task_job ON tasks(job_id, status);
-- Scheduler hot path: find tasks ready to run
CREATE INDEX idx_task_pending ON tasks(job_id, status)
    WHERE status = 'pending';

-- ─── Task Timeline ──────────────────────────────────────────────────────
-- Maps mock executionLogs [{ type, content }] with richer metadata.
CREATE TABLE timeline_entries (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id     UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    entry_type  TEXT NOT NULL
                CHECK (entry_type IN (
                    'thought', 'tool_call', 'tool_result',
                    'output', 'error', 'user_message', 'agent_message',
                    'checkpoint', 'skill_invocation', 'subagent_invocation'
                )),
    content     TEXT NOT NULL,
    metadata    JSONB NOT NULL DEFAULT '{}',         -- Tool name, exit code, etc.
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Paginated timeline queries (newest first)
CREATE INDEX idx_timeline_task ON timeline_entries(task_id, created_at DESC);

-- ─── Task Checkpoints ───────────────────────────────────────────────────
CREATE TABLE task_checkpoints (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id     UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    commit_sha  TEXT NOT NULL,
    message     TEXT NOT NULL,
    files_changed TEXT[] NOT NULL DEFAULT '{}',
    stats       JSONB NOT NULL DEFAULT '{"additions":0,"deletions":0}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_checkpoint_task ON task_checkpoints(task_id, created_at DESC);
```

### 2.8 Chat Sessions

Per WORKER_INFRASTRUCTURE.md §10.14 — all sessions persisted, browsable,
resumable/forkable.

```sql
-- ─── Chat Sessions ──────────────────────────────────────────────────────
CREATE TABLE chat_sessions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,

    -- Session categorization
    session_type TEXT NOT NULL DEFAULT 'chat'
                 CHECK (session_type IN ('chat', 'task')),
    task_id      UUID REFERENCES tasks(id) ON DELETE SET NULL,  -- If task-scoped

    -- Context scope (what the session is about)
    scope_type   TEXT NOT NULL DEFAULT 'workspace'
                 CHECK (scope_type IN ('workspace', 'node', 'task')),
    scope_ref    TEXT,                              -- node ID or task ID

    -- LLM details
    model        TEXT,                              -- e.g., 'claude-sonnet-4-20250514'

    -- Auto-generated metadata (populated by LLM or post-processing hooks;
    -- deferrable for MVP — empty defaults are safe)
    title        TEXT,                              -- 5-8 words, LLM-generated
    summary      TEXT,                              -- 2-3 sentences
    tags         TEXT[] NOT NULL DEFAULT '{}',

    -- Linked artifacts (auto-populated from tool calls during the session;
    -- useful for search/cross-referencing but not required for core flow)
    linked_commits TEXT[] NOT NULL DEFAULT '{}',
    linked_nodes   TEXT[] NOT NULL DEFAULT '{}',     -- Ontology node IDs

    -- Cost tracking
    total_input_tokens  BIGINT NOT NULL DEFAULT 0,
    total_output_tokens BIGINT NOT NULL DEFAULT 0,
    total_cost_usd      NUMERIC(10, 4) NOT NULL DEFAULT 0,

    -- Lifecycle
    status       TEXT NOT NULL DEFAULT 'active'
                 CHECK (status IN ('active', 'completed', 'archived')),
    message_count INT NOT NULL DEFAULT 0,

    -- Fork tracking
    forked_from_id    UUID REFERENCES chat_sessions(id),
    forked_at_index   INT,                          -- Message index where fork starts

    -- Branch association
    branch       TEXT,                              -- Git branch for this session

    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_session_workspace ON chat_sessions(workspace_id, status, updated_at DESC);
CREATE INDEX idx_session_task ON chat_sessions(task_id) WHERE task_id IS NOT NULL;

-- Full-text search across session titles and summaries
CREATE INDEX idx_session_search ON chat_sessions
    USING GIN (to_tsvector('english', COALESCE(title, '') || ' ' || COALESCE(summary, '')));

-- ─── Chat Messages ──────────────────────────────────────────────────────
CREATE TABLE chat_messages (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id  UUID NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,

    role        TEXT NOT NULL
                CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content     TEXT NOT NULL,

    -- Artifacts embedded in assistant messages (see FRONTEND_DESIGN.md §9.4)
    artifacts   JSONB,                              -- Array of Artifact objects

    -- Citations to ontology nodes / file ranges
    citations   JSONB NOT NULL DEFAULT '[]',

    -- Token accounting
    input_tokens  INT,
    output_tokens INT,

    -- Message lifecycle
    status      TEXT NOT NULL DEFAULT 'complete'
                CHECK (status IN (
                    'pending', 'streaming', 'complete', 'error', 'stopped'
                )),

    message_index INT NOT NULL,                     -- Ordinal within session
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_msg_session ON chat_messages(session_id, message_index);

-- Full-text search across message content
CREATE INDEX idx_msg_fts ON chat_messages
    USING GIN (to_tsvector('english', content));
```

### 2.9 Search Documents

The `search_documents` table powers the unified search API (§3.10). It
stores pre-indexed content from git-backed files so that full-text search
and semantic (vector) search run entirely inside Postgres — no external
search service required.

The table uses two complementary indexes:
- **`fts`** — a `tsvector` column with a GIN index for keyword / full-text search.
- **`embedding`** — a `vector(1536)` column with an HNSW index for semantic
  similarity search via `pgvector`.

Content is partitioned by `workspace_branch_id` so that search results
respect workspace scope.

```sql
-- Requires: CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE search_documents (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_branch_id UUID NOT NULL REFERENCES workspace_branches(id) ON DELETE CASCADE,
    file_path           TEXT NOT NULL,                  -- Relative path in repo
    node_id             UUID REFERENCES ontology_nodes(id) ON DELETE SET NULL,
    doc_type            TEXT NOT NULL DEFAULT 'file'
                        CHECK (doc_type IN ('spec', 'doc', 'code', 'file')),
                                                    -- 'spec' = .g5/specs/*.md
                                                    -- 'doc'  = .g5/docs/*.md
                                                    -- 'code' = source files matched
                                                    --          to ontology nodes
                                                    -- 'file' = any other indexed file
    title               TEXT,                           -- Extracted title (first heading, filename)
    content_hash        TEXT NOT NULL,                  -- SHA-256 of raw content (skip re-index if unchanged)
    chunk_index         INT NOT NULL DEFAULT 0,         -- For large files split into chunks
    content_snippet     TEXT NOT NULL,                  -- Searchable content (first N chars or full)
    fts                 tsvector NOT NULL,              -- Full-text search vector
    embedding           vector(1536),                   -- Semantic embedding (nullable until computed)
    indexed_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_branch_id, file_path, chunk_index)
);

CREATE INDEX idx_search_fts ON search_documents USING GIN (fts);
CREATE INDEX idx_search_embedding ON search_documents USING hnsw (embedding vector_cosine_ops);
CREATE INDEX idx_search_branch ON search_documents(workspace_branch_id);
```

### 2.10 Workers

Worker state is primarily in Redis (heartbeat, live assignments). Postgres
stores the audit trail and configuration.

```sql
-- ─── Worker Registry (audit / config) ───────────────────────────────────
CREATE TABLE workers (
    id          TEXT PRIMARY KEY,                    -- Worker instance ID
    role        TEXT NOT NULL DEFAULT 'worker'
                CHECK (role IN ('editor', 'worker')),
    status      TEXT NOT NULL DEFAULT 'provisioning'
                CHECK (status IN (
                    'provisioning', 'idle', 'assigned',
                    'executing', 'terminated'
                )),
    -- Ownership (who started this worker)
    owner_user_id UUID REFERENCES users(id),
    org_id        UUID REFERENCES orgs(id),

    -- Resource allocation
    memory_mb   INT,
    cpu_cores   INT,

    -- Current assignment
    task_id     UUID REFERENCES tasks(id) ON DELETE SET NULL,
    workspace_id UUID REFERENCES workspaces(id) ON DELETE SET NULL,
    branch      TEXT,
    base_commit TEXT,

    -- Health
    last_heartbeat TIMESTAMPTZ,
    last_task_completed_at TIMESTAMPTZ,           -- When the worker last finished a task
    last_node_id   UUID REFERENCES ontology_nodes(id)
                   ON DELETE SET NULL,             -- Last task's ontology node (locality scoring)
    started_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    terminated_at  TIMESTAMPTZ
);
```

**Worker lifecycle (pull model):** Workers are not passively assigned tasks
by the scheduler. Instead, each worker runs a poll loop:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      Worker Poll Loop                                        │
│                                                                              │
│  loop {                                                                     │
│    // 1. Ask the server for work                                            │
│    task = POST /api/internal/workers/:id/poll                               │
│            → Server scores all ready tasks by locality:                     │
│              ontological closeness to worker.last_node_id                   │
│              (graph distance in node_dependencies; closer = preferred)      │
│            → Respects workspace concurrency and spend limits                  │
│            → Atomically assigns the best-scoring task to this worker       │
│            → Returns TaskAssignment, or 204 No Content if nothing ready    │
│                                                                              │
│    if task:                                                                  │
│      // 2. Execute the task                                                 │
│      //    Bootstrap tasks: run analysis, submit task graph                 │
│      //    Regular tasks: run agent (code gen, uplift, review)             │
│      run_task(task)                                                          │
│      POST /api/internal/tasks/:id/result { status, cost }                  │
│      //    Server updates worker.last_node_id = task.node_id               │
│      // 3. Immediately poll again (there may be more work)                 │
│      continue                                                                │
│                                                                              │
│    // 4. No work available — enter idle wait                                │
│    wait(poll_interval)    // 2-5 seconds                                    │
│                                                                              │
│    // 5. Check idle timeout                                                 │
│    if now() - last_task_completed_at > idle_timeout:                        │
│      // Cloud worker: self-terminate                                        │
│      POST /api/internal/workers/:id/terminate { reason: 'idle_timeout' }   │
│      exit                                                                    │
│      // Local worker: keep running (never self-terminates)                  │
│  }                                                                           │
│                                                                              │
│  The server can also push a "task available" hint via Redis pub/sub        │
│  to wake a polling worker immediately instead of waiting for the next       │
│  poll interval. This is an optimization, not a requirement.                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.11 Workspace Settings

Each workspace has exactly one settings row controlling execution policy.
These are the knobs exposed on the **SettingsPage** and enforced by the
scheduler at runtime.

```sql
-- ─── Workspace Settings ────────────────────────────────────────────────────
-- One row per workspace. Created automatically when a workspace is created.
CREATE TABLE workspace_settings (
    workspace_id UUID PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,

    -- Concurrency: how many tasks can run in parallel across all jobs
    -- in this workspace. Overrides the per-job max_concurrency if lower.
    max_concurrent_tasks  INT NOT NULL DEFAULT 4
        CHECK (max_concurrent_tasks >= 1),

    -- Spend limits: hard caps on token/dollar spend.
    -- When a limit is hit, no new tasks are scheduled until the user
    -- raises the limit or the billing period resets.
    max_spend_usd         NUMERIC(10, 2),            -- NULL = unlimited
    spend_period          TEXT NOT NULL DEFAULT 'monthly'
                          CHECK (spend_period IN (
                              'hourly', 'daily', 'weekly', 'monthly', 'total'
                          )),
    spend_period_start    TIMESTAMPTZ,               -- Anchor for the billing window

    -- Token-level limits (alternative / in addition to dollar limits)
    max_input_tokens      BIGINT,                    -- NULL = unlimited
    max_output_tokens     BIGINT,                    -- NULL = unlimited

    -- Worker provisioning
    max_cloud_workers     INT NOT NULL DEFAULT 8,    -- Max cloud workers to provision
    allow_cloud_workers   BOOLEAN NOT NULL DEFAULT TRUE,
    worker_idle_timeout_min INT NOT NULL DEFAULT 5,  -- Kill cloud workers after N min idle

    -- Worker scaling (async process, see §3.4.3)
    tasks_per_worker      INT NOT NULL DEFAULT 1,    -- Ready tasks per worker before scaling up
    worker_scaling_interval_sec INT NOT NULL DEFAULT 5, -- How often the scaler runs

    -- Search / embedding configuration (§3.10)
    embedding_model       TEXT NOT NULL DEFAULT 'text-embedding-3-small',
    embedding_dimensions  INT NOT NULL DEFAULT 1536,
    embedding_provider    TEXT NOT NULL DEFAULT 'openai'
                          CHECK (embedding_provider IN ('openai', 'local', 'none')),
                                                    -- 'none' disables semantic search;
                                                    -- FTS still available

    updated_by  UUID REFERENCES users(id),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

**Spend enforcement** — the scheduler checks workspace-level spend before
assigning any new task:

```sql
-- Example: check whether the workspace has exceeded its monthly spend cap.
SELECT COALESCE(SUM(t.cost_usd), 0) AS period_spend
FROM tasks t
JOIN jobs j ON t.job_id = j.id
WHERE j.workspace_id = :pid
  AND t.status IN ('completed', 'running')
  AND t.created_at >= :period_start;

-- If period_spend >= workspace_settings.max_spend_usd:
--   Do NOT schedule any new tasks.
--   Set job status to 'paused' with reason = 'spend_limit_reached'.
--   Notify the user via WS event.
```

### 2.12 Authorization — OAuth Scopes

Authorization uses **OAuth-style scopes** rather than flat RBAC roles.
Scopes are granted at two levels:

1. **User-level scopes** — assigned directly to a user within an org
   (e.g., an admin gets broad scopes).
2. **Organization-level scopes** — default scopes inherited by all members
   of the org (e.g., every member can read workspaces).

Effective permissions = union of user-level scopes ∪ org-level default scopes.

Scope strings follow the pattern `resource:action` where action is one of
`read`, `write`, `execute`, `admin`. Examples:

- `workspaces:read` — view workspaces and their ontology
- `workspaces:write` — create/edit workspaces, branches, ontology nodes
- `jobs:execute` — create jobs, trigger compile/uplift
- `workers:admin` — provision/terminate workers, view all worker state
- `org:admin` — manage org members, billing, settings
- `sessions:write` — create/send chat messages, fork sessions

```sql
-- ─── OAuth Scope Definitions ─────────────────────────────────────────────
-- Registry of all valid scopes. Seeded at migration time.
CREATE TABLE oauth_scopes (
    scope       TEXT PRIMARY KEY,                    -- e.g., 'workspaces:read'
    description TEXT NOT NULL,
    category    TEXT NOT NULL                         -- 'workspaces', 'jobs', 'workers', etc.
);

-- ─── Organization Default Scopes ─────────────────────────────────────────
-- Scopes automatically granted to every member of this org.
-- Org owners can customize these per org.
CREATE TABLE org_default_scopes (
    org_id  UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    scope   TEXT NOT NULL REFERENCES oauth_scopes(scope),
    PRIMARY KEY (org_id, scope)
);

-- ─── User Scope Grants ──────────────────────────────────────────────────
-- Additional scopes granted to a specific user within an org.
-- These are on top of the org defaults.
CREATE TABLE user_scope_grants (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    scope       TEXT NOT NULL REFERENCES oauth_scopes(scope),
    granted_by  UUID REFERENCES users(id),
    granted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ,                        -- NULL = permanent
    UNIQUE (org_id, user_id, scope)
);

-- ─── Workspace-Scoped Permission Overrides ────────────────────────────────
-- Optional per-workspace overrides. If a row exists, it REPLACES the org-level
-- scope for that (user, workspace) pair. If no override exists, org-level applies.
CREATE TABLE workspace_permission_overrides (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    scope       TEXT NOT NULL REFERENCES oauth_scopes(scope),
    granted_by  UUID REFERENCES users(id),
    granted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, user_id, scope)
);

CREATE INDEX idx_usg_user ON user_scope_grants(user_id, org_id);
CREATE INDEX idx_ppo_workspace ON workspace_permission_overrides(workspace_id);
```

**Scope resolution algorithm (enforced by middleware):**

```
effective_scopes(user, org, workspace?) =
  org_default_scopes[org]
  ∪ user_scope_grants[org, user]
  ∪ workspace_permission_overrides[workspace, user]   -- if workspace context
```

**JWT integration:** The JWT `scopes` claim contains the user's effective
scopes for the org they're currently operating in. When the user switches
orgs, they must re-authenticate (or the client calls
`POST /api/v1/auth/refresh` with the new org context) to get a new JWT
with the correct scopes.

**Predefined scope sets (convenience roles):**

| Role | Scopes |
|------|--------|
| viewer | `workspaces:read`, `sessions:read`, `jobs:read` |
| editor | viewer + `workspaces:write`, `sessions:write` |
| operator | editor + `jobs:execute`, `workers:read` |
| admin | operator + `workers:admin`, `org:admin`, `billing:admin` |
| owner | all scopes (superuser within the org) |

### 2.13 Usage Records

Fine-grained per-request LLM usage records for cost attribution, billing
audits, and provider analytics. Each row represents one `chat()` or
`complete()` call to an LLM provider. Rolled-up totals are denormalized
onto `tasks.cost_usd`, `jobs.cost_usd`, and `chat_sessions.total_cost_usd`
for fast display; this table is the source of truth for reconciliation.

```sql
-- ─── Usage Records (LLM call-level billing) ─────────────────────────────
CREATE TABLE usage_records (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID NOT NULL REFERENCES orgs(id) ON DELETE CASCADE,
    workspace_id     UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,

    -- Execution context (at least one must be set)
    task_id         UUID REFERENCES tasks(id) ON DELETE SET NULL,
    session_id      UUID REFERENCES chat_sessions(id) ON DELETE SET NULL,
    job_id          UUID REFERENCES jobs(id) ON DELETE SET NULL,

    -- Provider / model
    provider        TEXT NOT NULL,                  -- 'openai', 'anthropic', 'openrouter', etc.
    model           TEXT NOT NULL,                  -- e.g. 'claude-sonnet-4-20250514'

    -- Token accounting (mirrors ChatResponse.usage)
    input_tokens    BIGINT NOT NULL DEFAULT 0,
    output_tokens   BIGINT NOT NULL DEFAULT 0,
    total_tokens    BIGINT NOT NULL DEFAULT 0,

    -- Anthropic cache tokens (§14.3 of AGENTS.md)
    cache_creation_tokens BIGINT NOT NULL DEFAULT 0,
    cache_read_tokens     BIGINT NOT NULL DEFAULT 0,

    -- Cost (computed by worker using per-model pricing)
    cost_usd        NUMERIC(10, 6) NOT NULL DEFAULT 0,

    -- Request metadata
    request_type    TEXT NOT NULL DEFAULT 'chat'
                    CHECK (request_type IN ('chat', 'completion', 'summarization', 'embedding')),
    finish_reason   TEXT,                           -- 'stop', 'tool_calls', 'length', etc.
    duration_ms     INT,                            -- Wall-clock latency of the LLM call

    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Fast lookups for billing dashboards
CREATE INDEX idx_usage_workspace_time ON usage_records(workspace_id, created_at DESC);
CREATE INDEX idx_usage_org_time ON usage_records(org_id, created_at DESC);
CREATE INDEX idx_usage_task ON usage_records(task_id) WHERE task_id IS NOT NULL;
CREATE INDEX idx_usage_session ON usage_records(session_id) WHERE session_id IS NOT NULL;

-- Aggregate queries: cost by model, provider
CREATE INDEX idx_usage_provider_model ON usage_records(provider, model, created_at DESC);
```

**Write path:** The worker writes a usage record after every LLM call via a
batched internal endpoint:

```
POST /api/internal/usage
[
  { taskId, sessionId, provider, model, inputTokens, outputTokens,
    cacheCreationTokens, cacheReadTokens, costUsd, requestType,
    finishReason, durationMs }
]
```

The server inserts the rows and updates the denormalized counters on
`tasks`, `jobs`, and `chat_sessions` in a single transaction.

**Read path:** The Spend History API (§3.15) and the billing report
(FRONTEND_DESIGN.md §9.16) query this table with various GROUP BY
dimensions (provider, model, day, task, job).

---

## 3. REST API

### 3.0 Conventions

**Versioning:** All public API endpoints are prefixed with `/api/v1/`. Internal
worker-to-editor endpoints (worker poll, terminate, heartbeat) use `/api/internal/`
and are authenticated via service tokens, not user JWTs.

| Prefix | Audience | Auth | Example |
|--------|----------|------|---------|
| `/api/v1/` | Browser clients, CLI tools | `Bearer <jwt>` | `GET /api/v1/workspaces/:id` |
| `/api/internal/` | Workers, schedulers | `Bearer <service-token>` | `POST /api/internal/workers/:id/poll` |
| `/api/v1/auth/` | Unauthenticated | None (initiates OAuth) | `GET /api/v1/auth/login/github` |
| `/api/v1/scm/` | Browser (authenticated) | `Bearer <jwt>` | `GET /api/v1/scm/github/repos` |

All endpoint paths in §3.1–§3.15 below include the full prefix
(e.g., `GET /api/v1/orgs/:orgId/workspaces`). Internal worker endpoints
use `/api/internal/` instead of `/api/v1/`.

When a breaking change is required, a new version (`/api/v2/`) is introduced.
The previous version remains available for a deprecation period (minimum 6
months). Non-breaking additions (new optional fields, new endpoints) do NOT
bump the version.

**Auth:** All `/api/v1/` endpoints require `Authorization: Bearer <jwt>` unless
noted otherwise (see WORKER_INFRASTRUCTURE.md §12).

**Response envelope:**

```json
{
  "data": { ... },
  "meta": { "page": 1, "perPage": 50, "total": 123 }
}
```

**Errors:**

```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "Workspace not found",
    "details": {}
  }
}
```

### 3.1 Workspaces API

| Method | Path | Description | Mock origin |
|--------|------|-------------|-------------|
| `GET` | `/api/v1/orgs/:orgId/workspaces` | List all workspaces in the org | `workspaces` array (9 items) |
| `POST` | `/api/v1/orgs/:orgId/workspaces` | Create workspace (creates branches) | — |
| `GET` | `/api/v1/workspaces/:id` | Get workspace detail | Single `Workspace` |
| `PATCH` | `/api/v1/workspaces/:id` | Update status, description | — |
| `DELETE` | `/api/v1/workspaces/:id` | Delete workspace (delete branches + drafts) | — |
| `GET` | `/api/v1/workspaces/:id/stats` | Computed stats (cost, tasks, divergence) | `divergence`, `cost`, `tasksCompleted`, etc. |
| `GET` | `/api/v1/workspaces/:id/dependencies` | Dependency DAG | `dependencies: string[]` |
| `GET` | `/api/v1/workspaces/:id/branches` | List per-repo branches (with head_commit + draft status) | — |
| `POST` | `/api/v1/workspaces/:id/branches` | Add a repo/branch to the workspace | — |
| `DELETE` | `/api/v1/workspaces/:id/branches/:branchId` | Remove a repo from the workspace | — |
| `POST` | `/api/v1/workspaces/:id/sync` | Pull from base branch (all repos) | — |
| `POST` | `/api/v1/workspaces/:id/merge` | Merge into base branch (all repos) | — |
| `POST` | `/api/v1/workspaces/:id/clone` | Deep-copy workspace (including dirty drafts) | — |

**`GET /api/v1/orgs/:orgId/workspaces` response:**

Supports query params: `?status=active,review` (comma-separated status filter).
Without filters, returns all workspaces.

```typescript
interface WorkspaceListItem {
  id: string;
  name: string;
  status: 'idea' | 'active' | 'blocked' | 'review' | 'integrated';
  description: string | null;
  clonedFromId: string | null; // Source workspace ID if this was cloned

  // Per-repo branches in this workspace.
  branches: Array<{
    id: string;                // workspace_branch ID
    repoId: string;
    repoName: string;
    branchName: string;
    baseBranch: string;
    trackingMode: 'tracking' | 'pinned'; // Whether this branch auto-tracks remote HEAD
    headCommit: string;        // Pinned SHA
    divergence: number;        // Commits ahead of base
    behindBase: number;        // Commits behind base
    hasDraft: boolean;         // Whether a draft_session exists
    dirtyFileCount: number;    // draft_sessions.dirty_count (0 if no draft)
  }>;

  // Aggregated across all repos
  tasksCompleted: number;
  totalTasks: number;
  runningTime: string | null;  // Duration of active jobs
  cost: number;                // Total USD spent
  concurrentTasks: number;     // Currently active tasks

  dependencies: string[];      // Workspace IDs

  lastUpdated: string;         // ISO 8601
  createdAt: string;
}
```

**`POST /api/v1/orgs/:orgId/workspaces` request:**

```typescript
interface CreateWorkspaceRequest {
  name: string;
  description?: string;
  dependencies?: string[];     // Workspace IDs
  // Per-repo branch configuration. Each entry creates a workspace_branch
  // row. It's fine to start with one repo and add more later via
  // POST /api/v1/workspaces/:id/branches.
  branches: Array<{
    repoId: string;
    branchName?: string;       // Auto-generated from workspace name if omitted
    baseBranch?: string;       // Default: repo's default branch
    trackingMode?: 'tracking' | 'pinned'; // Default: 'pinned'
  }>;
}
```

This endpoint is a fresh workspace init path. To fork an existing workspace,
use `POST /api/v1/workspaces/:id/clone`.

#### Workspace Cloning

Cloning a workspace creates a **deep copy** at its current
point in time — including any dirty (uncommitted CRDT) state. This is useful
for:
- **Branching work**: Splitting an in-progress workspace into two directions.
- **Snapshotting**: Preserving the current state before a risky change.
- **Collaboration**: Giving another team member an independent copy.

**Clone endpoint:**

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/workspaces/:id/clone` | Deep-copy a workspace |

```typescript
interface CloneWorkspaceRequest {
  name: string;                // Name for the cloned workspace
  description?: string;
  // Optional: override branch names in the clone. If omitted, branch
  // names are auto-generated as "<originalBranch>-clone-<timestamp>".
  branchOverrides?: Record<string, string>; // repoId → new branch name
}

interface CloneWorkspaceResponse {
  workspace: WorkspaceListItem;    // The newly created clone
  clonedBranches: number;      // How many workspace_branches were copied
  clonedDrafts: number;        // How many draft_sessions were copied (0 if source was clean)
  clonedOntologyNodes: number; // How many ontology_nodes were copied
}
```

**Clone semantics (transactional — all-or-nothing in a single DB transaction):**

1. **Workspace row**: New `workspaces` row with `cloned_from_id = source.id`,
   `status = 'active'`.
2. **Workspace branches**: For each `workspace_branch` in the source, create a
   new row with the same `repo_id`, `base_branch`, `tracking_mode`, and `head_commit`.
   `branch_name` is either from `branchOverrides` or auto-generated.
   The `UNIQUE(repo_id, branch_name)` constraint ensures no collision.
3. **Ontology nodes**: Deep-copy all `ontology_nodes` rows, remapping
   `workspace_branch_id` to the clone's branches, and remapping `parent_id`
   self-references to the new node IDs. `node_dependencies` edges within
   the workspace are also remapped; cross-workspace edges are dropped.
4. **Draft sessions (dirty state transfer)**: For each source `workspace_branch`
   that has an active `draft_session`, copy the `draft_sessions` row —
   including `yjs_state` (the serialized Yjs document), `diff_summary`,
   `dirty_files`, and `dirty_count`. The clone starts with the same
   uncommitted edits as the source. `draft_participants` are NOT copied
   (participants are live connections, not persistent state).
5. **Docs**: Deep-copied with remapped workspace_id. Doc node
   links are remapped to the cloned ontology node IDs. The clone
   gets `docs_repo_id` from the source; `docs_branch_name` is
   auto-generated (or from overrides). g5d initializes the independent
   docs repository checkout at the new branch on materialization.
6. **Jobs and tasks**: NOT copied — jobs are execution history, not workspace state.
7. **Chat sessions**: NOT copied — chat is session history, not workspace state.
8. **Workspace settings**: Copied from source.

### 3.2 Ontology API

| Method | Path | Description | Mock origin |
|--------|------|-------------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/ontology` | Full ontology tree | `ontologyTree` |
| `GET` | `/api/v1/workspaces/:workspaceId/ontology/:nodeId` | Single node (metadata + content) | `findNodeById` |
| `GET` | `/api/v1/workspaces/:workspaceId/ontology/:nodeId/children` | Direct children | `node.children` |
| `GET` | `/api/v1/workspaces/:workspaceId/ontology/:nodeId/content` | Node content from git | `node.content` |

**Content resolution for `GET .../ontology/:nodeId/content`:**

Content is **always read from git** (not cached in Postgres — open question #1).
The read path depends on deployment context:

1. **g5d editor connected:** `g5-api` delegates to the editor's git checkout
   via `GET /api/v1/instances/:id/files/{file_path}`. The editor returns the
   file content from its working tree, with draft overlay applied if a
   `draft_session` exists (uncommitted CRDT edits are visible).
2. **No editor connected (API-only):** `g5-api` shells out to `git show
   {head_commit}:{file_path}` on the bare repo (if available). Draft
   overlays are NOT applied (the caller sees the last committed state).
3. **Worker context:** Workers read from their own git worktree directly.

The response includes a `draftDirty: boolean` flag indicating whether
the returned content includes uncommitted CRDT edits.
| `PUT` | `/api/v1/workspaces/:workspaceId/ontology/:nodeId/content` | Update content (via draft) | — |
| `POST` | `/api/v1/workspaces/:workspaceId/ontology` | Create new node | — |
| `DELETE` | `/api/v1/workspaces/:workspaceId/ontology/:nodeId` | Delete node | — |
| `GET` | `/api/v1/workspaces/:workspaceId/ontology/:nodeId/diff` | Diff vs base branch | `nodeDiffs[nodeId]` |
| `GET` | `/api/v1/workspaces/:workspaceId/ontology/:nodeId/deps` | Dependencies & dependents | `ontologyDependencies` / `ontologyDependents` |
| `GET` | `/api/v1/workspaces/:workspaceId/ontology/:nodeId/annotations` | Comments on this node | `nodeAnnotations[nodeId]` |

**`GET /api/v1/workspaces/:workspaceId/ontology` response:**

```typescript
// Returns the full ontology forest — one tree per root, spanning all repos.
// Nodes from different repos are interleaved; each node carries its repo context.
interface OntologyTreeResponse {
  nodes: OntologyNodeDTO[];           // Multiple roots (one per root ontology)
}

interface OntologyNodeDTO {
  id: string;
  parentId: string | null;
  repoId: string;                     // Which repo owns this node
  repoName: string;                   // Human-readable repo name
  workspaceBranchId: string;            // workspace_branches FK
  name: string;
  nodeType: 'node' | 'document' | 'code'; // Maps to ontology_nodes.node_type
  filePath: string | null;
  contentHash: string | null;
  sortOrder: number;
  syncStatus: 'synced' | 'modified' | 'added' | 'deleted' | 'conflict';
  aheadOfBase: number;
  behindBase: number;
  children: OntologyNodeDTO[];
}
```

**`GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/diff` response:**

```typescript
// Maps directly to mock DiffData
interface NodeDiffResponse {
  file: string;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

interface DiffHunk {
  startLine: number;
  header: string;                      // e.g., '@@ -5,8 +5,11 @@ ## Overview'
  lines: DiffLine[];
}

interface DiffLine {
  type: 'context' | 'add' | 'remove';
  oldLine?: number;
  newLine?: number;
  content: string;
}
```

### 3.3 Docs API

| Method | Path | Description | Notes |
|--------|------|-------------|-------|
| `GET` | `/api/v1/workspaces/:workspaceId/docs` | List docs | — |
| `GET` | `/api/v1/workspaces/:workspaceId/docs/:docId` | Doc detail + content | Includes staleness fields, node links |
| `POST` | `/api/v1/workspaces/:workspaceId/docs` | Create doc | Body includes linked nodes |
| `PUT` | `/api/v1/workspaces/:workspaceId/docs/:docId` | Update doc content/name | Updates `content_updated_at` |
| `PUT` | `/api/v1/workspaces/:workspaceId/docs/:docId/links` | Set node links | Replaces `doc_node_links` |

### 3.4 Jobs & Tasks API

#### 3.4.1 Compile & Uplift (Core Operations)

**Compile (downlift)** takes a high-level ontology specification and produces
concrete source code. **Uplift (decompile)** takes existing source code and
synchronizes it back into the ontology specification. Both operations are
node-scoped — you compile or uplift a specific ontology node (and its
transitive dependencies are resolved server-side).

The client does **not** build the task graph. It simply says *"compile these
nodes"* and the server handles everything:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                  Compile vs. Uplift: Direction of Work                        │
│                                                                              │
│  COMPILE (downlift):  Ontology spec  ──────►  Source code                   │
│  ─────────────────                                                           │
│  • Walks dependencies bottom-up (leaves first)                              │
│  • A node can only compile after its dependencies have compiled             │
│  • Agent reads the ontology spec + dependency outputs, writes code          │
│  • Example: compile hashtable_agg → first compile join_ht, agg_ht,         │
│    radix, perfect_ht (leaves), then compile hashtable_agg (parent)          │
│                                                                              │
│  UPLIFT (decompile):  Source code  ──────►  Ontology spec                   │
│  ────────────────────                                                        │
│  • Walks the tree top-down (roots first)                                    │
│  • Parent spec is updated first so children inherit the updated context     │
│  • Agent reads source code, updates the ontology markdown                   │
│  • Example: uplift hashtable_agg → first uplift hashtable_agg (parent),     │
│    then uplift join_ht, agg_ht, radix, perfect_ht (children) in parallel   │
│                                                                              │
│  Both produce a job whose bootstrap task builds the task DAG.               │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.4.2 Job CRUD & Lifecycle

| Method | Path | Description | Mock origin |
|--------|------|-------------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/jobs` | List jobs for workspace | `mockJobs` (WorkspacePage) |
| `POST` | `/api/v1/workspaces/:workspaceId/jobs` | Create job + bootstrap task | — |
| `GET` | `/api/v1/jobs/:id` | Job detail + task graph | — |
| `POST` | `/api/v1/jobs/:id/pause` | Pause job (pauses active tasks, holds pending) | — |
| `POST` | `/api/v1/jobs/:id/resume` | Resume job | — |
| `POST` | `/api/v1/jobs/:id/cancel` | Cancel job (abort active, discard pending) | — |
| `POST` | `/api/v1/jobs/:id/rollback` | Rollback all tasks | — |
| `GET` | `/api/v1/jobs/:jobId/tasks` | List tasks in job (ordered by dependency) | `executionTasks` |
| `GET` | `/api/v1/tasks/:id` | Task detail | single `ExecutionTask` |
| `GET` | `/api/v1/tasks/:id/timeline` | Paginated timeline | `executionLogs` |
| `GET` | `/api/v1/tasks/:id/diff` | Current diff | `activeDiff` |
| `GET` | `/api/v1/tasks/:id/checkpoints` | List checkpoints | — |
| `POST` | `/api/v1/tasks/:id/complete` | User marks task as completed (after manual fix) | — |
| `POST` | `/api/v1/tasks/:id/rollback` | Rollback to checkpoint | — |
| `POST` | `/api/internal/workers/:id/poll` | Worker polls for next task (pull model, internal) | — |
| `POST` | `/api/internal/workers/:id/terminate` | Worker self-terminates (internal) | — |
| `POST` | `/api/internal/jobs/:id/tasks` | Bootstrap worker submits task graph (internal) | — |
| `POST` | `/api/internal/tasks/:id/result` | Worker reports task completion/failure (internal) | — |

**`POST /api/v1/workspaces/:workspaceId/jobs` — Create Job (server-orchestrated):**

```typescript
interface CreateJobRequest {
  // What kind of operation
  jobType: 'compile' | 'uplift' | 'downlift' | 'sync' | 'merge' | 'view_create' | 'preview' | 'test' | 'review' | 'custom';

  // Which ontology nodes to operate on.
  // The server resolves the full dependency subgraph automatically.
  targetNodes: string[];       // Ontology node IDs

  // Optional: scheduling policy
  maxConcurrency?: number;     // Max parallel tasks (default: 4)
  priority?: number;           // 0 = normal, higher = sooner

  // Job sequencing: by default, this job will not start until the
  // workspace's most recent previous job completes.  Set to null to
  // opt out and allow parallel execution with other jobs.
  dependsOnJob?: string | null;  // Job ID (default: auto-chain)

  // Optional: name override (auto-generated if omitted)
  name?: string;

  // Preview-only routing. previewMode is required for preview jobs.
  // previewSourceJobId is valid only with previewMode='unmerged' and previews
  // the retained branch of a completed job before it is merged. When
  // previewMergeResult is true, previewPreflightFingerprint is required and the
  // preview forks from computed direct-clean post-merge heads instead.
  previewMode?: 'merged' | 'unmerged';
  previewTtlSecs?: number;
  previewSourceJobId?: string;
  previewMergeResult?: boolean;
  previewTargetWorkspaceId?: string;
  previewPreflightFingerprint?: string;
}
```

**Server-side behavior on `POST /api/v1/workspaces/:workspaceId/jobs`:**

```
1. Validate targetNodes exist in the workspace's ontology

2. Resolve job ordering via lanes:
   Batch types = { compile, uplift, downlift, test, custom }
   Interactive types = { chat, review }

   IF dependsOnJob is explicitly provided:
     depends_on_job = provided value (client override)
   ELSE IF job_type IN interactive types:
     depends_on_job = NULL (interactive jobs always run immediately)
   ELSE:
     depends_on_job = most recent batch job in this workspace (by created_at)
     Batch jobs chain sequentially: compile → uplift → test runs in order.
     The predecessor must reach 'completed' or 'cancelled' before
     this job's tasks become eligible for scheduling.

   This lane model ensures that chat/review never queue behind long-running
   batch operations, while batch jobs still serialize to avoid concurrent
   modifications to the same ontology tree.

3. Create Job row (status = 'pending', depends_on_job = resolved)

4. Create a single bootstrap task:
   - task.task_type   = 'bootstrap'
   - task.node_id     = job.start_node
   - task.depends_on  = []  (no dependencies — it runs first)
   - task.name        = "Bootstrap: <job_name>"
   - task.status      = 'pending'
   The bootstrap task is a regular task from the scheduling
   perspective — a worker will pick it up via the normal poll loop.

5. Set job.total_tasks = 1 (bootstrap only; updated when graph is submitted)
   IF depends_on_job is NULL or predecessor is already completed:
     Set job.status = 'bootstrapping'
   ELSE:
     Set job.status = 'pending' (waits for predecessor)

6. Return job ID to the client (no task graph yet — that comes from bootstrap)
```

**`POST /api/v1/workspaces/:workspaceId/jobs` response:**

```typescript
interface CreateJobResponse {
  jobId: string;
  name: string;
  status: 'pending' | 'bootstrapping';
  bootstrapTaskId: string;     // The bootstrap task ID
}
```

**`POST /api/internal/jobs/:id/tasks` — Bootstrap submits task graph (internal):**

Called by the bootstrap worker after it has analyzed the codebase (for
uplift/compile) or read the spec tree (for downlift). The worker submits the
full set of tasks that the job should execute.

```typescript
interface SubmitTaskGraphRequest {
  tasks: Array<{
    nodeId: string;            // Ontology node ID
    name: string;              // Human-readable task name
    dependsOn: string[];       // Ontology node IDs (resolved to task IDs by server)
  }>;
}
```

**Server-side behavior on `POST /api/internal/jobs/:id/tasks`:**

```
1. Validate that the job exists and is in 'bootstrapping' status.

2. Validate acyclicity: build the dependency graph from the submitted
   tasks and check for cycles.  If cycles are found, reject the request
   with 400 and fail the job — cycles indicate a bug in the analysis
   pipeline (ANALYSIS_DESIGN.md §4 should have broken them).

3. Create Task rows — one per submitted node:
   - task.task_type   = 'regular'
   - task.node_id     = submitted nodeId
   - task.depends_on  = [task IDs for the submitted dependency nodes]
   - task.name        = submitted name
   - task.status      = 'pending'

4. Update job.total_tasks = count of created tasks + 1 (bootstrap)

5. Transition job.status = 'running'
   → Regular tasks are now eligible for scheduling

6. Return the created task graph to the caller
```

```typescript
interface SubmitTaskGraphResponse {
  totalTasks: number;
  tasks: Array<{
    id: string;
    nodeId: string;
    name: string;
    dependsOn: string[];       // Task IDs (not node IDs)
    status: 'pending';
  }>;
}
```

**`GET /api/v1/jobs/:jobId/tasks` response:**

```typescript
interface TaskDTO {
  id: string;
  name: string;
  taskType: 'bootstrap' | 'regular';
  status: 'pending' | 'running' | 'completed' | 'failed' | 'blocked' | 'cancelled';
  workspaceId: string;
  workspaceName: string;
  jobType: 'compile' | 'uplift' | 'downlift' | 'test' | 'review' | 'chat' | 'custom';

  // SpecContext (matches TaskAssignment.specContext)
  specContext: {
    componentId: string;
    srcFiles: string[];
    parentSpec: string | null;
    childSpecs: string[];
  } | null;

  dependsOn: string[];         // Task IDs
  workerId: string | null;
  branchName: string | null;
  filesChanged: string[];
  costUsd: number;
  sessionId: string | null;    // chat_sessions.id (task agent dialog)
  assignedAt: string | null;
  completedAt: string | null;
}
```

**`GET /tasks/:id/timeline` response (paginated):**

```typescript
interface TimelineResponse {
  entries: TimelineEntryDTO[];
  cursor: string | null;       // Opaque cursor for next page
  hasMore: boolean;
}

interface TimelineEntryDTO {
  id: string;
  entryType: 'thought' | 'tool_call' | 'tool_result'
           | 'output' | 'error' | 'user_message' | 'agent_message'
           | 'checkpoint';
  content: string;             // Human-readable summary
  metadata: TimelineMetadata;  // Structured payload per entry_type
  createdAt: string;           // ISO 8601
}

// Discriminated metadata by entryType:
type TimelineMetadata =
  | { type: 'thought' }                                      // No extra data
  | { type: 'tool_call';  toolName: string; args: Record<string, unknown> }
  | { type: 'tool_result'; toolName: string; exitCode?: number;
      durationMs: number; truncated: boolean }
  | { type: 'output';     stream: 'stdout' | 'stderr' }
  | { type: 'error';      errorCode: string; recoverable: boolean }
  | { type: 'user_message'; userId: string; source: 'injection' | 'attachment' }
  | { type: 'agent_message' }                                 // No extra data
  | { type: 'checkpoint';  commitSha: string;
      filesChanged: string[]; additions: number; deletions: number };
```

**Task Dialog Persistence:**

Every task agent's full conversation is persisted as a `chat_sessions` row
with `session_type = 'task'` and `task_id` FK. This includes all LLM
messages, tool calls, tool results, and injected user messages. The
`chat_messages` table stores the raw dialog; `timeline_entries` stores the
structured operational log (which includes non-dialog events like file edits,
build output, checkpoints).

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                   Task Dialog vs Timeline (dual record)                      │
│                                                                              │
│  chat_sessions (session_type='task', task_id=T-123)                         │
│  └── chat_messages                                                          │
│      ├── [0] system   "You are a code agent..."                            │
│      ├── [1] user     "Implement SIMD for matrix multiply"                 │
│      ├── [2] assistant "I'll start by examining..."                        │
│      ├── [3] tool_call read_file("src/matrix.rs")                          │
│      ├── [4] tool_result "pub fn multiply(a: &Matrix..."                   │
│      ├── [5] assistant "I see the naive implementation..."                 │
│      ├── [6] tool_call write_file("src/matrix.rs", ...)                    │
│      ├── [7] tool_result "OK"                                              │
│      └── ... (full LLM conversation, every turn)                            │
│                                                                              │
│  timeline_entries (task_id=T-123)                                           │
│  └── entries                                                                │
│      ├── thought    "Examining existing matrix multiply implementation"    │
│      ├── tool_call  { toolName: "read_file", args: {...} }                 │
│      ├── tool_result { toolName: "read_file", durationMs: 12 }             │
│      ├── thought    "Found naive O(n³) loop, will add AVX2 intrinsics"    │
│      ├── tool_call  { toolName: "write_file", args: {...} }                │
│      ├── output     "Build succeeded"                                      │
│      ├── checkpoint { commitSha: "abc1234", filesChanged: [...] }          │
│      └── ... (operational activity log)                                     │
│                                                                              │
│  Both records are queryable, linkable, and searchable.                      │
│  The dialog is the "why"; the timeline is the "what".                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.4.3 Scheduler & Worker Provisioning

The scheduler is a server-side loop running inside `g5d` (editor role). It is
**not** triggered by client requests — it runs continuously and reacts to
task-completion events. Worker provisioning is handled by a **separate async
process** that monitors queue depth independently.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Scheduler Loop (server-side)                             │
│                                                                              │
│  Every 500ms (or on task-completion event):                                 │
│                                                                              │
│  0. Advance pending jobs:                                                   │
│     UPDATE jobs SET status = 'bootstrapping', started_at = now()           │
│     WHERE status = 'pending'                                                │
│       AND (depends_on_job IS NULL                                           │
│            OR depends_on_job IN (                                           │
│              SELECT id FROM jobs                                            │
│              WHERE status IN ('completed', 'cancelled')                     │
│            ))                                                                │
│     This is how job-to-job sequencing works: a job stays 'pending'         │
│     until its predecessor job completes or is cancelled. Once unblocked,   │
│     it enters 'bootstrapping' and its bootstrap task becomes eligible.     │
│                                                                              │
│  1. Check workspace spend limits (from workspace_settings):                   │
│     For each workspace with running/bootstrapping jobs:                       │
│       period_spend = SUM(tasks.cost_usd) in the current billing period     │
│       IF period_spend >= workspace_settings.max_spend_usd:                   │
│         → Pause all running jobs for this workspace (reason: spend_limit)    │
│         → Push WS event: workspace.spend_limit_reached                       │
│         → Skip to next workspace                                             │
│     (Same check for max_input_tokens / max_output_tokens if set)           │
│                                                                              │
│  2. Query ready tasks (bootstrap + regular):                                │
│     SELECT t.* FROM tasks t                                                 │
│     JOIN jobs j ON t.job_id = j.id                                          │
│     WHERE t.status = 'pending'                                              │
│       AND j.status IN ('bootstrapping', 'running')                         │
│       AND NOT EXISTS (                                                      │
│         SELECT 1 FROM tasks dep                                             │
│         WHERE dep.id = ANY(t.depends_on)                                    │
│           AND dep.status <> 'completed'                                     │
│       )                                                                      │
│     ORDER BY j.priority DESC, j.created_at, t.created_at                   │
│     The ready-task set is materialized in Redis for fast poll responses.    │
│                                                                              │
│  3. For each ready task, check concurrency:                                 │
│     active_count = count tasks WHERE workspace_id AND status = 'running'     │
│     IF active_count >= workspace_settings.max_concurrent_tasks: skip          │
│     (workspace-level limit takes precedence over per-job max_concurrency)    │
│                                                                              │
│  4. Task assignment (via worker poll — locality-aware):                     │
│     Worker calls POST /api/internal/workers/:id/poll                        │
│     → Server scores each ready task for this worker by locality:           │
│                                                                              │
│     Locality scoring:                                                       │
│       score(task, worker) =                                                 │
│         w_priority * job.priority                                           │
│         + w_locality * locality_score(task.node_id, worker.last_node_id)   │
│         + w_age * seconds_since(task.created_at)                            │
│                                                                              │
│       locality_score(task_node, worker_node):                               │
│         IF worker_node IS NULL → 0 (new worker, no preference)             │
│         IF task_node = worker_node → max_locality (same node)              │
│         ELSE → max_locality / graph_distance(task_node, worker_node)       │
│                                                                              │
│       graph_distance walks the ontology parent_id chain and                │
│       node_dependencies edges (BFS, cached in Redis per workspace).          │
│       Workers that recently completed a "nearby" task in the ontology      │
│       will have warm build caches, reducing task startup overhead.          │
│                                                                              │
│     → Atomically assigns the highest-scoring ready task to the worker      │
│     → UPDATE tasks SET status = 'running', worker_id = W,                  │
│         assigned_at = now()                                                  │
│     → Returns TaskAssignment to the worker                                  │
│     → Server can also send a Redis hint to wake idle workers faster        │
│                                                                              │
│  5. On task completion (worker reports back via POST /tasks/:id/result):    │
│                                                                              │
│     a. status = 'completed':                                                │
│        UPDATE tasks SET status = 'completed'                                │
│        UPDATE workers SET last_node_id = task.node_id,                      │
│          last_task_completed_at = now()                                      │
│        UPDATE jobs SET completed_tasks = completed_tasks + 1                │
│        → Roll up cost: UPDATE jobs SET cost_usd = cost_usd + task.cost_usd │
│                                                                              │
│     b. status = 'failed' (agent could not resolve the issue):               │
│        IF task.retry_count < task.max_retries:                              │
│          → UPDATE tasks SET status = 'pending',                             │
│              retry_count = retry_count + 1                                   │
│          → Scheduler picks up on next iteration (auto-retry)                │
│          → CRDT draft is preserved — next attempt sees partial work         │
│        ELSE:                                                                 │
│          → UPDATE tasks SET status = 'blocked'                              │
│          → Retries exhausted; escalate to human intervention                │
│          → Mark transitive dependents as 'blocked'                          │
│                                                                              │
│     c. status = 'blocked' (agent explicitly requests human help):           │
│        UPDATE tasks SET status = 'blocked'                                   │
│        → Mark transitive dependents as 'blocked'                            │
│        → Worker retained for grace period (blocked_worker_retention_min)    │
│                                                                              │
│     Bootstrap task special cases:                                            │
│     IF task.task_type = 'bootstrap' AND status = 'completed':              │
│       → Worker already submitted the task graph via                         │
│         POST /api/internal/jobs/:id/tasks                                   │
│       → Job status was transitioned to 'running' at that point             │
│     IF task.task_type = 'bootstrap' AND status = 'failed':                 │
│       → Mark job as 'failed' (bootstrap failure is fatal)                  │
│                                                                              │
│     IF all tasks done: UPDATE jobs SET status = 'completed'                │
│       → This may unblock the next pending job (step 0 on next tick)        │
│     → Trigger next scheduler iteration immediately                         │
│                                                                              │
│  6. User-driven unblock (blocked tasks):                                    │
│     → User attaches to the blocked task's session (§10.9)                  │
│     → User sees agent's work + error in the session dialog                 │
│     → User sends a message via POST /api/v1/sessions/:id/messages          │
│       → Task status: 'blocked' → 'running'                                │
│       → Agent loop resumes with user's message as next dialog turn         │
│     OR: User edits files in the shared CRDT draft to fix the issue         │
│       → User calls POST /api/v1/tasks/:id/complete                         │
│       → Task status: 'blocked' → 'completed'                              │
│       → All 'blocked' dependents: 'blocked' → 'pending'                   │
│       → Scheduler picks them up on the next iteration                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Worker Pool Scaling (async process):**

Worker provisioning runs as a separate async process inside `g5d`, decoupled
from the scheduler loop. This keeps the scheduler fast (it only materializes
the ready-task set and handles state transitions) while scaling decisions
happen on their own cadence.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                  Worker Pool Scaling (async, per-workspace)                     │
│                                                                              │
│  Every 5s (configurable: worker_scaling_interval_sec):                      │
│                                                                              │
│  For each workspace with running/bootstrapping jobs:                          │
│                                                                              │
│    1. Count ready tasks:                                                    │
│       ready_count = tasks WHERE status = 'pending' AND all deps completed  │
│                                                                              │
│    2. Count active workers:                                                 │
│       active_workers = workers WHERE workspace_id = P                        │
│         AND status IN ('idle', 'assigned', 'executing')                    │
│                                                                              │
│    3. Compute deficit:                                                      │
│       desired_workers = ceil(ready_count /                                  │
│         workspace_settings.tasks_per_worker)                                  │
│       deficit = desired_workers - active_workers                            │
│                                                                              │
│    4. Apply limits:                                                         │
│       deficit = min(deficit,                                                │
│         workspace_settings.max_cloud_workers - active_workers)               │
│       IF NOT workspace_settings.allow_cloud_workers: deficit = 0             │
│                                                                              │
│    5. Provision if deficit > 0:                                             │
│       → Provision `deficit` new cloud workers (GKE pods)                   │
│       → Workers take ~30s to become ready                                  │
│       → Workers start polling immediately on boot                          │
│                                                                              │
│  Workers are self-managing for scale-down:                                  │
│    → Cloud workers: exit after worker_idle_timeout_min with no task        │
│    → Local workers: keep polling forever (never self-terminate)            │
│    → No server-side reaper needed — workers handle their own lifecycle    │
└─────────────────────────────────────────────────────────────────────────────┘
```

The `tasks_per_worker` setting (default: 1) controls how aggressively the
scaler provisions. A value of 1 means "one worker per ready task" (maximum
parallelism). A value of 3 means "provision a new worker only when there are
3 ready tasks per existing worker" (more conservative).

### 3.5 Comments API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/comments` | All comments (filterable) |
| `GET` | `/api/v1/workspaces/:workspaceId/comments?nodeId=X` | Comments on a node |
| `GET` | `/api/v1/workspaces/:workspaceId/comments?filePath=X&branch=Y` | Line comments for file on branch |
| `GET` | `/api/v1/workspaces/:workspaceId/comments/cross-branch?filePath=X` | Cross-branch comment discovery |
| `POST` | `/api/v1/workspaces/:workspaceId/comments` | Create comment |
| `PATCH` | `/api/v1/comments/:id` | Update / resolve comment |
| `POST` | `/api/v1/comments/:id/replies` | Add reply |
| `DELETE` | `/api/v1/comments/:id` | Delete comment |

**`POST /api/v1/workspaces/:workspaceId/comments` request:**

```typescript
interface CreateCommentRequest {
  scope: 'line' | 'node';
  content: string;

  // For scope = 'node'
  nodeId?: string;

  // For scope = 'line'
  filePath?: string;
  lineNumber?: number;
  commitSha?: string;
}
```

**`GET /api/v1/workspaces/:workspaceId/comments/cross-branch` response:**

```typescript
interface CrossBranchCommentResponse {
  // Comments on other branches that touch this file
  comments: Array<{
    id: string;
    originBranch: string;
    author: UserSummary;
    content: string;
    lineNumber: number;
    commitSha: string;
    // Best-effort mapped line on current branch (null if unmappable)
    mappedLine: number | null;
    createdAt: string;
  }>;
  // Summary for UI badges
  branchCount: number;        // "5 comments on 2 other branches"
  totalCount: number;
}
```

### 3.6 Chat Sessions API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/sessions` | List sessions (paginated, searchable) |
| `POST` | `/api/v1/workspaces/:workspaceId/sessions` | Create session |
| `GET` | `/api/v1/sessions/:id` | Session detail |
| `GET` | `/api/v1/sessions/:id/messages` | Full message history (paginated) |
| `POST` | `/api/v1/sessions/:id/messages` | Send message (triggers streaming) |
| `POST` | `/api/v1/sessions/:id/messages/:mid/stop` | Stop generation |
| `POST` | `/api/v1/sessions/:id/background/end` | End the wait on Claude's background work of the named running attempt (`{taskId, attempt}`, owner; 409 once that attempt is no longer running) |
| `POST` | `/api/v1/sessions/:id/resume` | Resume completed session |
| `POST` | `/api/v1/sessions/:id/fork` | Fork from message index |
| `POST` | `/api/v1/sessions/:id/summarize` | Auto-generate title + summary |
| `GET` | `/api/v1/workspaces/:workspaceId/sessions/search?q=...` | Full-text search |

**`POST /api/v1/sessions/:id/messages` request:**

```typescript
interface SendMessageRequest {
  content: string;
  context?: {
    nodeIds?: string[];
    fileRanges?: Array<{
      path: string;
      startLine: number;
      endLine: number;
    }>;
    taskId?: string;
  };
  model?: string;
  maxTokens?: number;
}
```

**Response:** `{ "messageId": "...", "status": "pending" }`
Actual content streams over WebSocket (see §4).

**`POST /api/v1/sessions/:id/fork` request:**

```typescript
interface ForkSessionRequest {
  fromMessageIndex: number;
  title?: string;
}
// Response: { "sessionId": "...", "messagesCopied": 12 }
```

### 3.7 Draft Sessions API

Draft sessions are scoped to a **workspace branch** (= one repo within a
workspace). A draft is created lazily when the first edit arrives for a
branch, and destroyed on commit or discard.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/drafts` | List active draft sessions across all branches |
| `GET` | `/api/v1/drafts/:draftId` | Get draft state |
| `POST` | `/api/v1/drafts/:draftId/commit` | Commit draft → git (advances `head_commit`) |
| `POST` | `/api/v1/drafts/:draftId/discard` | Discard all edits |
| `GET` | `/api/v1/drafts/:draftId/participants` | Current participants |
| `POST` | `/api/v1/branches/:branchId/drafts` | Fork: create new draft from branch HEAD |
| `POST` | `/api/v1/workspaces/:workspaceId/commit-all` | Commit all dirty drafts across repos |

The `:draftId` is the `draft_sessions.id` UUID. Callers typically discover
it via the list endpoint or the branch-scoped fork endpoint.

**`POST /api/v1/drafts/:draftId/commit` request:**

```typescript
interface CommitDraftRequest {
  message: string;
  expectedHeadCommit: string;   // CAS guard — must match current head_commit
}
// Response 200: { "commitSha": "abc1234", "headCommit": "abc1234" }
//   → workspace_branches.head_commit is advanced to the new commit
//   → draft_sessions row is deleted (clean state)
// Response 409 (DRAFT_CONFLICT): head_commit has changed since the client
//   last read it (another user committed first, or tracking branch advanced).
//   The draft is PRESERVED — no edits are lost. Client should:
//     1. Re-fetch the current head_commit
//     2. Show conflict resolution UI (rebase / discard / force)
//     3. Retry with updated expectedHeadCommit
//
// Commit atomicity (server-side):
//   BEGIN;
//     SELECT head_commit FROM workspace_branches WHERE id = $branch_id FOR UPDATE;
//     IF head_commit != expectedHeadCommit → ROLLBACK, return 409;
//     extract dirty files from Yjs → git add + commit + push;
//     IF git push fails (non-fast-forward) → ROLLBACK, return 409;
//     UPDATE workspace_branches SET head_commit = $new_sha;
//     DELETE FROM draft_sessions WHERE id = $draft_id;
//     DELETE FROM draft_updates WHERE session_id = $draft_id;
//   COMMIT;
//
// The SELECT FOR UPDATE prevents two concurrent commits on the same branch
// from racing. The expectedHeadCommit check prevents stale clients from
// silently overwriting newer work.
```

**`POST /api/v1/workspaces/:workspaceId/commit-all` request:**

```typescript
interface CommitAllRequest {
  message: string;             // Shared commit message for all repos
}
// Commits each dirty draft independently. Returns per-branch results.
// Response 200: { results: Array<{ branchId, repoName, commitSha, error? }> }
```

### 3.8 Execution Intervention API

Per WORKER_INFRASTRUCTURE.md §10.9.

> **Authoritative endpoint list.** Session-related endpoints in
> WORKER_INFRASTRUCTURE.md §10.13 (session control, worker attach/detach)
> are defined here in §3.6 and §3.8 as the single source of truth.
> WORKER_INFRASTRUCTURE.md describes behavior and workflows;
> this document defines the concrete API paths and contracts.

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/sessions/:id/inject` | Inject message into task agent (fire-and-forget) |
| `POST` | `/api/v1/sessions/:id/attach` | Attach to task session (join CRDT + WS) |
| `POST` | `/api/v1/tasks/:id/pause` | Pause agent |
| `POST` | `/api/v1/tasks/:id/resume` | Resume agent |
| `POST` | `/api/v1/tasks/:id/abort` | Abort task |
| `POST` | `/api/v1/tasks/:id/complete` | User marks a failed/blocked task as completed |

**`POST /api/v1/tasks/:id/complete` — User-Driven Unblock:**

This is the primary mechanism for recovering from task failures. When a
task fails, its worker stays alive with the branch in draft state. The
user attaches (Mode 2 from §10.9), sees the agent's work and the error,
fixes the issue in the shared CRDT draft, and marks the task done.

```typescript
interface CompleteTaskRequest {
  // Optional: commit the current draft state before marking complete.
  // If true, the server commits the task branch's draft to a real commit.
  commitDraft?: boolean;           // default: true
  commitMessage?: string;          // default: auto-generated
}

// Server-side behavior:
// 1. Validate: task.status must be 'failed' or 'blocked'
// 2. If commitDraft: commit the task branch's CRDT draft → git commit
// 3. UPDATE tasks SET status = 'completed'
// 4. UPDATE all dependents WHERE depends_on contains this task:
//    IF all their other deps are also completed:
//      SET status = 'pending' (re-eligible for scheduling)
//    ELSE:
//      Leave as 'blocked' (still waiting on other deps)
// 5. UPDATE jobs SET completed_tasks = completed_tasks + 1
// 6. Trigger scheduler iteration → picks up newly unblocked tasks
```

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    User-Driven Unblock Flow                                  │
│                                                                              │
│  1. Task T3 fails (agent hit an error it can't recover from)               │
│     → T3.status = 'failed'                                                 │
│     → T5 (depends on T3) → T5.status = 'blocked'                          │
│     → Worker stays alive, branch draft is preserved                        │
│                                                                              │
│  2. User sees T3 ❌ in the task list, clicks [Open]                        │
│     → POST /api/v1/sessions/:id/attach                                     │
│     → Browser connects to the task worker's WS + CRDT draft               │
│     → UI shows: agent's conversation history, file edits, the error        │
│                                                                              │
│  3. User fixes the code in the CRDT draft (Monaco editor)                  │
│     → Edits flow through the same Yjs doc as the agent's edits            │
│     → User can also type in the task chat to leave notes                   │
│                                                                              │
│  4. User clicks [Mark Complete] → POST /api/v1/tasks/T3/complete          │
│     → Server commits the draft, marks T3 completed                         │
│     → T5 unblocked: 'blocked' → 'pending'                                 │
│     → Scheduler assigns T5 to a worker on the next tick                    │
│                                                                              │
│  No automatic retry. No "skip". The human is the recovery mechanism.      │
└─────────────────────────────────────────────────────────────────────────────┘
```

**`POST /api/v1/sessions/:id/inject` request:**

```typescript
interface InjectMessageRequest {
  content: string;
  priority?: 'normal' | 'high';
}
// Fire-and-forget. Agent processes at next tool boundary.
```

### 3.9 Settings API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/settings` | Get workspace settings |
| `PUT` | `/api/v1/workspaces/:workspaceId/settings` | Update workspace settings (partial) |
| `GET` | `/api/v1/workspaces/:workspaceId/settings/spend` | Get current spend vs. limits |

**`GET /api/v1/workspaces/:workspaceId/settings` response:**

```typescript
interface WorkspaceSettingsDTO {
  workspaceId: string;
  maxConcurrentTasks: number;          // Default: 4
  maxSpendUsd: number | null;          // NULL = unlimited
  spendPeriod: 'hourly' | 'daily' | 'weekly' | 'monthly' | 'total';
  spendPeriodStart: string | null;     // ISO 8601
  maxInputTokens: number | null;
  maxOutputTokens: number | null;
  maxCloudWorkers: number;             // Default: 8
  allowCloudWorkers: boolean;          // Default: true
  workerIdleTimeoutMin: number;        // Default: 5
  tasksPerWorker: number;              // Default: 1 — ready tasks per worker before scaling up
  workerScalingIntervalSec: number;    // Default: 5 — how often the scaler runs
  updatedBy: string | null;
  updatedAt: string;
}
```

**`PUT /api/v1/workspaces/:workspaceId/settings` request (partial update):**

```typescript
interface UpdateWorkspaceSettingsRequest {
  maxConcurrentTasks?: number;         // Must be >= 1
  maxSpendUsd?: number | null;         // NULL removes the cap
  spendPeriod?: 'hourly' | 'daily' | 'weekly' | 'monthly' | 'total';
  maxInputTokens?: number | null;
  maxOutputTokens?: number | null;
  maxCloudWorkers?: number;
  allowCloudWorkers?: boolean;
  workerIdleTimeoutMin?: number;       // Must be >= 1
  tasksPerWorker?: number;             // Must be >= 1
  workerScalingIntervalSec?: number;   // Must be >= 1
  embeddingModel?: string;             // e.g. 'text-embedding-3-small'
  embeddingDimensions?: number;        // e.g. 1536
  embeddingProvider?: 'openai' | 'local' | 'none';
}
// Only org owners, admins, and workspace creators can update.
// Changes take effect immediately — the scheduler reads settings
// on every tick, not at job creation time.
```

**`GET /api/v1/workspaces/:workspaceId/settings/spend` response:**

```typescript
interface SpendSummaryDTO {
  periodStart: string;                 // ISO 8601
  periodEnd: string;                   // Computed from period + start
  totalSpendUsd: number;               // Sum of task.cost_usd in the window
  totalInputTokens: number;
  totalOutputTokens: number;
  limitSpendUsd: number | null;        // The configured cap
  limitInputTokens: number | null;
  limitOutputTokens: number | null;
  isSpendLimitReached: boolean;        // True if any limit is hit
  // Per-job breakdown for the settings page chart
  byJob: Array<{
    jobId: string;
    jobName: string;
    jobType: string;
    spendUsd: number;
    inputTokens: number;
    outputTokens: number;
  }>;
}
```

### 3.10 Search API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/search?q=...` | Unified search across ontology, docs, files, comments |
| `GET` | `/api/v1/workspaces/:workspaceId/sessions/search?q=...` | Full-text search over chat sessions (already in §3.6) |

Search runs **entirely inside Postgres** using two complementary strategies
against the `search_documents` table (§2.9) and existing metadata tables:

1. **Full-text search (FTS)** — `tsvector` + GIN indexes. Handles keyword
   queries with ranking (`ts_rank_cd`), prefix matching (`query:*`), and
   phrase matching.
2. **Semantic search** — `pgvector` HNSW index on `embedding vector(1536)`.
   Handles natural-language queries by comparing the query embedding against
   pre-computed document embeddings via cosine distance.
3. **Metadata search** — existing GIN indexes on `ontology_nodes`,
   `chat_sessions`, and `chat_messages` for names, titles, and comment content.

The API server computes a combined score: `0.6 * fts_score + 0.4 * semantic_score`
(tunable), deduplicates by `(file_path, chunk_index)`, and merges with metadata
results.

**`GET /api/v1/workspaces/:workspaceId/search?q=...` query parameters:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `q` | string | required | Search query (min 2 chars) |
| `types` | string | all | Comma-separated: `node,doc,file,comment` |
| `mode` | string | `hybrid` | `keyword` (FTS only), `semantic` (vector only), `hybrid` (both) |
| `limit` | int | 20 | Max results per type |

**Response:**

```typescript
interface SearchResponse {
  results: SearchResult[];
  totalCount: number;
  queryTimeMs: number;
  mode: 'keyword' | 'semantic' | 'hybrid';
}

interface SearchResult {
  type: 'node' | 'doc' | 'file' | 'comment';
  id: string;
  title: string;
  /** Snippet with <mark> tags around matches */
  snippet: string;
  /** Breadcrumb path to the result (e.g. "core > rust_engine > join_ht") */
  breadcrumb: string;
  /** Relevance score (0–1, higher = more relevant) */
  score: number;
  /** Extra fields depending on type */
  meta: {
    nodeType?: 'node' | 'document' | 'code';
    filePath?: string;
    lineNumber?: number;          // for comment results
    syncStatus?: string;          // for node results
  };
}
```

**Query execution (Postgres):**

```sql
-- FTS leg: keyword search against search_documents
SELECT sd.id, sd.title, sd.file_path, sd.content_snippet, sd.node_id,
       ts_rank_cd(sd.fts, websearch_to_tsquery('english', $1)) AS fts_score
  FROM search_documents sd
  JOIN workspace_branches wb ON wb.id = sd.workspace_branch_id
 WHERE wb.workspace_id = $2
   AND sd.fts @@ websearch_to_tsquery('english', $1)
 ORDER BY fts_score DESC
 LIMIT $3;

-- Semantic leg: vector similarity against search_documents
-- (query embedding computed by g5-api using the configured embedding model)
SELECT sd.id, sd.title, sd.file_path, sd.content_snippet, sd.node_id,
       1 - (sd.embedding <=> $4::vector) AS semantic_score
  FROM search_documents sd
  JOIN workspace_branches wb ON wb.id = sd.workspace_branch_id
 WHERE wb.workspace_id = $2
   AND sd.embedding IS NOT NULL
 ORDER BY sd.embedding <=> $4::vector
 LIMIT $3;
```

Results from both legs are merged in application code using reciprocal
rank fusion (RRF) or weighted linear combination, then enriched with
breadcrumb paths from the ontology tree.

**Search indexing pipeline** (runs inside g5-api):

```
Draft commit / Job completion / Branch sync
        │
        ▼
  g5-api receives event: index.reindex { workspaceId, branchId, commitSha }
        │
        ▼
  Search indexer (async Postgres job):
    1. Read git tree at commitSha (via g5d file API or bare repo)
    2. For each markdown / code / spec / doc file:
       a. Compute content_hash — skip if unchanged
       b. Extract title (first heading or filename)
       c. Chunk large files (>4 KB) into overlapping segments
          (chunk_size = 2048 tokens, overlap = 256 tokens;
           configurable in workspace_settings)
       d. Compute tsvector: to_tsvector('english', content)
       e. Compute embedding: call embedding model (e.g., text-embedding-3-small)
          If embedding_provider = 'none' in workspace_settings, skip this step
          (FTS-only mode). If the embedding API is unreachable, log a warning
          and leave embedding = NULL; the row is still FTS-searchable.
       f. Resolve to ontology node (via file_path → ontology_nodes)
       g. UPSERT into search_documents
    3. DELETE stale rows (file_path not in current tree)
```

**Embedding model:** Configurable in workspace settings (§2.11:
`embedding_model`, `embedding_dimensions`, `embedding_provider`). Default:
OpenAI `text-embedding-3-small` (1536 dimensions). Runs asynchronously —
FTS is available immediately on index; embeddings populate within seconds.
Semantic search degrades gracefully to FTS-only when embeddings are pending.
Embedding API calls are tracked as `usage_records` rows with
`request_type = 'embedding'` for cost attribution (see §3.15.1).

### 3.11 Auth & Users API

#### 3.11.1 Login (Identity)

Login establishes the user's **identity** in G5. It does *not* grant repo
access — that is handled separately by SCM connections (§3.11.2).

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/auth/login/:provider` | Initiate OAuth flow (GitHub, Google, GitLab) |
| `GET` | `/api/v1/auth/callback/:provider` | OAuth callback → issues JWT |
| `POST` | `/api/v1/auth/refresh` | Refresh JWT using refresh token |
| `POST` | `/api/v1/auth/logout` | Revoke refresh token |
| `GET` | `/api/v1/users/me` | Get current user profile |
| `PATCH` | `/api/v1/users/me` | Update display name, avatar |
| `GET` | `/api/v1/users/me/settings` | Get user preferences (theme, editor, notifications) |
| `PUT` | `/api/v1/users/me/settings` | Update user preferences (full replace) |
| `GET` | `/api/v1/users/:id` | Get user public profile (for avatars in comments/chat) |

**OAuth scopes requested at login (identity only):**

| Provider | Scopes | Notes |
|----------|--------|-------|
| GitHub | `user:email`, `read:user` | Identity only — no repo access |
| GitLab | `openid`, `email`, `profile` | Identity only |
| Google | `openid`, `email`, `profile` | Identity only |

**`GET /api/v1/users/me` response:**

```typescript
interface CurrentUserDTO {
  id: string;
  email: string;
  displayName: string;
  avatarUrl: string | null;
  oauthProvider: string;
  orgs: Array<{
    orgId: string;
    orgName: string;
    orgSlug: string;
    role: 'owner' | 'admin' | 'member' | 'viewer';
  }>;
  scmConnections: Array<{
    provider: 'github' | 'gitlab' | 'bitbucket';
    providerUsername: string;
    scopes: string[];
    hasInstallation: boolean;   // GitHub App installed?
    connectedAt: string;        // ISO 8601
  }>;
  createdAt: string;
}
```

**JWT payload (issued by `/api/v1/auth/callback`):**

```typescript
interface JWTPayload {
  sub: string;        // user ID
  email: string;
  orgId: string;      // currently-active org
  role: string;       // role in that org
  iat: number;
  exp: number;        // 1 hour
}
```

Refresh tokens are stored server-side (Redis or Postgres). The client stores
the JWT in memory (not localStorage) and the refresh token in an httpOnly
cookie.

##### User Settings (Preferences) API

User-level preferences (theme, editor settings, notification channels) are
stored as a JSONB column on the `users` table and exposed via two endpoints:

**`GET /api/v1/users/me/settings` response:**

```typescript
interface UserSettingsDTO {
  themeName: ThemeName;
  themeMode: 'dark' | 'light';
  defaultModel: string;
  budgetWarnAt: number;
  notifyTaskComplete: boolean;
  notifyAgentBlocked: boolean;
  notifyChannels: NotifyChannel[];
  compactSidebar: boolean;
  editor: EditorSettings;
}
```

Returns the user's stored preferences, or server-side defaults if no
preferences have been saved yet.

**`PUT /api/v1/users/me/settings` request body:** Same as `UserSettingsDTO`.
Full replacement — the client sends the complete settings object on each save.
Returns the updated `UserSettingsDTO`.

**Client-side persistence model:** The frontend maintains a dual-write strategy:
1. **localStorage** (`g5:user-settings`): Written synchronously on every change
   for instant responsiveness (theme switches, editor toggle states).
2. **Backend PUT**: Called debounced (500ms) after each change for cross-device
   sync. On mount, the client fetches from the backend and merges with the
   localStorage cache (server wins on field-level conflict).

> **Status (2026-02-13):** This endpoint is not yet implemented. The `users`
> table needs a `preferences JSONB` column, and a new route handler pair in
> `routes/users.rs` is required.

#### 3.11.2 SCM Connections (GitHub / GitLab Repo Access)

SCM connections are a **separate OAuth grant** that gives G5 access to the
user's repositories on a source-code provider. A user may log in with Google
but connect GitHub for repo access.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       SCM Connection Flow                                    │
│                                                                              │
│  1. User clicks "Connect GitHub" in Settings                                │
│     → GET /api/v1/scm/github/connect                                        │
│     → 302 redirect to GitHub OAuth authorize URL                            │
│        (scopes: repo, read:org)                                             │
│                                                                              │
│  2. User authorizes on GitHub                                               │
│     → GitHub redirects to /api/v1/scm/github/callback?code=xxx              │
│                                                                              │
│  3. Server exchanges code for access token                                  │
│     → Encrypts token (AES-256-GCM, key from Secret Manager)                │
│     → Upserts into user_scm_connections                                     │
│     → Optionally: triggers background sync of accessible repos              │
│                                                                              │
│  4. For GitHub Apps (preferred for org repos):                              │
│     → User also installs the G5 GitHub App on their org                     │
│     → Webhook: installation event → store installation_id                   │
│     → Installation tokens are short-lived (1hr), requested on demand        │
│       via GET /app/installations/:id/access_tokens                          │
│                                                                              │
│  Clone URL construction:                                                    │
│    GitHub: https://x-access-token:{token}@github.com/owner/repo.git        │
│    GitLab: https://oauth2:{token}@gitlab.com/owner/repo.git                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

**SCM Connection Endpoints:**

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/scm/connections` | List user's connected providers |
| `GET` | `/api/v1/scm/:provider/connect` | Initiate SCM OAuth (redirect) |
| `GET` | `/api/v1/scm/:provider/callback` | SCM OAuth callback → stores token |
| `DELETE` | `/api/v1/scm/:provider/disconnect` | Revoke + delete SCM connection |
| `GET` | `/api/v1/scm/:provider/repos` | List repos user can access on provider |
| `POST` | `/api/v1/scm/:provider/repos/import` | Import repo(s) into an org → creates `repos` rows |
| `GET` | `/api/v1/scm/:provider/orgs` | List orgs/groups the user belongs to on provider |

**OAuth scopes requested for SCM connections:**

| Provider | Scopes | What they grant |
|----------|--------|-----------------|
| GitHub (OAuth App) | `repo`, `read:org` | Full read/write on all accessible repos, list org memberships |
| GitHub (App install) | Contents: R/W, Metadata: R, PRs: R/W | Per-repo, granular; preferred for orgs |
| GitLab | `read_repository`, `write_repository`, `read_api` | Repo clone/push, list workspaces |
| Bitbucket | `repository:write`, `account:read` | Repo access + user info |

**`GET /api/v1/scm/:provider/repos` response:**

```typescript
interface SCMRepoListResponse {
  repos: SCMRepoDTO[];
  nextCursor: string | null;     // Provider pagination cursor
}

interface SCMRepoDTO {
  providerRepoId: string;        // Provider's numeric/string ID
  fullName: string;              // e.g. 'acme/backend'
  name: string;                  // e.g. 'backend'
  owner: string;                 // Org or user on the provider
  defaultBranch: string;
  isPrivate: boolean;
  cloneUrl: string;              // HTTPS clone URL
  description: string | null;
  language: string | null;       // Primary language detected by provider
  updatedAt: string;
  alreadyImported: boolean;      // true if repos row already exists in G5
  importedRepoId: string | null; // G5 repo ID if already imported
}
```

**`POST /api/v1/scm/:provider/repos/import` request:**

```typescript
interface ImportReposRequest {
  orgId: string;                 // G5 org to import into
  repos: Array<{
    providerRepoId: string;
    fullName: string;
    cloneUrl: string;
    defaultBranch: string;
  }>;
}
// Response: { imported: RepoDTO[] }
```

**Token refresh & lifecycle:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     SCM Token Lifecycle                                       │
│                                                                              │
│  GitHub OAuth App tokens:                                                   │
│    • Do not expire (unless user revokes)                                    │
│    • Check validity: GET /api.github.com/user  → 401 means revoked         │
│    • On 401 during clone/API call → mark connection as stale                │
│      → notify user to reconnect                                             │
│                                                                              │
│  GitHub App installation tokens:                                            │
│    • Expire after 1 hour                                                    │
│    • Requested on-demand from GitHub:                                       │
│      POST /app/installations/:id/access_tokens                              │
│    • Cached in Redis with TTL = expiry - 5 min                              │
│    • No user interaction needed for refresh                                 │
│                                                                              │
│  GitLab OAuth tokens:                                                       │
│    • Access token expires in 2 hours                                        │
│    • Refresh token valid for ~6 months                                      │
│    • Background job: refresh tokens where                                   │
│      token_expires_at < now() + interval '10 minutes'                       │
│    • On refresh failure → mark connection as stale → notify user            │
│                                                                              │
│  Encryption:                                                                │
│    • AES-256-GCM; key in GCP Secret Manager / Vault                        │
│    • Rotated via envelope encryption (KEK wraps DEKs)                       │
│    • DEK stored alongside ciphertext; only KEK needs rotation               │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.12 Org Management API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/orgs` | List orgs the current user belongs to |
| `POST` | `/api/v1/orgs` | Create a new org (cascade: base workspace + docs repo) |
| `GET` | `/api/v1/orgs/:orgId` | Get org details |
| `PATCH` | `/api/v1/orgs/:orgId` | Update org name/slug (owner/admin only) |
| `GET` | `/api/v1/orgs/:orgId/members` | List org members with roles |
| `POST` | `/api/v1/orgs/:orgId/members` | Invite user to org (by email + role) |
| `PATCH` | `/api/v1/orgs/:orgId/members/:userId` | Change member role |
| `DELETE` | `/api/v1/orgs/:orgId/members/:userId` | Remove member from org |
| `GET` | `/api/v1/orgs/:orgId/invitations` | List pending/recent invitations |
| `POST` | `/api/v1/orgs/:orgId/invitations/:id/resend` | Resend invitation email (resets expiry) |
| `DELETE` | `/api/v1/orgs/:orgId/invitations/:id` | Revoke pending invitation |
| `POST` | `/api/v1/invitations/:id/accept` | Accept invitation (authenticated, email must match) |
| `POST` | `/api/v1/invitations/:id/decline` | Decline invitation |
| `GET` | `/api/v1/orgs/:orgId/repos` | List repos in the org |

#### 3.12.1 Org Creation Cascade

`POST /api/v1/orgs` performs a multi-step cascade inside a single
transaction. The caller provides a name and optional slug; the server
provisions the full org scaffolding:

```typescript
interface CreateOrgRequest {
  name: string;
  slug?: string;                      // Auto-generated from name if omitted
}

interface CreateOrgResponse {
  org: { id: string; name: string; slug: string; createdAt: string };
  baseWorkspace: { id: string; name: string };
  docsRepo: { id: string; name: string; gitUrl: string };
}
```

**Server-side cascade (single transaction):**

```
POST /api/v1/orgs { name: "Acme" }
        │
        ▼
  1. INSERT INTO orgs (name, slug) → org_id
        │
        ▼
  2. INSERT INTO org_memberships (org_id, user_id, role='owner')
     (current user becomes owner)
        │
        ▼
  3. Provision docs repo:
     a. Create bare git repo for docs (or use provider API if SCM connected)
     b. INSERT INTO repos (org_id, name='docs', slug='docs', git_url=...,
                           default_branch='main')
     → docs_repo_id
        │
        ▼
  4. Create base workspace:
     INSERT INTO workspaces (
       org_id, name='Base', is_base=true, status='active',
       docs_repo_id, docs_branch_name='main',
       docs_tracking_mode='tracking', visibility='org',
       created_by=current_user_id
     ) → base_workspace_id
        │
        ▼
  5. Wire existing repos (if any imported during onboarding):
     For each repo in org:
       INSERT INTO workspace_branches (
         workspace_id=base_workspace_id, repo_id,
         branch_name=default_branch, base_branch=default_branch,
         head_commit=current_HEAD, tracking_mode='tracking'
       )
        │
        ▼
  6. Return CreateOrgResponse
```

**Post-transaction side effects** (async, non-blocking):
- Initialize the docs repo with a `README.md` template.
- Trigger search indexing for the base workspace.
- Send welcome notification to the org owner.

**CLI path:** Org creation is API-only. The CLI (`g5d`) operates on
workspaces, not orgs. A user creates an org via the web UI or a direct
API call (`curl -X POST /api/v1/orgs`), then uses `g5d clone` or
`g5d init` to work with workspaces in that org.

#### 3.12.2 Invite to Org

`POST /api/v1/orgs/:orgId/members` invites a user by email with an
explicit role. If the email matches an existing G5 user, membership is
created immediately. Otherwise, a pending invitation is recorded and the
user is added on their first login.

```typescript
interface InviteOrgMemberRequest {
  email: string;
  role: 'admin' | 'member' | 'viewer';   // 'owner' cannot be assigned via invite
}

interface InviteOrgMemberResponse {
  membership: OrgMemberDTO | null;        // Non-null if user already exists
  invitation: OrgInvitationDTO | null;    // Non-null if user not yet registered
}

interface OrgInvitationDTO {
  id: string;
  orgId: string;
  email: string;
  role: 'admin' | 'member' | 'viewer';
  invitedBy: UserRef;
  status: 'pending' | 'accepted' | 'declined' | 'expired';
  expiresAt: string;                      // 7 days from creation
  createdAt: string;
}
```

**Invitation state machine:**

```
                    ┌─────────┐
  POST /orgs/:id/  │ pending │  ─── created on invite
  members          └────┬────┘
                        │
          ┌─────────┼─────────┐
          │             │             │
          ▼             ▼             ▼
    ┌─────────┐  ┌─────────┐  ┌─────────┐
    │accepted │  │declined │  │ expired │
    └─────────┘  └─────────┘  └─────────┘
    (terminal)    (terminal)    (terminal)
```

**Acceptance triggers:**

1. **Explicit accept:** User calls `POST /api/v1/invitations/:id/accept`.
   The user must be authenticated and their email must match the invitation.
   Server creates `org_memberships` row and sets invitation status to `accepted`.
2. **Auto-accept on login:** When a user logs in (or signs up), the auth
   callback checks `org_invitations` for pending rows matching the user's
   email. Any matches are auto-accepted (org membership created, invitation
   marked `accepted`). This handles the case where the invited user did not
   have a G5 account at invite time.
3. **Decline:** User calls `POST /api/v1/invitations/:id/decline`. No
   membership is created. The invitation is terminal.
4. **Expiry:** A background job runs every hour:
   `UPDATE org_invitations SET status = 'expired', updated_at = now()`
   `WHERE status = 'pending' AND expires_at < now();`
   Expired invitations are retained for 90 days (audit trail), then deleted.

**Resend:** `POST /api/v1/orgs/:orgId/invitations/:id/resend` resets
`expires_at` to `now() + 7 days` and re-sends the email. Only works for
`pending` invitations.

**Edge cases:**
- Inviting an email that already has a pending invitation: returns the
  existing invitation (idempotent, does not create a duplicate).
- Inviting an email that belongs to an existing org member: returns the
  existing membership (no invitation created).
- User signs up with a different email than the invitation: the invitation
  remains pending. The user must log in with the invited email or have
  it added as a secondary email (future feature).
```

**Org roles and privileges:**

| Role | View workspaces | Create workspaces | Manage jobs | Manage members | Manage org settings | Transfer/delete org |
|------|:-:|:-:|:-:|:-:|:-:|:-:|
| `viewer` | org-visible only | ✗ | ✗ | ✗ | ✗ | ✗ |
| `member` | org-visible only | ✓ | own workspaces | ✗ | ✗ | ✗ |
| `admin` | all | ✓ | all workspaces | invite/remove (not owner) | ✓ | ✗ |
| `owner` | all | ✓ | all workspaces | all | ✓ | ✓ |

#### 3.12.3 Workspace Visibility & Members

Workspaces are either **org-visible** (default) or **invite-only**.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/members` | List workspace members (invite-only workspaces) |
| `POST` | `/api/v1/workspaces/:workspaceId/members` | Add member to workspace |
| `PATCH` | `/api/v1/workspaces/:workspaceId/members/:userId` | Change workspace member role |
| `DELETE` | `/api/v1/workspaces/:workspaceId/members/:userId` | Remove member from workspace |
| `PATCH` | `/api/v1/workspaces/:workspaceId` | Update visibility (`{ visibility: 'org' \| 'invite_only' }`) |

**Visibility rules:**
- `org` — all org members can see and access the workspace (subject to
  their org role). This is the default.
- `invite_only` — only the workspace creator and explicit
  `workspace_members` rows can see the workspace. Org admins/owners can
  always access all workspaces regardless of visibility.

**`POST /api/v1/workspaces/:workspaceId/members` request:**

```typescript
interface AddWorkspaceMemberRequest {
  userId: string;
  role: 'admin' | 'editor' | 'viewer';
}
```

**Access check (applied on every workspace API call):**

```
fn can_access_workspace(user, workspace) -> bool:
  if user.org_role in ('owner', 'admin'):
    return true                         // Org admins bypass visibility
  if workspace.visibility == 'org':
    return user is org member           // Any org member can access
  if workspace.visibility == 'invite_only':
    return workspace.created_by == user.id
        OR (workspace_id, user.id) in workspace_members
```

**`GET /api/v1/orgs/:orgId/members` response:**

```typescript
interface OrgMemberDTO {
  userId: string;
  email: string;
  displayName: string;
  avatarUrl: string | null;
  role: 'owner' | 'admin' | 'member' | 'viewer';
  joinedAt: string;
}
```

### 3.13 Workers API (Admin / Monitoring)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/workers` | List workers assigned to this workspace |
| `GET` | `/api/v1/workers/:id` | Get worker detail (status, heartbeat, task) |
| `POST` | `/api/internal/workers/:id/poll` | Worker polls for next task (internal) |
| `POST` | `/api/internal/workers/:id/terminate` | Worker self-terminates (internal) |
| `POST` | `/api/internal/tasks/:id/result` | Worker reports task completion/failure (internal) |

**`GET /api/v1/workspaces/:workspaceId/workers` response:**

```typescript
interface WorkerSummaryDTO {
  id: string;
  role: 'editor' | 'worker';
  status: 'provisioning' | 'idle' | 'assigned' | 'executing' | 'terminated';
  taskId: string | null;
  taskName: string | null;
  branch: string | null;
  memoryMb: number;
  cpuCores: number;
  lastNodeId: string | null;  // Ontology node of last completed task (locality)
  lastHeartbeat: string;
  startedAt: string;
  uptimeSeconds: number;   // computed: now - startedAt
}
```

**`POST /api/internal/workers/:id/poll` — Worker polls for next task:**

The worker sends a poll request. The server scores all ready tasks by
locality (ontological closeness to the worker's `last_node_id`) combined with
priority and age, then atomically assigns the best-scoring task. Returns
`204 No Content` if no task is available.

```typescript
// Request body is empty — the server reads worker.last_node_id from
// the database to compute locality scores.

// 200 response: task assigned
interface TaskAssignment {
  taskId: string;
  jobId: string;
  taskType: 'bootstrap' | 'regular';
  jobType: 'compile' | 'uplift' | 'test' | 'review' | 'custom';

  // SpecContext (AGENTS.md §9.1) — the ontology binding for this task.
  // Agents treat these four fields as immutable.
  specContext: {
    componentId: string;       // Hierarchical ID (e.g. "auth/token_validator")
    srcFiles: string[];        // Source files this spec describes
    parentSpec: string | null; // Parent component_id in the hierarchy
    childSpecs: string[];      // Child component_ids (for aggregation tasks)
  } | null;                    // null for bootstrap tasks (no spec yet)

  // Additional TaskContext fields (AGENTS.md §9.2)
  taskDescription: string;     // Human-readable description of what to do
  incrementalUpdate: boolean;  // Spec-diff-driven update vs full generation

  // Git context
  branchName: string;          // task/<task-id>
  baseCommit: string;          // SHA to branch from
  workspaceGitUrl: string;       // Clone URL

  // Task scope
  dependsOn: string[];         // Task IDs this task depends on

  // Dependency specs: the specs this task may call into.
  // Populated from node_dependencies for the task's node.
  dependencySpecs: Array<{
    componentId: string;
    specPath: string;          // e.g. .g5/specs/auth/validator.md
  }>;

  // Agent configuration (AGENTS.md §12)
  sessionId: string;           // Chat session for this task's agent dialog
  guardrails: {
    maxTurns: number;
    maxCostUsd: number;
    maxExecutionTimeSec: number;
    maxTotalTokens: number;
  };
}

// 204 response: no task available (empty body)
```

**`POST /api/internal/tasks/:id/result` — Worker reports task result:**

Called by the worker after a task completes (success or failure). The server
updates the task status, rolls up cost to the job, and triggers the scheduler
to check for newly-unblocked tasks.

```typescript
interface TaskResultRequest {
  status: 'completed' | 'failed' | 'blocked';
  error?: string;              // Failure reason (required if status = 'failed')
  blockedReason?: string;      // Why the agent is stuck (required if status = 'blocked')

  // Cost accounting
  inputTokens: number;
  outputTokens: number;
  costUsd: number;

  // Checkpoint (for completed tasks)
  commitSha?: string;          // Final commit on the task branch
  filesChanged?: string[];     // Files modified by this task
}

// 200 response
interface TaskResultResponse {
  acknowledged: boolean;
  jobStatus: 'bootstrapping' | 'running' | 'completed' | 'failed';
  // Hint: should the worker keep polling or self-terminate?
  hasMoreWork: boolean;
}
```

**Agent Tool → API Endpoint Mapping:**

The agent runs inside the worker and calls tools defined in AGENTS.md §6. The
worker translates each tool into the corresponding API call. This table makes
the coupling explicit:

| Agent Tool (AGENTS.md §6) | API Endpoint | Server-side Effect |
|---|---|---|
| `transition_task_state(state: "complete")` | `POST /api/internal/tasks/:id/result { status: 'completed', commitSha, filesChanged }` | Task → `completed`, cost rolled up, dependents unblocked, job may complete |
| `transition_task_state(state: "fail", error)` | `POST /api/internal/tasks/:id/result { status: 'failed', error }` | If retries remain → task → `pending` (auto-retry). If exhausted → task → `blocked`, dependents → `blocked` |
| `transition_task_state(state: "blocked", reason)` | `POST /api/internal/tasks/:id/result { status: 'blocked', blockedReason }` | Task → `blocked`, worker retained for grace period (AGENTS.md §6.5), resumes when human sends message via session API |
| `git_commit` (checkpoint) | `POST /api/internal/tasks/:id/result` is NOT called — checkpoint commits happen locally on the task branch. The worker calls `git commit` internally and reports the SHA in a timeline entry. | Timeline entry created, no state change |
| `respond_to_user` (chat only) | `POST /api/v1/sessions/:id/messages` | Message appended to session, no task state change |
| — (user marks complete) | `POST /api/v1/tasks/:id/complete` | Task → `completed` (externally driven), blocked dependents → `pending` |
| — (bootstrap submits graph) | `POST /api/internal/jobs/:id/tasks` | Task rows created, job → `running` |

**Task state machine** (from the server's perspective):

```
                    ┌─────────┐
              ┌────▶│ pending │  created by bootstrap or job creation
              │     └────┬────┘
              │          │ assigned to worker via poll
              │          ▼
              │     ┌─────────┐
              │ ┌──▶│ running │
              │ │   └────┬────┘
              │ │        │
              │ │   ┌────┼───────────────┐
              │ │   │    │               │
              │ │   ▼    ▼               ▼
              │ │ ┌─────────┐ ┌──────────┐ ┌───────────┐
              │ │ │ blocked │ │completed │ │  failed   │ (transient)
              │ │ └────┬────┘ └──────────┘ └─────┬─────┘
              │ │      │                         │
              │ │      │ human sends              ├── retries remain → pending
              │ └──────┘ message                  │   (auto, with backoff)
              │          → running (resumes)      │
              │                                   └── retries exhausted → blocked
              │
         cancelled: terminal (user aborted)
```

### 3.14 File Tree API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/branches/:branchId/files` | List file tree at HEAD commit |
| `GET` | `/api/v1/branches/:branchId/files/*path` | Get file content at HEAD commit |

The file tree is read from the editor's git checkout (or the cached bare repo).
Results are scoped to the branch's `head_commit`.

**`GET /api/v1/branches/:branchId/files` query parameters:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | `/` | Subdirectory to list (e.g. `src/engine`) |
| `depth` | int | 1 | Max depth (1 = immediate children, -1 = full recursive) |

**Response:**

```typescript
interface FileTreeEntry {
  name: string;
  path: string;              // relative to repo root
  type: 'file' | 'directory';
  size: number | null;       // bytes, null for directories
  /** Linked ontology node (if this file is owned by a node) */
  nodeId: string | null;
  children?: FileTreeEntry[];  // present when depth > 1
}
```

### 3.15 Spend History API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/workspaces/:workspaceId/settings/spend/history` | Spend history across multiple periods |

**Query parameters:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `periods` | int | 6 | Number of past periods to return |
| `granularity` | string | auto | `hourly`, `daily`, `weekly`, `monthly` (auto = match spend_period) |

**Response:**

```typescript
interface SpendHistoryDTO {
  granularity: string;
  periods: Array<{
    periodStart: string;
    periodEnd: string;
    totalSpendUsd: number;
    totalInputTokens: number;
    totalOutputTokens: number;
    jobCount: number;
    taskCount: number;
  }>;
  /** Daily breakdown within the requested range */
  daily: Array<{
    date: string;
    spendUsd: number;
    inputTokens: number;
    outputTokens: number;
  }>;
}
```

### 3.16 Permissions API

Manage OAuth scopes for users and organizations. See §2.12 for the data model.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/auth/me/scopes` | List effective scopes for current user in current org |
| `POST` | `/api/v1/auth/check` | Check whether current user has a set of scopes |
| `GET` | `/api/v1/orgs/:orgId/scopes` | List org default scopes |
| `PUT` | `/api/v1/orgs/:orgId/scopes` | Set org default scopes (admin/owner) |
| `GET` | `/api/v1/orgs/:orgId/members/:userId/scopes` | List user's scopes in org |
| `PUT` | `/api/v1/orgs/:orgId/members/:userId/scopes` | Set user's scopes in org (admin/owner) |
| `GET` | `/api/v1/workspaces/:workspaceId/permissions` | List all permission overrides for a workspace |
| `PUT` | `/api/v1/workspaces/:workspaceId/permissions/:userId` | Set per-workspace permission overrides |
| `DELETE` | `/api/v1/workspaces/:workspaceId/permissions/:userId` | Remove per-workspace overrides (revert to org-level) |

**`POST /api/v1/auth/check` request:**

```typescript
interface ScopeCheckRequest {
  scopes: string[];              // e.g., ['workspaces:write', 'jobs:execute']
  workspaceId?: string;            // Optional: check with workspace-scoped overrides
}
// Response 200: { "allowed": true, "missing": [] }
// Response 200: { "allowed": false, "missing": ["jobs:execute"] }
```

**`PUT /api/v1/orgs/:orgId/members/:userId/scopes` request:**

```typescript
interface SetUserScopesRequest {
  scopes: string[];              // Full replacement — not additive
  expiresAt?: string;            // ISO 8601; NULL = permanent
}
```

### 3.17 Instance API

Server metadata and health — used by the frontend shell and worker bootstrap.

> **Not to be confused with WORKER_INFRASTRUCTURE.md §11.9 "Instance API".**
> That section describes the `g5d` daemon's local API surface
> (`/api/v1/instances/`, search, file streaming) served by each `g5d`
> process. The endpoints below are served by `g5-api` (the central server)
> and provide global metadata — not per-instance state. See DESIGN.md §4.2
> for the binary responsibility matrix.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/instance/info` | Server version, features, auth providers |
| `GET` | `/api/v1/instance/health` | Health check (DB, Redis, search) |
| `GET` | `/api/v1/instance/chat/models` | Available LLM models |
| `GET` | `/api/v1/instance/search/status` | Search index status |

**`GET /api/v1/instance/info` response:**

```typescript
interface InstanceInfoDTO {
  version: string;               // Semver, e.g., '0.1.0'
  features: string[];            // Feature flags: ['cloud_workers', 'search', ...]
  authProviders: string[];       // ['github', 'google', 'gitlab']
  maxUploadSizeMb: number;
  motd?: string;                 // Optional message of the day
}
```

### 3.18 Slug Resolver API

Resolves human-readable `org/repo/workspace` slug paths to canonical UUIDs.
Used by the frontend slug route handler (FRONTEND_DESIGN.md §2) to redirect
`/:orgSlug/:repoSlug/:workspaceSlug` URLs to ID-based routes.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/resolve` | Resolve slug combination to IDs |

**`GET /api/v1/resolve` query parameters:**

| Param | Required | Description |
|-------|----------|-------------|
| `org` | yes | Org slug |
| `repo` | no | Repo slug (within the org) |
| `workspace` | no | Workspace slug (within the org) |

**Response:**

```typescript
interface ResolveDTO {
  orgId: string;                  // Always present
  repoId?: string;               // Present if `repo` param was given
  workspaceId?: string;            // Present if `workspace` param was given
}
// 404 if any slug does not resolve.
// 302 redirect variant: GET /api/v1/resolve?org=acme&repo=backend&workspace=streaming-api&redirect=/w/{workspaceId}
//   → 302 Location: /w/a1b2c3d4-...
```

Slugs are unique within their parent scope: org slugs are globally unique,
repo slugs are unique within an org, and workspace slugs are unique within an
org. When a workspace or repo is renamed, its slug changes but its UUID is
stable — existing ID-based bookmarks continue to work, and old slug URLs
return 404 (no silent redirect to avoid confusion).

---

## 4. WebSocket API

A single WebSocket connection per client, multiplexed via channels:

```
WS /api/v1/ws?token=<jwt>
```

### 4.1 Subscribe / Unsubscribe

```typescript
// Client → Server
{ "type": "subscribe",   "channel": "workspace:<id>" }
{ "type": "subscribe",   "channel": "job:<id>" }
{ "type": "subscribe",   "channel": "task:<id>" }
{ "type": "subscribe",   "channel": "chat:<sessionId>" }
{ "type": "subscribe",   "channel": "draft:<workspaceId>:<branch>" }
{ "type": "unsubscribe", "channel": "..." }
```

### 4.2 Server-Pushed Events

```typescript
// Task status changes
{ "type": "task.status",   "taskId": "...", "status": "active" }

// Timeline entries (streaming agent activity)
{ "type": "task.timeline", "taskId": "...", "entry": TimelineEntry }

// Live diff updates
{ "type": "task.diff",     "taskId": "...", "file": "...", "hunks": DiffHunk[] }

// Job cost updates
{ "type": "job.cost",      "jobId": "...", "costUsd": 0.45 }

// Workspace sync status
{ "type": "workspace.sync",  "workspaceId": "...", "behindBase": 3, "aheadOfBase": 7 }

// Chat streaming
{ "type": "chat.chunk",    "sessionId": "...", "messageId": "...", "delta": "..." }
{ "type": "chat.done",     "sessionId": "...", "messageId": "...",
  "usage": { "inputTokens": 1250, "outputTokens": 342 },
  "citations": Citation[] }
{ "type": "chat.error",    "sessionId": "...", "messageId": "...",
  "error": "context_length_exceeded", "message": "..." }

// Draft session events (for non-CRDT-connected clients — awareness only)
{ "type": "draft.update",  "workspaceId": "...", "branch": "...",
  "dirtyFiles": ["src/auth.rs"], "dirtyCount": 1 }
{ "type": "draft.commit",  "workspaceId": "...", "branch": "...",
  "commitSha": "abc1234" }
{ "type": "draft.participant", "workspaceId": "...", "branch": "...",
  "action": "joined" | "left", "user": UserSummary }

// Comment events (cross-branch notification)
{ "type": "comment.new",   "workspaceId": "...", "commentId": "...",
  "scope": "node", "nodeId": "...", "originBranch": "..." }

// Spend limit events
{ "type": "workspace.spend_limit_reached", "workspaceId": "...",
  "periodSpendUsd": 50.00, "limitSpendUsd": 50.00,
  "reason": "max_spend_usd" | "max_input_tokens" | "max_output_tokens" }
{ "type": "workspace.spend_limit_cleared", "workspaceId": "...",
  "reason": "period_reset" | "limit_raised" }

// Settings changed (so other open tabs refresh)
{ "type": "workspace.settings_updated", "workspaceId": "...",
  "updatedBy": "user-uuid", "changes": string[] }

// Notifications (aggregated operational events for the notification system;
// FRONTEND_DESIGN.md §9.17 subscribes to these via useNotifications)
{ "type": "notification", "workspaceId": "...",
  "notification": {
    "id": "notif-uuid",
    "severity": "info" | "warning" | "error",
    "category": "task_blocked" | "build_failure" | "test_failure"
                | "spend_warning" | "spend_limit" | "agent_error"
                | "worker_terminated" | "merge_conflict",
    "title": "Task blocked: uplift auth/handler",
    "message": "Agent is waiting for user input...",
    "taskId": "..." | null,
    "jobId": "..." | null,
    "actions": [
      { "label": "View Task", "action": "navigate", "target": "/tasks/:id" },
      { "label": "Send Message", "action": "open_chat", "target": ":sessionId" }
    ],
    "createdAt": "2026-02-10T...",
    "expiresAt": "2026-02-10T..." | null
  }
}
```

### 4.3 CRDT WebSocket (Yjs)

The CRDT sync channel is **separate** from the main event WebSocket. It uses
the Yjs WebSocket provider protocol directly:

```
WS /api/v1/yjs/:workspaceId/:branch?token=<jwt>
```

This endpoint speaks the standard `y-websocket` binary protocol. The server
(`g5d` editor role) is the Yjs authority:

1. On connect: server sends full Yjs state vector to the client
2. Client applies it to local Yjs document
3. Edits flow bidirectionally as Yjs update messages
4. Server persists Yjs state to `draft_sessions.yjs_state` on every
   debounced write (100ms debounce)
5. On disconnect: server keeps the Yjs document in memory for a grace
   period (60s), then flushes to DB and drops it

Awareness (cursors, presence) also travels on this channel using the Yjs
awareness protocol.

---

## 4A. Token Taxonomy

The system uses four distinct token types. Callers must never interchange them.

| Token | Format | Lifetime | Audience | Transport |
|-------|--------|----------|----------|-----------|
| **JWT (access token)** | Signed JWT (ES256) | 1 hour | Browser clients, CLI | `Authorization: Bearer <jwt>` header |
| **Refresh token** | Opaque 256-bit hex | 30 days (sliding) | Browser only | `httpOnly` / `Secure` / `SameSite=Strict` cookie |
| **Service token** | Opaque 256-bit hex | Long-lived (rotatable) | Workers, schedulers | `Authorization: Bearer <service-token>` header |
| **Signed data-channel token** | Signed JWT (ES256) | 5 minutes | Browser → WS/CRDT | `?token=<jwt>` query param on WS upgrade |

**JWT payload structure:**

```typescript
interface JwtPayload {
  sub: string;            // user ID
  org: string;            // current org ID
  scopes: string[];       // effective scopes (see §2.12)
  iat: number;            // issued at
  exp: number;            // expires at (iat + 3600)
}
```

**Signed data-channel token:** Used for WebSocket connections (§4). Short-lived
to prevent replay attacks. The client obtains one via `POST /api/v1/auth/ws-ticket`
(returns a 5-minute JWT scoped to a specific workspace).

**Service token:** Used by workers for `/api/internal/` endpoints. Provisioned
when a worker is created and stored in the worker's environment. Rotated via
`POST /api/internal/workers/:id/rotate-token`.

---

## 4B. Error Code Catalog

All error responses use the envelope from §3.0. The `code` field is a
machine-readable string from this catalog.

| Code | HTTP | Description |
|------|------|-------------|
| `UNAUTHORIZED` | 401 | Missing or invalid authentication token |
| `FORBIDDEN` | 403 | Valid token but insufficient scopes |
| `NOT_FOUND` | 404 | Resource does not exist or is not visible to caller |
| `CONFLICT` | 409 | Optimistic concurrency conflict (stale head_commit, duplicate, etc.) |
| `VALIDATION_ERROR` | 422 | Request body fails schema validation |
| `RATE_LIMITED` | 429 | Too many requests (per-user or per-org) |
| `SPEND_LIMIT_REACHED` | 402 | Workspace spend cap exceeded — no new tasks scheduled |
| `BRANCH_DIVERGED` | 409 | Branch head_commit is behind base — rebase required |
| `DRAFT_CONFLICT` | 409 | CRDT draft cannot be committed (concurrent modification) |
| `TASK_NOT_ELIGIBLE` | 409 | Task cannot transition to requested status |
| `WORKER_NOT_FOUND` | 404 | Worker ID not in registry |
| `WORKER_BUSY` | 409 | Worker already assigned to a task |
| `SCOPE_MISSING` | 403 | Specific scope(s) required for this action |
| `ORG_QUOTA_EXCEEDED` | 402 | Org-level resource quota exceeded |
| `SEARCH_UNAVAILABLE` | 503 | Search index is temporarily unavailable or being rebuilt |
| `INTERNAL_ERROR` | 500 | Unhandled server error |

All 4xx/5xx responses include a `requestId` for correlation:

```json
{
  "error": {
    "code": "SCOPE_MISSING",
    "message": "Required scope 'jobs:execute' not granted",
    "details": { "missing": ["jobs:execute"] },
    "requestId": "req_abc123"
  }
}
```

---

## 5. Data Flow Diagrams

### 5.1 Page → API Mapping

How each UI page fetches its data:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                            WorkspacesPage                                      │
│                                                                              │
│  On mount:                                                                   │
│    GET /api/v1/orgs/:orgId/workspaces            → workspace list + stats      │
│    WS subscribe workspace:* (each workspace)  → live status updates            │
│                                                                              │
│  Sidebar (TaskQueue):                                                        │
│    GET /api/v1/workspaces/:workspaceId/jobs?include=tasks                       │
│    WS subscribe job:*, task:*               → live task status               │
│                                                                              │
│  Computed from responses:                                                    │
│    • totalActiveTasks  (count status='running')                             │
│    • totalCost         (sum cost_usd)                                       │
│    • runningTime       (max job duration)                                    │
└──────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│                            OntologyPage                                      │
│                                                                              │
│  On mount:                                                                   │
│    GET /api/v1/workspaces/:workspaceId/ontology   → full tree                    │
│    GET /api/v1/workspaces/:workspaceId/docs      → sidebar docs list            │
│                                                                              │
│  On node select:                                                             │
│    GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/content  → markdown/code │
│    GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/deps     → dependencies  │
│    GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/annotations → comments   │
│                                                                              │
│  On diff toggle:                                                             │
│    GET /api/v1/workspaces/:workspaceId/ontology/:nodeId/diff?base=:branch      │
│                                                                              │
│  On cross-branch toggle:                                                    │
│    GET /api/v1/workspaces/:workspaceId/comments/cross-branch?filePath=...       │
│                                                                              │
│  CRDT connection:                                                            │
│    WS /api/v1/yjs/:workspaceId/:branch          → live collaborative editing  │
│                                                                              │
│  Chat (right panel):                                                         │
│    GET /api/v1/sessions/:id/messages             → history                   │
│    POST /api/v1/sessions/:id/messages            → send                      │
│    WS subscribe chat:<sessionId>             → streaming response            │
└──────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│                          WorkspacePage                                       │
│                                                                              │
│  Combines OntologyPage + ExecutionPage in a single view.                    │
│                                                                              │
│  Left sidebar (ontology or jobs toggle):                                    │
│    GET /api/v1/workspaces/:workspaceId/ontology    → tree                        │
│    GET /api/v1/workspaces/:workspaceId/jobs?include=tasks → job list             │
│                                                                              │
│  Center (document content):                                                  │
│    Same as OntologyPage node-select flow                                    │
│                                                                              │
│  Bottom drawer (task panel):                                                 │
│    GET /api/v1/tasks/:id/timeline               → agent activity             │
│    WS subscribe task:<id>                    → streaming timeline             │
│                                                                              │
│  Right sidebar (chat):                                                       │
│    Same as OntologyPage chat flow                                            │
└──────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│                          ExecutionPage                                       │
│                                                                              │
│  URL: /job/:jobId/task/:taskId                                              │
│                                                                              │
│  Left column:                                                                │
│    GET /api/v1/tasks/:id                        → task detail + deps        │
│    GET /api/v1/jobs/:jid/tasks                  → task queue                 │
│                                                                              │
│  Center column:                                                              │
│    GET /api/v1/tasks/:id/timeline?page=1&limit=50  → paginated timeline     │
│    WS subscribe task:<id>                       → streaming entries          │
│    GET /api/v1/tasks/:id/diff                      → active diff            │
│                                                                              │
│  Right column (chat):                                                        │
│    POST /api/v1/workspaces/:workspaceId/sessions { scopeType:'task', scopeRef } │
│    POST /api/v1/sessions/:id/messages              → send to agent          │
│    WS subscribe chat:<sessionId>               → streaming                   │
│                                                                              │
│  Intervention:                                                               │
│    POST /api/v1/tasks/:id/pause                                             │
│    POST /api/v1/tasks/:id/resume                                            │
│    POST /api/v1/sessions/:id/inject                                         │
└──────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│                          SettingsPage                                        │
│                                                                              │
│  URL: /workspace/:workspaceId/settings                                        │
│                                                                              │
│  On mount:                                                                   │
│    GET /api/v1/workspaces/:workspaceId/settings  → current settings              │
│    GET /api/v1/workspaces/:workspaceId/settings/spend → spend summary + chart    │
│    WS subscribe workspace:<workspaceId>         → live spend updates             │
│                                                                              │
│  Settings form (editable):                                                   │
│    ┌───────────────────────────────────────────────────────────┐             │
│    │  Concurrency                                              │             │
│    │    Max concurrent tasks: [ 4 ▼ ]                         │             │
│    │    (How many tasks can run in parallel across all jobs)   │             │
│    │                                                           │             │
│    │  Spend Limits                                             │             │
│    │    Maximum spend:  [ $50.00 ] per [ monthly ▼ ]          │             │
│    │    Current spend:  $12.34 / $50.00  ████████░░░░  24.7%  │             │
│    │                                                           │             │
│    │    Max input tokens:  [ 10,000,000 ] (optional)          │             │
│    │    Max output tokens: [ 2,000,000  ] (optional)          │             │
│    │                                                           │             │
│    │  Worker Provisioning                                      │             │
│    │    Allow cloud workers: [✓]                              │             │
│    │    Max cloud workers: [ 8 ▼ ]                            │             │
│    │    Idle timeout: [ 5 ] minutes                           │             │
│    │    (Cloud workers terminate after this much idle time)   │             │
│    │                                                           │             │
│    │                              [ Save Settings ]           │             │
│    └───────────────────────────────────────────────────────────┘             │
│                                                                              │
│  Spend breakdown (below the form):                                          │
│    Table of per-job spend: job name, type, $, input tokens, output tokens   │
│    Bar chart: spend over time within the current billing period              │
│                                                                              │
│  On save:                                                                    │
│    PUT /api/v1/workspaces/:workspaceId/settings                                 │
│    → Changes take effect immediately (scheduler reads on every tick)        │
│    → If max_spend_usd is lowered below current spend, running tasks         │
│      finish but no new tasks are scheduled until the next period            │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Compile / Uplift Job Orchestration Flow

```
   UI (user clicks              g5d (editor)                Workers
   "Compile" on node)           ────────────                ───────
      │                              │                         │
      │ POST /v1/workspaces/:workspaceId/jobs │                    │
      │ { jobType: 'compile',        │                         │
      │   targetNodes: ['ht_agg'] }  │                         │
      │─────────────────────────────►│                         │
      │                              │                         │
      │                              │── Resolve dependency    │
      │                              │   graph from ontology   │
      │                              │   node_dependencies     │
      │                              │                         │
      │                              │── Create tasks:         │
      │                              │   T1: join_ht (leaf)    │
      │                              │   T2: agg_ht (leaf)     │
      │                              │   T3: radix (leaf)      │
      │                              │   T4: perfect_ht (leaf) │
      │                              │   T5: ht_agg            │
      │                              │       depends_on:       │
      │                              │       [T1,T2,T3,T4]     │
      │                              │                         │
      │◄─ { jobId, tasks: [...] } ──│                         │
      │                              │                         │
      │   WS subscribe job:<id>      │                         │
      │─────────────────────────────►│                         │
      │                              │                         │
      │                              │── Scheduler: T1-T4 are  │
      │                              │   ready (no deps)       │
      │                              │                         │
      │                              │── Assign T1 → Worker A ─►│ Worker A
      │                              │── Assign T2 → Worker B ─►│ Worker B
      │                              │── Assign T3 → Worker A ─►│ (queued)
      │                              │── Assign T4 → (no idle)  │
      │                              │                         │
      │                              │── Provision cloud worker │
      │                              │   (GKE pod spin-up)     │
      │                              │                 ~30s     │
      │                              │── Assign T4 → Worker C ─►│ Worker C
      │                              │                         │
      │   WS task.status T1=active   │                         │
      │◄─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│                         │
      │   WS task.timeline (stream)  │◄──── T1 complete ───────│
      │◄─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│                         │
      │                              │                         │
      │                              │── T1 done, T3 now runs  │
      │                              │   on freed Worker A     │
      │                              │      ... T2,T3,T4 done  │
      │                              │                         │
      │                              │── All deps of T5 met    │
      │                              │── Assign T5 → Worker A ─►│
      │                              │                         │
      │   WS task.status T5=active   │                         │
      │◄─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│                         │
      │                              │◄──── T5 complete ───────│
      │                              │                         │
      │   WS job.status = completed  │                         │
      │◄─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│                         │
      │                              │                         │
      │                              │── Idle workers > 5min:  │
      │                              │   terminate Worker C    │
      │                              │   (cloud-provisioned)   │
```

### 5.3 Draft Edit → Commit Flow

```
   Browser                 g5d (editor)              Git / Postgres
   ───────                 ────────────              ──────────────
      │                        │                       │
      │── Yjs connect ────────►│                       │
      │◄─ Yjs state vector ───│                       │
      │                        │                       │
      │── User edits (Yjs) ──►│                       │
      │                        │── append delta ──────►│ (draft_updates INSERT)
      │                        │   every 2-5s           │
      │                        │                       │
      │── [Commit] button ───►│                       │
      │   expectedHeadCommit   │                       │
      │                        │── BEGIN; SELECT ─────►│ workspace_branches
      │                        │   FOR UPDATE           │ (row-level lock)
      │                        │                       │
      │                        │── check CAS: ────────►│ head_commit ==
      │                        │   expectedHeadCommit?  │ expectedHeadCommit?
      │                        │                       │
      │                        │   IF mismatch ────────►│ ROLLBACK
      │◄─ 409 DRAFT_CONFLICT ─│   (draft preserved)   │
      │   (show rebase UI)     │                       │
      │                        │                       │
      │                        │   IF match:           │
      │                        │── extract dirtied ───►│
      │                        │   files from CRDT      │
      │                        │                       │
      │                        │── git add + commit ──►│
      │                        │── git push ──────────►│ (remote branch)
      │                        │                       │
      │                        │   IF push fails ─────►│ ROLLBACK
      │◄─ 409 DRAFT_CONFLICT ─│   (draft preserved)   │
      │                        │                       │
      │                        │   IF push OK:         │
      │                        │── UPDATE head_commit ►│ workspace_branches
      │                        │── DELETE draft ───────►│ draft_sessions +
      │                        │                       │ draft_updates
      │                        │── COMMIT ────────────►│
      │                        │                       │
      │◄─ draft.commit event ─│                       │
      │   (via WS)             │                       │
```

### 5.4 Cross-Branch Comment Flow

```
   Alice (task/auth-fix)       g5d                    Bob (main)
   ─────────────────────       ───                    ──────────
      │                         │                        │
      │── POST /comments ─────►│                        │
      │   scope: 'line'        │                        │
      │   filePath: auth.rs    │                        │
      │   line: 42             │                        │
      │                         │                        │
      │                         │── INSERT comment_index─│
      │                         │── git notes add ──────│
      │                         │                        │
      │                         │── WS comment.new ─────►│
      │                         │   (Bob is subscribed   │
      │                         │    to workspace events)  │
      │                         │                        │
      │                         │          Bob sees badge:│
      │                         │          💬 1 on 1 branch
      │                         │                        │
      │                         │          Bob toggles     │
      │                         │          cross-branch   │
      │                         │          comment view   │
      │                         │                        │
      │                         │◄─ GET /comments/ ─────│
      │                         │   cross-branch         │
      │                         │   ?filePath=auth.rs    │
      │                         │                        │
      │                         │── compute line mapping─│
      │                         │   (git diff mapping)   │
      │                         │                        │
      │                         │── response with ──────►│
      │                         │   mappedLine = 44      │
      │                         │   (auth.rs shifted)    │
```

---

## 6. Reconciliation: Mock Types → Schema

This section maps every TypeScript interface from the mock to its Postgres
table and API endpoint, noting divergences.

| Mock Type | Postgres Table(s) | Key changes from mock |
|-----------|-------------------|----------------------|
| `Workspace` | `workspaces` + `workspace_branches` + `workspace_dependencies` | Scoped to `org_id` directly (no separate workspace entity). Each repo is a `workspace_branch` with `head_commit` (pinned SHA), `tracking_mode`, and optional `draft_session`. `dependencies` is a join table. `divergence`/`cost`/etc. computed at query time from `jobs`+`tasks`+git. |
| `OntologyNode` | `ontology_nodes` | `content` NOT stored in Postgres (lives in git). Added `content_hash`, `sync_status`, `sort_order`. `children` derived from `parent_id` FK. |
| `DocNode` | `docs` + `doc_node_links` | `linkedTo[]` becomes join table. `content` lives in git via `file_path`. Staleness columns for tracking. Old `FeatureNode` and `PlanNode` collapsed into `DocNode`. |
| `ExecutionTask` | `tasks` + `jobs` | `workspaceName` derived via JOIN. `job` type comes from parent `jobs.job_type`. Added `branch_name`, `base_commit`, `depends_on`. |
| `DiffData` / `DiffHunk` | Not stored — computed at query time | Diff is computed from git (`git diff base..branch -- <path>`). The API endpoint `/ontology/:nodeId/diff` runs this on demand. |
| `Annotation` | `comment_index` + `comment_replies` | Mock's `author.initial`/`color` derived from user profile. `resolved` is a boolean flag. `replies` are separate table rows. |
| `Job` (WorkspacePage) | `jobs` | Mock had `tasks: Task[]` inline; DB normalizes to `tasks` table with FK. `cost` is `cost_usd`. |
| `Task` (WorkspacePage) | `tasks` + `timeline_entries` | `timeline: TimelineEntry[]` is separate table. `files: string[]` tracked via git diff, not stored. |
| `mockChatMessages` | `chat_messages` + `chat_sessions` | Messages are always scoped to a session. Sessions have lifecycle tracking, fork/resume. |
| Dependency / Dependent | `node_dependencies` | Bidirectional: query `from_node_id` for deps, `to_node_id` for dependents. `dep_type` preserves 'direct'/'inherited'. |
| `activeDiff` (ExecutionPage) | Not stored — computed | Live diff from task's git worktree. Served via `GET /api/v1/tasks/:id/diff`. |
| `executionLogs` (ExecutionPage) | `timeline_entries` | Enriched with `metadata` JSONB, proper timestamps, entry types. |

---

## 7. Query Patterns & Indexes

### 7.1 Hot Queries

These are the queries that run on every page load or WebSocket reconnect:

```sql
-- WorkspacesPage: list all workspaces with stats
-- (stats computed via lateral join or application-level aggregation)
SELECT w.*,
       COALESCE(j.cost_sum, 0) AS cost,
       COALESCE(t.total, 0) AS total_tasks,
       COALESCE(t.completed, 0) AS tasks_completed,
       COALESCE(t.active, 0) AS concurrent_tasks
FROM workspaces w
LEFT JOIN LATERAL (
    SELECT SUM(cost_usd) AS cost_sum
    FROM jobs WHERE workspace_id = w.id
) j ON TRUE
LEFT JOIN LATERAL (
    SELECT COUNT(*) AS total,
           COUNT(*) FILTER (WHERE status = 'completed') AS completed,
           COUNT(*) FILTER (WHERE status = 'running') AS active
    FROM tasks t2
    JOIN jobs j2 ON t2.job_id = j2.id
    WHERE j2.workspace_id = w.id
) t ON TRUE
WHERE w.org_id = $1
ORDER BY w.updated_at DESC;

-- OntologyPage: full forest for a workspace (all repos)
-- (single query, application builds tree from flat rows; repo context via JOIN)
SELECT n.id, n.parent_id, n.name, n.node_type, n.file_path, n.content_hash,
       n.sort_order, n.sync_status, n.ahead_of_base, n.behind_base,
       pb.repo_id, r.name AS repo_name, pb.id AS workspace_branch_id
FROM ontology_nodes n
JOIN workspace_branches pb ON n.workspace_branch_id = pb.id
JOIN repos r ON pb.repo_id = r.id
WHERE pb.workspace_id = $1
ORDER BY r.name, n.parent_id NULLS FIRST, n.sort_order;

-- TaskQueue: tasks grouped by job for a workspace
SELECT t.*, j.name AS job_name, j.job_type, j.status AS job_status
FROM tasks t
JOIN jobs j ON t.job_id = j.id
WHERE j.workspace_id = $1
ORDER BY j.created_at, t.created_at;

-- Session history sidebar
SELECT id, session_type, title, summary, status,
       message_count, total_cost_usd, branch,
       linked_commits, linked_nodes, updated_at
FROM chat_sessions
WHERE workspace_id = $1
ORDER BY
    CASE WHEN status = 'active' THEN 0 ELSE 1 END,
    updated_at DESC
LIMIT 50;

-- Cross-branch comment count for a file
SELECT origin_branch, COUNT(*) AS comment_count
FROM comment_index
WHERE workspace_id = $1
  AND file_path = $2
  AND scope = 'line'
  AND origin_branch <> $3  -- exclude current branch
  AND branch_deleted = FALSE
GROUP BY origin_branch;
```

### 7.2 Index Summary

| Table | Index | Purpose |
|-------|-------|---------|
| `workspaces` | `(org_id, updated_at DESC)` | WorkspacesPage listing |
| `workspace_branches` | `(workspace_id, repo_id)` UNIQUE | Branch lookup |
| `ontology_nodes` | `(workspace_branch_id, parent_id)` | Tree reconstruction |
| `ontology_nodes` | `(workspace_branch_id)` | All nodes for a workspace branch |
| `node_dependencies` | `(to_node_id)` | Reverse dependency lookup |
| `jobs` | `(workspace_id, status)` | Job listing + filtering |
| `jobs` | `(status, priority DESC, created_at)` WHERE pending/running | Scheduler job ordering |
| `jobs` | `(depends_on_job)` WHERE status = 'pending' | Scheduler: advance pending jobs when predecessor completes |
| `tasks` | `(job_id, status)` | Task listing within job |
| `tasks` | `(job_id, status)` WHERE pending | Scheduler: find ready tasks |
| `workspace_settings` | `(workspace_id)` PK | Settings lookup (1:1 with workspaces) |
| `timeline_entries` | `(task_id, created_at DESC)` | Paginated timeline |
| `task_checkpoints` | `(task_id, created_at DESC)` | Checkpoint history |
| `chat_sessions` | `(workspace_id, status, updated_at DESC)` | Session sidebar |
| `chat_sessions` | `GIN(tsvector(title, summary))` | Full-text search |
| `chat_messages` | `(session_id, message_index)` | Message ordering |
| `chat_messages` | `GIN(tsvector(content))` | Full-text search |
| `comment_index` | `(workspace_id, file_path)` WHERE line | Cross-branch line comments |
| `comment_index` | `(node_id)` WHERE node | Node comments |
| `comment_index` | `(workspace_id, origin_branch)` | Branch comment listing |
| `comment_replies` | `(comment_id, created_at)` | Reply threading |
| `draft_sessions` | `(workspace_branch_id)` UNIQUE | Draft lookup (one per repo branch) |
| `draft_participants` | `(session_id)` | Participant listing |

---

## 8. Migration from Mock Data

To connect the React prototype to the real backend, the mock data file
(`client/src/data/mock.ts`) should be replaced with API client hooks:

```typescript
// Example: replace static `workspaces` array with SWR/React Query hook
// Before (mock.ts):
export const workspaces: Workspace[] = [
  { id: 'base', name: 'Base', status: 'Active', ... },
  ...
];

// After (hooks/useWorkspaces.ts):
export function useWorkspaces(orgId: string) {
  return useQuery({
    queryKey: ['workspaces', orgId],
    queryFn: () => api.get<WorkspaceListItem[]>(`/orgs/${orgId}/workspaces`),
  });
}
```

**Recommended migration order:**

1. **Workspaces** — simplest, powers WorkspacesPage
2. **Ontology tree** — powers OntologyPage sidebar
3. **Node content** — git-backed, requires editor checkout
4. **Comments** — requires comment_index table
5. **Jobs & Tasks** — powers TaskQueue, ExecutionPage
6. **Chat sessions** — requires WebSocket streaming
7. **Draft sessions** — requires Yjs WebSocket endpoint
8. **Diffs** — computed from git, requires git service integration

---

## 9. Open Questions

| # | Question | Impact | Current assumption |
|---|----------|--------|--------------------|
| 1 | Should ontology node content be cached in Postgres or always read from git? | Latency vs. freshness | Always from git; Postgres stores only metadata + content_hash. Trade-off: extra git read per node view. |
| 2 | How large can a Yjs state vector get, and should we shard `draft_sessions.yjs_state`? | Storage | Expect 10KB–10MB per draft. Single BYTEA column is fine up to ~100MB. Can move to object storage (GCS) for very large drafts. |
| 3 | ~~Should the full-text search index cover node content (git-backed) or only metadata?~~ **SETTLED** | Search scope | Yes — search covers content. Formalized in §3.10: `GET /api/v1/workspaces/:workspaceId/search?q=...` uses Postgres hybrid search: `tsvector` GIN indexes for keyword/FTS + `pgvector` HNSW indexes for semantic similarity. Content is pre-indexed into `search_documents` (schema §2.9) on draft commits and job completions. No external search service required. Results include type, breadcrumb, snippet with `<mark>` tags, and relevance score. |
| 4 | ~~Rate limiting strategy for LLM-backed chat sessions?~~ **SETTLED** | Cost control | Spend limits are configured per-workspace in `workspace_settings` (§2.11): `max_spend_usd`, `max_input_tokens`, `max_output_tokens` with configurable period (hourly/daily/weekly/monthly/total). Enforced by the scheduler (§3.4.3 step 1) — when a limit is hit, no new tasks are scheduled and jobs are paused with reason `spend_limit_reached`. Users manage this on the SettingsPage. Chat sessions share the workspace's token/dollar budget. |
| 5 | How do we garbage-collect stale `draft_sessions` for deleted branches? | Data hygiene | Trigger on branch deletion event. Mark `branch_deleted = TRUE`, GC after TTL (90 days). |
| 6 | Should `timeline_entries` be partitioned by time for large-scale deployments? | Performance | Not initially. If timeline tables exceed 100M rows, partition by `created_at` range (monthly). |
| 7 | ~~What should `max_workers` (cloud pool ceiling) default to per workspace/org?~~ **SETTLED** | Cost/performance | Configurable per-workspace in `workspace_settings` (§2.11): `max_cloud_workers` defaults to 8, `worker_idle_timeout_min` defaults to 5. Exposed on the SettingsPage. |
| 8 | Should compile and uplift jobs for the same node be mutually exclusive? | Correctness | Yes — server should reject a compile if an uplift is running on the same subtree, and vice versa. Enforce via row-level lock on `target_nodes` overlap check. |
| 9 | ~~How should the scheduler handle partial failure in a compile DAG?~~ **SETTLED** | UX | Mark direct dependents as 'blocked'. User attaches to the failed task's worker in draft state, fixes the issue, then marks the task as completed via `POST /api/v1/tasks/:id/complete`. Blocked dependents become 'pending' and the scheduler picks them up. No automatic retry, no skip — the human is the recovery mechanism. |
| 10 | ~~Should cloud workers be pre-warmed (min_workers > 0) or fully on-demand?~~ **SETTLED** | Latency vs. cost | Fully on-demand. Workers poll for tasks and self-terminate after `worker_idle_timeout_min` of idleness. No pre-warming. If latency is a concern, lower the idle timeout so workers stick around longer between bursts. |
