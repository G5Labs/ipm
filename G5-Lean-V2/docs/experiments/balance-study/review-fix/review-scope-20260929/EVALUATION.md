# Evidence behind the inventory result

This assesses **what the worker says a future reviewer should check**. It does not assess
code, test success, repair or how much of a feature is implemented. See the
[short result](README.md) and [protocol](PROTOCOL.md).

## Reading the tables

**C = covered:** all material conditions in the reference group are retained somewhere
in the inventory. **P = partial:** a meaningful obligation is retained, but a condition
is missing. These are broad organizing groups, not equally weighted requirements. A
single absent condition makes a group partial; read the explanation, not a percentage.
Different wording and grouping receive credit. A bare source ID or read-log entry does
not establish that a condition reached the review inventory.

The reference was frozen before dispatch: 15 retry, 12 host and nine Save groups. Each
includes feature rules plus applicable inherited design/verification/evolution concerns.
The independent assessments and full worker-row mappings remain unchanged in the local
`private/evaluations/` archive; reconciled assessments and parent adjustments are separate.

## Failed workflow retry

| Reference group | Run 1 | Run 2 | Retained meaning and remaining detail |
|---|:---:|:---:|---|
| Eligible failed slice | C | C | Direct failure, propagated descendants, outside blockers and successful/unrelated exclusions retained. |
| Atomic fresh run and immutable history | C | C | Fresh tasks/sessions/inputs/edges/lineage together; original history preserved. |
| Faithful admitted input | P | P | Copy/input binding/root-only edit/attachment validation retained. Neither concretely retains expected-revision editing protection. Run 2 better states exact reserved attachments. |
| Immutable outside-success evidence | C | C | Remapping, recorded task/outcome, no rerun and no claim about unchanged current files retained. |
| Normalized dependency semantics | P | P | Graph authority/validation retained. Both miss held versus final-failure/cancellation settlement; Run 2 also misses prerequisite deletion restrictions. |
| Bootstrap graph reconciliation | P | P | Both retain same-graph replay, changed-graph conflict and no-source-graph branch. Run 1 misses child dependence on bootstrap success; Run 2 misses authorization at later graph submission. |
| Exact accepted submission replay | C | C | Atomic receipt, same body/key, conflicts, new deliberate key and read-only replay after admission closes retained. |
| Current authority; no implicit Start | C | C | Current owner/incarnation/configuration and Stop/transfer restrictions retained. |
| Ordinary execution admission | P | P | Current accounts, policy, capacity and spending retained. Both miss explicit invalid-account fallback prohibition, narrowing tool restrictions and committed invocation identity/lost-invoke replay. |
| State-owner composition | C | C | JobService composes TaskService/ChatService in one transaction; adapters cannot own allocation; dispatch follows commit. |
| Truthful lineage/progress/history | C | C | Slice-only success, failed source, waiting/reused work and authorized durable reads retained. |
| Tenant/record authority | P | P | Scoped references/access retained. Forced RLS is not concretely stated; Run 1 also leaves privileged typed-check requirements incomplete. |
| Applicable design principles | P | P | Ownership is strong. Minimal sufficient public APIs, private representations, dependency direction and justification of delivered new mechanisms are incomplete. |
| Verification discipline | P | P | Concrete cases and honest evidence limits retained. Owner test-plan review before fanout and conditional guard checks missing; Run 2 also lacks concrete real-boundary/restricted-role evidence requirements. |
| Compatible evolution | P | P | Existing behavior and forward evolution retained. Neither specifies migration-role verification; Run 2 also omits fresh/populated/repeat-no-op cases. Only relevant if persistence/public contracts change. |

Useful additions outside the frozen reference: Run 1 retains the spending threshold's
in-flight exception and postcommit notifications if emitted; Run 2 distinguishes copied
queued intent from actual provider delivery. No specific storage encoding was required.

One unsupported restriction: Run 2 worker row `R21` calls **bidirectional** navigation
specified. `g5.agents.retry.progress` requires retry-to-source linkage; reverse navigation
is an optional design choice. This is a low-impact surplus UI constraint, not a failure
of the required lineage outcome.

## Host deletion by draining

| Reference group | Run 1 | Run 2 | Retained meaning and remaining detail |
|---|:---:|:---:|---|
| Durable one-request deletion | C | C | Accepted intent, placement closure, stable replay/recovery and automatic progression retained. |
| Empty fast path and provider proof | C | C | Empty eligibility retained; provider acceptance is not completed deletion. |
| Per-VM lifecycle Stop | C | P | Both retain Stop ownership/fencing/capture/exact incarnation/no Start. Run 2 does not state atomic Agents release settlement. |
| Recoverable ordinary shutdown failure | P | P | Both keep failed Stop blocked without implicit force. Neither retains deterministic fingerprint plus one owner notice until repaired explicit retry. Only Run 2 retains the provider-bundle exception. |
| Custody exit evidence | C | C | No release on lease expiry, timeout or stale inventory alone; late results cannot release successors. |
| Explicit force with loss record | C | C | Authorized force, per-VM loss before destruction and truthful provider/custody proof retained. |
| Useful recoverable progress | C | P | Both retain durable pending/blocked outcomes and postcommit status. Run 2 omits concrete authenticated tenant-scoped outcome reads. |
| Deletion ownership versus rotation | C | C | No outer-caller choreography, deletion replacement capacity or autoscaling; existing rotation preserved. |
| Operator and tenant authority | P | P | Both scope destructive requests. Neither states forced RLS or the permitted starting/draining lifecycle-callback case. Run 2 also misses progress-read authority. |
| Applicable design principles | C | C | Concrete ownership, adapter policy, dependency direction/private state and justified mechanisms retained. |
| Verification discipline | P | P | Boundary cases/evidence limits retained. Both miss owner test-plan review before fanout and conditional guard calibration; Run 2 misses restricted-runtime-role checks. |
| Compatible evolution | P | C | Both retain valid callers/records. Run 1 omits fresh-install and repeat-no-op migration checks; Run 2 states them conditionally. |

Useful additions include placement/drain races, uncertain control-command replay and
conditional pending-root-purge evidence. Run 1 also identifies a deployment-lane question;
Run 2 identifies the Host-to-Workspace composition question. They ask for applicability
investigation, not new protocols, mandatory self-hosted teardown or global cleanup.
No unsupported addition or material contradiction was established in either inventory.

## Stale editor Save

| Reference group | Run 1 | Run 2 | Retained meaning and remaining detail |
|---|:---:|:---:|---|
| Observed incarnation plus file version | C | C | Exact observed identity, independent version check, equal-hash rejection and no successor write retained. |
| Buffer preservation and fresh reads | C | P | Both preserve dirty content/fresh attachment reads/disk-change handling. Run 2 misses live-cache/stream invalidation on Stop, transfer or revocation. |
| Shared live authority | P | C | Both retain current admission and no Start. Run 1 does not concretely retain request deadlines and token lease validation. |
| Safe file mutation | C | C | Confinement, payload limit, atomic replacement, branch association and same-handle conflict enrichment retained. |
| File replay and drain | P | P | Exact-result replay and drain-through-outcome retained. Both omit full-capacity refusal/no unexpired eviction and pre-I/O retryability. |
| Ordinary consumers and durability boundary | P | C | Both cover ordinary editor entry points. Only Run 2 makes no implicit commit/flush/new durability promise explicit. |
| Applicable design principles | P | P | Both name the owning operation and avoid parallel authority. Some private-representation, dependency-direction and conditional facade checks remain unstated. |
| Verification discipline | P | P | Both retain cases/evidence limits. Run 1 misses test-plan-before-fanout; Run 2 misses real production boundary versus fakes. Both miss conditional DB-role/structural-guard checks. |
| Compatible evolution | P | P | Existing Save consumers are preserved. Neither checks explicit transitions for changed public/persistent contracts or conditional migration verification. No migration is required by this assessment. |

Both add legitimate native shared-checkout and clean-only Switch-back exceptions.
Dirty-buffer reconciliation remains an open UI choice; neither is penalized for not
inventing a specific merge/draft interface. Exact-result replay after closed admission
does not imply cross-incarnation receipt availability or permission to write a successor.
No unsupported addition or material contradiction was established.

## Parent adjudication and scope limits

- **Two status corrections:** independent evaluation called both Save evolution groups
  omitted. The parent credits existing-consumer preservation in Run 1 `SAVE-06/SAVE-13`
  and Run 2 `S10/S15`, changing both to partial. Explicit transition/migration checks
  remain missing. Original assessments are preserved.
- All evaluators flagged the shared evolution reference's other-feature examples. They
  were scoped to their named feature and never used as penalties on another feature.
- Migration checks apply only if schema changes are needed; role-specific DB checks
  apply where DB authority participates; facade/guard checks apply if such mechanisms
  are present or added. Their absence from a preparatory inventory is not an observed
  code defect. These lower-level conditional details should not outweigh central feature
  behavior in a delivery judgment.
- Broad “required capture” wording in host Run 1 is incomplete, not an affirmative ban
  on the bundle exception. Open implementation questions are not unsupported requirements.
- Reference group counts are retained for audit: retry runs each have 7 C/8 P; host has
  8 C/4 P and 7 C/5 P; Save has 3 C/6 P and 4 C/5 P after reconciliation. These are
  inventory-group classifications, **not quality scores or feature-completion fractions**.

## Evidence that some omissions followed retrieval

Exact text appears in the archived tool outputs at these UTC times. This establishes
exposure, not attention or understanding. The original public outputs are retained.

| Source condition | Run 1 exposure | Run 2 exposure | Inventory result |
|---|---|---|---|
| `g5.lifecycle.stop.bundle-exception`: invalid/oversized bundle, notice/fallback, task-event persistence | host1 17:27:01.707 | host2 17:30:14.994 | Only host2 `H08` retains the exception. |
| `g5.git.flush-failure.block`: fingerprint and one notice until repaired explicit retry | host1 17:27:12.220 | host2 17:30:25.374 | Neither retains the specific conditions; visible failure/retry alone is incomplete. |
| `g5.documents.files.replay`: full-capacity refusal without evicting unexpired results | save1 17:27:03.054 | save2 17:30:06.791 | Neither `SAVE-10` nor `S8` retains this rule. |

The pairwise union gains detail but does not close these shared omissions. No actual
consolidator was run, so even perfect mechanical union must not be assumed in production.

## Per-run effort and provenance

| Worker | Inventory rows | Seconds from first tool | Output tokens | Noncached input tokens |
|---|---:|---:|---:|---:|
| retry1 | 24 | 167.5 | 11,492 | 76,151 |
| retry2 | 26 | 176.8 | 12,437 | 39,511 |
| host1 | 25 | 161.6 | 11,198 | 37,582 |
| host2 | 25 | 166.1 | 11,594 | 39,875 |
| save1 | 14 | 169.1 | 11,454 | 40,920 |
| save2 | 17 | 168.5 | 11,023 | 36,045 |

Row counts reflect grouping and include open questions; they are not coverage measures.
No worker hit the 300-second allowance. Three independent Astra assessments took 263.0,
192.5 and 181.8 seconds from first tool, each below 360 seconds. The effort table in the
README uses native whole-turn time for Astra, including startup, and includes reference
creation. Parent work is additional.

Base ontology hashes matched all 33 archived source files for each feature. Retained
patches apply to retry `1738018`, host worker baseline `956c61a`, and Save `12676ae`.
Host's proposed ontology adds one file. Body diffs were regenerated after uniformly
removing metadata; no governing body was rewritten. Original patch/design hashes,
mechanically selected references and per-file input hashes are in `private/manifest.json`.

All six worker input sets and all three evaluator input sets stayed unchanged. Every
worker JSON row has the requested fields; all cited ontology IDs resolve. Worker and
evaluation outputs were frozen with hashes. Those mechanical checks do not establish
semantic completeness or negate the two-repetition, prepared-input and directory-only
isolation limitations.
