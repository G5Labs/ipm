# Personal workspace: per-user chat and persistent artifacts from Home

Design for [G5-Lean#624](https://github.com/G5Labs/G5-Lean/issues/624). Intent:
[chat intent](../../chat_intent/issue-624-personal-workspace/request.md) of 2026-10-18, with the
owner's 2026-10-19 follow-ups (the Home side panel, and the [mount](#mount-in-the-owners-other-workspaces) that replaced copy-out).
The issue's design text is the human source; this plan maps it onto the current ontology
and code, and records where verified code facts change the mechanism, but not the outcome.

## Ontology changes

| Block | Change | Summary |
|---|---|---|
| [`g5.purpose.personal`](../../ontology/features/personal_workspace.g5.md#g5.purpose.personal) | New | The goal: chat from Home, persistent artifacts, existing codebases, notes pulled into the owner's workspaces; sharing deferred. |
| [`g5.personal.identity`](../../ontology/features/personal_workspace.g5.md#g5.personal.identity) | New | Immutable kind bound to (org, user); at most one live; no parent; rejoin gets a fresh one. |
| [`g5.personal.names`](../../ontology/features/personal_workspace.g5.md#g5.personal.names) | New | `users/<handle>`, handle frozen at creation; `me`, `users`, `users/…` reserved; legacy names reported, kept. |
| [`g5.personal.routes`](../../ontology/features/personal_workspace.g5.md#g5.personal.routes) | New | `/{org}/me` (ensures creation) and `/{org}/users/{handle}`, resolved by kind. |
| [`g5.workspace.identity.reference`](../../ontology/features/personal_workspace.g5.md#g5.workspace.identity.reference) | New | One private providerless member; docs + Base code repos are reference checkouts derived at each Start. |
| [`g5.personal.project-docs`](../../ontology/features/personal_workspace.g5.md#g5.personal.project-docs) | New | No Project Docs folder; agent launches in the private repo; records stay there. |
| [`g5.personal.access`](../../ontology/features/personal_workspace.g5.md#g5.personal.access) | New | Owner + admins read (API and Git); invite-only, no invitations; product discloses admin access. |
| [`g5.personal.refusals`](../../ontology/features/personal_workspace.g5.md#g5.personal.refusals) | New | Fork source, merge, review, transfer, visibility, invitations, owner delete, stages, rebase, PR, Make Remote refused by kind. |
| [`g5.personal.lifecycle`](../../ontology/features/personal_workspace.g5.md#g5.personal.lifecycle) | New | One idempotent (org, user) creator after create/join/promote; reconciler retries and backfills. |
| [`g5.personal.retirement`](../../ontology/features/personal_workspace.g5.md#g5.personal.retirement) | New | Remove/demote tombstones, withdraws consent and requests Stop in the same transaction; never blocks. |
| [`g5.personal.start-and-send`](../../ontology/features/personal_workspace.g5.md#g5.personal.start-and-send) | New | Start consent and queued input in one request; ending consent withdraws the input. |
| [`g5.personal.capacity`](../../ontology/features/personal_workspace.g5.md#g5.personal.capacity) | New | Org VM quota; personal idle default (30 min) at creation; owner override. |
| [`g5.personal.durability`](../../ontology/features/personal_workspace.g5.md#g5.personal.durability) | New | No branch policy; harness says commit and push; Stop flushes; no implicit commit. |
| [`g5.personal.mount`](../../ontology/features/personal_workspace.g5.md#g5.personal.mount) | New | Every other workspace the owner starts checks out the private repo at `personal`, on the personal branch; not a member, not flushed, not rendered. |
| [`g5.personal.mount-push`](../../ontology/features/personal_workspace.g5.md#g5.personal.mount-push) | New | Fast-forward, update-only pushes of the personal branch from the mount, admitted as a move of the personal workspace's branch. |
| [`g5.personal.follow`](../../ontology/features/personal_workspace.g5.md#g5.personal.follow) | New | The running personal VM fast-forwards a checkout with no local commits when its branch moves. |
| [`g5.security.credentials.delegated-read`](../../ontology/features/personal_workspace.g5.md#g5.security.credentials.delegated-read) | New (deferral) | Delegated reads of platform records are deferred by the owner; the agent reads other workspaces through Git only. |
| [`g5.personal.home`](../../ontology/features/personal_workspace.g5.md#g5.personal.home) / [`.workbench`](../../ontology/features/personal_workspace.g5.md#g5.personal.workbench) | New | Home panel with the shared live gate; personal workbench presentation by kind. |
| [`g5.workspace.kind`](../../ontology/features/workspaces.g5.md#g5.workspace.kind) | New | Base / working / personal registry kind, immutable, never inferred from a name. |
| [`g5.workspace.transfer.formal`](../../ontology/features/workspaces.g5.md#g5.workspace.transfer.formal) | Amended | The owner-removal refusal counts working workspaces only; personal is retired instead. |
| [`g5.workbench.tabs`](../../ontology/features/ui.g5.md#g5.workbench.tabs) | Amended | Reserve `me` and `users` beside `fork`. |
| [`g5.ui.home-workspaces`](../../ontology/features/ui.g5.md#g5.ui.home-workspaces) | Amended | The table omits personal workspaces. |
| [`g5.lifecycle.idle.settings`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.idle.settings) | Amended | Personal default exception. |
| [`g5.data.configuration.org-admin-settings`](../../ontology/features/organizations.g5.md#g5.data.configuration.org-admin-settings) | Amended | The typed owner also holds the personal idle default. |
| [`g5.runtime.materialization`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.materialization) | Amended | Reference checkouts from the same cache at pinned heads, no receipt. |
| [root](../../ontology/root.g5.md) | Amended | Reading path to the new page. |

### The issue's block IDs

The issue names blocks from the ontology replaced in `5c66d337`. Their commitments map as
follows; none needs a new home beyond the table above.

| Issue's block | Current owner |
|---|---|
| `g5.workspace.identity`, `g5.architecture.workspace_registry` | [`g5.workspace.kind`](../../ontology/features/workspaces.g5.md#g5.workspace.kind), [`g5.personal.identity`](../../ontology/features/personal_workspace.g5.md#g5.personal.identity) |
| `g5.fork.names`, `g5.workspace.names`, `g5.workspace.identity.branch` | [`g5.personal.names`](../../ontology/features/personal_workspace.g5.md#g5.personal.names), [`g5.workbench.tabs`](../../ontology/features/ui.g5.md#g5.workbench.tabs); [`g5.workspace.branch`](../../ontology/features/workspaces.g5.md#g5.workspace.branch) is unchanged (the branch lives only in the private repo). |
| `g5.lifecycle.delete.owner-removal` | [`g5.workspace.transfer.formal`](../../ontology/features/workspaces.g5.md#g5.workspace.transfer.formal), [`g5.personal.retirement`](../../ontology/features/personal_workspace.g5.md#g5.personal.retirement) |
| `g5.lifecycle.idle.settings` | same ID, amended |
| `g5.runtime.materialization`, `g5.runtime.boot.image` | [`g5.runtime.materialization`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.materialization); image pins under [`g5.workspace.identity.reference`](../../ontology/features/personal_workspace.g5.md#g5.workspace.identity.reference), within [`g5.host.image-selection`](../../ontology/software_architecture/hosts.g5.md#g5.host.image-selection) |
| `g5.git.durability.no-implicit` | [`g5.personal.durability`](../../ontology/features/personal_workspace.g5.md#g5.personal.durability), [`g5.personal.mount`](../../ontology/features/personal_workspace.g5.md#g5.personal.mount) (only pushed work survives) |
| `g5.memory.repository` | [`g5.personal.project-docs`](../../ontology/features/personal_workspace.g5.md#g5.personal.project-docs) under [`g5.story.reuse-native-memory`](../../ontology/features/chat.g5.md#g5.story.reuse-native-memory) |
| `g5.conversation.composer.stopped`, `g5.lifecycle.start.compound` | [`g5.personal.start-and-send`](../../ontology/features/personal_workspace.g5.md#g5.personal.start-and-send), under [`g5.lifecycle.start`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start), which already admits "clearly labeled compound actions" |
| `g5.workbench.deferred` | Gone; the homepage-chat deferral it carried is no longer recorded. |
| `g5.architecture.workspace_presentation` | [`g5.personal.workbench`](../../ontology/features/personal_workspace.g5.md#g5.personal.workbench), [`g5.personal.home`](../../ontology/features/personal_workspace.g5.md#g5.personal.home) |

## Governing ontology

- [`g5.workspace.audience`](../../ontology/features/workspaces.g5.md#g5.workspace.audience): owner and admins read invite-only workspaces; unchanged.
- [`g5.workspace.parent`](../../ontology/features/workspaces.g5.md#g5.workspace.parent), [`.fork`](../../ontology/features/workspaces.g5.md#g5.workspace.fork), [`.delete`](../../ontology/features/workspaces.g5.md#g5.workspace.delete), [`.stages`](../../ontology/features/workspaces.g5.md#g5.workspace.stages), [`.project-docs`](../../ontology/features/workspaces.g5.md#g5.workspace.project-docs): human-provided rules a personal workspace is excepted from. The human wording stays; [`g5.workspace.kind`](../../ontology/features/workspaces.g5.md#g5.workspace.kind) points to the exceptions.
- [`g5.repo.docs`](../../ontology/features/repositories.g5.md#g5.repo.docs) "Every workspace includes it": met read-only by the reference checkout.
- Issue 423's [Git read decision](../../chat_intent/issue-423-git-read-boundary/request.md): the repository is the unit of Git read access. This is why the private repo exists.
- [`g5.git.flush-proof`](../../ontology/software_architecture/git.g5.md#g5.git.flush-proof): receipts prove exactly the members; references are not members, so the rule needs no exception.
- [`g5.lifecycle.start`](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.start) and [`g5.runtime.placement.capacity`](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.capacity): Start consent and quota, reused.
- [`g5.security.owner`](../../ontology/software_architecture/workspace_vm.g5.md#g5.security.owner): only the current owner uses a VM, which bounds who reaches the mount.
- [`g5.git.stopped-rebase`](../../ontology/software_architecture/git.g5.md#g5.git.stopped-rebase), [`.lifecycle-rebase`](../../ontology/software_architecture/git.g5.md#g5.git.lifecycle-rebase) and [`.pull-policy`](../../ontology/software_architecture/git.g5.md#g5.git.pull-policy): when a foreign move of a stopped branch is safe, refusal over path switching, and the import rule `g5.personal.follow` excepts.
- [`g5.security.guest-authority`](../../ontology/concepts/security.g5.md#g5.security.guest-authority) and [`g5.security.current-authority`](../../ontology/concepts/security.g5.md#g5.security.current-authority): bound retirement and the guest's Git reads.

## High-level design

### Identity

**Kind column.** `workspaces.personal_user_id uuid NULL`. A non-null value makes the kind personal. A CHECK ties it to `owner_user_id` and `NOT is_base`, and a companion trigger beside migration 039's kind-immutability trigger fixes it. 039's trigger function belongs to the bootstrap role, so the deployment migrator cannot replace it. A partial unique index `(org_id, personal_user_id) WHERE deleted_at IS NULL` gives "at most one live". Rust gets `WorkspaceKind { Base, Working, Personal }`, derived once in the model, and every guard reads it. A name is never consulted, which is the "name looks personal" acceptance case. This reuses the existing kind discipline rather than adding a text enum beside `is_base`.

**Why no parent.** `cloned_from_id` stays NULL. Nothing that walks ancestry, such as ancestor read, rebase, merge target or the lineage tree, can then reach into or out of a personal workspace. No code treats a parentless workspace as Base: Base is always found by `is_base`, there is no `cloned_from_id IS NOT NULL` constraint, and parentless paths already fail cleanly ("source workspace has no merge target") or render as tree roots. One query needs the kind guard: PR targets (`workspace_pull_request_create::targets`) offer Base to every source, so a personal workspace would otherwise be offered a PR into Base.

**Handle.** Users have no handle field. `email` changes on login and contains `@`; `display_name` is free text; `provider_username` is per provider, nullable and mutable. Adding a user-level handle would be a new account-wide identity concept. So the handle is chosen once by the personal creator, from the email local part with the deterministic normalization in [`g5.personal.names`](../../ontology/features/personal_workspace.g5.md#g5.personal.names), and frozen as the workspace name. `users/<handle>` is a valid ref name under `check_workspace_name`, and its branch exists only in the private repo, so it cannot hit the `users` vs `users/x` directory/file ref conflict in shared repos. See [decisions](#decisions).

**Rejoin.** "At most one live per pair" does not say whether a rejoining user gets back their tombstoned workspace. Reviving one would need an undelete path that does not exist. The plan creates a fresh one; the retired repository stays under the erasure policy.

**Reservation.**
- `RESERVED_WORKSPACE_NAMES` gains `me` and `users`, and `check_workspace_name` refuses the `users/` prefix case-insensitively.
- The client `NON_WORKSPACE_SEGMENTS` gains `me` and `users`.
- Live workspaces and pending fork claims named `me`, `users` or `users/…` are left unchanged and remain working workspaces. A migration cannot report them, because a migration session sees no tenant rows under FORCE ROW LEVEL SECURITY. The personal reconciler logs them per organization instead.
- Name freezing (migration 039) means they cannot be renamed. A legacy `me` or `users` workspace is shadowed by the routes, like the existing `fork` case recorded in `g5.workbench.tabs`.
- The personal creator's collision suffix avoids every live name, including legacy `users/…` ones.

### Members

The issue proposed a new member role. The code check found that every capture path consumes one full member vector and asserts it whole:
- flush `capture_all` returns `FLUSH_MEMBER_VECTOR_MISMATCH`;
- the host's `valid_repo_vector` and the VM config require one shared branch for all repos;
- `valid_restore_vector` needs receipts for exactly the member set;
- fork, Changes, comparison, rebase and publication each load the full vector;
- about 154 non-test `docs_repo_id` reads exist.

A stored reference role would have to be filtered at each of those sites. That is the largest risk the issue names.

**Choice: derive references, don't store them.**
- `workspace_repos` for a personal workspace has one row: the private repo, `role='code'`, `root_path='personal'`.
- `docs_repo_id` is NULL.
- Every existing member-vector consumer therefore sees a one-repo workspace and needs no filter.
- References are computed at one place, Start intent (`record_start_intent`): Base's current members (docs + code), each pinned at Base's durable branch head (`repo.default_branch`) under the same transaction.
- They travel in a new, separate boot-spec field `reference_repos: Vec<VmReferenceRepoSpec { repo_id, root_path, role, commit }>`. Because it is not in `repos`, the shared-branch, restore-vector and flush-vector checks are untouched.

What stays the same and what is added:

| Concern | Mechanism |
|---|---|
| Host materialization | The same bare cache and `materialize_exact_checkout` at the pinned commit, on Base's branch name, never the workspace branch. No restore and no receipt. The pristine-seed path is skipped when references exist, and the docs reference's role is withheld so it gets no `project_docs` alias. |
| Next Start | Re-derives from Base, so reference edits are discarded and new Base commits appear. |
| Dispatcher / flush | `WorkspaceLayout.references` sits beside the members. References get an origin remote through the guest transport so ordinary Git can fetch, but capture, flush, Changes and `G5D_AGENT_REPOSITORIES` iterate members only. |
| Root paths | Base root paths are unique among Base's members. The reserved `personal` root is refused at Start if Base ever has a member there; this is a typed error, not silent shadowing. |
| VM Git read | `require_current_workspace_incarnation` additionally admits **read** of a repo that is a current Base member when the incarnation's workspace is personal. Rechecking at request time follows current Base membership. Fetching other workspaces' branches through the reference checkouts is the issue's "no new credential" path, within the per-repository read rule. |
| VM Git write | Receive-pack admission (`require_push`) and ref updates use the member-only incarnation target, so pushes to reference repos are refused. `api_personal_workspace_runtime.rs` pins this. |
| Image pins | `resolve_workspace_image` takes the reference code repos at their pinned commits for a personal workspace. The private repo's own `.g5/workspace.yaml` is not consulted. |
| Agent cwd | Personal launches send `workspaceTarget { relativePath: "personal", personal: true }`. `memory::prepare` selects the member containing cwd, so Claude memory binds to the private repo. The `personal` flag adds the personal harness guidance. |

**What derivation gives up.** References track Base's members at Start, so a code repo added to Base appears at the next Start, which matches "Base's code repos". There is no per-user choice of reference set, and none was asked for.

### Access and refusals

**Reads.**
- The personal workspace is created `visibility='invite_only'` with no `workspace_members` rows, so `user_has_workspace_access` admits the owner and admins and no one else.
- Git read of the private repo follows the same check, because no other workspace includes the repo.
- Workspace lists include personal rows only for those readers, with `kind` in the DTO.

**Refusals.** Each is a kind check at the owning service, returning a new `G5Error::WorkspaceIsPersonal` (409):

| Operation | Guard site |
|---|---|
| Fork source | `fork_workspace` admission |
| Merge target | merge admission, and `merge_target_workspace_id` validation |
| Review copy | review creation |
| Transfer | `transfer` |
| Visibility | `update_workspace` |
| Invitations | `add_workspace_member` |
| Owner delete | `delete_workspace` |
| Phase | phase/journey mark, beside the `WorkspaceIsBase` sites |
| Rebase, update-from-parent | rebase admission |
| Publication, PR, Make Remote | publication admission, beside the Base sites |

Several of these have no kind check today, even for Base: transfer, visibility, member grants. The personal guard goes in the service, not the route, so `g5d` and the API share it.

### Lifecycle

**The creator** (`services/workspace/personal_workspace.rs`, `ensure`), idempotent:
1. Take a per-org advisory lock on `g5:personal-names:<org>`. It serializes handle choice. Ordinary fork admission already refuses `users/…`, so no fork can compete for the name.
2. Return the live personal workspace if one exists.
3. Confirm the user is an owner-eligible member.
4. Choose the handle, skipping every live workspace name and pending fork claim.
5. In one transaction, insert the providerless repo (`g5-local://personal/<workspace-id>.git`, mirroring Base's local code repo; the URL is only a label, since Git storage is keyed by repo ID, and the workspace ID keeps it distinct across organizations and rejoins), the workspace (`personal_user_id`, `invite_only`, `push_mode='manual'`), its `workspace_repos` row and `workspace_settings` with the personal idle default. The deferred owner foreign key refuses, at commit, a creation that races the owner's removal or demotion.
6. After commit, give the repo its first commit on `users/<handle>` through the ordinary initial branch commit. That path admitted only Base before; the authority inventory now carries `may_be_unborn`, true for Base and for a personal workspace. A retry completes this step for a row whose branch is missing.

**Callers.**
- After `create_org` commits and bootstraps Base.
- `invite_member`'s immediate insert, `accept_invitation`, and `update_member_role` to member or admin, each after commit. Failures are logged, never returned.
- After the OAuth login callback, which may auto-accept invitations, `ensure_after_login` checks up to 20 of the user's eligible organizations; the reconciler covers any beyond that.
- The per-org `workspace_lifecycle` loop runs `reconcile_org` every 15th tick (about every 30 s) for up to 8 members. It also completes branches that are still missing. This is the backfill.
- `GET /orgs/:org/me`.

**Retirement in the membership transaction** (`remove_member`, `update_member_role` to viewer):
1. Before the service's own checks, `retire_in_tx` tombstones the live personal workspace (`deleted_at`, `deleted_by`, `deletion_operation_id`). That clears `active_owner_user_id`, so the owner check `reject_active_workspace_owner_change` and the immediate `workspaces_owner_same_org_fk` check pass. It also advances `lifecycle_generation`, which invalidates Start consent, and cancels a Start that is still waiting. A refused membership change rolls all of this back.
2. After commit, the route spawns `stop_retired_personal`: a system Stop with `owner_removed` and an immediate deadline, so no save window. `admit_stop` admits this one reason for a tombstoned personal workspace, so the existing executor, its claims and `reconcile_stopping` all apply.
3. `reconcile_retired_personal` in the lifecycle loop re-admits that stop for any retired personal workspace whose VM still runs.

The tombstone removes nothing else: the private repo, its refs and the member rows are retained under the open erasure policy. The ordinary owner-delete cleanup is not reused, so it keeps its stopped-VM requirement. The removed user's credentials already fail current-authority admission, so the VM's short tail cannot act as them.

### Execution

**Start and send.** `POST /sessions/:id/messages` and `POST /workspaces/:id/sessions/start` accept `startCompute: true`, and their response carries `startCompute { intentId, status }`.
1. Before the chat transaction opens, the route records owner Start consent through ordinary Resume with the `compound_start` kind, which the schema already defined. It must come first because Resume locks the workspace row.
2. The chat transaction queues the input as usual. `start_and_send::bind` then takes a share lock on the VM row and binds the input (`session_inputs.start_consent_id`) when the consent is still pending, queued or starting. A consent that already reached ready needs no binding. One that already ended refuses the request, so nothing is queued.
3. If the rest of the request fails, a consent that is still only waiting is cancelled (`cancel_waiting_start`).

From then on the trigger `workspace_vms_settle_start_bound_inputs` settles the binding in the same transaction that changes `start_intent`:
- `consumed` (ready) clears the binding;
- `cancelled`, `expired`, `failed`, a different intent, or deletion of the row withdraws inputs still queued, recording `start_ended` in their audit.

A trigger covers every writer of the consent: Stop, the capacity queue, host release and supersession. Hunting each site in Rust would miss future ones.

Dispatch already skips a task with no queued input. `cancel_tasks_left_by_ended_starts`, in the agent task reconciler, marks such a task cancelled.

**Why bind the input.** The two acceptance cases, consent expiry and cancellation, would otherwise leave a queued input that a later unrelated Start would deliver. That would be the "queued work authorizes waking" path that `g5.lifecycle.start` forbids, deferred.

The API is general, but only the personal composer offers it. Start progress is shown inline, from the existing VM status stream.

**Idle.** New typed admin setting `default_personal_idle_timeout_secs` = 1800, same validation range, read once by the creator. The owner override is the existing `workspace_settings.idle_timeout_secs`.

**Harness.** The personal launch adds a short system-prompt section to the agent's launch guidance:
- the private repo is the only durable place;
- commit and push artifacts as you make them;
- reference checkouts are read-only views of Base, discarded at the next Start;
- put G5 records (`.g5/chat_intent`, plans, ontology drafts) in the private repo.

It replaces the Project Docs guidance `g5.workspace.project-docs-agent` gives ordinary workspaces.

### Mount in the owner's other workspaces

Copy-out is gone. On 2026-10-19 the owner replaced it with an implicit mount of the private repository in every other workspace they start ([chat intent](../../chat_intent/issue-624-personal-workspace/request.md)). Notes travel into their work through ordinary Git, so copying needs no special operation. Showing files to another person is deferred, with tracker issues as the interim route.

**Owners.**
- The internal Git service owns the personal branch head ([`g5.git.authority`](../../ontology/software_architecture/git.g5.md#g5.git.authority)).
- The workspace lifecycle owns the personal workspace's VM state and recovery receipt. It decides whether a foreign move of a stopped branch is safe, through `source_rebase_execution`, the stopped-rebase predicate copy-out also used.
- Smart-HTTP receive-pack coordinates the push. It resolves the target and holds the lifecycle's locks while Git applies the update, but it decides neither the branch rule nor the lifecycle rule itself.
- The personal VM owns its checkout ([`g5.git.live-rebase`](../../ontology/software_architecture/git.g5.md#g5.git.live-rebase)).

**Mount (`g5.personal.mount`).** This reuses the reference-checkout path end to end.
- `record_start_intent` already holds the starting workspace's commit fence. For a workspace that is not personal, it adds one `VmReferenceRepoSpec` for the current owner's live personal repository, from `personal_workspace::find_live`:
  - root `personal`, the personal workspace's own root, so `/work/personal` means the same repository everywhere;
  - `role` absent, so image selection and the docs alias ignore it;
  - `branch` the personal workspace's name;
  - `commit` its durable head, from `read_branch_head_oids`.
- It is left out when:
  - the owner has no live personal workspace;
  - the branch is unborn;
  - a member root overlaps `personal`. Failing an ordinary Start over an implicit convenience would be worse than its absence.
- The host's generic path already materializes references on a named branch from its cache. The dispatcher already gives references an origin remote and upstream (`branch.<b>.merge`), and leaves them out of capture, flush and Changes.
- The seed path changes. It used to skip seeds whenever references existed, because a seed holds members only and guest bootstrap restored members only. With the mount, nearly every Start has a reference, so that would have disabled seed acceleration everywhere. Guest bootstrap now fetches each reference over the VM token after restoring members (`restore_references_with_git`), and writes them into the layout.
- The host's cache warms over the VM token, so the read rule below admits the fetch.
- No schema change: the mount is derived at each Start, like the Base references.

**Read rule.**
- `workspace_incarnation::personal_mount_target(db, claims, repo_id)` returns the personal workspace and branch when all of these hold:
  - the token's incarnation is current;
  - its workspace is not personal;
  - `repo_id` is the member of the live personal workspace whose `personal_user_id` is the current owner, in the same organization.
- `require_current_workspace_incarnation` admits reads on it beside `personal_reference_readable`.
- Because the rule is evaluated per request, retirement, transfer, a newer epoch or a released lease end access at once.

**Push (`g5.personal.mount-push`).**
- *Ref rules.* `require_push` and `require_incarnation_ref_updates` accept the mount target for exactly `refs/heads/<personal branch>`, update-only. No flush leaves: the mount has no lifecycle refs. The `PushTarget` names the personal workspace and is marked fast-forward only.
- *Admission.* For a target that is not the token's own workspace, `acquire_push_admission`, in its one transaction:
  1. locks the pushing incarnation `FOR SHARE` and requires it running;
  2. rechecks the personal relationship under the authority lookup;
  3. takes `acquire_workspace_commit_fence(personal)`;
  4. locks the personal VM row, if any, `FOR SHARE`;
  5. requires `source_rebase_execution(personal)` to be `Vm` or `Central`. Its `Conflict` becomes a 409.
- *Why these locks.* Start captures heads and decides restore under that fence, so a push can never fall between Start's decision and its boot. Stop's status update waits for the share lock, so a running admission cannot become "stopped with a stale receipt" mid-push. Both are held until Git commits or refuses, which is the same lease receive-pack already holds for the pusher.
- *Lock order.* Pusher rows, then the personal fence, then the personal VM row. Start takes its own fence before its rows, and Stop takes no fence, so no cycle arises.
- *Fast-forward.* `ReceiveRefUpdate` gains `fast_forward_only`. When it is set, `NativeGit::receive_pack` runs Git with `receive.denyNonFastForwards=true`, so Git enforces it inside its locked ref transaction. Checking earlier is impossible, because the new commit arrives in the pack.
- *Effects.* Everything after the push is keyed to `target.workspace_id`, so the move is credited to the personal workspace with no new code: the branch-updated event, the after-push integration enqueue, and the review notification.
- *Stopped and clean.* After a stopped-clean push, `restore_is_superseded_by_authoritative_heads` already drops the clean receipt when a head moved. The next personal Start then checks out the new head.

**Follow (`g5.personal.follow`).**
- `VmRepoSpec` gains `follow_branch`, true only for a personal workspace's member and omitted when false. The host writes `spec.repos` into `layout.json` verbatim, and the dispatcher's `RepositoryLayout` reads the field.
- `refresh_provider_refs` passes it to `MemberTrackingRefresh`. That runs every 30 seconds and synchronously before each agent turn.
- After fetching, `refresh_tracking` fast-forwards with `merge --ff-only` only when all of these hold:
  - the checkout is on the branch;
  - its local branch is a strict ancestor of the refreshed tracking ref;
  - Git accepts the merge, which it refuses when local changes or untracked files would be overwritten.
- A refused fast-forward is reported, not failed.
- An agent that commits between the check and the merge makes the merge non-fast-forward, so Git refuses; the race is safe.
- Divergence is left alone. The VM's next push reports the move: platform Save as `RECONCILIATION_REQUIRED`, an agent's own `git push` as non-fast-forward. The harness tells the agent to pull with rebase.
- The flag stays off for every other workspace, because provider imports must leave live files unchanged ([`g5.git.pull-policy`](../../ontology/software_architecture/git.g5.md#g5.git.pull-policy)).

**Harness.**
- `PERSONAL_GUIDANCE`: the branch also advances from the owner's other workspaces; pull with rebase before pushing.
- The packaged instructions (`.agents/AGENTS.md`) gain one conditional bullet: if `/work/personal` exists and is not one of the workspace's repositories, it is the owner's personal repository. Read their notes there; commit and push changes to its branch, because unpushed work is lost at Stop; never treat it as part of this workspace's changes. It sits in the packaged file rather than a launch-time append because every launch, personal or not, reads it, and the condition is false in a personal workspace.

**Removed.** `services/workspace/personal_copy_out.rs`, `POST /workspaces/:id/copy-out` and its handler in `routes/personal.rs`, `CopyToWorkspaceDialog`, the notice's "Copy to workspace…" action, the file tree's copy entry (`onCopyEntry` and `onCopyFileEntry`), `useCopyOutPersonalFiles`, `CopyOutRequest`/`CopyOutResult`, `lib/personalWorkspace.ts`, and their tests and mock handlers. The helpers copy-out shared stay with their other callers: `write_live_file`, `checkpoint_selected_files`, `source_rebase_execution` and `prepare_files_commit_from_exact_base`.

**Limits and assumptions.**
- The mount is pinned at Start, and nothing fast-forwards it in a working VM. The agent pulls when it needs newer notes.
- Two working VMs pushing at once resolve by Git's old-head check: the loser pulls and retries.
- A diverged running personal checkout needs an agent or owner rebase; no platform button exists.
- Mixed-version rollout is not a concern: `reference_repos` and `follow_branch` ship with the hosts and API of this branch.

### Presentation

**Home.** Home has a collapsible right-hand side panel, `PersonalChatSidePanel`. It mounts the workbench's own `ChatPanel` for the personal workspace's workspace-scope chat, fed by `useWorkspaceChatController`. That hook builds the panel's data and actions from the same chat hooks the workbench uses:
- sessions and message scrollback;
- the persisted plus live WebSocket timeline;
- the open turn's task;
- Stop with the A→B handoff, and End;
- approvals, renames and forks;
- the provider choice and launch confirmation;
- Start and send.

The turn rules shared with the workbench move to `lib/chatTurns.ts`. `PersonalHomeProvider` computes the live gate, `useWorkspaceLiveGateValue`, and Start, including the `WORKSPACE_START_EVENT` handler, once for both Home surfaces.

The side panel's header shows the VM status and the admin-visibility notice. The main-column `PersonalPanel` shows only below the `lg` width, where the side panel is hidden, with status and Start, the notice, and a compact conversation list and composer.

Files are not on Home, because the owner found them cluttering it. They live in the personal workbench, which already has the committed-file tree.

The workbench keeps its own chat wiring in `WorkspacePage`. Moving it onto the controller would remove the duplicated wiring, but it is a separate refactor of a 12,000-line page and is left for later.

**Workbench.** The workspace DTO's `kind` selects a personal presentation in `WorkspacePage`, using the same branch points Base uses: empty ribbon actions, no phase chevrons, no Guided panel, and no transfer, Make Remote, PR, rebase or update controls. Lineage tree, fork picker, `CloneDialog` targets and the Home table filter out `kind === "personal"`.

**Routes.** `/:org/me` calls `GET /orgs/:org/me` (ensure plus DTO) and renders the workspace page for that ID. `/:org/users/:handle` resolves through `GET /orgs/:org/users/:handle/workspace`, by kind and name. Neither depends on a workspace being named `me`.

### Delegated read (deferred)

The owner deferred delegated reads on 2026-10-18: with Git access to every repository, most tasks are already possible. No credential, router or audit table is built. The issue keeps the proposed limits and the exposure options (a), (b) and (c) for when it is revived; deciding the exposure comes first then.

## Decisions

The owner accepted every recommended answer on 2026-10-18 ([chat intent](../../chat_intent/issue-624-personal-workspace/request.md#later-decisions)). The question set `docs/0536c0f6-8ff9-416c-9c21-2d1dbe76a966/questions/personal-workspace.g5q.json` in the organization docs repository records them (ids in parentheses).

1. **Delegated reads** (`delegated-read-exposure`): deferred; no exposure option is chosen.
2. **Handle source** (`handle-source`): the email local part, normalized and frozen at creation, with a numeric suffix on collision.
3. **Rejoin** (`rejoin`): a rejoining user gets a fresh personal workspace; the retired one stays tombstoned.
4. **Reference checkouts** (`reference-derivation`): derived from Base at each Start, not stored as a member role.
5. **Stop reason**: retirement stops the VM with a new `owner_removed` reason rather than reusing `delete`, so stop history distinguishes member removal from an owner's deletion.
6. **Mount instead of copy-out** (2026-10-19, in chat, [chat intent](../../chat_intent/issue-624-personal-workspace/request.md)): the owner's other workspaces mount the private repository and push to its branch (write strategy 1); copy-out is removed; sharing with other people is deferred, with tracker issues as the interim route.

## Schema

No new tables. Rows 1–3 land in slice 1, row 4 in slice 2 and row 5 in slice 4.

| # | Table | Change |
|---|---|---|
| 1 | `workspaces` | `personal_user_id uuid NULL` referencing `users(id)`; non-null means personal. |
| 2 | `workspaces` | CHECK `personal_user_id IS NULL OR (personal_user_id = owner_user_id AND NOT is_base)`. |
| 3 | `workspaces` | Partial unique index `(org_id, personal_user_id) WHERE deleted_at IS NULL`. |
| 4 | `workspace_vms` | Widen `workspace_vms_stop_reason_check` and `workspace_vms_pending_stop_reason_check` with `owner_removed`. |
| 5 | `session_inputs` | `start_consent_id uuid NULL`. It cannot be a foreign key: a Start consent is the `id` inside the JSONB `workspace_vms.start_intent`. |

Also:
- A companion trigger beside migration 039's kind-immutability trigger fixes `personal_user_id`.
- A read-only migration step warns about live workspaces and pending fork claims named `me`, `users` or `users/…`.
- `default_personal_idle_timeout_secs` (1800) is a new key in the existing `configs` table, behind the typed admin-settings owner.

Each personal workspace gets ordinary rows in `repos`, `workspaces`, `workspace_repos` (one row, `role='code'`, `root_path='personal'`) and `workspace_settings`. Reference checkouts and handles are not stored.

## Implementation slices

Each slice is one PR, with ontology metadata updated as it lands.

1. **Kind and naming.**
   - Migration: `personal_user_id`, CHECK, unique index, trigger extension.
   - `WorkspaceKind` in the model and DTO; reserved names and prefix, server and client.
   - Every refusal guard and `WorkspaceIsPersonal`.
   - No creator yet, so behaviour is unchanged except reservations and new guards. Tests use a fixture row.
2. **Creation and retirement.**
   - `PersonalWorkspaceService::ensure` and its callers; the reconciler and backfill.
   - Personal idle default.
   - Retirement in remove and demote, plus the tombstone mark/cleanup split.
   - `GET /orgs/:org/me` and `/users/:handle`.
3. **Runtime.**
   - Reference derivation at Start intent; the boot-spec field; host materialization.
   - Dispatcher layout; VM Git read admission; image pins; launch cwd; harness guidance.
4. **Start and send.** Consent-bound input, withdrawal on consent end, and the route.
5. **Frontend.** Live-gate and chat extraction, Home panel, personal workbench presentation, routes, filters.
6. **Mount.** Mount derivation at Start, the read and push rules, fast-forward receive, the personal VM's follow, harness guidance, and removing copy-out (route, service, dialog, entry points).

Delegated read is deferred and has no slice.

Measure Start latency with and without reference checkouts before enabling Start and send by default. No baseline exists today.

## Tests

Following [implementation testing](../../../.agents/skills/ontology-planning/references/implementation-testing.md). Each acceptance criterion maps to an API integration test (the g5-api `integration` target, selected by module prefix), unless marked as a host test or an E2E flow.

| Area | Tests |
|---|---|
| Kind and naming | Every refused operation, including an ordinary workspace named `users/x`. Reserved names at fork admission. Migration 110 applied by the production runner over a populated tenant, with no rows rewritten. |
| Lifecycle | `ensure` idempotence under concurrent calls for the same user, and for different users with the same email local part. Creation after org creation, invitation acceptance, OAuth auto-accept and promotion. A reconciler retry after an injected branch-initialization failure. |
| Retirement | Remove and demote with a running personal VM: the change commits, the VM stops, cleanup finishes. Remove still refuses with an owned working workspace. |
| Access | Owner and admin read at the API and Git (smart-HTTP `info/refs`); another member is refused at both. |
| Runtime (host tests, `process_host.rs`) | Reference checkouts at Base heads. An edit is discarded at the next Start. A new Base commit appears. Flush and restore capture only the private repo. A push to a reference repo is refused. Memory binds in the private repo. A pushed artifact survives SIGKILL of the VM; an unpushed one does not. |
| Start and send | Queued with consent. Withdrawn on cancel, on Stop superseding and on capacity-wait expiry. Delivered after ready. |
| Mount | Start spec carries the mount on the personal branch at its head, with no role; omitted without a personal workspace and on a root collision; image selection unchanged. The working incarnation reads and pushes the personal branch by real `git push`; refused: another user's personal repo, another branch, branch creation, a non-fast-forward with an exact lease, a starting or dirty-stopped personal workspace. Pushes land while the personal VM runs and while it is stopped clean, and the next personal Start restores from the new head. g5-scm: `fast_forward_only` receive; tracking refresh fast-forwards a clean checkout, leaves local commits and overlapping edits alone. Dispatcher: `follow_branch` reaches the refresh request. Harness: mount guidance on non-personal launches. |
| Frontend | Vitest for the Home panel gate states with the VM stopped, the personal presentation's absent controls, and the filters. A Playwright flow: Home side-panel Chat → Start and send → the message queues and the VM leaves Stopped. |

## Assumptions and risks

- **Workspace lists.** A workspace with no docs repo (`docs_repo_id` NULL) must not break list and detail DTOs or the docs features the personal presentation hides. Slice 1 audits the `docs_repo_id` readers reachable from personal routes, and they must refuse cleanly.
- **Stop ordering.** `stop_as_system` must work for a tombstoned workspace whose owner membership is gone, and the reconciler must not delete `workspace_vms` before host release.
- **Base members change between Start intent and boot.** The pinned commits are already frozen in the boot spec, so the boot uses what was admitted.
- **Admin access disclosure.** Admins reading a personal workspace see its full conversations through the existing audience rule. The product notice is the only mitigation the issue asks for.
