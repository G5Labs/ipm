# Design simplifications suggested by the model

Proposals only. Product repairs were not authorized, and none of these was implemented or modeled as a repair variant. Each one names the obligations it would make easier to keep, the defects in `bugs_issues.md` it would remove, and the behavior it must preserve.

## S1: Validate the whole load before the first write

Today, refusals are spread across the write sequence of `LoadNewModule` (`Utils/Module.cls:1152-1364`):
- path checks in `Main:LoadInternal`;
- the downgrade and update-required guards inside the transaction (`:1213-1222`);
- the no-transaction marker before the manifest import (`:1226`);
- manifest validation inside `OBJ.Load`;
- system requirements after the developer-mode commit (`:1321`).

Proposal:
1. Read the manifest once (`GetModuleObjectFromPath`, `:1164`).
2. Name the log (`SetName`, `SetVersion`).
3. Run every refusal on the in-memory object: path, manifest validity, downgrade, update-required, system requirements and the developer-mode overwrite guard.
4. Only then start the transaction or set the no-transaction marker.

- Obligations: `load.refusal` (no module data written by a refusal, in every mode), `history.committed` (no false "committed"), and `history.fields` `Package`.
- Removes: D4 (requirements after the dev commit), V4 (`Committed=1` with nothing kept under `-DNoTransaction`), D9 and V7 (empty package on refusals). It also makes Q-17 moot.
- Preserve: developer mode keeps the entries of an installable module that fails later (#363). Same-version loads succeed (PR #887).
- Check: model cases B1-C08, C09, C11-C15, C18, C23 and C24 conform. B2 cases B2-B4 and g3 conform.

## S2: One owner for the durable history row

Today a log has two copies, an IRISTEMP row and a namespace twin. The twin is written at `Init` and again from `%OnClose` (`HistoryTemp.cls:9-73`). Whether the twin survives depends on the transaction level at those moments and on when IRIS runs `%OnClose` (premises P-06a, P-06b, P-06c). Dependency logs are created and closed inside the parent's transaction.

Proposal:
- The durable `History` row is the only copy that matters after a failure. Every setter writes it directly, through one rollback-proof write: process journaling off for that save only (`%IPM.Utils.JournalManager(0)`), so neither the action's own rollback, a parent's rollback nor a process end undoes it. IRISTEMP is then only a cache.
- The rollback-proof write is premise R-01. N-P07 shows unjournaled sets survive `TROLLBACK` and the flag is restored. Mixed journaled and unjournaled updates of the same row inside a transaction, and SIGKILL, are checked natively by B2 (`premises.md`).
- `Committed` is written at each commit and set back to 0 by the rollback path when a parent's rollback undoes a dependency.
- `Finalized` is set only together with `TimeEnd`.
- A deferral (write the rows once after the top-level action settles) is simpler but does not survive a process end inside the action, so it would not remove V2, V9 or D10.

- Obligations: `history.survives-rollback` (including dependencies), `history.record` (an unfinished record lists its phases), `history.record.abnormal-end` (Q-08 provisional), and `history.locked.unfinished` (Q-21 provisional).
- Removes, if R-01 holds: D1, D10, V1, V2, V9 and V10. It also removes the model's reliance on `%OnClose` ordering (P-06c).
- If R-01 fails: only the outcomes at a normal return (A1, A3, A5) are repairable. Process-end cases (B1-C45K/T, C3, C4R) stay failing.
- Preserve: finalized rows are immutable (`history.locked`). A top-level failure reads `Committed=0`. IDs follow start order.
- Caller-held transactions (Q-05): the same unjournaled write would also make the record survive a caller's rollback. Whether IPM should write outside a caller's transaction at all is a maintainer decision.

## S3: Explicit `Committed` marking; drop `|| ISOK` from `Finalize`

Control 05 shows that `Finalize`'s `Committed = Committed || ISOK(status)` (`AbstractHistory.cls:131`) never changes the value on the load path, because every successful path already calls `SetCommitted`.

Proposal: mark `Committed` only where a write is kept, and let `uninstall`, the one caller that relies on `|| ISOK`, mark it explicitly.
- Obligations: `history.committed`, `history.committed.uninstall`.
- Check: control 05 becomes a detected control once `uninstall` is modeled.

## S4: Structured history filters

`ConstructSQLWhere` parses operators out of strings, stops at the first name it rejects (`General/History.cls:103-105`), and ignores query errors in `find` (`:132-150`). The retention purge builds its filter as a quoted string (`:173`).

Proposal: filters are `(column, operator, value)` triples. Columns are checked against the table and rejected with an error, and the purge passes a triple.
- Obligations: `history.read.filter`, `history.delete`, `history.retention`, Q-10.
- Removes: D2 and D3, and the silent empty listing on query errors (R1-ONT-10).
- Preserve: wildcard `*`, comparison operators, the `-confirm` gate, and the SQL-injection guard.

## S5: One exit path for a phase

In `ExecutePhases`, a phase's end is recorded on two paths: the catch for thrown errors and the line after success (`Storage/Module.cls:695`, `:710`). A returned error from an Invoke bypasses both (`:669-671`, `:704`).

Proposal: record the phase end in one place that every exit passes through.
- Obligation: `history.phases`, Q-14.
- Removes: D6 and V5.

## S6: Declared setting defaults

`config delete` restores whatever value was first stored as the setting's "default" (`Repo/UniversalSettings.cls:101`, `:178-179`).

Proposal: declare factory defaults in one table that both `GetValue` and `ResetToDefault` read.
- Obligation: `history.retention` (0 disables it), `settings`.
- Removes: D5.
