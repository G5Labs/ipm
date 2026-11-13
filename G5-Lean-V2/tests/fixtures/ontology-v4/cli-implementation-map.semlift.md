---
component_id: software_architecture/g5d/cli/implementation_map
title: g5d CLI Implementation Map and References
type: file
version: 0.1.0
language: multi
complexity: medium
parent_spec_id: software_architecture/g5d/cli
child_spec_ids: []
dependencies:
- software_architecture/g5d/cli/local_workspace_commands
- software_architecture/g5d/cli/sync_spec_remote
- software_architecture/g5d/cli/external_agent_tools
- software_architecture/g5d/cli/doctor_and_sidecar
source_files: []
---


# g5d CLI Implementation Map and References

*Maintenance note (2026-07-03):* this map was thinned from an embedded clap-definition copy to ownership pointers plus the checklist oracle below; the command surfaces are owned by the sibling command leaves named in D24. An unlabeled, sentence-fragmented block of daemon/sidecar notes that had been appended after I8 (a merge/paste artifact — numbered lists with missing items, mid-sentence truncation, no I/D/Q labels) was removed; the intact versions of those contracts live with the daemon/sidecar owners (`software_architecture/g5d/cli/doctor_and_sidecar`, `software_architecture/g5d/worker` claude/codex leaves).

## Dependencies
[D18. Dependencies — Why: documents the module graph for build ordering and interface contracts]

### Internal Dependencies
[D19. Internal Dependencies — Why: internal crate boundaries define compilation units and API surfaces]
- **g5-common** — see `software_architecture/g5-common#D1` , `software_architecture/g5-common#D2` , and `software_architecture/g5-common#D7` — shared types (`G5Error`, `G5Uuid`) and config structures; code/status semantics stay in @ref wire_contract/error_taxonomy#D1 and @ref wire_contract/error_taxonomy#D2 .
- **g5-common/error** — see `software_architecture/g5-common/error#D1` and `software_architecture/g5-common/error#D3` — CLI-visible `G5Error` variants; process exit classification is owned by `software_architecture/g5d/main#D10` .
- **g5-common/types** — see `software_architecture/g5-common/types#D1` — Shared DTOs (`WorkspaceManifest`, `RepoConfig`, `WorkspaceLock`) used for g5-api communication and config serialization.
- **g5-common/git** — see `software_architecture/g5-common/git#D1` and `software_architecture/g5-common/git#D2` — clone/fetch/checkout/commit/push/worktree/branch/COW operations used by `clone`, `fork`, `add`, `commit`, `push`, `revert`.
- **g5-common/auth** — see `software_architecture/g5-common/auth#I5` — OAuth token management for g5-api authentication. Used by `clone`, `fork`, `refresh`, `push` handlers that communicate with g5-api.
- **software_architecture/g5d/config** — see `software_architecture/g5d/config#D11` , `software_architecture/g5d/config#D12` , and `software_architecture/g5d/config#D14` — `G5DConfig`, `WorkspaceConfig`, `RepoConfig` for loading and writing workspace configuration. All handlers use the config module for reading `.g5/workspace.yaml`.

### External Dependencies
[D20. External Dependencies — Why: external crates chosen for async compatibility and ecosystem maturity]
- **clap** (derive API): CLI argument parsing — `Parser`, `Subcommand`, `Args`, `ValueEnum` derive macros. Provides help text, argument validation, shell completions.
- **tokio**: Async runtime — `#[tokio::main]` for the entry point, async operations in handlers that call g5-api or perform git operations.
- **reqwest**: HTTP client for g5-api communication — workspace manifest fetch, fork creation, lock updates, auth token exchange.
- **serde / serde_json / serde_yaml**: Serialization for `workspace.yaml` (YAML), g5-api payloads (JSON), and `--json` output formatting.
- **git2 (libgit2)**: Git operations — clone, fetch, commit, push, worktree, branch. Delegates to g5-common/git wrappers.
- **uuid (v7)**: Workspace ID and worker ID generation.
- **tracing**: Structured logging — verbosity controlled by `-v` flags, JSON output in `--json` mode.
- **tracing-subscriber**: Log subscriber configuration — sets up filters based on verbosity level.
- **colored** (or **owo-colors**): Terminal colors for human-readable output (✓/✗ indicators, status messages).
- **dirs**: Platform-specific directory resolution (home directory for `~/.g5/cache`).
- **ctrlc** (or tokio::signal): Signal handling for SIGTERM/SIGINT in the `worker` command.

## Domain Context
[I1. Domain Context]

### CLI as Thin Dispatch Layer
[I2. CLI as Thin Dispatch Layer]

The CLI module is intentionally thin. Each subcommand handler is responsible for:
1. Parsing and validating arguments (clap handles most of this)
2. Loading configuration (delegated to `config` module)
3. Calling the appropriate business logic (g5-api client, git operations, worker orchestrator)
4. Formatting and printing output

The CLI does NOT contain business logic itself. For example, `worker.rs` does not implement the agent — it constructs a `WorkerConfig` and calls `WorkerOrchestrator::run()`. Similarly, `clone.rs` does not implement git cloning — it calls `g5-common/git::clone_with_cow()`.

This separation ensures that:
- The CLI can be tested by constructing configs directly (no need to parse CLI strings)
- The worker can be embedded in other contexts (e.g., integration tests) without the CLI layer
- Output formatting is centralized and consistent

### Workspace Auto-Detection Strategy
[I3. Workspace Auto-Detection Strategy]

The workspace auto-detection walks up from `cwd` looking for `.g5/workspace.yaml`. This is the same pattern used by git (`.git`), cargo (`Cargo.toml`), and npm (`package.json`). The walk stops at the filesystem root.

For commands that accept an explicit `[path]` argument (like `init`, `clone`, `serve`), the path argument takes precedence over auto-detection.

### COW Clone in CLI Context
[I4. COW Clone in CLI Context]

The `clone`, `fork`, and `add` commands all use COW (copy-on-write) cloning when available. The CLI module delegates COW detection to `g5-common/git`, which probes the filesystem:

- **macOS APFS:** Uses `clonefile(2)` syscall for instant file copies that share physical storage until modified
- **Linux XFS/Btrfs:** Uses `ioctl(FICLONE)` for reflink copies
- **Fallback:** Standard `git clone --depth=1`

COW makes workspace creation fast (<5s) even with large repos, because no data is actually copied until files are modified.

### Error Handling Philosophy
[I5. Error Handling Philosophy]

CLI error handling follows these principles:
1. **Fail fast on configuration errors** — exit immediately with clear message
2. **Partial success for multi-repo operations** — if one repo fails to clone/push, continue with others and report failures at the end
3. **Non-fatal warnings** — independent docs checkout failures, g5-api update failures after successful local operations
4. **Structured errors in JSON mode** — machine-parseable error codes and messages
5. **Human-readable errors in text mode** — colored output with suggestions for resolution

## Implementation Requirements
[D21. Implementation Requirements — Why: implementation details guide code generation from this spec]

### Technology Stack
[D22. Technology Stack — Why: technology choices align with system-wide Rust/async architecture]
- **Language:** Rust (edition 2021)
- **CLI Framework:** clap 4.x (derive API with `Parser`, `Subcommand`, `Args`)
- **Async Runtime:** tokio (multi-threaded, `#[tokio::main]`)
- **HTTP Client:** reqwest (for g5-api calls)
- **Git:** git2 (libgit2 bindings, via g5-common/git)
- **Serialization:** serde + serde_json + serde_yaml
- **Logging:** tracing + tracing-subscriber

### Module Structure
[D23. Module Structure — Why: module organization supports separation of concerns]

```
g5d/src/cli/
├── mod.rs          — Top-level Cli struct (clap Parser), Command enum
│                     (Subcommand), GlobalFlags, OutputFormat, shared
│                     CLI types, workspace auto-detection helper,
│                     output formatting utilities, exit code constants
├── init.rs         — g5d init: create empty workspace, generate
│                     workspace_id, write workspace.yaml, init meta repo
├── clone.rs        — g5d clone: fetch manifest from g5-api, clone repos
│                     with COW, initialize independent docs repo, write config
├── fork.rs         — g5d fork: fork workspace in g5-api, clone from
│                     current workspace, create new branches, write config
├── refresh.rs      — g5d refresh: fetch latest manifest, add new repos,
│                     update remotes/branches, safe-additive behavior
├── add.rs          — g5d add: clone repo with COW, update workspace
│                     config, optionally push to g5-api; --local creates
│                     a G5-managed local repo through bootstrapMode=local
├── commit.rs       — g5d commit: materialize draft, git add + commit
│                     across modified repos + docs, update meta repo
├── push.rs         — g5d push: push repos, push docs, update
│                     workspace.lock, update g5-api, update meta repo
├── remote.rs       — g5d remote: remote-tracking API adapter for status,
│                     sync, PushToRemote, push_mode, sync runs, retry, and
│                     remote-tracking merge runs
├── revert.rs       — g5d revert: discard changes, reset to HEAD,
│                     reset CRDT draft
├── worker.rs       — g5d worker: construct WorkerConfig, install signal
│                     handlers, delegate to WorkerOrchestrator::run()
└── doctor.rs       — g5d doctor: COW detection, microVM readiness,
                      git version, toolchain detection, workspace
                      validation, cache status
```

### `mod.rs` — CLI Definitions
[D24. mod.rs — CLI Definitions — Why: file-level spec ensures implementation matches the design]

`mod.rs` owns the entrypoint types and the shared CLI utilities; the command surfaces themselves are owned by the sibling command leaves and are not restated here:

- **`Cli`** — the clap `Parser`: flattened `global: GlobalFlags` plus `command: Command` (Subcommand).
- **`GlobalFlags`** (global on every subcommand) — `-w/--workspace <path>` (explicit workspace root; otherwise auto-detected per I3), `-v/--verbose` (count action: 0=warn, 1=info, 2=debug, 3+=trace), `-q/--quiet` (suppress non-error output), `--json` (machine-readable output), `--api-url` (env `G5D_API_URL`).
- **`Command` enum** — one variant per command family. The variant/arg surfaces are owned by: `software_architecture/g5d/cli/local_workspace_commands#D2` (init/clone/fork/refresh/add/commit/push/revert/worker/workspace/auth plus output and config conventions), `software_architecture/g5d/cli/sync_spec_remote#D39` (connect/spec/remote/merge/view — including the `remote` subcommand tree: status/sync/push/push-mode/runs/retry/merge), `software_architecture/g5d/cli/external_agent_tools#D6D` and `software_architecture/g5d/cli/external_agent_tools#D13A` (issue/job/task/tool/mcp for external agents), and `software_architecture/g5d/cli/doctor_and_sidecar#D34` (doctor, preview serve, Claude sidecar). The D27 checklist below remains the field-level inventory oracle for the lifecycle arg structs.
- **`exit_codes` module** — the six-code convention (0 success … 5 isolation) owned by `software_architecture/g5d/cli#D1` and restated only in the I7 checklist row.
- **Shared helpers** — `resolve_workspace_path(explicit_path, start_dir) -> Option<PathBuf>` (I3 walk-up, explicit path wins); `init_logging(verbosity, json)` (tracing subscriber per the `-v` ladder, JSON format in `--json` mode); `format_success(data, json_mode)` / `format_error(code, message, exit_code, json_mode)` (single formatting seam for human vs JSON output).

### Subcommand Handler Signatures
[D25. Subcommand Handler Signatures — Why: design choice supports the component behavioral contract]

Every subcommand handler file exports exactly one uniform entrypoint: `pub async fn execute(args: &<Command>Args, global: &GlobalFlags) -> Result<(), G5Error>`. There are no bespoke handler shapes; `serve.rs` was removed with the deprecated editor mode (g5-api serves browser-facing endpoints).

### Dispatch Pattern in `main.rs`
[D26. Dispatch Pattern in main.rs — Why: pattern standardization enables consistent implementation across modules]

`main.rs` consumes this module in a fixed sequence: `Cli::parse()` → `init_logging(cli.global.verbose, cli.global.json)` → a single `match` on `cli.command` delegating to the matching `cli::<module>::execute(args, &cli.global)` → exit with `exit_codes::SUCCESS` on `Ok`, or print `format_error(e.code(), e.message(), e.exit_code(), cli.global.json)` to stderr and exit with `e.exit_code()` on `Err`. No business logic lives in the dispatch layer.

### Success Criteria Checklist
[I6. Success Criteria Checklist]

#### Public API Surface (MANDATORY)
[D27. Public API Surface (MANDATORY) — Why: explicit API inventory ensures implementation completeness]
- [ ] `Cli` struct with `global: GlobalFlags` and `command: Command` fields, deriving `clap::Parser`
- [ ] `GlobalFlags` struct with `workspace`, `verbose`, `quiet`, `json`, `api_url` fields, deriving `clap::Args`
- [ ] `Command` enum with all lifecycle variants plus `Merge`, `Conflicts`, `Worker`, `Doctor`, `Job`, `Task`, `Spec`, `Workspace`, `Tool`, and `Remote`
- [ ] `InitArgs` struct with `path: Option<PathBuf>`, `force: bool`, `docs_repo: Option<String>`
- [ ] `CloneArgs` struct with `workspace_id: String`, `path: Option<PathBuf>`, `token: Option<String>`
- [ ] `ForkArgs` struct with `path: Option<PathBuf>`, `name: Option<String>`
- [ ] `RefreshArgs` struct with `path: Option<PathBuf>`, `pull_images: bool`
- [ ] `AddArgs` struct with `repo_url: Option<String>`, `local: bool`, `name: Option<String>`, `branch: Option<String>`, `path: Option<PathBuf>`, `track_remote: Option<String>`
- [ ] `CommitArgs` struct with `message: String`, `all: bool`
- [ ] `PushArgs` struct with `all: bool`
- [x] `RemoteArgs`, `RemoteSubCommand`, `RemotePushModeCommand`, `RemotePushMode`, `RemoteRunsArgs`, and `RemoteMergeCommand` structs/enums for the `g5d remote ...` surface
- [ ] `RevertArgs` struct with `all: bool`
- [ ] `WorkerArgs` struct with `task`, `workspace`, `port`, `worker_id`, `service_token`, `cloud`, `idle_timeout`, `poll_interval`
- [ ] `DoctorArgs` struct with optional `profile`
- [ ] `ToolArgs` struct with optional tool name, `--list`, `--schema`, `--args-json`, `--args-file`, `--workspace`, and `--session-id`
- [ ] `exit_codes` module with `SUCCESS` (0), `GENERIC_ERROR` (1), `CONFIG_ERROR` (2), `NETWORK_AUTH_ERROR` (3), `REPO_GIT_ERROR` (4), `ISOLATION_ERROR` (5)
- [ ] `resolve_workspace_path(explicit_path, start_dir) -> Option<PathBuf>` function
- [ ] `init_logging(verbosity, json)` function
- [ ] `format_success(data, json_mode) -> String` function
- [ ] `format_error(code, message, exit_code, json_mode) -> String` function
- [ ] Each subcommand handler exports `pub async fn execute(args, global) -> Result<(), G5Error>`
- [ ] Actual current `Command` enum surface includes `Merge`, `Conflicts`, `Job`, `Task`, `Spec`, `Workspace`, `Tool`, and `Remote` in addition to the lifecycle commands
- [ ] Repo-local `.g5/auth_state.json` bootstrap works with missing or stale JWTs when refresh-capable auth exists
- [ ] `.g5/workspace.yaml` remains the canonical discovery anchor even when bundle metadata files also exist under `.g5/`
- [ ] The generic Rust binary remains usable via workspace-local `./.g5/g5d` and via optional global promotion of `./.g5/bin/g5d`

#### Required Behavior (NON-NEGOTIABLE)
[I7. Required Behavior (NON-NEGOTIABLE)]
- [ ] All subcommands use clap derive API (`Parser`, `Subcommand`, `Args`)
- [ ] Global flags (`--workspace`, `--verbose`, `--quiet`, `--json`, `--api-url`) available on all subcommands
- [ ] Workspace auto-detection: walk up from cwd looking for `.g5/workspace.yaml`
- [ ] `init` generates UUID v7 workspace_id, creates `.g5/workspace.yaml` and `.g5/workspace-meta/`
- [ ] `clone` fetches manifest from g5-api, clones repos with COW, initializes the independent docs repo checkout
- [ ] `fork` creates fork in g5-api, clones from current workspace using COW, creates new branches
- [ ] `refresh` is safe-additive — never deletes local repos not in manifest
- [ ] `add` clones repo with COW, updates config, optionally pushes to g5-api
- [x] `add --track-remote <remote-branch>` calls the add-repo API with `trackingMode = "remote_tracking"` and `remoteBranch`, and fails closed on old-server enum/field gaps
- [ ] `commit` ensures CRDT draft is materialized before committing, does NOT push
- [ ] Connected Provided-Agent `commit` atomically publishes a bounded runtime-only request, resumes an identical pending id across CLI restart, blocks for the matching server-origin result, and never falls back to loopback proxy, raw Git, or fixed-delay success
- [ ] `push` pushes repos BEFORE updating workspace.lock (ordering invariant)
- [x] `remote push` calls server PushToRemote and remains distinct from top-level `push`
- [x] `remote push-mode set manual|auto` persists `workspaces.push_mode` without immediately publishing
- [x] `remote runs`, `remote retry`, and `remote merge` resolve branch ids/names unambiguously and call the branch-scoped API routes
- [ ] `push` retries failed pushes up to 3 times with exponential backoff
- [ ] `revert` discards changes and resets CRDT draft, leaves workspace.lock unchanged
- [ ] `worker` delegates to `WorkerOrchestrator::run()` after constructing config
- [ ] `doctor` checks: COW support, microVM readiness, git version, toolchains, workspace validity, cache status
- [ ] All workspace-modifying commands commit to workspace meta repo at `.g5/workspace-meta/`
- [ ] Exit codes: 0 success, 1 generic, 2 config, 3 network/auth, 4 repo/git, 5 isolation
- [ ] `--json` flag produces machine-readable JSON output for all commands
- [ ] `--verbose` flag controls log verbosity: 0=warn, 1=info, 2=debug, 3+=trace
- [ ] Configuration precedence: CLI flags > env vars > workspace.yaml > g5.yaml > defaults
- [ ] `--api-url` can be set via flag or `G5D_API_URL` env var
- [ ] `--service-token` (worker) can be set via flag or `G5D_SERVICE_TOKEN` env var
- [ ] `--token` (clone) can be set via flag or `G5D_TOKEN` env var

#### Performance Requirements
[D28. Performance Requirements — Why: performance targets ensure responsiveness for interactive use]
- [ ] CLI argument parsing: <1ms (clap derive is compile-time generated)
- [ ] Workspace auto-detection: <10ms (directory walk)
- [ ] `init` command: <1s (local filesystem operations only)
- [ ] `doctor` command: <5s (all diagnostic checks)
- [ ] Config loading: <50ms (YAML parsing)

## Design References (MUST READ)
[I8. Design References (MUST READ)]

Agents implementing this spec MUST read and comprehend the following design documents:

1. **`designs/G5D_CLI.md`** — AUTHORITATIVE for the CLI surface. Defines all CLI commands, `workspace.yaml` format, `g5.yaml` format, workspace meta repo, COW clone strategy, and the minimal clap skeleton.
2. **`designs/WORKER_INFRASTRUCTURE.md` §11.1-11.5`** — One binary two roles, local multi-instance, cloud overflow, instance configuration, desktop vs cloud workers.
3. **`designs/DESIGN.md` §4.2`** — Two binaries architecture, g5d binary role.
4. **`designs/API_AND_STORAGE.md` §3`** — API endpoints that CLI commands call (workspace CRUD, workspace lock, fork).
5. **`designs/REMOTE_REPO_TRACKING.md` §9.4`** — remote-tracking CLI surface and its mapping to PushToRemote, push_mode, sync-runs, and merge-runs routes.

Code that contradicts the design documents is incorrect. When ambiguity exists, G5D_CLI.md takes precedence for CLI commands and config schemas.

[D29. CLI crate ownership and compatibility boundary]

The independently compilable `g5d-cli` crate owns the complete clap command
and argument type surface, CLI exit-code constants, workspace discovery,
logging and output formatting, top-level `G5Error` exit classification, panic
hook, and the process shell that invokes an injected async dispatcher. It may
depend only downward on `g5-common`, foundation/runtime contracts, isolation,
tooling, worker, and host crates; it must never depend on the outer `g5d`
package or `g5-api`.

The shipped executable remains the package and binary named `g5d`. Its
`main.rs` is only the Tokio entrypoint that passes the outer command dispatcher
to `g5d_cli::run_process` and exits with the returned code. The outer
`g5d::cli` and `g5d::config` paths remain source-compatible re-exports while
current integration consumers migrate. Command implementations that still
compose outer worker/bootstrap, host production adapters, shared local-HEAD
reconciliation, or the legacy in-guest runner may remain explicit outer
adapter modules during the phased move; they must not be copied into the new
crate or hidden behind a reverse dependency. Their exit criterion is a neutral
lower-crate port or physical implementation owner, after which the outer file
becomes a compatibility re-export.

The extraction changes no binary name, clap spelling/help/default, output or
exit-code mapping, default `libkrun` feature, no-default isolated MCP build,
or image/seed packaging path. Parser/help tests, default and no-default builds,
the exact CI feature matrix, and the Cargo DAG guard are required acceptance
evidence.
