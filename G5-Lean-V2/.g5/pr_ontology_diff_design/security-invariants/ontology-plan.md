# Security invariants: the two-tier threat model as ontology

## Source and scope

The owner's threat model of 2026-10-30, written after the 2026-10-29 meeting with
James Moore, is recorded verbatim with the owner's decisions in the
[request](../../chat_intent/security-invariants/request.md). The owner's Project Doc
(`threat-model.md`, docs repository, workspace `d09348e2-1a56-47b3-b0bb-4704d0ce4641`)
also prescribes the blocks, their order, markers and provenance; this plan records how the
ontology applies it.

This is an ontology-only change. No code changes, and no gap it records is implemented
here. Where the code does not meet a statement, the statement stands and the block's
metadata names the gap. Code facts were verified by the owner on 2026-10-30 at
`c351b446c` and re-checked for this change on 2026-10-31 at the same commit; the two
places where the re-check narrowed the request are listed below. No TLA+/TLC work was
requested or run.

## Governing decisions

The owner's five decisions and the meeting's points, each with the block that now owns it:

| Decision or point | Owning block |
|---|---|
| Two tiers by the boundary a failure crosses; "nobody" for tier 0; removal bound; trusted colleagues (2026-10-14) | [g5.security.tiers](../../ontology/concepts/system_invariants.g5.md#g5.security.tiers) |
| Decision 1: platform integrity and the CVE scan stay on tier 1 | [g5.security.tier1.platform-integrity](../../ontology/concepts/system_invariants.g5.md#g5.security.tier1.platform-integrity) |
| Decision 2: commit author and committer text is advisory; the recorded push is authoritative | [g5.git.commit-identity-question](../../ontology/software_architecture/git.g5.md#g5.git.commit-identity-question), referenced by [g5.security.tier1.attribution](../../ontology/concepts/system_invariants.g5.md#g5.security.tier1.attribution) |
| Decision 3: the CVE requirement covers every deployed image; regular proactive monitoring | [g5.quality.delivery.image-cve-reporting](../../ontology/features/operations.g5.md#g5.quality.delivery.image-cve-reporting) |
| Decision 4: the shared worker-host cloud identity is the accepted design; a compromised host exposes only its organization | [g5.security.tier0.host-reach](../../ontology/concepts/system_invariants.g5.md#g5.security.tier0.host-reach) |
| Decision 5: every active member reads every ordinary workspace; invite-only retired | [g5.workspace.audience](../../ontology/features/workspaces.g5.md#g5.workspace.audience), [g5.repo.permissions](../../ontology/features/repositories.g5.md#g5.repo.permissions), [g5.security.tier1.visibility](../../ontology/concepts/system_invariants.g5.md#g5.security.tier1.visibility), [g5.security.permission-model](../../ontology/concepts/system_invariants.g5.md#g5.security.permission-model) |
| Breakout treated as possible; microVM adopted for dependency isolation | [g5.security.tier1.platform-integrity](../../ontology/concepts/system_invariants.g5.md#g5.security.tier1.platform-integrity) |
| Ledger definition and impersonation wording | [g5.security.tier1.attribution](../../ontology/concepts/system_invariants.g5.md#g5.security.tier1.attribution) |
| Parent merge: only a G5 merge by someone with merge authority (parent's owner, an administrator for Base, or the child's owner under source-owner merge); nobody pushes into the parent (owner decision of 2026-11-06) | [g5.security.tier1.ownership](../../ontology/concepts/system_invariants.g5.md#g5.security.tier1.ownership) |
| Encrypted at rest means the cloud provider cannot read repositories; customer-managed keys | [g5.security.encryption](../../ontology/concepts/security.g5.md#g5.security.encryption) |
| Secrets as environment variables are an accepted, unmitigated threat | [g5.security.tier1.visibility](../../ontology/concepts/system_invariants.g5.md#g5.security.tier1.visibility) |
| Customer content never in logs, telemetry or error messages | [g5.security.tier0.log-content](../../ontology/concepts/system_invariants.g5.md#g5.security.tier0.log-content) |
| Defense in depth on tenancy: tenant-scoped queries plus row-level security | [g5.security.tier0.outsiders](../../ontology/concepts/system_invariants.g5.md#g5.security.tier0.outsiders) |

## Ontology changes

- `concepts/system_invariants.g5.md` gains the section "Security tiers" after
  `g5.architecture.commands`: the ranking decision, four tier 0 blocks (identity,
  outsiders, host reach, log content), five tier 1 blocks (attribution, ownership,
  administration, visibility, platform integrity) and the permission model read as one
  table. `g5.security.tenant` is unchanged; the outsiders block specializes it. The tier
  blocks link the existing owners of each mechanism instead of restating them.
- `concepts/security.g5.md`: `g5.security.encryption` widens from "secrets stored in the
  database are encrypted" to secrets at rest and in transit plus customer data at rest,
  naming the stores with customer-managed keys and the gap (basic-tier file store, host
  and VM disks).
- `features/repositories.g5.md`: `g5.repo.permissions` regains the per-repository read
  rule from the owner's 2026-10-10 decision on issue 423, which lost its home when the
  2026-10-13 rebuild dropped `g5.security.mvp-exposure.objects`, and adds that each
  workspace writes only its own branches.
- `features/workspaces.g5.md`: `g5.workspace.audience` carries decision 5 as
  human-provided provenance on the fixed-by-identity text that issue #1107 landed on main
  (configurable sharing removed, migration 149), adding the branch read through
  `g5.repo.permissions` and the clause that no per-workspace read restriction exists until
  a private-workspace feature protects records and Git branches together. The invite-only
  gap recorded on 2026-10-31 is closed by #1107; the owner's 2026-11-05 edit dropped the
  reason sentence, and the rebase keeps it dropped.
- `features/operations.g5.md`: `g5.quality.delivery.image-cve-reporting` widens to every
  deployed image and to regular monitoring, keeping the report-only behavior, the
  incomplete-versus-clean distinction, the existing gates and read-scoped report access.
- `software_architecture/git.g5.md`: new decision `g5.git.commit-identity-question`
  after `g5.git.vm-authority`; the ID keeps the word "question" because the meeting
  raised it as one and the owner settled it on 2026-10-30.
- `features/ui.g5.md`: no change after the rebase; main already replaced the invite-only
  example in `g5.workbench.tabs.deleted`.

Wording that is the agent's rather than the owner's, marked human-verified in metadata:
the qualifications in host reach (same-organization hosts reach each other, the internet
is reachable, the cache prefix is written by the deployment's producer), the scope wording
of log content (identifiers, shapes and reasons; operator-visible cloud logs), the reason
in the commit-identity decision. The tier headings are navigation and carry no provenance.

## Facts re-checked and where the request was narrowed

- **CVE report scope.** `scripts/report_image_cves.py` iterates `app_release.IMAGES`,
  which names the application images (api, g5d, edge) and the guest environment images
  (g5d-g5dev, g5d-tungsten, g5d-pairsoft-preview, g5d-pnc-bankworkflow,
  g5d-pnc-cash-receipt), over the references `resolve-image-refs` records in
  `infra/ci/cloudbuild-deploy.yaml`. The worker image build
  (`infra/ci/cloudbuild-packer-worker.yaml`) writes a syft software bill of materials in
  its boot gate and no vulnerability report; that image carries the guest kernel and
  Cloud Hypervisor runtime built by `scripts/build-cloud-hypervisor-runtime.sh`. The base
  image tiers published by `publish-base-tiers` have no scan step. The request said only
  the application containers are reported; the recorded gap names what is and is not
  covered instead.
- **Push record.** `receive_pack` authorizes ref updates through `require_push_ref_updates`
  (incarnation-bound for VMs; `workspaces:write` and own-branch targets for person
  credentials) and inspects no commit identity, as the request says. G5 keeps no record
  dedicated to pushes: a VM push leaves the branch update and a live
  `workspace.branch_updated` event, and the general `user_action.http` audit stream covers
  authenticated end users' requests only. The decision that the recorded push is the
  authoritative attribution therefore needs a durable push record to be fully usable;
  recorded as a gap in `g5.git.commit-identity-question`.
- **Secret values.** The configuration routes return at most a bounded hint of a stored
  secret value (`secret_key_hints`), never the full value, so the visibility block says
  "never shown back in full".
- **Trusted colleagues.** The owner's document dated the rule to 2026-10-14; that record
  was not found in the repository, issue #610 (opened 2026-10-17) restates the rule, and
  the owner's edit of 2026-11-04 removed the date from the block.
- **Shared host identity.** `gcp.rs` requires `G5_WORKER_SERVICE_ACCOUNT` and refuses to
  start the cloud provider without it; the identity's grants are the two telemetry writer
  roles, registry read on `g5`, `g5-tiers` and `g5-worker-images`, read on the
  `g5-worker-v*` worker-image objects, and object read on the `guest-sccache/` prefix
  (the registry and worker-image reads were missing from the first version; audit C5). The instance still requests the
  `cloud-platform` OAuth scope, which grants nothing beyond those roles. Whether the live
  hosts already run as that identity was not verified; issue #611 also raises host JWT
  delivery through instance metadata, which decision 4 does not settle. The cache read
  grant covers all of `guest-sccache/`, not one organization's sub-prefix (RUNTIME-006 in
  `docs/code_gaps.md`); the host-reach block states that breadth and what the cache holds.
- The remaining section 5 facts held as stated: login providers and admission, per-organization
  VPCs and firewall rules, customer-managed keys and the Filestore tier, Cloud SQL
  `ENCRYPTED_ONLY`, the `wss://` host tunnel, dispatcher author and committer identity,
  `merge_workspace` owner refusal, actor from the authenticated principal in the record
  routes, migration 087, and the closed issue #610 with the 30-second bound. The `org`
  visibility default no longer matters: migration 149 fixed audiences by identity.

### Re-check after the rebase onto main `89eeea5e4` (2026-11-06)

Every fact above was re-read on main. Changes and corrections, all recorded in the
blocks' metadata with the date and the main commit:

- Applied from the owner's audit of 2026-11-02 ([issue #1047](https://github.com/G5Labs/G5-Lean/issues/1047)):
  C3 (person-authored provider writes versus G5's App-installation writes), C5 (the host
  identity also reads the image registries and worker-image objects), C6 (credentials
  never hold a person session), C7 (the personal-mount push is the one VM push outside
  the workspace's own branches), and the audit's choice for C4 (administrative stops and
  approval administration named as sanctioned exceptions). The outsiders block now scopes
  forced row-level security to the tenant role and holds the system role to
  `g5.security.tenant.rls`'s explicit checks.
- Recorded as open for the owner, without deciding: C1 and C2 under
  `g5.security.tier1.administration`; H2 under `g5.security.tier1.ownership` as a
  recorded violation.
- Gaps widened: the encryption gap adds GKE node disks (the API keeps working trees under
  `/tmp`, and the GKE module sets no node disk key); the CVE gap adds the migration job's
  PostgreSQL image (now digest-pinned on main) and the External Secrets chart images; the
  permission model records that owner hand-over and step-down are not implemented
  (`update_member_role` accepts only admin, member and viewer); the push-record gap names
  `workspace_revision_events` (migration 100) as partial evidence that records heads and
  push times, not the pusher.
- Closed on main: the invite-only setting and reader grants (issue #1107, migration 149).
- Unchanged on main: login providers and admission, per-organization VPCs and firewall
  rules, `G5_WORKER_SERVICE_ACCOUNT`, the five customer-managed keys and the Filestore
  `BASIC_HDD` default, Cloud SQL `ENCRYPTED_ONLY`, the `wss://` host tunnel, dispatcher
  author and committer identity, `require_push_ref_updates` with no commit-identity check,
  `merge_workspace` owner refusal, actor from the authenticated principal in the record
  routes, the `report_image_cves.py` image list, the Packer SBOM-only gate, the bounded
  secret hints, the `user_action.http` audit scope, RUNTIME-006 in `docs/code_gaps.md`,
  and issue #611 still open.

## Follow-up work

Recorded here, not done in this change:

- Add vulnerability scanning for the worker host boot image, including the guest kernel
  and VM runtime it carries, for the base image tiers, for the migration job's PostgreSQL
  image and the External Secrets chart images, and re-run the report for already deployed
  images when new vulnerabilities are published
  (`g5.quality.delivery.image-cve-reporting` gap).
- Customer-managed keys for repository storage: a Filestore tier that supports them, and a
  decision on host, VM and GKE node disks (`g5.security.encryption` gap). Decide the provider
  trust boundary behind "the cloud provider cannot read": customer-managed keys give
  revocable, audited control while the storing service still decrypts server-side;
  provider-blind storage would need client-side encryption or an external key manager with
  key-access justifications (review finding of 2026-11-06).
- Owner decisions on the audit's contradictions C1, C2 and C4 and on `g5.host.cache-fleet`'s
  per-organization credential clause. Owner hand-over and step-down are superseded by
  [issue #1106's retirement](../issue-1106/ontology-plan.md): the final roles are Admin,
  Member and Viewer, with last-active-Admin continuity and no handover mechanism. The
  retirement preserves existing additive grants; the broader audit findings remain
  tracked in issue #1047.
- Audit API, host and guest logging and database error texts for customer content
  (`g5.security.tier0.log-content`).
- A durable per-push record naming the workspace, its owner at push time, the refs and
  the time (`g5.git.commit-identity-question` gap).
- Close issue #611 with the accepted shared-identity decision, after checking that the
  live hosts run as the dedicated identity.

## Verification

A second pass on 2026-10-31, at the owner's request, shortened the new and revised blocks
without changing any statement: mechanisms that linked blocks own are linked instead of
restated, gap detail moved to metadata behind a one-clause body qualifier, the shared
source and check date sit once under `g5.security.tiers`, and the CVE gap evidence lives
only in `g5.quality.delivery.image-cve-reporting`. Every ID, marker and body link of the
first version was confirmed present afterwards.

Review round 1 (CodeRabbit on PR #1026, 2026-10-31): the body qualifiers now carry the
date of the observation, the host-reach block states that the cache read grant spans every
organization's sub-prefix and what the cache holds, the CVE block records that nothing
re-runs the report for deployed images, and the Git decision names the owner at push time.
The two provenance findings were answered without change: the tier and encryption text
paraphrases the owner's document, which the format counts as human-provided.

Rebase onto main `89eeea5e4` (2026-11-06): the branch's five commits, the owner's edits
of 2026-11-04 and 2026-11-05 included, were landed as one commit on main; the conflicts
in the audience and tab-list blocks were resolved by adopting main's text, with decision 5
as the audience block's provenance. The structural check on the result reports 25 pages
and 933 blocks with nine issues, every one in a file this change does not touch, so they
come from main. Before the rebase the same check on the branch reported 859 blocks with
the two metadata-order issues in `features/reviews.g5.md` and `features/ui.g5.md` that
the request allowed. Every block ID the new statements link was confirmed
to exist before writing. The checker establishes structure and local links only; the
statements' correctness rests on the owner's document and the facts listed above.

Approval preparation (2026-11-06, main `c825367d5`): `g5.security.tier1.ownership` said only
the parent's owner (or an administrator for Base) lands a merge, contradicting
`g5.governance.pr.source-owner-merge` from #1104. At the owner's decision the invariant now
names every merge authority, the source-owner path included. A trial merge with main is
clean, and the structural check on it adds no issue beyond main's own.
