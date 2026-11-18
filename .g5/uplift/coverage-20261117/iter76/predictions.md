# Iteration 76 predictions: what survives a failing load

Written 2026-11-18T05:38Z, before reading `Utils/Module.cls::LoadNewModule`, `Utils/JournalManager.cls`, the resource processors, `Lifecycle/Base.cls` or the History classes at this iteration, and before any iteration76 run.

## Exposure

Read before writing this page:

- Ontology pages `concepts/transactions-and-dev-mode`, `features/load` (`load.outcome`, `load.outcome.dev`, `load.outcome.dev.early-failure`, `load.status.after-commit`), `features/history` (`history.committed`, `history.committed.side-effects`, `history.dependencies.parent-rollback`, `history.record`), `features/resources` (`module.processor.file-copy`, `module.processor.cpf-effects`), `features/web-applications` (`module.webapp.failure`), `features/dependencies` (`deps.lockfile`), `features/artifact-download` (`artifact.failure`).
- Models `consolidated/iter64/failkeep.rs` (per-kind phase and external tables; it says the CPF merge runs before Initialize), `consolidated/verus/loadtx.rs` (flag merge, `survived(w, cut)` with `journal` as input), `consolidated/iter5/webapp.rs` (`failed_after_activate` table).
- Gap texts in census, `iter72/HANDOFF.md` §6 item 10 and `reviews/review-71.md` §D. They name `LoadNewModule`, the JournalManager restore at `Utils/Module.cls:1191-1193`, the lock write in `LoadDependencies` (`:1323-1328`), and G:D1 (a dependency's permanent history row is lost with the parent's rollback).

So the ontology and models already state most of the normal-mode table. The predictions below that go beyond them are marked **new**.

## Fixture plan

Module `rb76` (version 1.0.0) with, in manifest order: a class `Rb76.Hook`, a `<Mapping>` of global `Rb76M` to another database, a web application `/csp/rb76`, a CPF merge that creates role `Rb76Role`, an immediate `FileCopy` to `/tmp/iter76/out/imm/`, a `Defer` `FileCopy` to `/tmp/iter76/out/def/`, and an Activate `<Invoke>` that throws when the file `/tmp/iter76/FAIL` exists. Loads run with and without `-create-lockfile`.

Readers: module record (`%IPM.Storage.Module` exists), mapping (`Config.MapGlobals:Exists`, plus the `iris.cpf` file text and the live namespace map), web application (`Security.Applications:Exists`), CPF effect (`Security.Roles:Exists`), files (`##class(%File).Exists`), lock file (`module-lock.json` in the module root and in the process directory), history row (latest `%IPM.General.History` row: exists, `Success`, `Committed`).

## Predicted histories

| ID | History | Record | Mapping | Web app | CPF role | Immediate file | Deferred file | Lock file | History row / Committed |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P1 | normal, Activate Invoke fails | absent | absent | absent | present | present | absent | present | row present, Success 0, Committed 0 |
| P2 | normal, success | present | present | present | present | present | present | present | Success 1, Committed 1 |
| P3 | `-DNoTransaction=1`, fails | present | present | present | present | present | absent | present | Committed 1 |
| P4 | `-DNoJournal=1`, fails | present | present | present | present | present | absent | present | Committed 1 |
| P5 | `-dev`, fails in Activate (after first commit) | present | present | present | present | present | absent | present | Committed 1 |
| P6 | `-dev`, fails before its first commit | absent | absent | not created | **new:** present if the CPF merge runs before the first commit, else absent | absent | absent | uncertain | Committed 0 |

Notes on the table:

- P1 and P2 differ only in the failure. The web application and the mapping are the database writers that turn from present to absent. The CPF role, the immediate file and the lock file do not change. The deferred file is the only filesystem effect that changes (Defer runs after a successful Activate).
- **New (P1):** the mapping has three readers. I expect `Config.MapGlobals:Exists` to read absent after rollback. I am unsure whether `iris.cpf` still holds the `[Map.USER]` line and whether the live namespace map still routes `^Rb76M`. If the Config API writes the CPF file or activates the map outside the transaction, one of these differs from the database reader. That would qualify `transaction.rollback-scope` B4.
- **New (P3 vs P4):** NoJournal behaves like NoTransaction for every reader here. The model should derive both from "no transaction open" and not from journaling alone.
- **New (P5):** in developer mode the web application is created after the first commit, outside any transaction, so it survives. failkeep had no developer-mode Activate failure case.

## Dependency histories

Parent `rbp` (normal mode, fails in Activate) depends on `rbd`.

| ID | `rbd` `<Defaults>` | `rbd` record | `rbd` history row | `rbp` record | `rbp` web app (created after `rbd` loaded) | `rbp` row Committed |
| --- | --- | --- | --- | --- | --- | --- |
| P7 | `NoJournal=1` | present (escapes the rollback) | **new:** absent, because the row is inserted under the parent's journaled transaction before `rbd`'s own flags apply | absent | absent (journaling restored after `rbd`'s activation) | **new:** 0, which contradicts the working assumption `history.committed` B10 |
| P8 | none | absent | absent (G:D1) | absent | absent | 0 |
| P9 | `NoJournal=1`, `rbd` requires `rbe` (journaled) | present | as P7 | absent | absent | 0; `rbe` absent, so `rbd` is left without its dependency |

If P7's `rbd` row is present with Committed 1, the working assumption B5 holds as built.

## Filesystem and lock histories

| ID | History | Prediction |
| --- | --- | --- |
| P10 | `FileCopy` with a missing source, normal load | load fails, target absent, record absent |
| P11 | `uninstall rb76` inside `TSTART` ... `TROLLBACK` | record present again, copied immediate file still removed (**new**: Clean's delete is not restored) |
| P12 | first `load -create-lockfile` of `rb76` (record not yet stored), failing | `module-lock.json` written relative to the process directory (C:D16), not under the module root, and still present after the rollback |
| P13 | reload with `-create-lockfile` (record stored, Root set), failing | **new:** the earlier record's Root is the stored one inside the transaction, so the lock file goes under the module root and survives |

## Process end and compile

| ID | History | Prediction |
| --- | --- | --- |
| P14 | an Activate hook that `HALT`s, run as a background job, normal mode | record absent (open transaction rolled back at process exit); history row present with no end time |
| P15 | same with `-DNoTransaction=1` | record present |
| P16 | two classes, the second fails to compile, normal | both class definitions absent, neither callable |
| P17 | same with `-DNoTransaction=1` | both definitions present, first callable, second not |

## Model predictions

The successor model should compute each survivor from the position of its writer step and the storage class of what it writes:

- journaled global written inside an open transaction: undone by its rollback;
- write with process journaling off, or with no transaction open: kept;
- a `%SYS` configuration write: same as a journaled global in the caller's transaction (provider premise; native P1);
- a child process (CPF merge): commits on its own, kept (provider premise; native P1);
- filesystem write or delete: kept and never restored (provider premise);
- history row: the top-level row is restored from a temporary copy after the rollback; a dependency row is a plain journaled write.

Committed is then derived: 1 exactly when some database write of that action is kept.
