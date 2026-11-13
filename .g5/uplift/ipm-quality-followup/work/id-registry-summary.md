# ID registry summary: IPM Verus uplift (ipm-verus-20261113)

Companion to `id-registry.json` in this folder. Built from the read-only artifacts under `.g5/uplift/ipm-verus-20261113/`, `.g5/Verus_verification/*.md` (plus `crates/ipm-load-history/BINDINGS.md` for citations only) and `.g5/ontology/**/*.g5.md`. No source file was edited. Paths below are relative to `.g5/`.

How the registry was built:

- One object per identifier and owning artifact. An ID that is defined in several places (for example Q-05 in A1, in the ontology and in the bugs_issues.md deviation table) has one object per owner, with the family telling them apart.
- `statement` is the first sentence of the defining cell or heading, copied, not paraphrased. `original_status` is copied from the defining source. Later status changes are in `notes`.
- `referenced_by` is found by matching the ID across every `.md` file named above, including shorthand such as `INT-LOAD-05, -06`, `OBS-L4, L5`, `B1-C44T/K`, `C08-C18`, `D1-D6` and ranges like `Q-01 to Q-17`. Bare labels with several meanings (A1, B2, C1-C3, D1, R1-R4, S1-S4, F1, P-10) use context heuristics, and their entries say so. Treat those lists as approximate. Coverage rows have an empty `referenced_by`. Cite the A1, A2 or ontology entry with the same ID instead.
- The unique key of an object is (`id`, `family`, `defined_at`). The same `id` and `family` can repeat, for example A2 `P-10` vs Verus `P-10`, or a commitment that has rows in several coverage-map sections.
- `in_bugs_issues_md` is true when `Verus_verification/bugs_issues.md` contains the ID with the same matcher.

## 1. Counts per family

| Family | Count | What is in it |
|---|---|---|
| coverage-row | 163 | A3 coverage-map rows that are partial/unmatched/unowned/no-operation/section-4 clauses/section-9 non-conforming, and Verus coverage-map rows that are not modeled, unestablished, unresolved or violated |
| native-case | 146 | N-P01..N-P20 (+N-P20AN, N-P20C, N-P20C2), B2 phase-1 cases, stage C cases, B1-C01..C52 with T/K variants |
| other | 123 | A1 conflicts 1-37, A3 disagreements 1-10, A4 uncertainties U1-U13, bugs_issues.md deviation rows, L1 contract clauses, controls, theorems, model variants, round-2 reproduction steps |
| A2-operation | 108 | CMD-1..CMD-39 (A2 1.2, labelled CMD-n by the coverage map), 14 EP rows (A2 1.3), OP-LOAD-01..29, OP-HIST-01..26 |
| review-finding | 66 | R1-ONT-01..14, R1-MOD-01..15, R2-01..11, R3-01..09, A4 reconstruction rows R1..R17 |
| Q | 47 | Q-01..Q-17 as defined in A1; Q-18..Q-23 at first numbering; model-only Q-06b and Q-FIN; 22 ontology Q blocks by ontology ID |
| A1-catalog | 42 | CAT-01..CAT-42 (A1 section 1) |
| A1-commitment | 35 | INT-LOAD-01..18 and INT-HIST-01..17 (A1 sections 2.1 and 3.1) |
| A2-OBS | 34 | OBS-L1..L17, OBS-H1..H16, OBS-X1 |
| premise | 26 | A2 P-1..P-10; Verus P-01..P-05, P-06a/b/c, P-07..P-13, R-01 |
| defect | 20 | D1..D20 (D1-D6 and D9-D20 confirmed, D7 withdrawn, D8 reclassified as I1) |
| model-finding | 12 | V1..V12 |
| simplification | 6 | S1..S6 |
| D-decision | 5 | 5 ontology D blocks |
| issue | 2 | I1, I2 |
| **total** | 835 | |

Native-case breakdown: 23 provider-premise checks (N-P*), 59 B1 model cases replayed, 64 B2 phase-1, reader, repro and stage C cases.

Q breakdown: 17 A1 rows (Q-01..Q-17), 6 later first-numbering rows (Q-18..Q-23), 2 model-only parameters (Q-06b, Q-FIN) and 22 ontology Q blocks. The ontology also has 5 D blocks, one of which (`history.delete.global-unfiltered`) was Q-12. That matches the stated "149 blocks: 122 I, 5 D, 22 Q" (handoff.md:11).

## 2. OBS rows (34 found: 17 load, 16 history, 1 cross-cutting)

| OBS | Title (A2) | A2 label | In bugs_issues.md | Later status (from notes) |
|---|---|---|---|---|
| OBS-L1 | Dependency history is lost when the parent rolls back | code-read, P-2, P-4, P-5 | no | Later status: confirmed natively as defect D1 (B2 cases A1-A3, "as A2 OBS-L1 predicted", uplift/ipm-verus-20261113/B/native-report.md:88); model V1/V2. bugs_issues.md D1 does not cite OBS-L1 by name. |
| OBS-L2 | `.tar.gz` archives are not recognized by `load` | code-read | no | Later status: confirmed as D15 by stage C case S1 ("DEVIATES, as A2 OBS-L2 predicted", uplift/ipm-verus-20261113/B/native-replay.md:188). bugs_issues.md D15 does not cite OBS-L2 by name. |
| OBS-L3 | A plain `load <dir>` does not persist Root/Repository/Deployed in `LoadNewModule` | code-read | no | Later status: never run natively or modeled; not in the bugs_issues.md "Suspected, not observed natively" list and not in the handoff suspected list (handoff.md:90), although A/coverage-map.md still lists it as partial (rows INT-LOAD-15, CAT-35, deps.lockfile, dev-mode.sticky). Status lags: no disposition anywhere. |
| OBS-L4 | `update <m> -dev` from a registry ends in non-dev mode | code-read | yes (221) | Later status: suspected (bugs_issues.md suspected list; handoff.md:90 "L4"). A/coverage-map.md INT-LOAD-07 row partial. |
| OBS-L5 | A dependency newly installed during `update` runs all its update steps instead of seeding them | code-read | yes (222) | Later status: suspected (bugs_issues.md suspected list; handoff.md:90). |
| OBS-L6 | The `update -path` module-mismatch guard runs after the manifest and archive were written | code-read | yes (156) | Later status: prediction refuted by stage C S3 ("OBS-L6's prediction (\"B's manifest stays\") is refuted", uplift/ipm-verus-20261113/B/native-replay.md:194); the path still fails differently and is confirmed as D16 (crash with <INVALID OREF>). A/coverage-map.md CAT-15 row still cites OBS-L6 as partial evidence. |
| OBS-L7 | Archive installs and loads delete the installed module's directory before any guard; rollback does not restore it | code-read, P-7 | yes (223) | Later status: suspected (bugs_issues.md suspected list; handoff.md:90). Falls under Q-04 (`transaction.rollback-scope`). |
| OBS-L8 | `%Compile` ignores the status of resource processors' `OnPhase("Compile")` | code-read | no | Later status: never run natively or modeled; absent from both the bugs_issues.md suspected list and the handoff suspected list; only A/coverage-map.md (CAT-07, module.processor) and ontology `module.processor` metadata carry it. |
| OBS-L9 | Update steps are silently skipped when the update lock is not acquired in 3 s | code-read | yes (224) | Later status: suspected (bugs_issues.md suspected list; handoff.md:90). |
| OBS-L10 | `-dev` commits the manifest before system requirements are checked | code-read, consistent with the comment at `1310` | no | Later status: confirmed as D4 (B2 cases B2-B4, "as A1 INT-LOAD-11 (d) and A2 OBS-L10 predicted", uplift/ipm-verus-20261113/B/native-report.md:104), decision pending Q-17; model V3. A2 itself said "Listed for the matrix. Not an inconsistency with the comment." |
| OBS-L11 | Lock file is written before the parent's commit | code-read | yes (225) | Later status: suspected (bugs_issues.md suspected list; handoff.md:90). |
| OBS-L12 | A `.modules` directory that cannot be cached makes a successful load report `<INVALID OREF>` | code-read, needs native confirmation | no | Later status: confirmed as D13 via model case B1-C29 (V6) and replay 20261113T193952Z-replay-return-b; round 2 review notes "OBS-L12 is still \"not yet run\". This is D13, B1-C29" (C/round-2/review.md:177). bugs_issues.md D13 does not cite OBS-L12 by name; round 1 review says bugs_issues then had "only OBS-L12 under \"suspected\"" (C/round-1/review-model.md:87). |
| OBS-L13 | URL loads leave clones behind and ignore the git exit code | code-read | yes (220) | Later status: suspected and never run (bugs_issues.md first suspected item; R2-08; handoff.md:88 "git URL loads (OBS-L13)" under native checks not run). The handoff suspected list (handoff.md:90) omits L13. |
| OBS-L14 | `PermitDowngrade` does not reach `LoadNewModule`'s downgrade refusal | code-read | yes (226) | Later status: suspected (bugs_issues.md suspected list; handoff.md:90). A3 disagreement 1 keeps `PermitDowngrade` out of intent and treats OBS-L14 as a note on `load.refusal.downgrade`, so whether it is a defect is unresolved. |
| OBS-L15 | `LoadedResource` rows survive a rolled-back load | code-read, P-2 | yes (227) | Later status: suspected (bugs_issues.md suspected list; handoff.md:90). premises.md P-10 puts `LoadedResource` (OBS-L15) out of scope. |
| OBS-L16 | Legacy XML payload installs run outside any transaction but record `Committed=0` on failure | code-read | no | Later status: not run (native-replay.md:226 "not run (optional)"; handoff.md:85). Not in the bugs_issues.md suspected list. Model notes: the legacy XML path is the one caller relying on `\|\| ISOK` (R1-MOD-07). |
| OBS-L17 | Uninstalling a module that others require, without `-force`, reports success | code-read | yes (228) | Later status: suspected (bugs_issues.md "OBS-L17 and OBS-H8"; handoff.md:90). Related CAT-16 / INT-HIST-06 partial rows. |
| OBS-H1 | One invalid filter column silently widens `history find` and `history delete` | code-read | no | Later status: confirmed as D2 (B2 E1, E2, E4, E8). bugs_issues.md D2 does not cite OBS-H1 by name. |
| OBS-H2 | Namespace default modifiers become history filters | code-read | no | Later status: not exercised in phase 1 ("A2 OBS-H2 (default-modifier filters) was not exercised", uplift/ipm-verus-20261113/B/native-report.md:240); confirmed in stage C case H2 ("A2 OBS-H2 confirmed", uplift/ipm-verus-20261113/B/native-replay.md:200) as D18, decision pending Q-18. |
| OBS-H3 | Auto-purge likely deletes nothing | needs native confirmation | no | Later status: confirmed as D3 (B2 F1, repro autopurge-noop). A2 label was "needs native confirmation". |
| OBS-H4 | `config delete HistoryRetain` restores the first value ever set | code-read | yes (229) | Later status: confirmed natively as D5 (B2 F0). Contradiction inside bugs_issues.md: it is still listed under "Suspected, not observed natively" as "OBS-H4: covered by D5" (Verus_verification/bugs_issues.md:229). |
| OBS-H5 | Same as OBS-L1, seen from the history side. | (no status label; "Same as OBS-L1, seen from the history side") | no | Alias of OBS-L1 ("Same as OBS-L1, seen from the history side"); confirmed with it as D1. |
| OBS-H6 | The durable row lags until `%OnClose` | code-read | no | Later status: refuted for the Terminate path (N-P10 REFUTED; native-report.md section 7 lists "A2 OBS-H6, which predicted the Init-value row for Terminate"); holds for SIGKILL (C3, C4R). Renamed in the ontology: "Confirmed product defect D10, formerly OBS-H6" (ontology/features/history.g5.md:362). R1-ONT-02 replaced the OBS-H6 metadata. Not cited by name in bugs_issues.md. |
| OBS-H7 | Action and source attribution for nested logs | code-read | no | Later status: partly observed natively (A0 dependency record has `Action=load` and `SourceName=ipm-temp-modules-1`); governed by Q-15. Not in bugs_issues.md; not in handoff suspected list. |
| OBS-H8 | Recursive uninstall records skipped dependencies as successful uninstalls | code-read | yes (228) | Later status: suspected (bugs_issues.md "OBS-L17 and OBS-H8"; handoff.md:90). |
| OBS-H9 | `history delete -globally` needs no filter or confirmation | code-read, consistent with the `-confirm` help text "Ignored if -globally is passed too" | no | Later status: documented behavior; A4 R10 turned Q-12 into the D block `history.delete.global-unfiltered`, whose metadata cites OBS-H9. Not a defect. Not in bugs_issues.md. |
| OBS-H10 | Global readers and namespaces without IPM | needs native confirmation | yes (230) | Later status: split. The wrong-rows part is confirmed as D20 (stage C H10, "A2 OBS-H10 confirmed"). The remaining part ("IPM is not enabled" message unreachable) stays suspected (Verus_verification/bugs_issues.md:230). The handoff suspected list omits OBS-H10. |
| OBS-H11 | `Records` declares `CONTAINID=1` while its first column is `Action` | code-read, `History.cls:28` | no | Later status: not run ("`Records` ID column not run", A/coverage-map.md:174; ontology history.g5.md "Not run: `Records` declares an ID column it does not return first (OBS-H11)"). Absent from bugs_issues.md and from the handoff. |
| OBS-H12 | AutoPurge runs only from `Shell`, and its failure blocks the command | code-read | yes (59, 231) | Later status: suspected, purge failure not run (bugs_issues.md suspected list; D3 "Related" note calls it "(A2 OBS-H12, suspected)"; handoff.md:90). |
| OBS-H13 | Full cleanup kills the instance-wide history temp global | code-read, P-1 | yes (232) | Later status: suspected, not run (concurrent loads; bugs_issues.md suspected list; handoff.md:83, :90). |
| OBS-H14 | Invoke failures leave the phase unfinalized | code-read | no | Later status: confirmed as D6 (repro activate-invoke-phase-open), decision pending Q-14; model V5. R2-07 widened D6 beyond Activate. bugs_issues.md D6 does not cite OBS-H14 by name. |
| OBS-H15 | Self-uninstall reports success before it runs and exits the shell even on failure | code-read | yes (233) | Later status: suspected, not run (self-uninstall; only its persist step probed in N-P16). bugs_issues.md suspected list; handoff.md:84, :90. |
| OBS-H16 | A caller's transaction absorbs IPM's commits and the history row | code-read, P-4 | no | Later status: observed natively by case D1 ("DEVIATES (as A2 OBS-H16 predicted)"); filed as a deviation under Q-05 (bugs_issues.md deviation row Q-05, model V8), not as a numbered defect. bugs_issues.md does not cite OBS-H16 by name. |
| OBS-X1 | "Namespace" default modifiers are stored in a `^%` global | needs native confirmation, P-10 | no | Later status: confirmed as D19 by stage C case X1 ("A2 OBS-X1 confirmed"). A2 premise P-10 (not run in phase 1) is thereby observed. |

Mentioned in bugs_issues.md: OBS-L4, OBS-L5, OBS-L6, OBS-L7, OBS-L9, OBS-L11, OBS-L13, OBS-L14, OBS-L15, OBS-L17, OBS-H4, OBS-H8, OBS-H10, OBS-H12, OBS-H13, OBS-H15.
Not mentioned in bugs_issues.md: OBS-L1, OBS-L2, OBS-L3, OBS-L8, OBS-L10, OBS-L12, OBS-L16, OBS-H1, OBS-H2, OBS-H3, OBS-H5, OBS-H6, OBS-H7, OBS-H9, OBS-H11, OBS-H14, OBS-H16, OBS-X1.

OBS rows with no final disposition anywhere: OBS-L3, OBS-L8 and OBS-H11 are not run, not modeled and absent from both suspected lists (bugs_issues.md and handoff.md:90). OBS-L16 and OBS-H7 are only in "not run" or Q metadata. The handoff suspected list (handoff.md:90: "OBS-L4, L5, L7, L9, L11, L14, L15, L17, H8, H12, H13, H15") leaves out OBS-L13, OBS-H4 and the remaining part of OBS-H10, which bugs_issues.md lists.

## 3. Q identifiers, ontology IDs and handoff placement

| Q | Ontology ID (file:line) | Current provisional answer (ontology) | First defined | Handoff decision table (handoff.md) |
|---|---|---|---|---|
| Q-01 | `dev-mode.sticky` (ontology/concepts/transactions-and-dev-mode.g5.md:116) | Provisional answer: yes, it keeps the stored mode. | uplift/ipm-verus-20261113/A/intent-inventory.md:190 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-02 | `load.refusal.update-required.dev-bypass` (ontology/features/load.g5.md:114) | Provisional answer: yes. A maintainer test expects a newer version to install with `-dev`, and the original request asked IPM to point users to `-dev` or `-force`. | uplift/ipm-verus-20261113/A/intent-inventory.md:191 | line 65: own row |
| Q-03 | `dev-mode.overwrite-guard.force` (ontology/concepts/transactions-and-dev-mode.g5.md:107) | Provisional answer: keep the current scope. The guard covers `install` only, and `-force` does not lift it. | uplift/ipm-verus-20261113/A/intent-inventory.md:192 | line 66: own row |
| Q-04 | `transaction.rollback-scope` (ontology/concepts/transactions-and-dev-mode.g5.md:24) | Provisional answer: no. Files copied into the module root or by `FileCopy`, pip installs, CPF merges, the folder of a git clone, IRISTEMP data and calls to registries stay after a rollback. | uplift/ipm-verus-20261113/A/intent-inventory.md:193 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-05 | `history.committed.nested` (ontology/features/history.g5.md:151) | Provisional answer: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit. | uplift/ipm-verus-20261113/A/intent-inventory.md:194 | line 60: collapsed row "Q-05 / Q-06b" |
| Q-06 | `history.dependencies.parent-rollback` (ontology/features/history.g5.md:176) | Provisional answer: it keeps the dependency's own `Success`, because `Success` is the status that action returned. | uplift/ipm-verus-20261113/A/intent-inventory.md:195 | absent from the table |
| Q-07 | `history.committed.side-effects` (ontology/features/history.g5.md:161) | Provisional answer: no. It is defined by database commits and by writes outside a transaction. | uplift/ipm-verus-20261113/A/intent-inventory.md:196 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-08 | `history.record.abnormal-end` (ontology/features/history.g5.md:58) | Provisional answer: `Committed` reflects the changes kept, even on an unfinished record. IPM writes it to the permanent record at each commit. | uplift/ipm-verus-20261113/A/intent-inventory.md:197 | line 61: own row |
| Q-09 | `history.record.unlogged-refusals` (ontology/features/history.g5.md:49) | Provisional answer: record them. Examples are `update` or `reinstall` of a module that is not installed, and `uninstall` with no name. | uplift/ipm-verus-20261113/A/intent-inventory.md:198 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-10 | `history.read.filter.invalid-column` (ontology/features/history.g5.md:214) | Provisional answer: filters on listed columns, including version parts, must work, and IPM must never silently drop a filter. When `find` cannot run its query, it reports the error instead of showing an empty log. | uplift/ipm-verus-20261113/A/intent-inventory.md:199 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-11 | `history.delete.confirm-with-filter` (ontology/features/history.g5.md:262) | Provisional answer: only the matching ones. `-confirm` only permits a delete without a filter. | uplift/ipm-verus-20261113/A/intent-inventory.md:200 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-12 | `history.delete.global-unfiltered` (ontology/features/history.g5.md:271) | decision (D block); provenance tags: human-provided, user-facing-behavior, security | uplift/ipm-verus-20261113/A/intent-inventory.md:201 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-13 | `history.retention.effect` (ontology/features/history.g5.md:307) | Provisional answer: no. The command should still run, which is a proposed change. | uplift/ipm-verus-20261113/A/intent-inventory.md:202 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-14 | `history.phases.returned-errors` (ontology/features/history.g5.md:115) | Provisional answer: yes. The docs read a phase with no end time as a hang or crash, so an ordinary error must not look like one. | uplift/ipm-verus-20261113/A/intent-inventory.md:203 | line 63: own row |
| Q-15 | `history.fields.dependency-source` (ontology/features/history.g5.md:95) | Provisional answer: keep the `load` label for dependencies of `load` and `update`, keep their stored source, and keep the rule that listings show a source only for installs. | uplift/ipm-verus-20261113/A/intent-inventory.md:204 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-16 | `history.keep-on-ipm-uninstall.other-namespaces` (ontology/features/history.g5.md:321) | Provisional answer: no. History belongs to each namespace. | uplift/ipm-verus-20261113/A/intent-inventory.md:205 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-17 | `load.refusal.requirements.timing` (ontology/features/load.g5.md:129) | Provisional answer: yes. The manifest docs say requirements are checked "before module installation", and a module that fails them was never installable. | uplift/ipm-verus-20261113/A/intent-inventory.md:206 | line 57: own row |
| Q-18 | `history.read.filter.default-modifiers` (ontology/features/history.g5.md:223) | Provisional answer: no. Default modifiers are meant for package changes, such as `-DNoTransaction=1`, while `history` filters are explicit requests. | uplift/ipm-verus-20261113/A/coverage-map.md:189 | line 64: own row |
| Q-19 | `reinstall.version` (ontology/features/install.g5.md:43) | Provisional answer: the version already installed, as the command summary and its examples say. | uplift/ipm-verus-20261113/A/coverage-map.md:190 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-20 | `history.delete.running-records` (ontology/features/history.g5.md:276) | Provisional answer: a delete or purge may remove any matching record, and the running action must still finish normally whether or not its record comes back. | uplift/ipm-verus-20261113/A/reconstruction-dispositions.md:20 | line 67: collapsed row "Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19, Q-20 (Q-12 became a decision)" |
| Q-21 | `history.locked.unfinished` (ontology/features/history.g5.md:290) | Provisional answer: no. A finalized record is one whose action has ended, so a record without an end time stays unfinalized. | uplift/ipm-verus-20261113/C/round-1/repairs-ontology.md:3 | line 62: own row |
| Q-22 | `transaction.flag-sources.dependency-defaults` (ontology/concepts/transactions-and-dev-mode.g5.md:50) | Provisional answer: yes. A module's `<Defaults>` apply whenever that module is installed, including as a dependency. A `NoJournal` dependency therefore keeps its changes when the parent rolls back, and its history record must survive and say so. | uplift/ipm-verus-20261113/C/round-2/repairs-ontology.md:3 | line 58: own row |
| Q-23 | `transaction.flag-sources.dependency-no-transaction` (ontology/concepts/transactions-and-dev-mode.g5.md:59) | Provisional answer: yes. A module that declares `NoTransaction` must not run inside a transaction opened for its parent, so the package change runs without one, and a failure may leave a partial install. | uplift/ipm-verus-20261113/C/round-2/repairs-ontology.md:16 | line 59: own row |
| Q-06b | `history.committed.nested` (ontology/features/history.g5.md:151) | Provisional answer: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit. | uplift/ipm-verus-20261113/B/model-notes.md:36 | line 60: collapsed row "Q-05 / Q-06b" |
| Q-FIN | `history.locked.unfinished` (ontology/features/history.g5.md:290) | Provisional answer: no. A finalized record is one whose action has ended, so a record without an end time stays unfinalized. | uplift/ipm-verus-20261113/B/model-notes.md:40 | absent from the table |

Where the handoff collapses several Q into one row:
- handoff.md:60 "Q-05 / Q-06b" merges ontology Q-05 (`history.committed.nested`) with the model-only parameter Q-06b, which is the survival half of the same block. It is not Q-06.
- handoff.md:67 merges Q-01, Q-04, Q-07, Q-09, Q-10, Q-11, Q-13, Q-15, Q-16, Q-19 and Q-20 into one row, with "see the coverage map", "see the ontology" and "no current defect verdict", plus "(Q-12 became a decision)".
- Q-06 (`history.dependencies.parent-rollback`) appears in no handoff row. It is still an open Q block (22 Q blocks include it), and its provisional `Success` value could not be observed natively because D1 loses the record.
- Q-FIN appears only in the model. Its ontology counterpart Q-21 has its own handoff row (line 62).
- bugs_issues.md has its own deviation table keyed by Q labels: "Q-05", "Q-08" and "new Q (round 1)" (= Q-21) at Verus_verification/bugs_issues.md:192 to Verus_verification/bugs_issues.md:194. These rows are registered as family `other`.

Ontology Q blocks whose metadata never states their Q number: `history.read.filter.default-modifiers` (Q-18, metadata says "new question from A2 OBS-H2"), `history.locked.unfinished` (Q-21, "new question from round 1 R1-ONT-02") and `reinstall.version` (Q-19, "A1 conflict 27"). The number comes only from A/coverage-map.md section 6.

## 4. IDs whose status differs between files

| ID | One file says | Another file says |
|---|---|---|
| D6 | native-report.md lists it as a plain confirmed defect (uplift/ipm-verus-20261113/B/native-report.md:216) | bugs_issues.md heading "(decision pending: Q-14)" (Verus_verification/bugs_issues.md:77), after R1-MOD-12. A/coverage-map.md still says "confirmed defect D6" (uplift/ipm-verus-20261113/A/coverage-map.md:271), and ontology Q-14 metadata says "Confirmed product defect B2 D6" (ontology/features/history.g5.md:445) |
| D8 / I1 | native-report.md "D8: an open phase is displayed as ..." (uplift/ipm-verus-20261113/B/native-report.md:220) | bugs_issues.md "I1 (issue, formerly D8)" (Verus_verification/bugs_issues.md:198). A/coverage-map.md still says "reader defect D8" (uplift/ipm-verus-20261113/A/coverage-map.md:271) |
| D9 | native-report.md lists it under "Further reader observations", not in the defect table (uplift/ipm-verus-20261113/B/native-report.md:221) | bugs_issues.md files it under "## Confirmed" (Verus_verification/bugs_issues.md:85). A/coverage-map.md says "(confirmed, D9)" (uplift/ipm-verus-20261113/A/coverage-map.md:170) but also "deviation D9" (uplift/ipm-verus-20261113/A/coverage-map.md:266) |
| D14 | round-1 dispositions.md: "Orchestrator: suspected defect until observed" (uplift/ipm-verus-20261113/C/round-1/dispositions.md:38) | bugs_issues.md under "## Confirmed" with "(decision pending: Q-22)" (Verus_verification/bugs_issues.md:134), after R2-03 |
| D11 / V12 | model-notes.md V12 Native column: "C51/C52 not yet run natively" (uplift/ipm-verus-20261113/B/model-notes.md:81) | native-replay.md: B1-C51 and B1-C52 replayed in `20261113T205137Z-replay-c51-c52` (uplift/ipm-verus-20261113/B/native-replay.md:66) |
| OBS-H4 / D5 | bugs_issues.md "Suspected, not observed natively": "OBS-H4: covered by D5" (Verus_verification/bugs_issues.md:229) | the same file confirms D5 natively (B2 F0) (Verus_verification/bugs_issues.md:69) |
| OBS-L6 | A2 code-read prediction (uplift/ipm-verus-20261113/A/operation-inventory.md:583) | refuted by stage C S3 (uplift/ipm-verus-20261113/B/native-replay.md:194); A/coverage-map.md marks it refuted at uplift/ipm-verus-20261113/A/coverage-map.md:161 yet still lists it as partial evidence for CAT-15 (uplift/ipm-verus-20261113/A/coverage-map.md:81) |
| OBS-H6 | A2 prediction of the Init-value row (uplift/ipm-verus-20261113/A/operation-inventory.md:632) | refuted for Terminate by N-P10 (uplift/ipm-verus-20261113/B/native-report.md:233); renamed in the ontology as "D10, formerly OBS-H6" (ontology/features/history.g5.md:362) |
| OBS-L10 | A2: "Listed for the matrix. Not an inconsistency with the comment." (uplift/ipm-verus-20261113/A/operation-inventory.md:596) | confirmed defect D4, decision pending Q-17 (Verus_verification/bugs_issues.md:61) |
| INT-HIST-03 (d) | A1 qualifier "Such a record keeps Success=0, Committed=0 and Finalized=0 ... Its IRISTEMP row is orphaned until restart" (uplift/ipm-verus-20261113/A/intent-inventory.md:129) | refuted for the Terminate path by N-P10 and case C2 (uplift/ipm-verus-20261113/B/native-report.md:234) |
| CAT-33 | A1 note "History records land in the VERIFY namespace (d)" (uplift/ipm-verus-20261113/A/intent-inventory.md:56) | A3 disagreement 3: "A2 is right" (no record for the module itself) (uplift/ipm-verus-20261113/A/coverage-map.md:186) |
| Q-03 | A1 provisional: "Treat the wiki \"or use `-force`\" as a doc error" (uplift/ipm-verus-20261113/A/intent-inventory.md:192) | ontology `dev-mode.overwrite-guard.force` flags a maintainer conflict instead (R1-ONT-05) (ontology/concepts/transactions-and-dev-mode.g5.md:107) |
| Q-05 | A1 scope includes dev-mode dependencies inside a parent (uplift/ipm-verus-20261113/A/intent-inventory.md:194) | ontology `history.committed.nested` covers caller-held transactions only and adds record survival (R1-ONT-01) (ontology/features/history.g5.md:151) |
| Q-06 | A1 provisional "Yes" (dependency record must survive) (uplift/ipm-verus-20261113/A/intent-inventory.md:195) | ontology `history.dependencies.parent-rollback` asks only about `Success` and survival became intent (ontology/features/history.g5.md:176); absent from the handoff table (handoff.md:55-67) |
| Q-08 | A1 provisional "unknown outcome" (uplift/ipm-verus-20261113/A/intent-inventory.md:197), kept by A3 R3 (uplift/ipm-verus-20261113/A/reconstruction-dispositions.md:11) | ontology provisional "Committed reflects the changes kept" after R1-ONT-02 (ontology/features/history.g5.md:58) |
| Q-09 | A1 provisional "Not required" (uplift/ipm-verus-20261113/A/intent-inventory.md:198) | ontology provisional "record them" after R1-ONT-06 (ontology/features/history.g5.md:49) |
| Q-12 | A1 open question (uplift/ipm-verus-20261113/A/intent-inventory.md:201) | ontology D block `history.delete.global-unfiltered` (ontology/features/history.g5.md:271); A/coverage-map.md section 8 still says "Q-01 to Q-19 need maintainer answers" (uplift/ipm-verus-20261113/A/coverage-map.md:238) |
| Q-13 | A1 two-part question (uplift/ipm-verus-20261113/A/intent-inventory.md:202) | ontology `history.retention.effect` narrowed to the purge-failure case by A4 R10 (ontology/features/history.g5.md:307) |
| Q-15 | A1 "Propose labeling ... `install` or displaying their source" (uplift/ipm-verus-20261113/A/intent-inventory.md:204); A3 R9 disposition "label a repository-pulled dependency `install` with its source" (uplift/ipm-verus-20261113/A/reconstruction-dispositions.md:17) | ontology provisional "keep the `load` label" (R1-ONT-07), alternatives restored by R3-09 (ontology/features/history.g5.md:95) |
| Q-21 | bugs_issues.md deviation row label "new Q (round 1)" (Verus_verification/bugs_issues.md:194) | A/coverage-map.md names it Q-21 (uplift/ipm-verus-20261113/A/coverage-map.md:221) |
| Q-22 | round 2 wording "`NoTransaction` or `NoJournal`" (uplift/ipm-verus-20261113/C/round-2/repairs-ontology.md:7) | ontology now `NoJournal` only, with Q-23 split off by R3-03 (ontology/concepts/transactions-and-dev-mode.g5.md:50). The A/coverage-map.md header attributes Q-21..Q-23 to "../C/round-1/repairs-ontology.md" (uplift/ipm-verus-20261113/A/coverage-map.md:6), but Q-22 and Q-23 are in C/round-2/repairs-ontology.md |
| P-06c, P-10 (Verus) | round 3 review: "`premises.md` still lists P-06c as UNVERIFIED and P-10 as PARTIAL" (uplift/ipm-verus-20261113/C/round-3/review.md:78) | premises.md now CONFIRMED for both after R3-05 (Verus_verification/premises.md:16, Verus_verification/premises.md:20), matching native-replay.md section 6 |
| P-13 | premises.md: CONFIRMED with delay (Verus_verification/premises.md:23); model-notes.md "P-01..P-13" (uplift/ipm-verus-20261113/B/model-notes.md:146) | handoff.md "P-01..P-11 ... confirmed natively. P-12 not run" (uplift/ipm-verus-20261113/handoff.md:14) and Verus_verification/coverage-map.md:21 omit P-13 |
| N-P19, N-P20 | premises.md header: "B2 IDs are N-P01..N-P18" (Verus_verification/premises.md:3) | N-P19 and N-P20 exist (native-replay.md section 6) |
| V8, V9, V10 | model-notes.md section 4 "Confirmed as-built violations" (uplift/ipm-verus-20261113/B/model-notes.md:77) | native-report.md section 6 "Deviations governed by open questions (not claimed as defects)" (uplift/ipm-verus-20261113/B/native-report.md:223) and the bugs_issues.md deviation table |
| B1-C* (final cases.json) | native-replay.md calls `05842975` the "final `cases.json`" (uplift/ipm-verus-20261113/B/native-replay.md:11) | model-notes.md lists later revisions d3334b16 (r2) and 10410864 (r3, final) (uplift/ipm-verus-20261113/B/model-notes.md:127); Verus_verification/coverage-map.md uses `10410864` (Verus_verification/coverage-map.md:11) |
| B1-C K/T replays | model-notes.md: "K variants and C49T/K were still running at the last read" (uplift/ipm-verus-20261113/B/model-notes.md:138) | native-replay.md section 3 reports them complete (sigkill-b and approx-c runs) |
| history.survives-rollback.temp-copy (D) | ontology metadata: premise "refuted for process termination" (ontology/features/history.g5.md:407) | Verus_verification/coverage-map.md: "supported for top-level actions at `$TLEVEL` 0" with no Terminate caveat (Verus_verification/coverage-map.md:67) |
| Round 1 dispositions | round-1 dispositions.md has three "Accept, modified" rows (R1-ONT-02, R1-ONT-06, R1-MOD-08) | handoff.md says round 1 findings were "accepted, five with modifications" (uplift/ipm-verus-20261113/handoff.md:126) |
| EP rows (A2 1.3) | A2 counts "Non-shell entry-point groups (§1.3) \| 13" (uplift/ipm-verus-20261113/A/operation-inventory.md:690) | the A2 1.3 table has 14 rows (operation-inventory.md:89-102) |
| Q-10 / D2 | handoff.md catch-all row lists Q-10 with "no current defect verdict" (uplift/ipm-verus-20261113/handoff.md:67) | ontology Q-10 metadata cites "Confirmed product defect B2 D2" (ontology/features/history.g5.md:566). D2 is judged against the I block `history.read.filter`, so both can hold, but the wording invites a misreading |
| As-built proof count | round 3 dispositions: "as-built proof 260/0 on unchanged sources" (uplift/ipm-verus-20261113/C/round-3/dispositions.md:3) | model-notes.md and handoff.md: final run verified 263 (uplift/ipm-verus-20261113/B/model-notes.md:44). This is time-ordered (before and after the R3 repairs), not a contradiction |

## 5. Coverage-map rows that are partial, unmodeled, unestablished or not run

Every row below is also an object in the JSON (family `coverage-row`). Status and reason are quoted from the row.


### A/coverage-map.md section 1 (commitment map)

| Row ID | Line | Status / reason |
|---|---|---|
| INT-LOAD-01 | 17 | partial: `.tar.gz` not recognized (OBS-L2); git clone kept and exit code ignored (OBS-L13, no intent either way) |
| INT-LOAD-02 | 18 | partial: an existing relative path is accepted; no test |
| INT-LOAD-03 | 19 | partial: install folder replaced before version checks and not restored (OBS-L7, Q-04); no `load` archive test |
| INT-LOAD-04 | 20 | partial: legacy XML packages bypass it (OBS-L16) |
| INT-LOAD-06 | 22 | partial: nested dependency commits are not final (Q-05) |
| INT-LOAD-07 | 23 | partial: `update -dev` from a registry ends in normal mode (OBS-L4) |
| INT-LOAD-08 | 24 | partial: rollback scope (Q-04); default-modifier storage scope (OBS-X1) |
| INT-LOAD-10 | 26 | partial: `-dev` bypass unresolved (Q-02) |
| INT-LOAD-11 | 27 | partial: check runs after the manifest write in developer and no-transaction modes (Q-17, OBS-L10) |
| INT-LOAD-12 | 28 | partial: `-force` and `load` coverage unresolved (Q-03); no test |
| INT-LOAD-14 | 30 | partial: cleanup can fail after commit (OBS-L12) |
| INT-LOAD-15 | 31 | partial: plain load keeps developer mode (Q-01); lock file path and survival after rollback (OBS-L3, OBS-L11) |
| INT-LOAD-16 | 32 | partial: no explicit check on the `load` archive path |
| INT-LOAD-18 | 34 | partial: no test |
| A1 2.2 valid completion and neighbors | 35 | partial: tests do not read module state after failures |
| INT-HIST-02 | 42 | partial: dependency records lost (D1); caller transactions open (Q-05) |
| INT-HIST-03 | 43 | partial: phase listing after a crash; Q-08; Q-21 |
| INT-HIST-04 | 44 | partial: D9; dependency label (Q-15) |
| A1 3.2 Phases | 45 | partial: returned Invoke errors leave a phase open (Q-14, OBS-H14) |
| INT-HIST-05 | 46 | partial: nested transactions (Q-05, OBS-H16); side effects (Q-07); legacy XML (OBS-L16); requirement refusal reads committed (Q-17) |
| INT-HIST-06 | 47 | partial: skipped uninstall reads success while the module stays (OBS-L17, OBS-H8) |
| INT-HIST-08 | 49 | partial: D1; Q-06 (`Success`), Q-15 |
| INT-HIST-09 | 50 | partial: update and uninstall argument refusals unrecorded (Q-09) |
| INT-HIST-10 | 51 | partial: dropped filters (Q-10, OBS-H1); default modifiers as filters (Q-18, OBS-H2) |
| INT-HIST-11 | 52 | partial: `GlobalRecords` and `Records` shape (OBS-H10, OBS-H11) |
| INT-HIST-12 | 53 | partial: Q-11; deleting a running action's record (Q-20) |
| INT-HIST-13 | 54 | partial: D3, D5; Q-13; Q-20 |
| INT-HIST-14 | 55 | partial: Q-21 |
| INT-HIST-15 | 56 | partial: shared IRISTEMP global killed (Q-16, OBS-H13); success recorded before Clean (OBS-H15) |
| INT-HIST-17 | 58 | unmatched: future option only |
| CAT-02 | 68 | partial: `Scope="verify"` deferred |
| CAT-03 | 69 | partial: dependency `Scope` described only by code and tests |
| CAT-05 | 71 | partial: Q-17 |
| CAT-06 | 72 | partial: returned errors leave phase open (Q-14) |
| CAT-07 | 73 | partial: Compile ignores processor status (OBS-L8); `<WSGIApplication>` has no processor |
| CAT-09 | 75 | partial: lock timeout skips steps (OBS-L9); new dependency runs all steps (OBS-L5); mirroring future |
| CAT-10 | 76 | partial: `${authUnauthenticated}` not implemented |
| CAT-11 | 77 | partial: `register` phase documented but missing |
| CAT-14 | 80 | partial: version unresolved (Q-19) |
| CAT-15 | 81 | partial: OBS-L4, OBS-L5, OBS-L6 |
| CAT-16 | 82 | partial: dependents not protected (OBS-L17); `-purge` has no reader |
| CAT-19 | 85 | partial: no operation in A2 |
| CAT-20 | 86 | partial: `-app`, `-local`, `-info`, `-p4` not implemented |
| CAT-26 | 92 | partial: `config delete` restores first value (OBS-H4) |
| CAT-27 | 93 | partial: storage scope (OBS-X1); history filters (Q-18) |
| CAT-28 | 94 | partial: `NoLock` undocumented and inverted; `PermitDowngrade`, `IgnoreInstalled` code-only (disagreement 1) |
| CAT-30 | 96 | partial: Q-01, Q-03 |
| CAT-35 | 101 | partial: OBS-L3, OBS-L11; install from lock file is future |
| CAT-36 | 102 | partial: `run-from-file` help disagrees with the format |
| CAT-40 | 106 | partial: payload undocumented |
| CAT-41 | 107 | partial: purge failure blocks the command (Q-13) |

### A/coverage-map.md section 2 (operations without an owner)

| Row ID | Line | Status / reason |
|---|---|---|
| OP-LOAD-08 | 116 | no ontology owner; disposition: structural |
| CMD-2 | 117 | no ontology owner; disposition: structural |
| EP %IPM.Main:ShellInternal | 118 | no ontology owner; disposition: structural |
| EP .ZPM document (%IPM.StudioDocument.Module) | 119 | no ontology owner; disposition: structural |
| EP other @API methods: LoadedResource Validate/Clear/ReportConflicts, LogManager, Utils.Build, DependencyAnalyzer run methods, XSLTProvider, LocalizationExport, TempFileManager, GetDependentsList/GetDependentsAsTree | 120 | no ontology owner; disposition: excluded |
| EP SQL procedures and queries (GET_DEFAULT_RESOURCE_PROCESSOR, OrphanedResources, VersionRequirements and others) | 121 | no ontology owner; disposition: excluded |
| EP source-control hooks (%IPM.ExtensionBase.SourceControl.Interface) | 122 | no ontology owner; disposition: gap |
| Lifecycle.Module:%Activate Studio project creation in developer mode | 123 | no ontology owner; disposition: excluded |
| Lifecycle.Module:%Activate update-step seeding | 124 | no ontology owner; disposition: structural, noted |

### A/coverage-map.md section 3 (commitments without an operation)

| Row ID | Line | Status / reason |
|---|---|---|
| INT-HIST-17 | 130 | no operation; disposition: Future option. No block, because it is not a current commitment. |
| CAT-35 | 131 | no operation; disposition: Future. Stated as "planned but not available". |
| CAT-09 | 132 | no operation; disposition: Future (`wiki/08` L5). The class comment's promise has no operation. |
| CAT-10 | 133 | no operation; disposition: Documented, not implemented. Not stated as intent. |
| CAT-11 | 134 | no operation; disposition: Documented in help, not implemented. Not stated as intent. |
| CAT-20 | 135 | no operation; disposition: Documented in help, not implemented. Not stated as intent. |
| CAT-07 | 136 | no operation; disposition: No processor. Not stated as intent. |
| CAT-16 | 137 | no operation; disposition: Operation accepts the flag, but nothing reads it. Suspected gap. |
| CAT-16 | 138 | no operation; disposition: Operation skips instead of refusing. Suspected gap. |
| CAT-37 | 139 | no operation; disposition: Documentation missing. |

### A/coverage-map.md section 4 (partially covered clauses)

| Row ID | Line | Status / reason |
|---|---|---|
| load.path | 147 | partially covered clause (A3 section 4); what is missing: Refused (confirmed, D15) |
| load.refusal.path | 148 | partially covered clause (A3 section 4); what is missing: An existing relative path is accepted (confirmed, D17) |
| load.refusal, load.refusal.manifest | 149 | partially covered clause (A3 section 4); what is missing: Late manifest checks under NoTransaction and validation refusals under NoJournal keep module data (confirmed, D11); refusals after the marker read committed (D12) |
| load.refusal.requirements | 150 | partially covered clause (A3 section 4); what is missing: Checked after the manifest write and the developer-mode first commit (confirmed, B2 D4: B2-B4) |
| load.refusal.downgrade | 151 | partially covered clause (A3 section 4); what is missing: Holds. `PermitDowngrade` honored by the graph but not the guard |
| load.path.archive, install.package | 152 | partially covered clause (A3 section 4); what is missing: Install folder replaced before version checks |
| load.dependencies.local-repo | 153 | partially covered clause (A3 section 4); what is missing: Cleanup fails with `<INVALID OREF>` after commit (confirmed, D13); left registered after a process end (I2) |
| load.write-check | 154 | partially covered clause (A3 section 4); what is missing: Only repository archives are checked |
| load.core | 155 | partially covered clause (A3 section 4); what is missing: Legacy XML packages bypass it |
| load.outcome | 156 | partially covered clause (A3 section 4); what is missing: A dependency's own `<Defaults>` `NoJournal` escapes the rollback (D14, decision pending); its `NoTransaction` does not suppress the parent's transaction |
| deps.lockfile | 157 | partially covered clause (A3 section 4); what is missing: Wrong folder on a first plain load; survives rollback |
| dev-mode.not-propagated, update.dev-mode | 158 | partially covered clause (A3 section 4); what is missing: Module itself ends in normal mode when updated from a registry |
| dev-mode.sticky | 159 | partially covered clause (A3 section 4); what is missing: Open, and `Root` and `Repository` are not saved |
| update.steps | 160 | partially covered clause (A3 section 4); what is missing: Lock timeout skips steps; new dependency runs all steps |
| update.refusal | 161 | partially covered clause (A3 section 4); what is missing: Crashes with `<INVALID OREF>` before the check; `Committed=1` under NoTransaction (confirmed, D16; OBS-L6 refuted) |
| module.processor | 162 | partially covered clause (A3 section 4); what is missing: Compile ignores processor status |
| history.survives-rollback | 163 | partially covered clause (A3 section 4); what is missing: Lost (confirmed, B2 D1) |
| history.record, history.phases | 164 | partially covered clause (A3 section 4); what is missing: Permanent record keeps only start values, so no phases and an empty package show until the end |
| history.record.abnormal-end | 165 | partially covered clause (A3 section 4); what is missing: Open |
| history.phases | 166 | partially covered clause (A3 section 4); what is missing: Returned Invoke errors leave the phase open (confirmed, B2 D6) |
| history.committed, history.committed.nested, history.dependencies.parent-rollback | 167 | partially covered clause (A3 section 4); what is missing: Nested commits for every dependency and caller transactions; legacy XML failure reads uncommitted |
| history.committed.uninstall, uninstall.dependents, uninstall.recurse | 168 | partially covered clause (A3 section 4); what is missing: Skipped uninstall reads success and committed |
| history.fields | 169 | partially covered clause (A3 section 4); what is missing: Dependency records labeled `load` carry a source, listing shows `n/a` |
| history.fields | 170 | partially covered clause (A3 section 4); what is missing: Refused loads whose manifest was read record no package (confirmed, D9) |
| history.locked | 171 | partially covered clause (A3 section 4); what is missing: `$SYSTEM.Process.Terminate` finalizes an unfinished developer-mode record (B2 C2) |
| history.read.filter, history.delete | 172 | partially covered clause (A3 section 4); what is missing: First invalid column drops the rest, widening delete (confirmed, B2 D2); `find` hides query errors (E3) |
| history.read.filter | 173 | partially covered clause (A3 section 4); what is missing: Default modifiers become filters |
| history.read.api | 174 | partially covered clause (A3 section 4); what is missing: Returns the current namespace's rows once per namespace (confirmed, D20); `Records` ID column not run |
| history.retention | 175 | partially covered clause (A3 section 4); what is missing: Cutoff likely matches nothing; error blocks commands; `config delete` restores first value |
| history.keep-on-ipm-uninstall, uninstall.ipm | 176 | partially covered clause (A3 section 4); what is missing: Shared IRISTEMP global killed; success recorded before Clean |
| settings.default-modifiers | 177 | partially covered clause (A3 section 4); what is missing: Instance-wide (confirmed, D19) |
| settings | 178 | partially covered clause (A3 section 4); what is missing: Restores the first value ever set (confirmed, B2 D5) |

### A/coverage-map.md section 9 (native status, non-conforming rows)

| Row ID | Line | Status / reason |
|---|---|---|
| load.refusal.update-required.dev-bypass (Q-02), dev-mode.sticky (Q-01) | 248 | consistent with the provisional answer (g5, g8, g9) |
| load.refusal.requirements.timing (Q-17) | 249 | confirmed defect D4 pending the decision (B2-B4); B0, B1 conform |
| load.path (.tar.gz) | 252 | confirmed defect D15 (S1); `.tgz` neighbour S1N conforms |
| load.refusal, load.refusal.manifest | 253 | confirmed defects D11 (S2, S2B, B1-C10, B1-C52) and D12 (B1-C08); S2N conforms |
| update.refusal (-path mismatch) | 254 | confirmed defect D16 (S3, S3N1); OBS-L6 refuted; S3N2 conforms |
| load.refusal.path | 255 | confirmed defect D17 (S4R); S4N1, S4N2 conform |
| load.dependencies.local-repo | 256 | confirmed defect D13 (B1-C29); issue I2 (leftover temporary repositories after a process end) |
| transaction.flag-sources.dependency-defaults (Q-22), transaction.flag-sources.dependency-no-transaction (Q-23, NoTransaction variant not run), load.outcome | 257 | M5 keeps a `NoJournal` dependency after the parent's rollback (D14, decision pending); M5N rolls back; both lose the dependency record (D1) |
| transaction.flag-sources, settings.default-modifiers | 258 | confirmed defect D19 (X1, instance-wide defaults) |
| history.read.filter.default-modifiers (Q-18) | 259 | D18, decision pending (H2) |
| history.read.global, history.read.api | 260 | `find -globally` conforms; `GlobalRecords` confirmed defect D20 (H10) |
| history.record | 261 | D10 named (C1, C3, C4R); model cases C44-C48 T/K agree |
| legacy XML (load.core) | 262 | not run |
| history.survives-rollback, history.dependencies, history.dependencies.parent-rollback | 263 | confirmed defect D1 (A1-A3) |
| history.committed.nested (Q-05) | 264 | deviates (D1 caller case); D0 conforms |
| history.record, history.record.abnormal-end (Q-08), history.locked.unfinished (Q-21), history.survives-rollback.temp-copy | 265 | C1, C3, C4R show no phases; C2 finalized without end; C4R `Committed=0`; temp-copy premise refuted for Terminate (N-P10) |
| history.fields | 266 | deviation D9 (refused loads record no package) |
| history.read.filter.invalid-column (Q-10) | 267 | confirmed defect D2 (E1, E2, E4, E8); E3/E7 differ |
| history.delete | 268 | E5, E6 conform; E4 under D2 |
| history.delete.running-records (Q-20) | 269 | observed: the record disappears and is not recreated (N-P15) |
| history.retention, history.retention.effect, settings | 270 | confirmed defects D3 (F1) and D5 (F0); purge failure not run |
| history.phases, history.phases.returned-errors (Q-14) | 271 | confirmed defect D6; reader defect D8 |
| history.keep-on-ipm-uninstall and its Q-16, uninstall cases | 274 | not run (self-uninstall persist step probed in N-P16; IRISTEMP sharing confirmed in N-P06, N-P17) |

### Verus_verification/coverage-map.md

| Row ID | Line | Status / reason |
|---|---|---|
| load.path, load.path.archive | 29 | `.tgz` supported. `.tar.gz` confirmed violation D15 (regression `f83b207e`). Git URL loads unestablished (no native or formal evidence; suspected OBS-L13, clone folder under Q-04) |
| load.core | 30 | unestablished for install and update beyond the suites |
| load.options | 31 | supported for those flags. `-env`, `-bypass-py-deps`, `-create-lockfile`, `-skip-sync` unestablished |
| load.refusal | 32 | supported for downgrade and update-required in every mode. Confirmed violations: D4 (requirements, decision Q-17), D11 (manifest refusals under NoTransaction or NoJournal keep module data), D12 (`Committed=1` with nothing kept), D16 (`update -path` mismatch crashes) |
| load.refusal.path | 33 | relative nonexistent and absolute paths supported. Existing relative path accepted: confirmed violation D17 |
| load.refusal.manifest | 34 | default and `-dev` supported. NoTransaction and NoJournal: confirmed violations D11, D12. Refusals record no package: D9 |
| load.refusal.update-required | 36 | supported. Q-02 provisional matches the product (unresolved authority for the docs) |
| load.refusal.requirements | 37 | confirmed violation D4 under Q-17 provisional (decision pending) |
| load.outcome | 39 | supported for database state within scope (P-05, P-06b confirmed). Confirmed violation D13 (failure reported after the commit, B1-C29). D14 (dependency `<Defaults>` NoJournal escapes the rollback) is decision pending (Q-22). Non-database effects (Q-04) unestablished. Temporary repository left after a process end: issue I2 |
| load.dependencies | 41 | module state supported. Dependency history: confirmed violation D1 |
| load.dependencies.local-repo | 42 | supported by observation only |
| load.write-check | 43 | unestablished |
| transaction | 49 | supported, conditional on P-01 |
| transaction.rollback-scope | 50 | unresolved authority |
| transaction.flag-sources | 52 | confirmed violation D19 (namespace defaults shared by every namespace). Dependency `<Defaults>` inside a parent transaction: D14, decision pending (Q-22) |
| dev-mode.overwrite-guard | 55 | unestablished. Q-03 unresolved authority |
| dev-mode.sticky | 56 | provisional answer matches the product |
| history.actions | 62 | load supported. Others by suite only |
| history.record | 63 | record existence supported for top-level actions. Unfinished record content: confirmed violation D10 |
| history.record.abnormal-end | 64 | confirmed violation under the provisional answer (decision pending) |
| history.record.unlogged-refusals | 65 | unresolved authority |
| history.survives-rollback | 66 | top-level supported. Dependencies: confirmed violation D1 |
| history.fields | 68 | Success supported. Package on refusals: confirmed violation D9 |
| history.fields.dependency-source | 69 | unresolved authority |
| history.phases | 70 | confirmed violation D6 under Q-14 provisional (decision pending) |
| history.committed | 71 | "changed implies committed" supported. "Committed implies changed": confirmed violation D12 |
| history.committed.uninstall | 73 | by suite only |
| history.committed.nested | 75 | confirmed deviation under the provisional answer (decision pending) |
| history.committed.side-effects | 76 | unresolved authority |
| history.dependencies | 77 | confirmed violation D1 |
| history.namespace | 78 | supported by suite and premise |
| history.read, .find, .filter, .details | 79 | confirmed violation D2. Default modifiers as filters: D18 (decision Q-18). Unfinished phase display: issue I1 |
| history.read.global, .api | 80 | `-globally` supported. `GlobalRecords`: confirmed violation D20 |
| history.delete | 81 | confirmed violation D2 |
| history.delete.running-records | 82 | unresolved authority |
| history.locked.unfinished | 84 | confirmed violation under the provisional answer (decision pending) |
| history.retention | 85 | confirmed violations D3, D5 |
| history.keep-on-ipm-uninstall | 86 | unestablished. Self-uninstall suspected (OBS-H15) |

## 6. ID label collisions to watch when reconciling

- `D1` is both defect D1 (dependency record lost) and native case D1 (caller-held transaction, a Q-05 deviation, model V8). A/coverage-map.md:264 "D1 caller case" means the case.
- `A1`..`A4`, `B1`, `B2` are stage names (A1 intent inventory, B2 native validation) and also native case IDs. For example, "B2 D4" means stage B2's defect D4, while "B2-B4" means cases B2 to B4.
- `C1`, `C2`, `C3` are native process-end cases, and also the commit points of `LoadNewModule` (A2 OP-LOAD-11/-14/-15, the model, and the bugs_issues.md D1 mechanism). `C2` is also caveat C2 of R-01 (= N-P20C2). `C01`..`C52` are B1 cases.
- `R1`..`R4` are native reader cases, `R1`..`R17` are A4 reconstruction rows, and `R1`, `R2` are also L1 invariants and theorems (`theorem_r1`, `theorem_r2`).
- `S1`..`S4` are stage C native cases (S1 = `.tar.gz`, D15). `S1`..`S6` are also design simplifications (S1 = validate before writing). bugs_issues.md uses both.
- `P-1`..`P-10` are A2 premises and `P-01`..`P-13` are Verus premises. `P-10` means different things in the two sets (`^%` placement vs class rollback).
- `F1` is native retention case F1 and also round 2 reproduction step F1. `H2`, `H10` and `X1` are stage C cases named after OBS-H2, OBS-H10 and OBS-X1.
- Q-06 (ontology) and Q-06b (model parameter of Q-05) are different questions. Q-21 (ontology) = Q-FIN (model) = "new Q (round 1)" (bugs_issues.md).
- The round 1 ontology review uses its own expectation labels L1..L11 and H1..H12. Round 2 and round 3 use E-L*/E-H*. These collide with L1 (contract) and native H1/H2/H10. They were not registered (see section 7).

## 7. Not registered or unresolved

- Reviewer pre-read expectation labels (round 1 ontology review L1-L11 and H1-H12, round 2 E-L1..E-L8 and E-H1..E-H10, round 3 E-L1..E-L6 and E-H1..E-H5) and the source IDs A1-S01..A1-S30 in A/sources-intent.md were not registered. They are expectations or source handles, not findings.
- A1 section 5 seed-verification rows and the A1 2.2, 2.3, 3.3 and 3.4 case tables have no IDs and were not registered. The rows "A1 2.2 valid completion and neighbors" and "A1 3.2 `Phases`" appear only as coverage rows. "A1 3.3" is marked covered and is not registered. `T0` in the round 1 model review is `$TLEVEL` notation, not an ID.
- Variants 201-206 were described by the round 1 reviewer in C/round-1/review-model.md. Their per-number descriptions here are taken by order from the list in C/round-2/review.md:233. The order-based mapping is an inference.
- The D6 native run ID is not timestamped in bugs_issues.md ("listed in `index.jsonl`") and the ontology says "run ID not listed in the B2 report". `results/index.jsonl` was not read, because it is outside the files named for this task.
- `referenced_by` for collision-prone labels (section 6) uses context heuristics and is approximate. Coverage rows and EP rows have no computed `referenced_by`.
