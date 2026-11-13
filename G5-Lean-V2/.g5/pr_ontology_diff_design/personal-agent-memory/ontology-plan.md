# Private agent memory in the personal repository

Intent: [chat intent](../../chat_intent/personal-agent-memory/request.md) of 2026-10-27.
Claude's native memory moves from the launch code repository's `.g5/memory/claude/`, which
every reader of that repository sees, to `memory/claude/` in the owner's
[personal repository](../../ontology/features/personal_workspace.g5.md#g5.workspace.identity.reference).
G5 commits and pushes it after each code push, best effort.

## Ontology changes

| Block | Change | Summary |
|---|---|---|
| [`g5.story.reuse-native-memory`](../../ontology/features/chat.g5.md#g5.story.reuse-native-memory) | Amended (owner) | Reuse memory across the owner's sessions and workspaces, privately; no longer shared with teammates. |
| [`g5.personal.memory`](../../ontology/features/personal_workspace.g5.md#g5.personal.memory) | New | Per-provider folders in the personal repository; off where it is not checked out; `.g5/memory/` left unchanged. |
| [`g5.personal.memory-sync`](../../ontology/features/personal_workspace.g5.md#g5.personal.memory-sync) | New | After each code push, commit only the memory folders and fast-forward the personal branch; skip rather than discard. |
| [`g5.personal.project-docs`](../../ontology/features/personal_workspace.g5.md#g5.personal.project-docs) | Amended | Memory location now references `g5.personal.memory`. |
| [`g5.personal.durability`](../../ontology/features/personal_workspace.g5.md#g5.personal.durability), [`g5.personal.mount`](../../ontology/features/personal_workspace.g5.md#g5.personal.mount) | Amended | Memory sync is the one commit G5 makes there. |

## Owners and interactions

- **Binding (`g5d-agent`).** The dispatcher tells an `agent.turn` launch the personal
  repository's root (`G5D_AGENT_PERSONAL_ROOT`). It derives this from the layout it already
  has, with no new field: in a personal workspace, the member that
  [follows its branch](../../ontology/features/personal_workspace.g5.md#g5.personal.follow);
  elsewhere, the reference checkout at `personal`, the only reference an ordinary
  workspace has ([mount](../../ontology/features/personal_workspace.g5.md#g5.personal.mount)).
  `memory::prepare` binds `<root>/memory/claude` or `<root>/memory/codex` and keeps its existing refusals (ignored by Git,
  not writable, symlinks, nested `.git`). With no root, the binding is `Skipped` and the
  existing `shared_memory_skipped` notice says why. The launch directory no longer selects
  the folder.
- **Trigger (dispatcher Git relay).** Every push from the VM passes through the
  [relay](../../../backend/g5d-dispatch/src/git_transport.rs). A lease can be *observed*:
  the native routes of workspace members, and the leases of the Changes operations that
  publish a member branch (push, update, rebase, adopt). When an observed
  `git-receive-pack` exchange completes with a success status, the relay wakes the sync.
  Flush, tracking refresh, the mount's own route and the sync's own lease are not observed.
  A pushed ref Git refused inside a successful exchange still wakes the sync. That is
  harmless, because the sync only publishes memory.
- **Sync (dispatcher, `g5-scm`).** One background loop waits on that wake-up, so pushes
  during a sync cause exactly one more. Each sync holds the dispatcher's Git-operation lock
  under a bounded deadline. It calls `MemberGit::sync_paths` on the personal checkout with the
  prefixes `memory/claude/` and `memory/codex/`:
  1. Skip unless the checkout is on its branch with no sequencer in progress.
  2. Commit the changed files under the prefixes with the existing selected-path checkpoint.
     It leaves the user's index and other staged work in place.
  3. Fetch the branch. Skip if the local commits beyond it change anything outside the
     prefixes, since the sync publishes memory only.
  4. If the branch moved, replay the local commits onto it with `git rebase`. Run that only
     when no tracked file is changed. Merge `MEMORY.md` as a union, so concurrent index
     additions keep both lines. On a conflict, `rebase --abort`. Never `reset --hard`.
  5. Push with a lease on the fetched head. The push is admitted as a
     [mount push](../../ontology/features/personal_workspace.g5.md#g5.personal.mount-push)
     from other workspaces, and as an ordinary member push in the personal workspace.

  Every skip leaves the commit or change in place for the next push. Outcomes are logged,
  never surfaced as turn or save failures.
- **Flush.** The personal workspace's member keeps the "ignore rules must not omit memory"
  capture guard, now for `memory/claude` and `memory/codex`. Code repositories lose the
  guard for `.g5/memory/*`, which agents no longer write. Existing tracked files there are
  captured as before.
- **Harness.** The packaged `.agents/AGENTS.md` says where memory lives, who can read it,
  that G5 commits it after each code push (best effort), and that it must never be copied
  into a workspace repository.

## Choices

- **Owner choices.** The trigger (every code push, rather than Save only or relying on
  the agent), the paths, and best-effort loss are the owner's choices.
- **No new spec field.** A `personal` flag on `VmReferenceRepoSpec` was rejected. Host and
  guest specs use `deny_unknown_fields`, so a new field would fail Start on a host that
  lags the API. The layout already identifies the repository.
- **No reuse of `rebase_onto`.** It was rejected for the replay: its failure path runs
  `reset --hard`, which is safe only under the Rebase module's clean-tree admission. The
  sync runs while agents write files, so it relies on Git's own refusal and `rebase --abort`.
- **Union merge via the sync.** The `MEMORY.md` union attribute comes from a
  dispatcher-owned attributes file passed with `-c core.attributesFile`. Nothing is written
  into the user's checkout or `.git`. Hand-run rebases do not get the union.
- **Memory off without a mount.** Falling back to the code repository was rejected, because
  it would share memory again.

## Limits and gaps

- **Stop.** Memory written after the last code push, or not yet published when the VM
  stops, is lost in a mounted checkout. Accepted as best effort.
- **Personal VM busy.** A mount push is refused while the personal workspace is starting,
  stopping or stopped with unsaved work. The commit stays local until the next code push.
- **Snapshot at Start.** Memory pushed from other workspaces reaches a running mount only
  through the sync's replay, or the agent's own pull. Claude reads `MEMORY.md` when a
  session starts.
- **Codex.** The owner's [2026-10-28 request](../../chat_intent/codex-memory/request.md) enables native memory in `memory/codex/`. Bind `$CODEX_HOME/memories` to that directory for new and resumed chats, keeping credentials, sessions and SQLite state at home. Enable native use and generation only with a valid personal binding; read-only turns use memory without generating it. Extend workspace-write access only to that memory folder, not other personal files. Formal reviews remain isolated with memory off. Refuse an existing nonempty local memory directory or a conflicting link rather than replacing its contents. Native generation timing and push-triggered best-effort persistence still apply.
- **Earlier personal-workspace memory.** Memory that personal-workspace agents saved before this change sits at `.g5/memory/claude/` in the personal repository. It is left in place and no longer loaded.
