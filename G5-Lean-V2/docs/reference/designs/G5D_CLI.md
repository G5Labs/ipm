# G5D CLI and Workspace Operations

This document defines the g5d command-line interface, workspace lifecycle, and
the point at which g5d begins managing Yjs sync. It assumes g5d is installed on
hosts that are already suitable for executing remote jobs from g5-api
(virtualization, storage, and toolchains are pre-provisioned).

## Scope

- g5d is the local daemon that materializes workspaces, manages checkouts, and
  serves the editor/worker APIs.
- The CLI focuses on workspace lifecycle commands: `init`, `clone`, `refresh`,
  `add`, plus runtime commands to start the daemon.
- Yjs sync is owned by g5d (editor role) and persisted through g5-api.

## Execution and Isolation Defaults

- Control flow is **browser → g5-api → g5d (editor/worker)**. g5d does not
  accept arbitrary inbound commands; it connects outbound to g5-api and only
  executes authorized jobs.
- **MicroVM isolation is the default** for worker tasks when supported. Process
  isolation is an explicit opt-out (or a fallback when microVM is unavailable).
- The default worker image is `g5/worker:base` unless overridden per workspace.
- For quick local experimentation, set `isolation.mode: process` to skip VM
  startup at the cost of weaker isolation.

## Workspace Files

- Workspace config (local): `.g5/workspace.yaml`
- Per-repo config: `g5.yaml` at the repo root

### Workspace Config (local)

```yaml
workspace_id: "ws_123"
remote:
  enabled: true
  api_url: "https://api.g5.example"
  project_id: "proj_456"
clone:
  method: "cow"        # cow | full
  base_dir: "~/.g5/cache/repos"
fork:
  strategy: "fork"     # fork | mirror | none
  upstream: "origin"
isolation:
  mode: "microvm"      # microvm | process
  fallback: "process"  # optional; used if microvm unavailable
  base_image: "g5/worker:base"
  resources:
    cpus: 4
    memory_gb: 16
cache:
  mode: "local"        # local | gcs
  local_dir: "~/.g5/cache"
  gcs:
    bucket: "g5-build-cache"
    prefix: "tenants/acme/workspaces/ws_123"
jobs:
  accept_remote: true
docs:
  repo_url: "git@github.com:acme/g5-docs.git"
  branch: "main"                 # tracks base workspace by default
  tracking_mode: "pinned"         # pinned | tracking
  path: ".g5/docs"               # independent docs checkout path (convention)
repos:
  - name: "g5-api"
    url: "git@github.com:acme/g5-api.git"
    branch: "main"
    path: "repos/g5-api"
  - name: "g5d"
    url: "git@github.com:acme/g5d.git"
    branch: "main"
    path: "repos/g5d"
```

Notes:
- `clone.method: cow` uses APFS clones on macOS and reflinks on Linux (XFS/Btrfs
  where available). Falls back to full clone if COW is unsupported.
- `remote.enabled` governs whether this workspace can receive remote jobs.
- `isolation.mode` selects microvm vs process isolation for worker tasks.
  MicroVM is the preferred default; process mode is an explicit opt-out.
- `isolation.base_image` defaults to `g5/worker:base` when not specified.

### Per-Repo `g5.yaml`

```yaml
name: "g5-api"
language: ["rust"]
setup:
  bootstrap:
    - "cargo build -p g5-api"
  env:
    RUST_LOG: "info"
oci:
  image: "ghcr.io/acme/g5-api:dev"
  build:
    context: "."
    dockerfile: "Dockerfile"
    target: "dev"
  push:
    registry: "ghcr.io"
    repository: "acme/g5-api"
  remote_host:
    url: "ghcr.io"
    auth_profile: "ghcr"
runtime:
  kind: "compose"      # native | docker | compose | custom
  command: "docker compose up"
  env_file: ".env"
tooling:
  build: "cargo build -p g5-api"
  test: "cargo test -p g5-api"
paths:
  workdir: "."
  ignores:
    - "target/"
open_in_ide:
  recommended: ["rust-analyzer"]
  workspace_root: "."
```

Notes:
- `g5.yaml` is **not** an LSP config; it provides setup and IDE hints only.
- Language servers use the repo's native build files (Cargo.toml, go.mod, etc.).
- The `oci` block is used by g5d to build/push repo-specific container images
  when a remote container host or registry is part of the workflow.
- The `runtime` block documents how to run the project (docker, compose, or
  native). g5d can surface this for "run" or "open in IDE" flows.
- Agents should update `g5.yaml` when they change setup steps, dependencies, or
  runtime assumptions so future tasks inherit the correct environment.

## CLI Commands

All workspace lifecycle commands (`init`, `clone`, `fork`, `refresh`, `add`) are
**backed by g5-api**. g5d calls the API to read/write the workspace manifest
and records the change in the workspace meta repo.

### `g5d init [path]`

Creates an empty workspace with a new local config:
- Generates a `workspace_id` (local UUID).
- Writes `.g5/workspace.yaml` with `remote.enabled: false` by default.
- Initializes the independent docs repository at `.g5/docs/` if `docs.repo_url` is
  configured (or inherited from the org's base workspace via g5-api).
- Does not clone any repos.

### `g5d clone <workspace-id> [path]`

Materializes a workspace from g5-api **without local configuration**:
- Pulls the workspace manifest from g5-api (repos, branches, base image, docs config).
- Creates `.g5/workspace.yaml` from the manifest.
- Clones repos using COW where possible (APFS/XFS reflinks).
- Initializes and checks out the independent docs repository at `.g5/docs/` at the
  workspace's `docs_branch_name`.
- Does not require pre-existing local config.

### `g5d fork [path] [--name <name>]`

Forks the current workspace in g5-api and materializes it locally:
- Creates a new workspace manifest in g5-api based on the current workspace.
- Uses the workspace meta repo commit map to pin repo SHAs.
- Creates a new branch in the docs repo (or keeps tracking mode from source).
- Writes a new `.g5/workspace.yaml` with a new `workspace_id`.
- Clones repos using COW where possible.

### `g5d refresh [path]`

Updates an existing workspace from g5-api:
- Fetches the latest manifest.
- Adds any new repos and updates repo remotes/branches.
- Optionally pulls updated base images (if configured).
- Does not delete local repos by default (safe-additive behavior).

### `g5d add <repo-url> [--name] [--branch] [--path]`

Adds a new repo to the current workspace:
- Clones repo into `repos/<name>` (COW if possible).
- Updates `.g5/workspace.yaml`.
- If `remote.enabled: true`, pushes the change to g5-api.

### `g5d commit -m "<message>" [--all]`

Commits the current CRDT draft to git for **all modified repos**, including the
independent docs repo, and the workspace meta repo:
- Ensures the Yjs draft is fully materialized to disk (repos + docs).
- Creates a git commit in each repo with changes.
- Creates a git commit in the docs repo if docs were modified.
- Does **not** touch `workspace.lock`; publishing is a separate Git step (see "Publishing" below).

This is the workspace-level equivalent of "save draft → git".

### Publishing (native `git push`; `g5d push` is retired)

A workspace member is published with plain Git against the API's smart-HTTP
mirror in two phases, followed by explicit projection refreshes (the receive
endpoint only moves refs and emits branch events; it queues no spec/docs
sync):

1. **Plan** every member before the first push: read the mirror head, check
   that the staged commit builds on it, and pin both SHAs. One invalid member
   publishes nothing.
2. **Apply** each pinned row: push the pinned staged SHA (not a re-read
   `HEAD`) leased to the pinned mirror head.

```
set -euo pipefail                                        # or chain every step with &&
auth="http.$API/.extraHeader=Authorization: Bearer $TOKEN" # the token goes to the API only
# no url.<base>.insteadOf / pushInsteadOf may apply to $API/git/ (the helper refuses them)
! git -C "$dir" config --get-regexp '^url\..*\.(insteadof|pushinsteadof)$' | grep -q " ${API}/git/" || exit 1
old=$(git -C "$dir" -c "$auth" ls-remote "$API/git/$REPO" "refs/heads/$B" | cut -f1)
[ -n "$old" ]                                             # the branch must already exist
new=$(git -C "$dir" rev-parse HEAD)
git -C "$dir" merge-base --is-ancestor "$old" "$new" || exit 1   # plan: fast-forward only
git -C "$dir" -c "$auth" push --no-follow-tags --recurse-submodules=no \
  --force-with-lease="refs/heads/$B:$old" "$API/git/$REPO" "$new:refs/heads/$B"
source scripts/lib/native-publish.sh                      # projections: validated summaries
native_publish_configure "$API" "$TOKEN"
native_publish_sync_projections "$WS"                     # specs of every manifest code member, then docs
```

The read and the push run in the member's directory under the same Git
configuration, which must not rewrite the API URL (a rewrite could send the
push, and the token, elsewhere). The token is scoped to the API origin. The
push names one refspec and disables `push.followTags` and submodule recursion
so inherited Git configuration cannot widen the update. The projection helper
fails on any docs error except the tolerated materialized node-link warning;
a bare `curl -fsS` would not, because both endpoints answer HTTP 200 with an
`errors` array. `scripts/lib/native-publish.sh` does all of this for the seed
scripts; prefer it to the plain form.

The mirror only updates existing `refs/heads/*` (branch creation belongs to
the fork service), refuses the obsolete `X-G5-Force-Push` header, and
compare-and-swaps every update against the head it advertised. It does not
refuse an authorized workspace rewrite whose `old` matches, which is why the
plan phase checks ancestry client-side. Protected branches (a repo's default
and base branches) and base workspaces are never writable through the mirror;
fixtures install those cuts through the repo volume instead. `sync-specs` runs
once per code member of the workspace (its manifest lists them under `repos`);
both syncs return a summary object (`created`/`updated`/`unchanged`/... and an
`errors` array) that the caller validates. `scripts/lib/native-publish.sh`
packages this recipe for the seed scripts (`native_publish_plan`,
`native_publish_apply`, `native_publish_sync_projections`).

### `g5d revert [--all]`

Reverts the current CRDT draft and working tree back to git `HEAD`:
- Discards uncommitted changes across repos (hard reset to HEAD).
- Resets the workspace draft to match git state.
- Leaves `workspace.lock` unchanged (it tracks pushed commits).

### `g5d serve [path]`

Starts the g5d editor role for a workspace:
- Loads `.g5/workspace.yaml`.
- Registers with g5-api (if remote enabled).
- Starts the local REST/WS server for browser + CLI.
- Spawns worker processes or microVM workers as needed.

### `g5d worker --task <id> [--workspace <path>]`

Starts a worker process bound to a task:
- Uses `isolation.mode` from workspace config.
- In `microvm` mode, boots a microVM with `base_image` and mounts the workspace.
- Executes the agent loop and tool calls for the task.

## Copy-on-Write Clone Strategy

1. g5d maintains a bare or cached clone per repo in `clone.base_dir`.
2. Workspace clones are created using:
   - **macOS:** APFS `clonefile` for fast snapshots.
   - **Linux:** XFS/Btrfs reflinks where supported.
3. If COW is not available, fall back to full clone.

This keeps workspace creation fast while preserving isolation between workspaces.

## Workspace Meta Repo (Workspace Git)

Each workspace has a **small git repository** that versions workspace state
independent of the project repos. It stores:

- `workspace.yaml` (authoritative manifest)
- `workspace.lock` (repo → commit SHA pin, including the docs repo)
- Optional metadata (created_at, fork lineage, tags)

Location: `.g5/workspace-meta/` (non-bare, human-readable history).

On any workspace operation (`init`, `clone`, `fork`, `add`, `refresh`), g5d
updates the meta repo and commits a new revision. This gives a full audit trail
of workspace composition and makes it easy to reconstruct a workspace at a
specific point in time.

Example `workspace.lock`:

```yaml
repos:
  g5-api:
    branch: "main"
    commit: "3c9a2d71c0f5b1d2dbe2a8a3f8e1d61f6a8e90a1"
  g5d:
    branch: "main"
    commit: "9f4c7e3b52a1b4e9f9b76a7dcb2e2a4e98a53d11"
docs:
  branch: "main"
  commit: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0"
```

## Yjs Sync Lifecycle

g5d begins managing Yjs when the editor role is started for a workspace.

**Start (on `g5d serve`):**
1. Load workspace config and ensure `workspace_id` is present.
2. Create or attach to the workspace Yjs document.
3. Materialize the Yjs draft to disk (if needed).
4. Begin sync with g5-api for persistence and multi-client access.

**During runtime:**
- Browser/CLI/worker edits arrive at g5d (editor), which is the Yjs authority.
- g5d persists deltas to g5-api and applies file materialization between tool
  calls or at safe points.
- If the editor is offline, Yjs persists locally and syncs on reconnect.

**Stop (on shutdown):**
- Flush outstanding deltas to g5-api.
- Close Yjs doc and release workspace locks.

## Remote Execution Behavior

When `remote.enabled: true`, g5d accepts jobs from g5-api for this workspace:
- g5d does not expose a public port by default; it connects outbound to g5-api.
- Jobs are validated against `workspace_id` and scope before execution.

This model ensures g5d is only driven by g5-api and does not accept arbitrary
remote callers.

## Build Cache Guidance (Local + GCS)

g5d maintains a workspace-scoped build cache with two tiers:
- **L1 local disk** for hot caches.
- **L2 GCS** for shared, content-addressed caches (optional).

Recommended configuration:
- Keep `cache.mode: local` for single-user/local workspaces.
- Enable `cache.mode: gcs` for remote execution or shared CI to improve cache
  hit rates across machines.

Repository setup should use standard cache env vars (examples):
- Rust: `SCCACHE_DIR=/cache/sccache`, `RUSTC_WRAPPER=sccache`
- Go: `GOCACHE=/cache/go-build`
- Node: `PNPM_STORE_DIR=/cache/pnpm-store`

For full cache behavior and mounting details, see
`/Users/lumost/workspace/g5/WORKER_INFRASTRUCTURE.md`.

## Rust CLI Skeleton Requirements (Draft)

### Command Structure

- `g5d init [path]`
- `g5d clone <workspace-id> [path]`
- `g5d fork [path] [--name <name>]`
- `g5d refresh [path]`
- `g5d add <repo-url> [--name] [--branch] [--path]`
- `g5d commit -m "<message>" [--all]`
- `g5d revert [--all]`
- `g5d serve [path] [--port]`
- `g5d worker --task <id> [--workspace <path>]`
- `g5d doctor` (optional diagnostics: COW support, microVM readiness)

### Config Loading

1. Explicit `--workspace <path>` (if provided)
2. `cwd`
3. `.g5/workspace.yaml` within the workspace
4. Environment overrides (e.g., `G5D_API_URL`, `G5D_ISOLATION_MODE`)

### Required Features (MVP)

- YAML config read/write for `.g5/workspace.yaml` and `g5.yaml`
- g5-api auth + manifest fetch for `clone` and `refresh`
- g5-api fork endpoint + workspace meta repo commit map for `fork`
- COW clone detection with fallback to full clone
- Isolation selection: microvm (default) or process (config opt-out)
- Base image default to `g5/worker:base`
- Workspace ID generation and persistence
- Workspace meta repo management (`workspace.yaml` + `workspace.lock`)
- CRDT draft commit across all modified repos + workspace meta repo
- Push ordering: repos first, then workspace lock + g5-api update
- Basic logging (JSON or structured text), exit codes per command

### Suggested Rust Modules

- `cmd::{init, clone, refresh, add, serve, worker}`
- `config::{workspace, repo}`
- `git::{clone, fetch, cow_clone}`
- `isolation::{microvm, process}`
- `api::{auth, manifest, jobs}`
- `yjs::{attach, materialize, flush}`
- `workspace_meta::{init, commit, lockfile}`

### Error Handling + Exit Codes

- `1` generic error, `2` config error, `3` network/auth error, `4` repo error,
  `5` isolation/microVM error.

### Minimal Clap Skeleton (Draft)

```rust
#[derive(clap::Parser)]
#[command(name = "g5d")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    Init { path: Option<String> },
    Clone { workspace_id: String, path: Option<String> },
    Fork { path: Option<String>, name: Option<String> },
    Refresh { path: Option<String> },
    Add { repo_url: String, name: Option<String>, branch: Option<String>, path: Option<String> },
    Commit { message: String, all: bool },
    Push { all: bool },
    Revert { all: bool },
    Serve { path: Option<String>, port: Option<u16> },
    Worker { task: String, workspace: Option<String> },
    Doctor,
}
```
