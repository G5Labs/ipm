# Manual-retry repair recheck — reviewer two

Read-only recheck of the consolidated repairs against `4504311be48c72aa19e4ceed49fbc7bd9d679a8b`. Read the updated first-section refinement and the saved consolidation before rechecking the full delta and affected interactions. The initial independent report remains unchanged in `manual-retry-two.md`. No implementation/test/ontology edits or shared tests were performed.

Inspected source snapshot (Git blob hashes, read without writing Git objects):

- `coordinated_repo_publication.rs`: `7a2fa4fa00c5dc37783e82501f82d76f76e92704`
- `workspace_pull_request_create.rs`: `0c7af8346d408ee08b32d7d2eb511724be5e7d0f`
- `workspace_pull_request_provider_lifecycle.rs`: `c9ead2a4fbfef4dfb0657f9d6fead16e75283740`
- `api_workspace_pull_requests.rs`: `463b87dafc52aab5fad022f0fe23deeed3affb3d`

## Result

No new actionable defect found in this repaired snapshot. The three original causes are addressed by source inspection; runtime results remain the main agent's responsibility. In particular, claim preservation of the recorded provider version separately from successful end-to-end import/merge recovery after subsequent source movement.

| Initial finding / consolidated obligation | Repair and inspected downstream effect | Recheck |
| --- | --- | --- |
| F1: Included-member retry replaces completed provider pins | `create_or_update_one` refuses a recorded terminal status immediately and observes recorded included PR completion before its own Git-publication stage. The outer included-member error path uses `update_existing_provider_status`; its SQL retains source/target pins and observed proof. A recorded merged status remains merged. The generic candidate upsert also preserves existing pins on failed/action-required outcomes and preserves observed proof when no new pull is available. | Original pin/proof overwrite is repaired by inspection. The expanded public Update and VM Update regression asserts the actual four persisted SHA columns and recorded merged status. It also rejects provider POST/PATCH calls. Not executed by this reviewer. |
| F2: Local-only refresh hides automatic failure | The summary selects vector-bearing `provider_failed`/`provider_updated` events, excluding local `refreshed` and individual provider events. `record_completed_sync` is called only after the shared explicit/automatic provider phase; failure and completion timestamps use `clock_timestamp()` under the PR row lock. Provider-row errors are projected first. | A local refresh cannot settle the failure anymore. The real local-refresh API regression checks the failure message remains unchanged while provider pins remain old, then checks it clears after successful publication. Not executed here. |
| F3: Missing required provider member becomes invisible after local evidence refresh | `needs_push_refresh` calls `has_missing_provider_pull_request`; this uses the same `repo_needs_provider_pr` and `load_provider_candidate` as synchronization, with current scoped configuration. It does not create a second provider eligibility policy or perform external effects during discovery. | Missing-row counterexample is repaired. New API fixture checks that an included no-row member is a no-op, then advances/publishes its source, refreshes local evidence, and verifies continuation records the recovered PR and current source pin. Not executed here. |
| Consolidated false ready-preflight error | Both summary SQL owners now use preflight messages only when local status is outside ready/merged/closed; real provider errors and incomplete full-flow errors retain precedence. | Informational successful preflight cannot produce the new UI error state. Actual error visibility remains intact. |

## Repair interactions and retained obligations

- **Publication completion versus partial outcome:** per-repository failures recompute a non-success aggregate and retain `last_error`; `record_completed_sync` only appends success for open/local-only aggregates. Individual provider events lack evidence vectors, so a successful member does not clear an earlier full-flow failure. A provider-row error remains visible independently of event ordering.
- **Title intent:** the request-scoped title still compares with the accepted G5 title under the PR lock, omission still passes `None`, and failed explicit requests still provide `--title` recovery guidance. Preserved pins allow later publication reconciliation without storing or replaying failed title intent. No additional title writer or persistence was introduced.
- **Current authority and shared flow:** typed user/VM/continuation admission, source preparation, target-access rechecks and source restrictions remain in the same owner. The new missing-candidate discovery is read-only and runs inside the existing attempt deadline; external effect admission is unchanged.
- **Progress and notifications:** per-PR attempt and failure-record budgets remain, failure deduplication ignores irrelevant local/provider events, and update notifications still follow commit. The dedicated maintenance scan lock is unchanged.
- **Accepted-version protection:** the automatic guard continues to refuse recorded/observed completed PRs before its evidence refresh. The repair prevents the newly introduced included-member manual failure path from replacing provider pins. It does not redesign pre-existing explicit local evidence capture or merge admission.
- **Minimality and ontology:** the refinement explicitly distinguishes full publication outcomes from local preflight outcomes while reusing the existing event fields/types. That distinction is consequential and now recorded before relying on it. The corrected readiness ID matches `g5.governance.publish.readiness`. No new queue, column, credential policy or publication owner was added.
- **Migration/API/UI/CLI removal:** the pending columns, migrations/checksums, public fields, supported CLI field names and UI consumers remain removed; the code/docs search found no live residual reference. Main 001–054 remain untouched in this diff.

## Verification limits

`git diff --check 4504311be48c72aa19e4ceed49fbc7bd9d679a8b` passed. Searches for the removed snake-case/camel-case fields and migration names across backend/frontend/docs returned no matches.

The new completed-member test proves persisted pins/proof through the public and VM update paths, but it does not subsequently execute merge import/recovery. The existing merge owner still checks the *current* source snapshot against pinned provider source before recognizing an already-merged provider result; advancing the source can therefore remain a separate recovery precondition. This predates the reviewed repair and is not a new finding in this delta. Do not describe the new test as proving arbitrary post-advance merge recovery succeeds.

No database/build/UI/provider test was run here, as requested. Main's running checks may alter a fixture after the hash above. Fresh database setup, live GitHub, deployed/native guest paths and process/restart behavior remain outside this reviewer's executed evidence. Metadata updates should use the main agent's actual final results, retaining any unexecuted guarantees.
