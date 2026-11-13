# Retained workspace roots: build the warm cache or stop keeping roots

After a workspace's VM stops, its worker host keeps the workspace's `/work` directory (`<workspaces_root>/<workspace_id>`) for `retain_hours` (default 24). No Start has ever read it back: every Start builds a fresh tree, swaps it in and deletes the old one ([request](../../chat_intent/retained-roots/request.md)). The kept directories still use host disk, steer placement, and drive purge, receipt and recovery code. This plan compared two ways out; the user chose option B, removing retention, and asked to implement and ship it ([request](../../chat_intent/retained-roots/request.md)).

## Ontology changes and governance

**Comparison base:** `origin/main` at `dcd13c54`. **Branch:** `zcl/retained-roots`.

**Changes (option B):**

- **Amended** `g5.runtime.placement`: places on the least-occupied ready host with a free slot; no host is preferred for a workspace.
- **Reworded** `g5.runtime.materialization`, now "Every Start builds a fresh workspace root": states today's behavior (fresh tree from the host repository cache, then the last complete receipt; nothing from an earlier tree is reused). Lineage records 7577fb45.
- **Repurposed** `g5.runtime.purge`, now "Delete a workspace root when its VM is released": after exit proof the host moves the root aside and deletes it; failures are retried and host start sweeps leftovers; release, transfer and Delete do not wait. Replaces purge generations and R02's false-purge concern.
- **Retired** `g5.runtime.purge.cache` ("Treat retained roots as disposable caches").
- **New** `g5.scope.warm-roots`, "Keep no warm workspace roots", in `scope.g5.md` (scope owns removals), with the evidence and the condition for bringing warm caching back.
- **Reworded** `g5.git.restore`, now "Restore starts from a fresh tree" (there is no warm restore), and `g5.git.restore.proof` ("the host repository cache never substitutes for restore-source proof").
- **Amended** `g5.lifecycle.transfer`, now "Transfer changes authority only after clean release": no root purge; the recipient's VM never sees the former owner's VM disk or root. **Amended** `g5.lifecycle.delete`: waits for compute release instead of compute purge.
- **Links and metadata only:** `worker_host.g5.md` (no `purge` operation), `workspace.g5.md`, `git_governance.g5.md`, `workspaces.g5.md`, `provider_accounts.g5.md` (credentials live in the VM's own `/home/agent`, not in `/work`), `documents.g5.md`, `system_invariants.g5.md`. No new broken anchors (the 10 broken anchors already on `origin/main` are unchanged).

The full diff, metadata included, is at the end of this file.

### Relevant and governing ontology

| Block | Why relevant | Implementation must enforce or consider |
|---|---|---|
| [`g5.runtime.materialization`](../../ontology/software_architecture/runtime.g5.md#every-start-builds-a-fresh-workspace-root) | The round-6 security rule: host Git never runs in guest-written content | Keep the fresh build at every Start; root deletion never runs Git or follows guest symlinks |
| [`g5.runtime.custody`](../../ontology/software_architecture/runtime.g5.md#release-capacity-only-with-exit-proof) | Deletion must not race a live VMM | Delete only after reap or other exit proof; an uncertain child keeps its root |
| [`g5.runtime.control.versions`](../../ontology/software_architecture/runtime.g5.md#negotiate-control-and-vm-spec-versions-separately) and [`g5.runtime.control.sizing-transition`](../../ontology/software_architecture/runtime.g5.md#preserve-custody-through-the-measured-sizing-rollout) | Every host-control struct uses `deny_unknown_fields`; QA pins hosts at or before the API | Keep `workspace_root` in `VmSpec`, `retained_roots` in hello/heartbeat and the `roots.purge` method as inert wire fields until a later protocol/VM-spec version drops them |
| [`g5.lifecycle.transfer`](../../ontology/software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release) and [`g5.lifecycle.delete`](../../ontology/software_architecture/lifecycle.g5.md#deletion-and-offboarding-preserve-outstanding-custody) | Both wait for `purge_retained_roots` today | Remove the wait; transfer still completes stop and release first |
| [`g5.git.restore`](../../ontology/software_architecture/durability.g5.md#restore-starts-from-a-fresh-tree) | Restore semantics stay identical | No restore behavior change; tests for dirty/foreign/detached restore keep passing |
| [`g5.quality.migrations`](../../ontology/concepts/system_design_principles.g5.md#preserve-deployed-data-through-forward-migrations) | Columns `workspace_vms.retained_root_*`, `worker_hosts.retained_roots`, `workspace_settings.retain_hours` | Stop reading and writing them now; drop them in a follow-up forward migration, because the Helm pre-upgrade migration job runs while the previous API, which writes them, still serves |

## Options

**A. Build the designed warm cache.** Design v7 §5.5 and §6.3, v9 §6.3 and MVP plan P4.4b describe a kept root as a cache of ignored files and Git objects. A safe version would keep today's fresh build, then move ignored paths that the restored tree lacks from the old tree into the new one, without running Git in the old tree and without following its symlinks. Retention, placement preference and transfer purge all stay, because ignored files can hold an owner's secrets (for example an ignored `.env`). It also needs the eviction under disk pressure that v7 described and nobody built.

**B. Stop keeping roots.** The host deletes a workspace's root when its VM is released. Remove the retention state, the placement preference, the `retain_hours` setting, and the purge waits in transfer and Delete. Every Start already builds a fresh root, so users see no change.

| | A. Warm cache | B. No retained roots |
|---|---|---|
| Round-6 security rule (no host Git in guest-written trees) | Must be kept by a new carry-over step that touches guest-written files; new attack surface (symlinks, special files, ownership) | Kept trivially: nothing from an old tree is read |
| Warm/cold equivalence | Tracked and non-ignored content identical; ignored files differ by design | Only one kind of restore |
| Disk | Keeps exactly the data that filled hosts: the 116–119 GB root on 2026-10-07 was `target/` (ignored). Needs eviction under pressure to be safe | A stopped workspace frees its disk at release |
| Restart time | Saves rebuilding ignored output when the restart lands on the same host while the root lives. Measured on an Apple M5 (10 cores): cold `cargo build --workspace` 205 s, unchanged rebuild 0.6 s, `target/` 6.6 GB. A 4-vCPU worker VM is slower; not measured there | Same as today: VM boot to ready 12.8–13.2 s (two live QA boots); build output rebuilt on demand |
| How often the warm path would hit | Only same-host restarts within the host's life. QA hosts since 2026-10-01: org g5 n=37, median life 2.2 h, p75 5.9 h, max 22.7 h (zcl similar). Rotation retires a host whose only workspaces are stopped | Not applicable |
| Transfer and Delete | Must keep purge before owner change (ignored secrets) | No purge step; the recipient's Start builds a fresh root and provider credentials live on the VM's own disk |
| State and code | Keeps host retained map, receipts, recovery, `roots.purge`, placement preference, 3 DB columns, the setting; adds carry-over and eviction | Removes all of these; adds root deletion at release and a startup sweep |
| Existing defects | `retain_hours = 0` leaks the root (neither kept nor deleted); API accepts up to 8,760 h but the host refuses any start over 720 h. Both need fixing | Both disappear with the setting |

**Recommendation: B.** A buys a benefit that QA would rarely see (hosts live about two hours) at the cost of more state, a new security-sensitive step and the disk problem that already caused an outage. B matches the cleanest-design preference and changes nothing users can observe except freed disk. If restart speed matters later, it can return as a measured design (scope block `g5.scope.warm-roots` states the condition).

## Design (option B)

```text
Start ─▶ host: build fresh root from bare cache ─▶ restore last receipt ─▶ mount /work ─▶ VM runs
                                                                                             │
VM release (stop, kill, crash, boot failure) ─▶ drop the VM record ─▶ claim unheld roots ─▶ report Exited
                                                                     (rename to .trash-<uuid>
                                                                      under the state lock)
                                          background delete of .trash-* (retried) ◀──────────┘
host start ─▶ claim every root, .<id>.staging-*, .<id>.previous-*, .g5-retained (no VM survives a restart)
```

The host owns the root's whole life. One routine, `RootReaper`, runs after every release and at host start. Under the runtime state lock (the lock that admits a Start), it renames every host-created entry whose workspace has no VM record to `.trash-<uuid>`. A Start admitted afterwards therefore always builds a fresh tree, and a booting or running VM's root is never touched, because its record exists from admission. A background pass, one at a time, deletes the claimed trees; it never runs Git and does not follow symlinks. A failed claim or deletion is logged and retried at the next release, at a disk refusal (so leftover trash cannot keep refusing Starts), or at host start. The sweep only touches names the host itself creates in `workspaces_root` (canonical workspace ids, their staging/previous copies, the legacy `.g5-retained`), never other contents and never the repository cache (`/var/g5d-data/cache/repos`). The API no longer tracks, prefers or purges roots: placement orders by occupancy then host age; transfer and Delete wait only for stop and release.

**Compatibility.** Host-control messages reject unknown fields and QA hosts can run an older worker image than the API. So this change keeps three wire fields inert:
- the API still sends `VmSpec.workspace_root` with `retain_hours: 24` and the lifecycle generation, which new hosts ignore;
- the API still accepts `retained_roots` in hello and heartbeat, and new hosts send an empty list;
- new hosts still answer `roots.purge` with `NotFound`.

Old hosts keep today's behavior until rotation replaces them. A later change removes the fields with a protocol/VM-spec version bump.

**Data.** This change stops reading and writing `workspace_vms.retained_root_host_id`, `workspace_vms.retained_root_cache_generation`, `worker_hosts.retained_roots` and `workspace_settings.retain_hours`, and leaves the columns in place. The Helm pre-upgrade migration job runs while the previous API still serves and writes them, so dropping them in the same release would break that API's heartbeat, placement and release SQL for the length of the rollout. A follow-up forward migration drops them once this API is deployed. The settings API stops returning `retainHours` and ignores it in requests; the frontend schema (`frontend/src/lib/api-schemas.ts`, `types.ts`, mocks) drops it in the same change. No UI control edits it today.

**Code removed or changed:** host `RetainedRoot` state, `persist_retained_root`, `recover_retained_roots`, `prune_expired_roots`, `purge`, `retained_metadata` and the `host-root.json`/`.g5-retained` files; `warm_root` in the boot summary (old hosts' extra field is ignored when parsed); host qualification checks of retained roots; API `RETAIN_ROOT_FROM_BOOT_SPEC` and the release SQL that set the retained-root columns; `purge_retained_roots` and its callers in transfer and the delete route; the placement `retained_root` column and `PlacementReservation.retained_root`; storing host-reported roots on hello and heartbeat; `retain_hours` in settings, fork settings copy and spend defaults; the e2e boot-timing scenario's warm run (now a restart run); `docs/codex-device-auth.md` and `docs/host-kvm-qualification.md`.

## Acceptance outcomes

1. After any VM release (graceful stop, kill, natural exit, boot failure after the root was built), the workspace's root no longer exists under its workspace path, and its renamed copy is deleted.
2. A Start right after a stop of the same workspace on the same host succeeds and sees only restored content (a test that stops and restarts while deletion is slow or failing).
3. Host start removes leftover roots and staging/previous/trash directories, and leaves the repository cache and unrelated files alone.
4. Placement no longer depends on which host last ran the workspace.
5. Transfer and Delete complete after stop and release without any purge command.
6. Mixed versions: a new host accepts an old API's `VmSpec` and `roots.purge`; a new API accepts an old host's hello/heartbeat with retained roots.
7. Existing restore tests (dirty, foreign/detached, binary, sequencer, null receipt) pass unchanged.

## Risks and assumptions

- **Deletion cost on release.** A 100+ GB `target/` takes time to delete; the rename keeps it off the release path. Assumption to verify: rename within `workspaces_root` is atomic on the host's XFS volume.
- **Leftovers after a failed deletion** stay on the organization's own host disk until the next sweep or host termination. No guest can mount them. This matches today's "no physical secure erasure" stance. The sweep only matches the exact names the host writes (canonical workspace ids and canonical UUID suffixes), so entries an operator leaves in `workspaces_root` survive.
- **Old hosts during rollout** still keep roots for 24 h and are no longer purged on transfer or Delete. Their roots are never mounted into another VM (fresh build), and rotation replaces old-image hosts.
- **Columns kept until a follow-up.** Rolling back to the previous API stays possible because nothing is dropped; that API would simply resume writing retained-root columns and purging on transfer and Delete.
- **Custody paths (verified in code):** every path that removes a VM record (stop, the exit monitor, `settle_natural_exit`, boot failure and cancellation; reconcile and shutdown go through stop) calls the reaper afterwards. Boot failure and late boot cancellation remove the record without guaranteed reap proof; the root stays held until a kill proves reap (review: Codex P1, CodeRabbit). Their slot and workspace exclusion are still released without proof, the existing custody gap.

## Review summary

- Checked against the custody rule: a root is claimed only once its VM record is gone and no unreaped VMM holds it. Stop and natural exit remove the record after reap proof. Boot failure and late boot cancellation can remove the record without that proof (the existing custody gap above); they then hold the root, per VM (`unreaped`), until that VM's kill proves reap, a later Start of the workspace replaces the tree, or the host restarts. A running or booting VM with a record is never touched.
- Checked against transfer: provider credentials are in `/home/agent` on the per-VM disk, deleted after VM exit (`backend/g5d-dispatch/src/git.rs`, `backend/g5d-isolation/src/isolation/microvm.rs`), so transfer never depended on the workspace root for credential safety. Ignored files in `/work` (such as an ignored `.env`) are the remaining owner data; B deletes them at release, and no recipient VM can mount them in either case.
- Checked against H013: the human chose not to persist ignored caches in durable saves; B is consistent, and A does not contradict it either (a host-local cache is not a save).
- Remaining uncertainty: restart-time cost on a 4-vCPU worker VM was not measured; the Mac figure is an upper bound of what A could save per same-host restart.

<details>
<summary>Ontology diff (option B), metadata included</summary>

```diff
diff --git a/.g5/ontology/concepts/system_invariants.g5.md b/.g5/ontology/concepts/system_invariants.g5.md
index 82c25bf0..80f66048 100644
--- a/.g5/ontology/concepts/system_invariants.g5.md
+++ b/.g5/ontology/concepts/system_invariants.g5.md
@@ -17,7 +17,7 @@ identifier never grants access, including through privileged operations.
 
 Only the current active owner may mutate or use VM files, logs, execution, terminal or preview; administrator status, creation, invitation and past ownership confer no bypass.
 
-[Transfer](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge) is the explicit authority change.
+[Transfer](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release) is the explicit authority change.
 
 ## Compute requires explicit Start consent
 
diff --git a/.g5/ontology/features/documents.g5.md b/.g5/ontology/features/documents.g5.md
index 2f93ff2a..a538e7f1 100644
--- a/.g5/ontology/features/documents.g5.md
+++ b/.g5/ontology/features/documents.g5.md
@@ -88,7 +88,7 @@ Governing contracts: [open FILES files for reading](documents.g5.md#open-files-f
 Default durable browsing uses a complete saved receipt only while its observed member
 branch heads still match the complete current vector; otherwise select the complete
 published branch cut. Explicit published views select branch refs. Browsing must not
-rewrite the separate [recovery receipt](../software_architecture/durability.g5.md#warm-and-cold-restore-mean-the-same-thing).
+rewrite the separate [recovery receipt](../software_architecture/durability.g5.md#restore-starts-from-a-fresh-tree).
 
 <a id="g5.story.know-which-version-i-am-seeing"></a>
 ### Label the source being shown
@@ -412,7 +412,7 @@ Governing concepts: Repository content follows [Git authority](../concepts/data_
 
 Only the current workspace owner starts, resolves, reopens or deletes a thread — any thread in the workspace, whoever wrote it. A person edits only messages they wrote. These are durable record actions that need no running VM; a read-only workspace, [Base](workspaces.g5.md#keep-base-workspaces-as-protected-readers) included, accepts none.
 
-This applies [durable-action admission](../concepts/security.g5.md#admit-actions-against-current-authority) and the [write grant](../concepts/security.g5.md#separate-membership-from-write-permission); [transfer](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge) moves this authority with ownership. [Replies](#let-readers-reply-in-threads) follow their own rule.
+This applies [durable-action admission](../concepts/security.g5.md#admit-actions-against-current-authority) and the [write grant](../concepts/security.g5.md#separate-membership-from-write-permission); [transfer](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release) moves this authority with ownership. [Replies](#let-readers-reply-in-threads) follow their own rule.
 
 ### Let readers reply in threads
 
diff --git a/.g5/ontology/features/provider_accounts.g5.md b/.g5/ontology/features/provider_accounts.g5.md
index f084089f..2758bba9 100644
--- a/.g5/ontology/features/provider_accounts.g5.md
+++ b/.g5/ontology/features/provider_accounts.g5.md
@@ -166,7 +166,7 @@ Stop, owner interruption or VM loss ends pending authorization and cleans its lo
 
 Native subscription credentials remain in the owner VM home with restrictive permissions.
 
-[Transfer purge](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge) owns release before an ownership change.
+[Transfer release](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release) owns release before an ownership change.
 
 ### Exclude credentials from conversation recovery bundles
 
@@ -334,7 +334,7 @@ This developer-desktop trust model provides neither a credential broker nor isol
 
 - agent
 - From [Codex device authorization](../../../docs/codex-device-auth.md) and [S018–S021](../../uplift/g5-lean/candidates/supporting-docs.md).
-- ~75% · native auth file mode and owner-home login guards; failed retained-root deletion can forget pending inventory and falsely report purge completion.
+- ~75% · native auth file mode and owner-home login guards; the login lives in `/home/agent` on the VM's own disk, which is deleted after VM exit (`backend/g5d-isolation/src/isolation/microvm.rs`, per-VM rootfs removal).
 
 ### g5.provider.credential-files.harvest
 
@@ -352,4 +352,4 @@ This developer-desktop trust model provides neither a credential broker nor isol
 
 - agent
 - From [Codex device authorization](../../../docs/codex-device-auth.md) and [S018–S021](../../uplift/g5-lean/candidates/supporting-docs.md).
-- ~90% · owner VM `CODEX_HOME` and native credential storage; same-user access is intentional, and transfer safety still depends on retained-root purge whose failure inventory is incomplete.
+- ~90% · owner VM `CODEX_HOME` and native credential storage; same-user access is intentional; `CODEX_HOME` is `/home/agent/.codex` on the VM's own disk (`backend/g5d-dispatch/src/git.rs`), deleted after VM exit, so transfer safety does not depend on the workspace root.
diff --git a/.g5/ontology/features/workspaces.g5.md b/.g5/ontology/features/workspaces.g5.md
index 8640e646..9bb3994f 100644
--- a/.g5/ontology/features/workspaces.g5.md
+++ b/.g5/ontology/features/workspaces.g5.md
@@ -56,7 +56,7 @@ Clients remain responsible for safe-lease use under the [Git write boundary](../
 
 As an owner, I can stop my workspace and later start it from its last complete durable state.
 
-Governing contracts: [stop fences future work before releasing custody](../software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody), [warm and cold restore mean the same thing](../software_architecture/durability.g5.md#warm-and-cold-restore-mean-the-same-thing).
+Governing contracts: [stop fences future work before releasing custody](../software_architecture/lifecycle.g5.md#stop-fences-future-work-before-releasing-custody), [restore starts from a fresh tree](../software_architecture/durability.g5.md#restore-starts-from-a-fresh-tree).
 
 ### Always be able to stop
 
@@ -74,7 +74,7 @@ As an owner or org admin, I can transfer a workspace to an eligible teammate wit
 
 Governing concepts: [Owner eligibility](../concepts/security.g5.md#require-an-eligible-workspace-owner) and [credential ownership](../concepts/security.g5.md#dispatch-credentials-to-the-intended-owner-environment) govern transfer.
 
-Governing contracts: [transfer changes authority only after clean release and purge](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge).
+Governing contracts: [transfer changes authority only after clean release](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release).
 
 ### Remove a workspace
 
@@ -118,7 +118,7 @@ Governing contracts: [give each stop a bounded save window](../software_architec
 
 As a new owner, I can resume transferred work using my own account.
 
-Governing contracts: [transfer changes authority only after clean release and purge](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release-and-purge), [freeze the current owner account at launch](provider_accounts.g5.md#freeze-the-current-owner-account-at-launch).
+Governing contracts: [transfer changes authority only after clean release](../software_architecture/lifecycle.g5.md#transfer-changes-authority-only-after-clean-release), [freeze the current owner account at launch](provider_accounts.g5.md#freeze-the-current-owner-account-at-launch).
 
 ### Reboot onto the selected image
 
@@ -584,13 +584,13 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 
 - human-verified
 - From [H072/H104](../../uplift/g5-lean/candidates/human-decisions.md), [v9](../../../docs/lean-backend-design-v9.md).
-- ~75% · workspace_vm_service.rs, useWorkspace.ts, host retained-root cleanup; stop/transfer/purge sequencing exists; a failed retained-root deletion can lose pending inventory, so end-to-end credential custody is not fully supported.
+- ~75% · workspace_vm_service.rs, useWorkspace.ts; stop/transfer sequencing exists; transfer waits for stop and release only since the [retained-roots decision](../../chat_intent/retained-roots/request.md).
 
 ### g5.story.remove-a-workspace
 
 - human-verified
 - From [H058](../../uplift/g5-lean/candidates/human-decisions.md), [v9](../../../docs/lean-backend-design-v9.md).
-- ~80% · routes/workspaces.rs, workspace_service.rs, RemoveWorkspaceDialog.tsx; stop/tombstone/ref cleanup exists; retained-root purge failure and all dependent retention/recovery paths remain gaps or unassessed.
+- ~80% · routes/workspaces.rs, workspace_service.rs, RemoveWorkspaceDialog.tsx; stop/tombstone/ref cleanup exists; the route waits for the stop only since the [retained-roots decision](../../chat_intent/retained-roots/request.md); dependent recovery paths remain unassessed.
 
 ### g5.story.share-read-access
 
@@ -614,7 +614,7 @@ The workspace switcher lists other review copies nowhere, but the copy currently
 
 - human-verified
 - From [human decisions](../../uplift/g5-lean/candidates/human-decisions.md) H072/H104.
-- ~75% · workspace_vm_service.rs, agent_dispatch.rs; transfer gates and new-owner credential resolution exist; failed retained-root deletion can lose cleanup inventory and weaken credential custody.
+- ~75% · workspace_vm_service.rs, agent_dispatch.rs; transfer gates and new-owner credential resolution exist; provider credentials live in the VM's own `/home/agent`, which is deleted with the VM, and the recipient's Start builds a fresh root.
 
 ### g5.story.reboot-selected-image
 
diff --git a/.g5/ontology/scope.g5.md b/.g5/ontology/scope.g5.md
index 59c0d984..ebec349c 100644
--- a/.g5/ontology/scope.g5.md
+++ b/.g5/ontology/scope.g5.md
@@ -66,6 +66,12 @@ Keep no task Pause control; a lifecycle hold is not a Pause feature.
 
 Remove the IDE mockup, hidden notification settings and takeover remnants.
 
+### Keep no warm workspace roots
+
+[D · g5.scope.warm-roots]
+
+A stopped workspace keeps no root on its worker host, and placement prefers no host for a workspace. No Start has ever reused a kept root, worker hosts are replaced within hours, and kept build output filled host disks. Warm caching returns only with a measured restart benefit and a design that keeps [every Start on a fresh root](software_architecture/runtime.g5.md#every-start-builds-a-fresh-workspace-root).
+
 ## Deferred work and conditional proposals
 
 ### Preserve explicit later work
@@ -255,6 +261,12 @@ Current deferral follows the [erasure gate](#decide-erasure-policy-before-enabli
 - From [H049/H058](../uplift/g5-lean/candidates/human-decisions.md).
 - Unassessed · these specific UI remnants need a complete removal audit.
 
+### g5.scope.warm-roots
+
+- human-verified
+- [Retained-roots decision](../chat_intent/retained-roots/request.md). Warm reuse existed only in PR #117's early commits and was removed by 7577fb45 before merge; retention, placement preference and purge stayed. QA worker hosts created since 2026-10-01 lived a median 2.2 hours (37 hosts of org g5, longest 22.7 h, GCE audit log). A 116 GB `target/` in a kept root blocked starts on 2026-10-07. Consistent with [H013](../uplift/g5-lean/candidates/human-decisions.md): ignored caches are not persisted.
+- Implemented, locally validated · see [`g5.runtime.purge`](software_architecture/runtime.g5.md#delete-a-workspace-root-when-its-vm-is-released) and [`g5.runtime.placement`](software_architecture/runtime.g5.md#placement-and-host-admission-have-distinct-owners); the `retain_hours` workspace setting is gone from the API and UI types. Kept for compatibility: `VmSpec.workspace_root` (sent as 24 h for hosts from before this change), hello/heartbeat `retained_roots` (always empty) and `roots.purge` (`not_found`) in `backend/g5-internal-api/src/host_ctl.rs`, to be dropped with the next protocol/VM-spec version; the columns `workspace_vms.retained_root_host_id`, `workspace_vms.retained_root_cache_generation`, `worker_hosts.retained_roots` and `workspace_settings.retain_hours` are no longer read or written and wait for a follow-up migration, because the pre-upgrade migration job runs while the previous API still serves.
+
 ### g5.scope.later-work
 
 - human-verified
diff --git a/.g5/ontology/software_architecture/durability.g5.md b/.g5/ontology/software_architecture/durability.g5.md
index 68c1f4af..189b6625 100644
--- a/.g5/ontology/software_architecture/durability.g5.md
+++ b/.g5/ontology/software_architecture/durability.g5.md
@@ -123,11 +123,11 @@ Incomplete capture refs age after 24 hours, while ref removal alone does not est
 
 [Serialized repository GC](../concepts/operations.g5.md#serialize-repository-garbage-collection) remains later work.
 
-## Warm and cold restore mean the same thing
+## Restore starts from a fresh tree
 
 [D · g5.git.restore]
 
-Warm and cold restore use [host-owned materialization](runtime.g5.md#host-git-never-trusts-retained-guest-control-files) and exact pinned boot OIDs, keeping mapped tip, foreign/detached checkout and unstaged snapshot distinct.
+Restore runs on a [freshly built root](runtime.g5.md#every-start-builds-a-fresh-workspace-root) at exact pinned boot OIDs, keeping mapped tip, foreign/detached checkout and unstaged snapshot distinct.
 
 ## Restore without stale local Git operation state
 
@@ -139,13 +139,13 @@ Clear old sequencer state and extra nonignored files without changing remote-tra
 
 [D · g5.git.restore.proof]
 
-Missing refs fail attach, and a retained cache never substitutes for restore-source proof.
+Missing refs fail attach, and the host repository cache never substitutes for restore-source proof.
 
 ## Advance a wholly clean stopped workspace after landing
 
 [D · g5.git.clean-advance]
 
-For [restore](#warm-and-cold-restore-mean-the-same-thing), a newer authoritative head may supersede the whole old receipt only when every member is fully published, clean, on its mapped branch and has no sequencer. Any dirty/unpushed/foreign member retains the whole vector. This avoids restoring stale pre-merge content without rewriting receipt evidence.
+For [restore](#restore-starts-from-a-fresh-tree), a newer authoritative head may supersede the whole old receipt only when every member is fully published, clean, on its mapped branch and has no sequencer. Any dirty/unpushed/foreign member retains the whole vector. This avoids restoring stale pre-merge content without rewriting receipt evidence.
 
 ## Git network writes are bounded by ownership and CAS
 
@@ -318,7 +318,7 @@ Native Git must exclude runtime/cache/session/layout and staging artifacts, whil
 
 - agent
 - From [C060–C061](../../uplift/g5-lean/candidates/contracts.md).
-- Scoped evidence · host spec/receipt selection calls `g5-scm::native::restore_checkout` after fresh materialization. Host tests and independent real-Git probes preserve old mapped tip, foreign/detached HEAD and large binary unstaged content despite newer fetched tracking refs. Full VM boot and the complete warm/cold failure matrix remain unexecuted. [Review evidence](../../ontology-review-git-abstraction.md).
+- Scoped evidence · host spec/receipt selection calls `g5-scm::native::restore_checkout` after fresh materialization. Host tests and independent real-Git probes preserve old mapped tip, foreign/detached HEAD and large binary unstaged content despite newer fetched tracking refs. Full VM boot and the complete restore failure matrix remain unexecuted. [Review evidence](../../ontology-review-git-abstraction.md).
 
 ### g5.git.restore.cleanup
 
diff --git a/.g5/ontology/software_architecture/git_governance.g5.md b/.g5/ontology/software_architecture/git_governance.g5.md
index 26eadf82..47a8583b 100644
--- a/.g5/ontology/software_architecture/git_governance.g5.md
+++ b/.g5/ontology/software_architecture/git_governance.g5.md
@@ -212,7 +212,7 @@ Platform operations never write global or system Git config. Installing persiste
 configuration and the owner's author identity ([shared native identity](durability.g5.md#keep-platform-artifacts-outside-native-git))
 are declared runner operations; other settings, such as a flush's temporary index, apply
 only to a single invocation.
-Host materialization keeps [ignoring retained guest control files](runtime.g5.md#host-git-never-trusts-retained-guest-control-files).
+Host materialization keeps [building a fresh root at every Start](runtime.g5.md#every-start-builds-a-fresh-workspace-root).
 The guest context takes its command path and loader environment from the
 [workspace CLI runtime setup](runtime.g5.md#make-the-workspace-cli-available-to-people-and-agents)
 rather than composing its own.
diff --git a/.g5/ontology/software_architecture/lifecycle.g5.md b/.g5/ontology/software_architecture/lifecycle.g5.md
index b2604e43..0d070048 100644
--- a/.g5/ontology/software_architecture/lifecycle.g5.md
+++ b/.g5/ontology/software_architecture/lifecycle.g5.md
@@ -129,11 +129,11 @@ Attach session notices to later ordinary input without starting work or injectin
 
 Under the retained [MVP daily rotation](runtime.g5.md#keep-the-mvp-host-credential-and-rotation-model), cloud drain provisions replacement host capacity before stopping, flushing and releasing old VMs.
 
-## Transfer changes authority only after clean release and purge
+## Transfer changes authority only after clean release
 
 [D · g5.lifecycle.transfer]
 
-A durable expected-owner transfer changes [owner authority](../concepts/security.g5.md#one-eligible-owner-uses-live-compute) only after rechecking recipient eligibility and completing stop, release and credential-root purge.
+A durable expected-owner transfer changes [owner authority](../concepts/security.g5.md#one-eligible-owner-uses-live-compute) only after rechecking recipient eligibility and completing stop and release. The recipient's VM never sees the former owner's VM disk or workspace root, because [every Start builds a fresh root](runtime.g5.md#every-start-builds-a-fresh-workspace-root). A release by lease expiry does not prove exit ([lease replacement](runtime.g5.md#separate-authority-expiry-from-proof-of-exit)): an unreachable host may keep running the old VM, without authority, until it reconnects and kills it.
 
 ## Preserve administrator transfer of a base workspace
 
@@ -163,7 +163,7 @@ Authorized administrator transfer records audit/notices, and the recipient expli
 
 [D · g5.lifecycle.delete]
 
-Delete tombstones the workspace and waits for compute purge and read/fork/restore pins before cleanup, respecting [unfinished fork claims](../features/workspaces.g5.md#unfinished-forks-own-durable-claims).
+Delete tombstones the workspace and waits for compute release and read/fork/restore pins before cleanup, respecting [unfinished fork claims](../features/workspaces.g5.md#unfinished-forks-own-durable-claims).
 
 ## Resolve owned workspaces before removing the owner
 
@@ -260,8 +260,8 @@ When an owner's Start brings the workspace back to ready, should a job's agent a
 ### g5.lifecycle.stop.release
 
 - agent
-- From [C065/C069–C070](../../uplift/g5-lean/candidates/contracts.md). The two host shutdown modes and the unreachable-host boundary come from the [stop-always-completes request](../../chat_intent/nuclear-stop/request.md); nothing required remains in the guest after a complete save, and [retained roots are disposable caches](runtime.g5.md#treat-retained-roots-as-disposable-caches).
-- ~75% · `workspace_vm_service.rs::stop_with_authority` sends `vm.stop` graceful with `CLEAN_STOP_GRACE_SECS` (30 s) after a complete save and kill otherwise, each under its own idempotency key (`stop:<op>:host` and `stop:<op>:host-kill`) because the host binds a key to its payload; the host answers `Busy` while an earlier stop of the same VM is still running. `backend/g5-api/tests/api_workspace_vms.rs`: `workspace_vm_stop_then_resume_flushes_and_starts_a_new_incarnation` checks the graceful 30 s command, the forced-stop tests the kill. Only the API replica holding the host's tunnel can send the kill, so a replica without it yields the stop, also past the window, and leaves it due at once (`stop_with_authority`, `defer_stop_to_tunnel_owner`; `::expired_stop_on_a_replica_without_the_host_tunnel_still_reaches_the_owner`, found by the independent review). Failed boot can still drop unreaped custody and failed root deletion can lose pending purge state. Every VM release clears custody in `backend/g5-api-core/src/services/workspace_vm_release.rs`, and each one that ends a VM's lifecycle (all but a refused placement) calls `task_lifecycle.rs::settle_agents_after_release` in the same transaction. Settlement takes effect on every such release: completed stop, failed start, lease expiry and forced host removal run in an org-scoped transaction, and host-reported releases (inventory missing, exit, out of memory, stale kill, boot failure) run under the [host's organization](runtime.g5.md#host-facts-apply-under-the-hosts-organization). `backend/g5-api/tests/api_worker_hosts.rs` covers forced removal, a reported exit and a hello without the VM.
+- From [C065/C069–C070](../../uplift/g5-lean/candidates/contracts.md). The two host shutdown modes and the unreachable-host boundary come from the [stop-always-completes request](../../chat_intent/nuclear-stop/request.md); nothing required remains in the guest after a complete save, and a stopped workspace [keeps no root](runtime.g5.md#delete-a-workspace-root-when-its-vm-is-released).
+- ~75% · `workspace_vm_service.rs::stop_with_authority` sends `vm.stop` graceful with `CLEAN_STOP_GRACE_SECS` (30 s) after a complete save and kill otherwise, each under its own idempotency key (`stop:<op>:host` and `stop:<op>:host-kill`) because the host binds a key to its payload; the host answers `Busy` while an earlier stop of the same VM is still running. `backend/g5-api/tests/api_workspace_vms.rs`: `workspace_vm_stop_then_resume_flushes_and_starts_a_new_incarnation` checks the graceful 30 s command, the forced-stop tests the kill. Only the API replica holding the host's tunnel can send the kill, so a replica without it yields the stop, also past the window, and leaves it due at once (`stop_with_authority`, `defer_stop_to_tunnel_owner`; `::expired_stop_on_a_replica_without_the_host_tunnel_still_reaches_the_owner`, found by the independent review). Failed boot can still drop unreaped custody; the host then keeps that VMM's root held ([root deletion](runtime.g5.md#delete-a-workspace-root-when-its-vm-is-released)). Every VM release clears custody in `backend/g5-api-core/src/services/workspace_vm_release.rs`, and each one that ends a VM's lifecycle (all but a refused placement) calls `task_lifecycle.rs::settle_agents_after_release` in the same transaction. Settlement takes effect on every such release: completed stop, failed start, lease expiry and forced host removal run in an org-scoped transaction, and host-reported releases (inventory missing, exit, out of memory, stale kill, boot failure) run under the [host's organization](runtime.g5.md#host-facts-apply-under-the-hosts-organization). `backend/g5-api/tests/api_worker_hosts.rs` covers forced removal, a reported exit and a hello without the VM.
 
 ### g5.lifecycle.stop.bundle-exception
 
@@ -320,8 +320,8 @@ When an owner's Start brings the workspace back to ready, should a job's agent a
 ### g5.lifecycle.transfer
 
 - human-verified
-- From [C071–C073](../../uplift/g5-lean/candidates/contracts.md), [H080](../../uplift/g5-lean/candidates/human-decisions.md).
-- ~75% · owner transfer and purge; failed deletion can falsely permit transfer.
+- From [C071–C073](../../uplift/g5-lean/candidates/contracts.md), [H080](../../uplift/g5-lean/candidates/human-decisions.md). The lease-expiry sentence records, after review of PR #365, what removing the root purge newly allows: the purge had also waited for an unreachable host's old VM to go, which authority fencing makes unnecessary.
+- ~75% · owner transfer; `WorkspaceVmService::transfer` (`backend/g5-api-core/src/services/workspace_vm_service.rs`) waits for stop and release only since the [retained-roots decision](../../chat_intent/retained-roots/request.md) (`backend/g5-api/tests/api_workspace_vms.rs`: `workspace_transfer_stops_before_new_owner_may_resume` asserts no `roots.purge`).
 
 ### g5.lifecycle.transfer.base
 
@@ -351,7 +351,7 @@ When an owner's Start brings the workspace back to ready, should a job's agent a
 
 - agent
 - From [C027–C028](../../uplift/g5-lean/candidates/contracts.md).
-- ~70% · workspace deletion and pins; failed root purge can lose pending cleanup state.
+- ~70% · workspace deletion and pins; the delete route (`backend/g5-api-routes/src/routes/workspaces.rs`) waits for the stop only since the [retained-roots decision](../../chat_intent/retained-roots/request.md) (`backend/g5-api/tests/api_workspaces.rs`: `test_delete_workspace_does_not_wait_for_a_reported_root`).
 
 ### g5.lifecycle.delete.owner-removal
 
diff --git a/.g5/ontology/software_architecture/runtime.g5.md b/.g5/ontology/software_architecture/runtime.g5.md
index 5502422f..6e637f7a 100644
--- a/.g5/ontology/software_architecture/runtime.g5.md
+++ b/.g5/ontology/software_architecture/runtime.g5.md
@@ -4,7 +4,7 @@
 
 [D · g5.runtime.placement]
 
-The API places work in operator-provisioned single-organization pools with default quota ten, preferring a retained workspace root before free capacity.
+The API places work in operator-provisioned single-organization pools with default quota ten, on the least-occupied ready host with a free slot; no host is preferred for a particular workspace.
 
 Releasing capacity requires [custody proof](#release-capacity-only-with-exit-proof).
 
@@ -193,23 +193,19 @@ broaden the selected [agent permission policy](../features/agents.g5.md#freeze-p
 network-disabled agents cannot call the API and must report the block for owner-terminal
 execution rather than bypass the sandbox.
 
-## Host Git never trusts retained guest control files
+## Every Start builds a fresh workspace root
 
 [D · g5.runtime.materialization]
 
-Retain useful repository/image caches, but host Git rematerializes from host-owned bare repositories rather than guest-retained hooks/config/attributes. Preserve [warm/cold restore equivalence](durability.g5.md#warm-and-cold-restore-mean-the-same-thing); cache replacement needs measured equivalent performance.
+Each Start builds the workspace root from the host-owned bare repository cache and then [restores the last complete receipt](durability.g5.md#restore-starts-from-a-fresh-tree) into it. Nothing from an earlier incarnation's tree is reused, so host Git never runs in content a guest wrote. The repository cache stays; replacing it needs measured equivalent performance.
 
-## Purge is proved deletion, not forgotten inventory
+## Delete a workspace root when its VM is released
 
 [D · g5.runtime.purge]
 
-Track pending root purge by generation until every required deletion succeeds, including restart and unreachable-host recovery before new admission.
+After [exit proof](#release-capacity-only-with-exit-proof) the host moves the workspace root out of its path and deletes it. A failed deletion is retried, and host start removes every root no active VM holds. Release, transfer and Delete do not wait for the deletion, because no later guest mounts an earlier tree; deletion implies no physical secure erasure.
 
-## Treat retained roots as disposable caches
-
-[D · g5.runtime.purge.cache]
-
-Retained roots normally expire after 24 hours without implying physical secure erasure.
+A stopped workspace [keeps no root](../scope.g5.md#keep-no-warm-workspace-roots).
 
 ## Streams and mutations have different drain lifetimes
 
@@ -235,9 +231,9 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
 
 ### g5.runtime.placement
 
-- agent
-- From [C075/C102](../../uplift/g5-lean/candidates/contracts.md).
-- ~85% · placement service; complete quota/FIFO/fallback matrix unexecuted.
+- human-verified
+- From [C075/C102](../../uplift/g5-lean/candidates/contracts.md); the retained-root preference was removed by the [retained-roots decision](../../chat_intent/retained-roots/request.md).
+- ~85% · placement service; `WorkerHostService::place` (`backend/g5-api-core/src/services/worker_host_service.rs`) orders ready hosts with a free slot by occupancy, then age, and ignores reported roots (`backend/g5-api/tests/api_worker_hosts.rs`: `placement_ignores_reported_roots_orders_by_occupancy_and_enforces_region_quota`). Complete quota/FIFO/fallback matrix unexecuted.
 
 ### g5.runtime.placement.host-slots
 
@@ -315,7 +311,7 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
 
 - human-verified
 - Proposed by the [round-2 leakiness plan](../../pr_ontology_diff_design/abstraction-leakiness-round-2/ontology-plan.md) for the [round-2 request](../../chat_intent/abstraction-leakiness/round-2.md), after the round-1 review found that host-reported releases settle no task or message row; approved by the user on 2026-10-03.
-- Partial · Host control applies `accept_hello` and the VM facts that release a VM (`settles_agents_work`: exit, out of memory, stale kill, boot failure) in the maintenance actor's transaction scoped to the host's organization on the tenant pool, as org-scoped maintenance does (`backend/g5-api-core/src/services/host_control_state.rs::org_scope`, `maintenance::maintenance_org_scope`); other facts stay on the system pool, and the scope ends with the transaction. Ready stays on the system pool, so its release of held tasks still matches no task row, pending [whether Start continues held work](lifecycle.g5.md#decide-whether-start-continues-held-work). Every release that settles Agents work first locks its workspace rows (`KEY SHARE`, in workspace order; `workspace_vm_release.rs::lock_for_release`), and its fenced update then locks only the VM rows it releases. That is the owner operations' order, so a release never holds a VM row while waiting for a workspace row. Retained-root purge, the one path that locked a VM row before its workspace, now takes the workspace row first too (`workspace_vm_service.rs::purge_retained_roots`). API tests in `backend/g5-api/tests/api_worker_hosts.rs`: a reported exit and a hello without the VM cancel the queued preview, settle the preview intent and withdraw the queued notice, and the tenant connection keeps no scope afterwards; races with the owner's cancellation and a held workspace row cover hello, exit and lease expiry, and a delayed exit does not wait for a stopped VM that retained-root purge holds. A replayed fact is acknowledged again but not applied again: recording and applying it commit together.
+- Partial · Host control applies `accept_hello` and the VM facts that release a VM (`settles_agents_work`: exit, out of memory, stale kill, boot failure) in the maintenance actor's transaction scoped to the host's organization on the tenant pool, as org-scoped maintenance does (`backend/g5-api-core/src/services/host_control_state.rs::org_scope`, `maintenance::maintenance_org_scope`); other facts stay on the system pool, and the scope ends with the transaction. Ready stays on the system pool, so its release of held tasks still matches no task row, pending [whether Start continues held work](lifecycle.g5.md#decide-whether-start-continues-held-work). Every release that settles Agents work first locks its workspace rows (`KEY SHARE`, in workspace order; `workspace_vm_release.rs::lock_for_release`), and its fenced update then locks only the VM rows it releases. That is the owner operations' order, so a release never holds a VM row while waiting for a workspace row. API tests in `backend/g5-api/tests/api_worker_hosts.rs`: a reported exit and a hello without the VM cancel the queued preview, settle the preview intent and withdraw the queued notice, and the tenant connection keeps no scope afterwards; races with the owner's cancellation and a held workspace row cover hello, exit and lease expiry, and a delayed exit or a replayed exit does not wait for a stopped VM row that another transaction holds (`delayed_exit_does_not_wait_for_a_locked_stopped_vm`, `exit_replay_waits_for_neither_an_owner_stop_nor_a_vm_row_holder`). A replayed fact is acknowledged again but not applied again: recording and applying it commit together.
 
 ### g5.runtime.events.supervision
 
@@ -398,20 +394,14 @@ Changes/diagnostics use facade-issued operation identity with replay/cancel/audi
 ### g5.runtime.materialization
 
 - agent
-- From [C087](../../uplift/g5-lean/candidates/contracts.md) and [C2 source rationale](../../uplift/g5-lean/investigations/runtime-security.md#reverse-discovered-consequential-decisions-and-tradeoffs).
-- Scoped evidence · `g5d-host::workspace_vm` replaces retained roots before SCM restore; public retained-hostile-control and restore tests pass. Foundation cache locking/cancellation and exact materialization retain native behavior. Caches are preserved, not replaced; KVM performance comparison and eviction remain unverified. [Git review](../../ontology-review-git-abstraction.md).
+- From [C087](../../uplift/g5-lean/candidates/contracts.md) and [C2 source rationale](../../uplift/g5-lean/investigations/runtime-security.md#reverse-discovered-consequential-decisions-and-tradeoffs). Reuse of a kept root existed in PR #117's early commits (09f11e34–6556d241) and was removed by 7577fb45 for the round-6 finding that host Git ran in the guest-writable reused root ([retained-roots decision](../../chat_intent/retained-roots/request.md)).
+- Scoped evidence · `prepare_workspace_root` (`backend/g5d-host/src/workspace_vm.rs`) always calls `WorkspaceCheckout::materialize_with_git_token` (`backend/g5d-foundation/src/workspace/restore.rs`), which builds a staging tree from the reflinked bare cache, swaps it in and deletes the previous tree before SCM restore; public retained-hostile-control and restore tests pass. Foundation cache locking/cancellation and exact materialization retain native behavior. The repository cache is preserved, not replaced; KVM performance comparison remains unverified. [Git review](../../ontology-review-git-abstraction.md).
 
 ### g5.runtime.purge
 
-- agent
-- From [C086](../../uplift/g5-lean/candidates/contracts.md); principle sharpened by [R02](../../uplift/g5-lean/investigations/runtime-security.md#r02--a-failed-retained-root-deletion-can-become-false-purge-evidence).
-- ~60% · host purge; failed or expired deletion can lose pending generation/inventory and falsely appear complete.
-
-### g5.runtime.purge.cache
-
-- agent
-- From [C086](../../uplift/g5-lean/candidates/contracts.md); principle sharpened by [R02](../../uplift/g5-lean/investigations/runtime-security.md#r02--a-failed-retained-root-deletion-can-become-false-purge-evidence).
-- ~90% · worker host retained roots; failed-deletion retry/persistence remains incomplete.
+- human-verified
+- Replaces the retention and purge-generation design of [C086](../../uplift/g5-lean/candidates/contracts.md) by the [retained-roots decision](../../chat_intent/retained-roots/request.md); [R02](../../uplift/g5-lean/investigations/runtime-security.md#r02--a-failed-retained-root-deletion-can-become-false-purge-evidence) (a failed deletion became false purge evidence) no longer applies because no operation waits for deletion.
+- Implemented, locally validated · `RootReaper` in `backend/g5d-host/src/workspace_vm.rs` claims every root, staging/previous copy and legacy `.g5-retained` entry that no VM record holds by renaming it to `.trash-<uuid>` under the runtime state lock, after every release (stop, natural exit through status or the exit monitor, boot failure or cancellation), after a disk refusal, and at host start (`release_unheld_roots`, called by `backend/g5d-cli/src/host.rs` before the host serves); one background pass deletes claimed trees without following symlinks. A boot cleanup whose kill cannot prove reap (`CancelledChildReapUnconfirmed`) keeps that workspace's root held (`unreaped`, per VM) until that VM's kill proves reap, a later Start of the workspace replaces the tree, or the host restarts. The host answers `roots.purge` with `not_found`; transfer and Delete no longer purge. Tests: `qualification::lifecycle::{start_then_stop_deletes_the_root_and_journals_the_boot_summary, a_release_deletes_only_roots_no_vm_holds, natural_exit_settles_through_status_and_through_the_exit_monitor, reconcile_kills_superseded_incarnations_and_ignores_older_generations}`, `qualification::control_session` (purge over the wire), `workspace_vm::tests::host_start_deletes_every_root_and_leaves_other_entries`. Tests add `qualification::lifecycle::{a_boot_whose_kill_cannot_prove_reap_keeps_its_root, a_disk_refusal_retries_deleting_released_roots}` (the former includes a replacement Start that fails before replacing the tree) and `workspace_vm::tests::a_stale_kill_completion_keeps_a_newer_vms_hold`. Gaps: such a boot still releases its slot and workspace exclusion without reap proof (the existing [custody gap](#release-capacity-only-with-exit-proof)), so the next Start of that workspace on the host replaces the tree the VMM may still use; a failed deletion is retried only at the next release, disk refusal or host start; real-KVM and deployed behavior unobserved.
 
 ### g5.runtime.drain
 
diff --git a/.g5/ontology/software_architecture/worker_host.g5.md b/.g5/ontology/software_architecture/worker_host.g5.md
index 9031728c..b6330da1 100644
--- a/.g5/ontology/software_architecture/worker_host.g5.md
+++ b/.g5/ontology/software_architecture/worker_host.g5.md
@@ -5,7 +5,7 @@ Control-plane placement and host-local process custody remain distinct owners.
 
 ## Governing contracts and collaborators
 
-- [Start consent and release](lifecycle.g5.md), [exit proof](runtime.g5.md#release-capacity-only-with-exit-proof), [purge proof](runtime.g5.md#purge-is-proved-deletion-not-forgotten-inventory) and [reproducible boot](runtime.g5.md#boot-is-bound-to-a-reproducible-source) govern runtime mechanics.
+- [Start consent and release](lifecycle.g5.md), [exit proof](runtime.g5.md#release-capacity-only-with-exit-proof), [root deletion](runtime.g5.md#delete-a-workspace-root-when-its-vm-is-released) and [reproducible boot](runtime.g5.md#boot-is-bound-to-a-reproducible-source) govern runtime mechanics.
 - Host deployment and rotation follow [deployment](../concepts/deployment.g5.md) and [operations](../concepts/operations.g5.md); credential exposure stays within [declared security boundaries](../concepts/security.g5.md).
 - Placement serves Workspace lifecycle and operators; task outcome belongs to [Agents](agents.g5.md), branch/approval policy to [Git/governance](git_governance.g5.md), and Start consent to [Workspace lifecycle](lifecycle.g5.md#explicit-consent-owns-each-start).
 - [Separation of concerns](../concepts/system_design_principles.g5.md#group-one-coherent-responsibility), [explicit replay](../concepts/system_design_principles.g5.md#make-failures-and-retries-explicit) and [cleanup proof](runtime.g5.md#release-capacity-only-with-exit-proof) govern the public boundary.
@@ -16,7 +16,7 @@ Control-plane placement and host-local process custody remain distinct owners.
 |---|---|---|
 | WorkerHostService | `place`, `bind_boot_record`, `release_placement`, `request_drain`, `set_org_image_policy` | [Placement](#capacity-and-runtime-placement) |
 | HostCommandProcessor | Replayable `execute` and stream relay | [Host boundary](#host-transport-and-vm-custody) |
-| WorkspaceVmRuntime / VmLauncher | `start`, `stop`, `status`, `purge`, `inventory`, `reconcile` | [Custody](#host-transport-and-vm-custody), [runtime protocol](runtime.g5.md) |
+| WorkspaceVmRuntime / VmLauncher | `start`, `stop`, `status`, `inventory`, `reconcile` | [Custody](#host-transport-and-vm-custody), [runtime protocol](runtime.g5.md) |
 
 ## Owned responsibilities
 
@@ -31,7 +31,7 @@ creating user Start consent or deciding task outcomes.
 
 [D · g5.architecture.host_custody]
 
-The host exposes replayable VM controls and stream relay under the [custody](runtime.g5.md#release-capacity-only-with-exit-proof) and [purge](runtime.g5.md#purge-is-proved-deletion-not-forgotten-inventory) contracts.
+The host exposes replayable VM controls and stream relay under the [custody](runtime.g5.md#release-capacity-only-with-exit-proof) and [root deletion](runtime.g5.md#delete-a-workspace-root-when-its-vm-is-released) contracts.
 
 ## Metadata
 
@@ -45,4 +45,4 @@ The host exposes replayable VM controls and stream relay under the [custody](run
 
 - agent
 - [Design v9 §6.2](../../../docs/lean-backend-design-v9.md#62-host-daemon-and-protocol-g5d-host).
-- ~70% · `backend/g5d-host/src/{ctl, tunnel, workspace_vm, runtime}.rs`; outer boot/cancel failures can release custody without positive reap proof, failed root deletion can lose pending inventory and falsely complete purge, and readiness lacks complete runtime pin closure.
+- ~70% · `backend/g5d-host/src/{ctl, tunnel, workspace_vm, runtime}.rs`; outer boot/cancel failures can release custody without positive reap proof, a released root is claimed and deleted after exit proof (`RootReaper`), and readiness lacks complete runtime pin closure.
diff --git a/.g5/ontology/software_architecture/workspace.g5.md b/.g5/ontology/software_architecture/workspace.g5.md
index 2aafaa6d..9d26b90b 100644
--- a/.g5/ontology/software_architecture/workspace.g5.md
+++ b/.g5/ontology/software_architecture/workspace.g5.md
@@ -8,7 +8,7 @@ and [preview/terminal](../features/preview_terminal.g5.md).
 
 - Authority follows [live ownership](../concepts/security.g5.md#one-eligible-owner-uses-live-compute), [durable reading](../concepts/security.g5.md#serve-nonowners-from-durable-state) and [explicit Start consent](lifecycle.g5.md#explicit-consent-owns-each-start).
 - Repository effects use [Git](git_governance.g5.md#git-policy-owns-authoritative-ref-effects), task settlement uses [Agents](agents.g5.md#one-logical-task-owner), and compute placement/custody uses [Worker & Host](worker_host.g5.md); the [data model](../concepts/data_model.g5.md) and [complete receipt](durability.g5.md#only-verified-complete-capture-advances-recovery) retain their authority.
-- The boundary follows [complete operations](boundaries.g5.md#keep-complete-operations-with-their-state-owner), [minimal APIs](../concepts/system_design_principles.g5.md#expose-minimal-sufficient-apis) and [custody](runtime.g5.md#release-capacity-only-with-exit-proof) and [purge proof](runtime.g5.md#purge-is-proved-deletion-not-forgotten-inventory).
+- The boundary follows [complete operations](boundaries.g5.md#keep-complete-operations-with-their-state-owner), [minimal APIs](../concepts/system_design_principles.g5.md#expose-minimal-sufficient-apis) and [custody](runtime.g5.md#release-capacity-only-with-exit-proof) and [root deletion](runtime.g5.md#delete-a-workspace-root-when-its-vm-is-released).
 
 ## Public operations
 
@@ -142,7 +142,7 @@ execution evidence under the [event contract](runtime.g5.md#event-facts-survive-
 
 - agent
 - [Design v9 §5.8](../../../docs/lean-backend-design-v9.md#58-workspace-actions-teardown-reasons-and-the-stop-sequence); the [study's applied improvements](../../../docs/experiments/balance-study/README.md#candidate-improvements-applied-after-the-study) link composition to operation scope and recovery progress.
-- ~75% · `backend/g5-api-core/src/services/workspace_vm_service.rs`; the first definite outer boot failure ends Start consent instead of allowing three, quiet connected terminals do not maintain activity, and host reap/purge gaps can defeat complete release evidence. The release SQL of every cause (stop, loss, failed start, lease expiry, refused placement, forced host removal) lives in `workspace_vm_release.rs`. The lifecycle also holds the VM-token reader (`workspace_incarnation.rs`) and the crate-private stop without an owner check (`stop_as_system`); routes call `stop` with the actor or `stop_for_capacity`.
+- ~75% · `backend/g5-api-core/src/services/workspace_vm_service.rs`; the first definite outer boot failure ends Start consent instead of allowing three, quiet connected terminals do not maintain activity, and host reap gaps can defeat complete release evidence. The release SQL of every cause (stop, loss, failed start, lease expiry, refused placement, forced host removal) lives in `workspace_vm_release.rs`. The lifecycle also holds the VM-token reader (`workspace_incarnation.rs`) and the crate-private stop without an owner check (`stop_as_system`); routes call `stop` with the actor or `stop_for_capacity`.
 
 ### g5.architecture.runtime_facade
 
```

</details>
