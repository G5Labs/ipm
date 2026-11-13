# Runtime quantitative refinement

Baseline `312b09609d76d539de502e5eb68f787fa37c96ef`. This continues the existing Ben uplift and adopts RT-P01–RT-P03; it does not restore either archived campaign, change production or run runtime tests. Source/model correspondence and final receipts live in [the fresh runtime campaign](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/TLA_verification/ben-quantitative/runtime/README.md).

The original quantitative intent rows change as follows:

| Row | Before → after | Adopted semantic owner |
|---|---|---|
| RT-08 | missing → adequate | `g5.runtime.control.replay` in hosts.g5.md |
| RT-09 | missing → adequate | `g5.runtime.events` in hosts.g5.md |
| RT-14 | partial → adequate | existing `g5.workspace.module-execution` amendment |

These are intent changes only. Fresh formal association checks selected command, event and journal trajectories; it does not turn the whole quantitative row into proved enforcement. All other quantitative statuses and earlier historical evidence remain unchanged.

## Model-driven precision

Command budget is an **admission** bound. The caller's wait may expire while host work continues; the same method/key/original payload must be retried. `ctl.rs` hashes the complete command parameters, including the budget, so retry cannot silently change those parameters. A cached result is checked before the second post-gate budget test. Completed result retention is host-process memory; event/sequence persistence is separate. No crash-durable command cache or exact lifetime literal was introduced.

The event contract now explicitly retains sequence watermarks after pruning and requires cleanup to preserve acknowledgment progress for facts still awaiting host replay. Complete inventory and stale-incarnation guards alone do not establish that progress. The recovery amendment requires successful durable lost-supervision settlement before listening, preserves the exact previously settled outcome, and retains unacknowledged journal records beyond the soft target.

## Event deletion investigation

`HostCommandProcessor` runs Stop independently of the control carrier. Host Stop journals Exited before returning `Stopped` (`g5d-host/workspace_vm.rs:1325–1349`), but `ctl.rs:750–757` selects command results and journal changes independently. The API releases custody from a definite Stop reply (`workspace_vm_service.rs:980–1034`), so workspace Delete can pass its stopped check before the Exited notification is acknowledged.

Delete retains the workspace tombstone but removes both `worker_vm_events` and `workspace_vms` (`workspace_service.rs:2990–3077,3100–3144,3180–3240`). A later event still satisfies the event table's workspace FK and organization RLS; ingestion checks the host, records the fact, and the absent VM row makes release effects a no-op (`host_control_state.rs:365–538`, `workspace_vm_release.rs:47–75,143–185`). Normal ctl delivery is ordered; no arbitrary network reordering is needed.

The contiguous query computes `first_gap.seq - 1` (`host_control_state.rs:870–897`). With a removed prefix and only final event 3 retained, it returns 2. The host retains event 3 and reconnect replays it, while the API keeps returning 2. Fresh `HostEvents` checks and independent workspace review confirm the bounded source correspondence. `RUNTIME-EVENT-DELETE` is stuck journal acknowledgment/retention, not replacement state corruption or missing acknowledged history. The temporal trace remains divergent with actual ordered control messages and snapshot-batch exclusion; the retained-ledger comparison passes.

Minimal comparison: retain the existing per-VM event ledger through pending replay instead of clearing it at workspace tombstoning. This introduces no second durable sequence owner. It is a repair candidate, not an implemented change or a decision to retain all data forever; eventual safe cleanup must be designed with the workspace deletion owner. Changing only the SQL to calculate the longest prefix starting at 1 does not reconstruct a deliberately deleted prefix.

## Terminal producer gap: RUNTIME-EXIT-FACT

Source binding: Stop removes `active`, releases roots, ignores the Exited write error and returns Stopped (`workspace_vm.rs:1329–1348`). Natural-exit status and the monitor do likewise (`1390–1450`); competing observers find no active record and do not retry that fact. `EventJournal::record` restores memory after a failed persist (`ctl.rs:124–126`). A temp-file create/write/fsync failure before rename (`159–179`) leaves the old on-disk journal intact, so this case does not rely on ambiguous post-rename errors.

`TerminalFact` demonstrates physical exit → active removal → pre-rename write failure → successful Stop reply → restart without durable terminal replay evidence (`SuccessfulStopRecoverable`). This is a source-bound fact-reporting gap, not proof of continuing compute, stranded API custody, or failure of process-tree cleanup. Existing process-mode custody metadata (`runtime.rs:256–305`) and CLI recovery (`host.rs:603–678`) remain recognized; neither inspected path reconstructs the missing event payload.

The conditional repair comparison reuses the existing journal for a durable obligation **before commanded stop**, retains the local owner/receipt until terminal persistence succeeds, and reaches successful in-process retry. Its crash path retains an unfinished reconciliation obligation, erases the local receipt and does not fabricate an exact exit outcome. Completion after that restart remains unresolved repair design. The comparator excludes spontaneous exit before stop reservation; status/monitor would need the same reporting obligation established under their actual lifecycle owner. This is a repair candidate, not an implemented fix or a new governing owner.

## Independent review and evidence status

The workspace reviewer checked command replay after disconnect, event replay of a crash-retained fact, deletion versus ordered response handling, snapshot batch exclusion, and the terminal persistence/recovery boundary. Two witness-ordering issues, one stream-order abstraction issue and one witness predicate typo were corrected as model-development issues, not application findings. Canonical comparison against baseline confirms all pre-existing host blocks unchanged and only the authorized agent `g5.workspace.module-execution` body amended; anchors remain unique.

Read-only identity cross-review covered all three models, all 39 configurations, manifest, bindings, README and canonical additions. Direct source samples: `workspace_messages.rs:314–455`, migration032:1–51, chat route2160–2210, owner FK migration003:111–142, transfer1520–1610, membership850–885/935–970, fork materialization1847–1885, auth852–955/1069–1118, invitation revoke1267–1296. No new concrete blocker was found within the stated bounds. PostgreSQL key-lock semantics, conditional physical child deletion, invitation expiry beyond admission, and unmodeled authority writers remain explicit limits; the provisional admission Q is not promoted to settled policy.

The final coordinator runtime run matched **55/55 cases with zero unexpected outcomes**, including all 9 terminal-fact cases: 8 safety, 21 controls, 21 witnesses, 3 liveness checks and 2 source-bound divergences. The frozen formal/source bindings and per-case receipts govern freshness. Earlier intermediate errors are superseded. No production reproduction or runtime test is claimed.
