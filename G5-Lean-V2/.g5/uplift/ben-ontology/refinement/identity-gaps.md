# Identity refinement: incremental gap dispositions

Baseline `832651d0bb3b1754eb9511d1cc5ccde7c2aafde4`. No production edits or runtime tests authorized. Model findings are investigated against source before classifying a defect.

- Current source trace supports assign/delete row-lock serialization and approval policy snapshot locking; no production divergence claimed for those two boundaries.
- CLI request consumption precedes token issuance. A crash/issue failure can consume the grant without returning credentials; start a new request. The recovered intent promises at-most-once exchange, not durable retry of minted credentials. Do not report this as a defect without stronger recovery intent.
- Budget and granular-grant lifecycle remain unresolved candidates; neither is promoted. Existing role-ceiling and UI preference gaps remain unchanged.

## CI-LAUNCH-CONFIRMATION — confirmed choices are lost between enqueue and attempt admission

**Classification:** source-bound implementation divergence from recovered C21, fresh `queued-launch-asbuilt` TLC receipt fails `ExecutionHonorsConfirmation` at the queued-settings-change trace; no runtime reproduction. No production repair made.

**Evidence:** `config_service.rs:687-720,848-862` checks a submitted confirmation at launch/enqueue. `workspace_messages.rs:259-288` saves resolved next-turn display/default fields but queues only `launch.preferences`. `agent_dispatch.rs:785-793` stores only those preferences in the task spec; `:1420-1440` re-resolves them at delayed dispatch and `:1503-1515` freezes that new resolution. `agent_launch.rs:116-161` makes omitted field preferences `InheritConfigured`. A user can confirm inherited model/account A, enqueue behind capacity, then an authorized settings writer changes the inherited choice to B; the first attempt can freeze B without re-confirmation.

**Authority reconciliation:** former `g5.settings.launch-confirmation` explicitly rejects changing the confirmed launch; its agent provenance remains. The newer v9 design `:616-620,807,815` requires current-owner credentials/current per-attempt resolution. That requirement remains: obsolete credentials must not be reused. No inspected later decision explicitly retires submitted confirmation. Revalidate the visible confirmed choices against the current allowed resolution; a change must refuse and request review instead of silently selecting another account/model. Ordinary unconfirmed launches may continue inheriting current defaults. Secret rotation alone and later continuation policy are not the counterexample.

**Minimal repair proposal:** carry the optional confirmed resolution/visible-choice binding with the queued input/task and compare it before the first attempt's frozen snapshot; keep owner/account authorization current. Reuse the existing inbox and snapshot owners, with a stale-confirmation outcome and fresh-review retry; no parallel launch authority store.

**Discriminating proposed runtime reproduction (not run):** with no execution slot free, submit a confirmed default-inheriting launch, update its org model/account, release a slot, and inspect the authoritative first attempt snapshot. Expect stale refusal/required fresh confirmation rather than B execution. Control: an otherwise identical unconfirmed launch uses B; successful fresh confirmation can execute B.

## Final executed dispositions

The final 73-case identity run matched all declared outcomes. `queued-launch-intended` satisfies the confirmation invariant; `queued-launch-reconfirmed` reaches successful first-attempt admission after stale refusal and fresh confirmation; `queued-launch-live-default-witness` preserves ordinary unconfirmed inheritance. Other negative cases remove guards deliberately and are controls, not code gaps. [Receipts and exact bounds](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/identity/README.md) retain the distinction.

The root-owned fresh WebSocket org-subscription gap applies to `g5.security.current-authority`; it is not canceled by this domain's passing locked-transaction pattern. No additional production divergence is claimed for account locks, approval capture locks, attachment preflight or capacity.

## Independent workspace model cross-check

Read the workspace domain's models, manifests, bindings, README, canonical additions/proposals and selected retained traces. Directly compared reader admission/observation/commit (`workspace_reviews.rs:960–1180`), review application/checkpoint/SQL decisions (`:1396–1429,1640–1818,1912–2090,2222–2335`; `workspace_review_service.rs:4410–4540`), captured composer ownership (`useComposerStore.ts:1–389`, `suggestionRecords.ts:1–29`, `ReviewsPanel.tsx:1545–1668`), policy admission/drain (`workspace_mutation_policy.rs:1–109,125–246,247–341`, `workspace_service.rs:2249–2539,2540–2825`, `routes/chat.rs:1688–1720,1790–1848`) and PR identity (`workspace_pull_request_create.rs:381–580`, `workspace_pull_request_provider_lifecycle.rs:490–580,600–755,895–1125`). No production files were edited or executed.

The interrupted review application divergence is source-supported: the guest effect survives loss of its SQL transaction, while a later contrary decision sees an open SQL row. Reader staleness is correctly qualified at the observed read, captured-store async completion preserves principal isolation, and policy finalization excludes the inspected SHARE-locked task admission. PR replacement and first provider publication are distinct, with recorded merge recovery excluded.

Two concrete evidence corrections were sent to the workspace owner and accepted. The original `review-recovery-witness` ended with the intended `Close` recovery branch and did not retry acceptance; the owner added explicit post-crash retry progression through recognition, successful checkpoint and commit. The refreshed trace was independently read and reaches `Write → Crash → Start → Write recognition → Checkpoint → Commit`, with the other edit still open. The PR binding now includes `provider_lifecycle.rs:615–685`, whose dispatch selects prior versus recorded provider evidence. These are model-evidence corrections, not newly discovered product defects; the workspace owner owns the refreshed receipts.

One additional boundary was reported: `ContentPolicy.Revert` modeled freezing cancellation, not the separate steady read-only-to-editable restoration path. The owner accepted a distinct `RestoreEditing` action and witness for the final workspace campaign. Broader renderer coverage, hunk/byte correctness, all task producers and provider HTTP ambiguity remain outside this cross-check.
