# Independent review of the three owner-facing files (audit findings R09, R10, R11)

Reviewer: independent of the follow-up's authors. I did not write any of the reviewed files. I did not read `review/review-technical.md`, so this review is not anchored on it.

Files reviewed, all under `.g5/Verus_verification/`:

- `bugs_issues.md` (replaces the original, which is at `ae619795`)
- `open_questions.md` (new)
- `design_simplification.md` (replaces the original, at `ae619795`)

Supporting record read: `.g5/uplift/ipm-quality-followup/scope-ledger.md`, `work/id-registry.json` and its summary, `work/followup-findings.md`, the original handoff `.g5/uplift/ipm-verus-20261113/handoff.md`, the audit (R09 to R11 and context), the ontology, `followup/models/MODELS.md`, `loadtx.py` and the receipts named below.

## Review base and method

- Base: commit `8c576c4b`. During the review, commit `84b96f6e` landed with edits to all three files. I diffed them. Most edits repoint receipts from `FR/20261113T235302Z-loadtx-model`, `FR/20261113T234915Z-history-readers-model` and `FR/20261113T232332Z-replay-reeval` to the `FR/20261114T00264*Z-final-*` runs. The others reword L-12 in D1, S2 and Q-30 ("60 end states, 30 of them distinct", and the crate's variant 103 "models the mark as atomic with the commit"). No finding below depends on those edits, and every phrase quoted below is still present at `84b96f6e`.
- Every count the three files cite is identical in both receipt sets. The final loadtx run changed as-built REC-EXISTS from 9,558 to 9,942, which none of the three files cites.
- No product file, existing file, IRIS or docker was touched. Product paths are at `571e04a1` (`git diff 571e04a1 -- src` is empty). Commit `84b96f6e` also committed `review/scratch/ontology-tags.txt`, which I had created there. It is unchanged.
- Scratch work is in `review/scratch/owner-files/`: `ontology-tags.txt` (a copy of all 149 block tags with line numbers), `guarded_unfinalized.py`, `guarded_unfinalized2.py` and their output `guarded_unfinalized.out`. The two scripts import `followup/models/loadtx.py` read-only and count dependency rows that the model's `COMMITTED` clause does not inspect.
- Quotes from GitHub issue and PR pages in `open_questions.md` could not be checked, because those pages are outside the sources this review may read. Where the CHANGELOG or git history carries the same fact, I checked that instead.

### What I expected the replacements to contain

From R09: one row per unique finding with an explicit alias for every ID from A1 and A2 (34 OBS rows), the model counterexamples (V1 to V12), the native reports, the issues, the reviews and every prior disposition. Each row needs expected and observed, evidence, impact, the human decision, missing evidence and a direction. Supported, conditional, suspected, withdrawn and out-of-scope findings are counted separately. Decision-dependent defects and issues without a governing requirement are kept apart.

From R10: for every proposal, the owner, old complexity, what gets simpler, preserved behavior, tradeoffs, a decisive neighboring case, unresolved choices and exact model evidence. S4 needs a worked preserving and refusing pair, S5 and S6 need discriminating checks, the intro must stop saying nothing was modeled, S1 must stop calling Q-17 moot, and the temporary-repository, cross-namespace and dependency-flag findings need a disposition.

From R11: one section per current Q block and per new question, with links to the canonical block, competing interpretations with their authority, the decision, the affected findings and the stakes. Resolved and renamed questions and empirical unknowns must be distinguished, the one-to-many historical IDs reconciled and coverage of the final set reported.

## Check 1: accuracy of citations

I checked 55 citation groups (more than 150 individual locators). Ontology tag lines were checked mechanically: 57 of 57 in `bugs_issues.md` and 41 of 43 in `open_questions.md` match the `[I|D|Q · id]` tag line. All 63 cited run directories exist.

| # | Where | Citation | Checked against | Outcome |
|---|---|---|---|---|
| 1 | bugs D2 | `General/History.cls:103-105` (quit on `^[a-zA-Z][a-zA-Z0-9]*$`) | source | correct |
| 2 | bugs D2 | `Main.cls:4165` delete gate | source | correct |
| 3 | bugs D3 | `General/History.cls:173`, `:176`, `:107-109` | source | correct |
| 4 | bugs D5 | `Repo/UniversalSettings.cls:101`, `:178-179`, `:97-99` | source | correct |
| 5 | bugs D6 | `Storage/Module.cls:608-610`, `:669-671`, `:692-697`, `:707`, `:710`, and the correction "`:707`, not `:704`" | source | correct |
| 6 | bugs D13 | `Main.cls:2530`, `General/TempLocalRepoManager.cls:63-68`, correction 2529 to 2530 | source | correct |
| 7 | bugs D15 | `Main.cls:2466-2467`, help `:282`, regression in `f83b207e` | source, `git show f83b207e` | correct |
| 8 | bugs D17 | `Main.cls:2447-2452` | source | correct |
| 9 | bugs D19 | `src/inc/IPM/Common.inc:31` | source | correct |
| 10 | bugs D20 | `General/History.cls:253-281`, `:528`; H10 458, 229, 173 rows | source; `RN/20261113T204152Z-stagec/raw.log` (OBS H10 lines) | correct |
| 11 | bugs D21 | `Utils/Module.cls:1164-1169`, `:1328`, `:1368-1413`, `Storage/Module.cls:1139-1154`; NESTF outcome | source; `FRN/…r05-modes-nesting/verdict.json` (d1 present with class, d2 and m absent, row1.committed=0) | correct |
| 12 | bugs D24 | `General/History.cls:146-147`, `:157`; model `find:STATUS` 104, `find-global:STATUS` 52 | source; history-readers `raw.log` | correct |
| 13 | bugs D26 | `General/AbstractHistory.cls:120-125`, `Utils/Module.cls:1348-1350` | source | correct |
| 14 | bugs D27 | `General/HistoryTemp.cls:53` | source | correct |
| 15 | bugs D28 | `Utils/Module.cls:1172`, `:1194-1196`, `:1358-1362`; 5,780 | source; loadtx receipts | correct |
| 16 | bugs I3 | `Utils/Module.cls:309-311`, `:109`, `:318-319`, `Main.cls:2457-2460`; N-R05-DEVUP | source; R05 verdict (dev 1 to 0, version 1.0.0 to 2.0.0) | correct |
| 17 | bugs I4 | `General/History.cls:26-28`, `:233-235` | source | correct |
| 18 | bugs I5 | `Lifecycle/Base.cls:1081`, `Storage/Module.cls:683-684` | source | correct |
| 19 | bugs I8 | `Lifecycle/Base.cls:1194-1198`, wiki/02 L447 | source, wiki | correct |
| 20 | bugs OBS-L9 | `src/inc/IPM/Common.inc:105`, `Storage/Module.cls:105-108`, A1 conflict 24 | source, `A/intent-inventory.md:270` | correct |
| 21 | bugs OBS-L17 | `Lifecycle/Base.cls:290-294`, `Lifecycle/Module.cls:12-15`, `Storage/Module.cls:742`, `InstallModule.cls:135` | source, test | correct |
| 22 | bugs OBS-H10 | `General/History.cls:309-311`, `:496-499`, `:212` | source | lines correct, but the "empty groups" claim is contradicted by `:501-515` (finding M4) |
| 23 | bugs OBS-H15 | `Storage/Module.cls:630-643`, `Main.cls:2746-2749`, `:1152-1156` | source | correct |
| 24 | bugs D29 | `Main.cls:4415-4424`, `:4438`, `:2686-2687`, `:2710-2711` | source | correct |
| 25 | bugs D30 | `Main.cls:505`, `Lifecycle/Base.cls:554`, `:561`, A1 conflict 28 | source (no other reader of `"Purge"` in `src/`), `A/intent-inventory.md:281` | correct |
| 26 | bugs D18 | `Main.cls:1067-1069`, `CLI.cls:815-819`, `Main.cls:4136`, `:628-629`; H2 SQLCODE -29 | source; stagec `raw.log` (OBS H2) | correct |
| 27 | bugs D1 | proofs 263 verified, 0 errors; `repair-103-dep-clauses` PASS 189,577 states | `FR/20261113T232529Z-r12-asbuilt-proofs/raw.log`, `FR/20261113T233031Z-repair-103-dep-clauses/raw.log` | correct |
| 28 | bugs D1, OQ Q-30, DS S2 | 818 (27 returned, 791 process ends), 60, 2,712, 141, 1,004, 846 | `loadtx.json` in both receipts; MODELS.md breakdown 962 = 818 + 60 + 84 | correct |
| 29 | bugs D1 | N-P21A value C; N-P21R committed=1 and finalized=1; first run N-P21B INCOMPLETE | `FRN/…p21-ordering` and `…-r2` verdicts | correct |
| 30 | bugs D2 | N-R04-SEL 9 rows, SEL2 2 rows; 24 local and 12 global DELETED-UNSELECTED; 69 valid-neighbor commands | R04 verdict; history-readers `raw.log` | correct |
| 31 | bugs D3 | N-R04-CUTOFF `2026-10-14 00:00:00`, quoted 0, unquoted 1 | R04 verdict | correct |
| 32 | bugs D14, OQ Q-22 | 2,440; wiki/00 L149, L155, L175 | loadtx; wiki | count correct. L155 is stated for crashes ("if the process crashes mid-install, changes cannot be rolled back"). D14 and Q-22 A quote it without that qualifier, while Q-25 keeps it (minor) |
| 33 | bugs D22, OQ Q-26, DS S8 | 30 end states | loadtx | correct |
| 34 | bugs I2 | six `ipm-temp-modules-N`, sort -1000 | `RN/20261113T210229Z-temp-repos-leftover/raw.log` | correct |
| 35 | bugs A.3 | 36 violating or approximate and 23 permitted B1 cases | final re-evaluation verdict files | correct, set-equal |
| 36 | bugs App. B | git URL and `-branch` unquoted in a shell command, `Main.cls:2356-2357` | source | correct |
| 37 | OQ crosswalk | 22 Q-block tag lines | `ontology-tags.txt` | 22 of 22 correct |
| 38 | OQ Q-28 row and section | `history.read.api` at `features/history.g5.md:247` | tag list | wrong line, the tag is at `:246` (bugs D20 uses `:246`) |
| 39 | OQ Q-12 | wiki/09 L195, help `Main.cls:825`, D block `:271`, A4 R10 | wiki, source, `A/reconstruction-dispositions.md:18` | correct, resolved |
| 40 | OQ wiki authors | 00 L126, L129, L149, L175; 09 L46, L195; 03 L261; 02 L880 | `git blame` in `../ipm-wiki` | all correct |
| 41 | OQ git history | `8a88d5bf`, `ffba34ec`, `f522900e`, `2cef3073` (co-author isc-dchui), `d541c5b4`, `57f5bc02`, `7afb109f`, `9713db77`, `f83b207e` | `git log` | correct |
| 42 | OQ Q-01 | `git log -S'name="nodev"'` shows `-nodev` at `56346977` (2022-11-03) | `git log -S` | the first commit is `8e136249` (2022-09-27). The substance holds (minor) |
| 43 | OQ Q-13 | `General/History.cls:165`, `:168`, `:174-176`, `:183-189`, `:206-207` | source | correct |
| 44 | OQ Q-19 | `Main.cls:448`, `:450-451`, `:455-458`, `:2695-2697` | source | correct |
| 45 | OQ Q-22 | `Utils/Module.cls:1033-1040`, `:1316-1317` | source | correct |
| 46 | OQ Q-14, Q-15, Q-02 | tests `History.cls:702-706`, `:750-770` (inside `TestWithDependencies`, `:716`), `InstallModule.cls:177-178` | tests | correct |
| 47 | OQ lagging statuses | `handoff.md:14`, `:57-67`, `:72-76`, `:80`, `:86`, `:96`, `:98` | handoff | correct |
| 48 | OQ lagging statuses | "`A/coverage-map.md:6` attributes Q-21 to Q-23 to round 1" | coverage map line 6 | inaccurate. The line assigns Q-22 to round 2 and Q-23 to round 3. Only its link points at the round-1 file (minor) |
| 49 | OQ crosswalk | `B/model-notes.md:36` (Q-06b), `:40` (Q-FIN), `contract.rs:35`, `:47` | files | lines correct. model-notes lists three Q-FIN cases (C46T, C48T, C49T), OQ says two (minor) |
| 50 | OQ, bugs | CHANGELOG 0.10.11 #1183 (L37), #1237 (L45), 0.10.4 #706, 0.10.0 #474 | `git show 571e04a1:CHANGELOG.md` | correct |
| 51 | DS S6 | archived sequence 7, 0, `config delete`, result 7 | `RN/20261113T184628Z-repro-config-delete-retain` input and raw.log (`after.delete=7`) | correct |
| 52 | DS S4 table | six R04 rows | R04 verdict | correct |
| 53 | DS S8 | M-GLOBAL-MODE 144 (Q-22 B, Q-23 B) and 3,080 (Q-22 A) | loadtx | counts correct. "All are caller rollbacks" matches the two saved examples, which is all the receipt keeps |
| 54 | DS S2 | twin written at `Init` and `%OnClose`, `HistoryTemp.cls:30-70` | source | imprecise, `Init` is `:9-26` (minor) |
| 55 | DS S6 | seeding at `UniversalSettings.cls:150-163` | source | imprecise, the seed is at `:178-179` and `:150-163` are the getters (minor) |

Accuracy overall: high. I found no fabricated locator, no wrong run directory and no wrong receipt number. The errors are two line offsets, one wrong first-commit hash, one misread historical line and one factual error inside a finding (row 22, finding M4).

## Check 2: classification logic in bugs_issues.md

| Finding | Class given | Governing source and evidence | Assessment |
|---|---|---|---|
| D1 | supported | `history.survives-rollback` (I, `:67`) names the parent's rollback. Native A1 to A3, repro, B1-C31/32/33/36/37, C45/47 T and K | justified |
| D2 | supported | `history.read.filter` (I) and wiki/09 L112. Archived X3 deleted every row natively | justified |
| D12 | supported | `history.committed` (I) last bullet: "A refusal before anything is written is not committed, in any mode". B1-C08 natively | justified on the I block. `open_questions.md` Q-31 says that under its option B "D12 would be withdrawn", which makes the class decision-dependent there. The two files disagree (finding M2) |
| D14 | conditional, defect only under Q-22's alternative | Q-22's provisional answer (`transactions-and-dev-mode.g5.md:50`) says the dependency's changes escape and "its history record must survive and say so" | justified. Under the provisional answer the escape is intended and what remains is D1 and D22 |
| D21 | supported, "the violation holds under both options of Q-22" | Under Q-22 B it is `load.outcome`. Under Q-22 A it is "a necessary implication of `load.dependencies` and `uninstall.dependents`" | not justified (finding M1). `load.outcome` says whether dependency defaults change the table "is open" and that with NoJournal "the namespace may be left inconsistent". No I block states that an installed module always has its dependencies. `open_questions.md` makes this exact point an open design choice (Q-25, option A "accept it and document it"). Under Q-22 B the finding is the same cause as D14 |
| D22 | conditional (Q-22) | `history.committed` read as covering dependency writes | class right, decision incomplete. The entry itself says a per-action reading reduces it to D1. That reading is Q-26, which the entry does not name |
| D23 | suspected | code comments `General/History.cls:213`, `:221`; model only | unobserved natively, correct. It depends on Q-27 and does not name it, against the file's own rule for suspected findings |
| D24 | conditional (Q-10) | Q-10's provisional answer and both alternatives report the error | inconsistent with the class table. The entry says it "is a defect under every listed option". By the file's definition ("the verdict depends on an open maintainer question") it is supported against `history.read.find` and every Q-10 option (minor m1) |
| D28 | conditional (Q-23) | Q-23 provisional: suppress the parent's transaction. Native N-R05-Q23F and Q23S | justified |
| I3 | issue, "Expected: no source found" | `dev-mode.overwrite-guard` (I, `:100`): "A module installed in developer mode cannot be overwritten by a normal-mode install". `install.refusal` (I, `install.g5.md:30`) lists the guard without a dependency exception | not justified (finding M3). The `install` variant is a code-read violation of two I blocks, so it is suspected. The native `load` variant depends on Q-03's open "does the guard cover load" part and on Q-24 |
| I4 | issue | no source states the query columns. OBS-H11 is an explicit alias | justified. It also depends on the new Q-28, which the entry does not name |
| OBS-L16 | suspected | `history.committed` (I): kept writes outside a transaction read committed. `Utils/Module.cls:175-184` and `:207-209` | justified and unobserved (handoff: legacy XML not run). OBS-L16 now has an explicit entry as R09 asked |
| OBS-H11 | alias of I4 | see I4 | justified. The explicit entry R09 asked for exists |
| OBS-H10 (remainder) | suspected | `history.read.global`: "Namespaces without IPM add no records" | not justified (finding M4). `DisplayQueryResult` writes the "Namespace:" header only when a row exists (`General/History.cls:501-515`), so a namespace without rows prints nothing and the I block holds. Archived H10 printed only USER-VERIFY with `find.not.enabled.lines=0`. What remains is an unreachable message, which is not a violation |
| D4, D6, D18, D25, D26, D27, OBS-H7 | conditional | Q-17, Q-14, Q-18, Q-05/Q-06b, Q-08, Q-21, Q-15 | justified. Each provisional answer is quoted correctly |
| D7, OBS-H9 | withdrawn | wiki/09 L133; D block `history.delete.global-unfiltered` and wiki/09 L195 | justified |
| OBS-L15 | out of scope | `A/coverage-map.md` section 2 excludes `Validate` and `ReportConflicts`, and Q-04 provisional | justified. The ledger also calls OBS-L5 and OBS-L9 excluded features, while bugs keeps them suspected without stating the criterion (minor m10) |

Suspected items: apart from OBS-H10 (remainder), every suspected entry is unobserved natively. D23 rests on the model only and OBS-H15 on one persist probe (N-P16), which the entries say.

## Check 3: reconciliation coverage

The claim "34 of 34 OBS rows accounted for" is true. The registry has 34 A2-OBS IDs (OBS-L1 to L17, OBS-H1 to H16, OBS-X1). Appendix A.1 has 34 rows with no duplicates. Each ID appears with a disposition that matches its finding's class.

Five other families, checked against `id-registry.json`:

| Family | Registry | Accounted for in the owner files | Result |
|---|---|---|---|
| defect | 20 (D1 to D20) | 20 in A.2 | complete |
| issue | 2 | 2 | complete |
| model-finding | 12 (V1 to V12) | 12 in A.2 | complete |
| native-case | 146 (59 B1, 87 other) | B1: 36 listed plus 23 named as permitted, which matches the final re-evaluation. Other: 39 plus 8 plus 40 named, set-equal to the registry's 87 | complete |
| Q | 47 objects | every ID appears in the `open_questions.md` crosswalk | complete |
| review-finding | 66 (R1-ONT-01 to 14, R1-MOD-01 to 15, R2-01 to 11, R3-01 to 09, A4 R1 to R17) | about 20 are cited as provenance. There is no family table | not reconciled (finding M9) |
| A1 conflicts ("other" family) | 37 | conflicts 2, 24, 25, 26, 28, 29, 30 and 32 are cited. The rest have no disposition | not reconciled (finding M9) |

Against the original `bugs_issues.md`, the original handoff and the follow-up brief: every finding ID is present in the new files. The only token missing from all three new files is premise P-09, which the original D11 cited. Content differences I found:

- The original D11 "Preserve: valid NoJournal loads (B4 neighbour)" became B1-C04. B4 is itself a D4 deviating case, so the change is a correct fix, but it is not listed under "Corrections to the original file".
- The original D1's "176 configurations" for round 3 variant 16 is gone. The reference to variant 16 stays.
- The original handoff (`handoff.md:80`) said that in NoTransaction and NoJournal "residual violations come from D12 and the Q-08 deviation". That caveat on the D1 repair is not carried into D1, S2 or Q-30. The follow-up model hides the same residual (finding M5).
- Brief L-4 calls D21 "decision-independent". `bugs_issues.md` follows the brief, while `open_questions.md` Q-25 treats the same point as an open decision (finding M1).
- Brief E-3 (the T and K replay helpers were never committed and were not re-executed) is not reflected in the missing-evidence fields of D1, D10, D26 and D27, which rest on those runs (minor m14).
- Brief E-5 records that IPM printed "1 passed" for a suite whose only method was skipped. That is IPM's own test reporting. Appendix A.5 files E-1 to E-5 as "not a product finding" (minor m13).
- Archived reproduction labels X4 to X7 (`repro-autopurge-noop` and the three after it) are not in the registry and not aliased. DS S4 uses "X4" with no definition in any owner file (minor m13).

## Check 4: cross-file consistency

Numbers are consistent. 818, 60, 2,712, 2,440, 5,780, 30, 141, 846, 144, 3,080 and 62,764 carry the same values and meaning in all three files and in both receipt sets. The scope ledger does not cite them.

IDs and classes are not consistent:

- `bugs_issues.md`, `design_simplification.md` and the scope ledger never mention Q-24 to Q-31. The one exception is Q-29 once in DS S4. `open_questions.md` attaches eight findings to these questions, so the linkage differs by file:

| Finding | bugs_issues.md decision | open_questions.md |
|---|---|---|
| D1 | "none for the defect" (Q-06 for `Success`) | Q-30 decides the repair |
| D3 | Q-20 decides the purge of running records | Q-29 is the purge half, and Q-20's reason "does not apply" |
| D12 | "none for the defect" | Q-31 option B would withdraw D12 |
| D21 | Q-22 shapes the repair, supported either way | Q-25 is an open design choice |
| D22 | Q-22 | Q-22 read with Q-26 |
| D23 | no decision | Q-27 |
| I3 | "a new question, or an extension of Q-03" | Q-24, and OQ's cross-check says "Issue I3 asks Q-24", which the bugs entry does not do |
| I4 | none | Q-28 |

- DS uses V9 and V10 where bugs numbers the findings D26 and D27, and it never mentions D25 to D27, D29, D30 or I5 to I8. bugs D16 says "Design: S1", while S1's "Removes" list omits D16. bugs D25 says "Design: S2", while S2 lists no caller-transaction finding.
- DS S9's "Policy choice" column names Q-16, which concerns IPM self-uninstall, and omits Q-27 and Q-28, which are exactly S9's choices.
- Within `open_questions.md`, the ranking says Q-31 "decides the repair path of D12", while the Q-31 section says option B withdraws D12.

## Check 5: open_questions.md

- Crosswalk. It is complete for Q-01 to Q-23, Q-06b, Q-FIN, "new Q (round 1)", "Q-05 / Q-06b" and the collapsed handoff row. Statuses match the ontology and the handoff, with the two small errors in rows 48 and 49 of Check 1.
- Resolved questions. Q-12 is resolved by the cited sources. wiki/09 L195 states the behavior, help `Main.cls:825` agrees and A4 R10 made it a D block. The "details" part of Q-28 is settled by wiki/09 L171, as claimed.
- Real decisions and authority. Q-01 to Q-23 each give the ontology's provisional answer, real alternatives with sourced authority and a concrete decision. The git and wiki-blame research checks out. The problems:
  - Q-25 gives as its reason that "the sources lean against A" the fact that `bugs_issues.md` "files the broken closure as D21". That is circular (finding M1).
  - Q-31 is filed as a policy disagreement with option B "withdraw D12". The B authority is CHANGELOG L45 read out of context. That line is about whether a failed run "kept any changes". `../docs/ipm-uplift/index.md` (worked example) also treats "an early validation refusal" as a separate case. The I block decides the defect, so Q-31 is a repair and D-block wording question (finding M2).
  - Q-24 is typed "policy disagreement between sources" while saying "No source addresses dependencies directly". Option A's authority is the I block `dev-mode.overwrite-guard`. Option B's sources (wiki/00 L133, `dev-mode.not-propagated`) are about not using `-dev` for consumed dependencies, not about overwriting one already in developer mode (finding M3).
  - Q-30 is a repair-design choice rather than an intent question. That is legitimate and the type says so. Its evidence bullets carry the overstatements of findings M5 and M6.
- Types. The counts are consistent (12 policy, 17 design, 1 missing test evidence, with second types listed). Q-17's option B rests on #363, which by the file's own check "does not mention system requirements". That makes Q-17 closer to a design choice than a disagreement between sources (minor).
- Ranking. Item 3 says Q-06b changes "more than any other parameter". Q-08's alternative also changes four cases (`B/model-notes.md:37`). Item 2 says every repair variant leaves violations "unless Q-22 and Q-23 are settled one way". GUARDED's 141 `COMMITTED` violations are the same under every Q option, because only a product change (S8) removes them (minor m7).

## Check 6: design_simplification.md

R10's named items are addressed. S1 now says adopting it chooses Q-17 option A and no longer calls Q-17 moot. The intro now says that S2's dependency part is variant 103. S4 has a worked native table of preserving and refusing rows. S5 and S6 have decisive pairs. S7, S8 and S9 give the temporary-repository, dependency-flag and cross-namespace findings a disposition.

Field completeness against R10 (present: yes; missing: no):

| Proposal | Owner | Old complexity | Simpler | Preserved | Tradeoffs | Decisive case | Unresolved choices | Model evidence |
|---|---|---|---|---|---|---|---|---|
| S1 | yes | yes | yes | yes | yes | yes (B1-C23 vs C26) | yes (Q-17), Q-31 not named | yes, not modeled |
| S2 | yes | yes | yes | yes | yes (mirroring) | yes (B1-C31 vs C38) | yes (Q-05, Q-22), Q-30 not named | overstated (M5, M7) |
| S3 | yes | yes | yes | yes | no | abstract, no case IDs | no ("none") | yes (REPAIR-NAIVE) |
| S4 | yes | yes | yes | yes | no | yes (table) | yes (Q-18, Q-20, Q-29) | yes |
| S5 | yes | yes | yes | yes | no | yes (B1-C28 vs C25) | yes (Q-14) | yes, not modeled |
| S6 | yes | yes | yes | yes | no | yes (archived X5) | no | yes, not modeled |
| S7 | yes | yes | yes | yes | yes | yes (B1-C29 vs C30) | no | yes, not modeled |
| S8 | yes | yes | yes | yes | no | yes (NESTF vs Q23S, NESTS) | yes (plan rule) | overstated (M6) |
| S9 | yes | yes | yes | yes | no | abstract | no (names Q-16, omits Q-27 and Q-28) | yes |

S1's Q-17 correction is right. S2's variant-103 correction is right in substance. Its wording "its checks pass when every activation of a load uses the same transaction mode" is broader than the evidence. The `repair-103-dep-clauses` PASS covers transaction mode 0 only (default and `-dev`, receipt field `"txms": [0]`). The original handoff (`handoff.md:80`) kept the "default and developer modes" restriction (finding M6).

## Check 7: misuse detail

- `bugs_issues.md` follows its own rule. It names no command that deletes data or ends a process. It also removed the original D2's `history delete -DVersion_Major=99`.
- `open_questions.md` has complete destructive commands as code spans: `history delete -globally` (Q-12's wiki quote, Q-27's heading and body) and `history delete -confirm` (Q-11 and Q-18). These name the behavior under discussion and are not given as steps, but each deletes records if pasted (minor m11).
- DS S6's decisive pair is a runnable sequence: `config set HistoryRetain 7`, `config set HistoryRetain 0`, `config delete HistoryRetain`. Because of D5 it leaves 7-day retention on for the whole instance. On a build where D3 is fixed, the next IPM command would purge every namespace's records older than 7 days. The archived run saved and restored `^IPM.settings`, and the file does not say so (minor m11).
- No file names a process-terminating command beyond describing triggers (SIGKILL, `$SYSTEM.Process.Terminate`, `HALT`).

## Check 8: prose style (AGENTS.md)

- No mid-sentence bold and no em-dashes in any of the three files. En-dashes appear only in ranges (DS lines 3, 43, 47).
- ", and" joining two independent clauses: DS lines 5, 54 (twice), 63, 160 and 164.
- Semicolons joining clauses inside parentheses: bugs 143; OQ 532, 548, 577 and 616. The semicolon-ended list items in DS 146-149 are list punctuation.
- An imperative joined to a clause: bugs 145 ", so mark it durably at the dependency's own close".
- Serial commas are used inconsistently (DS 33, 132; bugs 126, 448, 751) against the files' usual "A, B and C".
- Labels from the follow-up's plan are used without stating the reason, against AGENTS.md's standalone rule: "follow-up R05d deferred" (bugs D1), "R02a in `followup/replay/`" (DS S2). Most L-, H- and E- labels are defined in each file's conventions.

## Findings

Severity: material means a classification, count, evidence claim or reconciliation statement that a maintainer would rely on is wrong or incomplete. Minor means a local error that does not change a conclusion.

### Material

M1. D21 is not decision-independent (bugs D21; OQ Q-25; ledger 1e and section 4).
- Under Q-22 B the dependency's NoJournal is ignored, and D21 is the same cause as D14.
- Under Q-22 A the only basis is an inferred invariant. `load.outcome` explicitly leaves dependency defaults open and accepts an inconsistent namespace with NoJournal. Q-25 offers "accept it and document it" as option A.
- Q-25's statement that the sources lean against A cites the bugs classification, which is circular.
- Fix: classify D21 conditional (Q-22, with Q-25 under option A) and recount the summary. Alias it to D14 under Q-22 B. In Q-25, replace the circular sentence with the actual authorities (`uninstall.dependents`, `load.dependencies`) and say that they do not decide the case.

M2. Q-24 to Q-31 are missing from `bugs_issues.md`, `design_simplification.md` and the scope ledger, so eight findings carry different decision linkage per file (table in Check 4). For D12 the class itself differs: supported in bugs, "withdrawn under Q-31 B" in OQ.
- Fix: add the new Q IDs to each bugs decision field and to the summary table's Q column (D1 Q-30, D3 Q-29, D21 Q-25, D22 Q-26, D23 Q-27, I3 Q-24, I4 Q-28). Give DS S2, S1 and S9 their questions (Q-30, Q-31, Q-27 and Q-28 instead of Q-16).
- Fix for D12: keep it supported on the `history.committed` I block. Reframe Q-31 as confirming the D-block wording "after validation, before the first write", without a withdraw option. If the authors keep option B, D12 must move to conditional.

M3. I3 is misclassified and Q-24 mistyped.
- `dev-mode.overwrite-guard` (I) and `install.refusal` (I) state the guard with no exception for dependencies. The `install` variant, code-read only, is therefore a suspected defect.
- The native `load` variant (N-R05-DEVUP) depends on Q-03's open "does the guard cover `load`" part and on Q-24.
- Fix: split I3 into a suspected `install` part and a conditional `load` part (Q-03 and Q-24), or state why the I blocks do not apply. Retype Q-24 as a design choice, or cite the I block as option A's authority and the not-propagated rule as option B's.

M4. OBS-H10 (remainder) is a false suspected defect.
- The Observed text ("such namespaces appear as empty groups") is contradicted by `General/History.cls:501-515`, where the namespace header is written only when a row exists.
- The governing `history.read.global` ("Namespaces without IPM add no records") holds. Archived H10 is consistent (`find.not.enabled.lines=0`, only USER-VERIFY listed).
- Fix: withdraw it, or reclassify it as an issue (an unreachable message at `:496-499`), and correct the Observed text and the counts.

M5. The residual of the guarded D1 repair is understated (bugs D1 constraints, DS S2 correction 2, OQ Q-30 and Q-21).
- The follow-up `COMMITTED` clause inspects only rows that are finalized and ended (`loadtx.py`, `check`: `if row and row.get("fin") and row.get("tend")`). The original crate's DEP-REPAIR clause also checked dependency `Committed` at process ends (`crates/ipm-load-history/src/explore.rs:177-178`, `:231`).
- My scratch count (`review/scratch/owner-files/guarded_unfinalized.out`, counted per end state and dependency row, without a caller transaction) shows the effect. The guarded close drops the checked dependency-row violations from 902 to 81, a drop of 821. Of these, 84 are the Terminate-window hazard, which the guard really fixes, because no unfinalized row reads `Committed=1` with nothing kept. Meanwhile the Terminate end states that leave an unfinalized dependency row reading `Committed=0` with kept writes rise from 363 to 1,142 (+779). All of them have a dependency's own NoJournal default. Most of the drop was therefore moved out of the clause's view, not fixed.
- In both REPAIR-103 variants, a SIGKILL of a load run with NoTransaction or NoJournal (from the caller or the top module's defaults) leaves dependency rows reading `Committed=0` with kept writes in 9,216 end states. That is the residual `handoff.md:80` attributed to "the Q-08 deviation".
- Under Q-08's provisional answer ("`Committed` reflects the changes kept, even on an unfinished record") all of these are wrong. "Part of that drop" and "removes every violation that does not come from a dependency's own NoJournal default" overstate what the 141 shows.
- Fix: state the clause's scope, present 141 as a lower bound under Q-08 A, and restore the NoTransaction and NoJournal residual in D1, S2 and Q-30. Either check `Committed` on unfinalized dependency rows in the model or record the Q-08 and Q-30 interaction as open.

M6. The evidence for "one transaction plan per load" is misattributed (DS S8 evidence, DS intro, OQ Q-30 option A).
- M-GLOBAL-MODE has no repair-103 rows. Its REC-EXISTS is 10,950 in the final receipt because dependency rows are lost as built, so its `COMMITTED` of 0 says nothing about dependency rows. No follow-up variant shows that "the 30 top-record and 818 repair-103 dependency violations disappear".
- The only evidence for variant 103 under a single mode is the original crate's PASS, which covers transaction mode 0 (default and `-dev`) only. "The repair then runs in the domain where it was proved" holds only if the plan is transactional.
- Fix: cite `FR/20261113T233031Z-repair-103-dep-clauses` for the 818 claim and state its transactional-only domain, or add a combined REPAIR-103 with GLOBAL-MODE variant. Narrow the intro sentence to "the same transactional mode (default or `-dev`)".

M7. The DS proposals do not all meet R10's field list, and S2 overclaims.
- Tradeoffs are missing for S3, S4, S5, S6, S8 and S9. Unresolved choices are missing for S3, S6, S7 and S9.
- S2 claims to remove D10, V9 (D26) and V10 (D27), but no model variant covers top-level rows. bugs D10 itself says the top-level close "was not modeled".
- S2 also dropped the original rule "`Finalized` is set only together with `TimeEnd`", which is what removes V10.
- Fix: add the missing paragraphs. For S8, the main tradeoff is that the plan needs every dependency's `<Defaults>` before the first write, while dependencies are resolved inside `LoadDependencies` after C1. State that S2's top-level part is unmodeled, restore the `Finalized` rule and use the D25 to D27 IDs.

M8. The DS reconciliation of simplification opportunities is incomplete.
- Explicit dispositions are given only for I3, I1, D15 and D17.
- Nothing is said for D16 (bugs says S1), D25 (bugs says S2), D29, D30, I5, I6, I7, I8 or the suspected OBS items. I6 has an obvious structural option, which its own direction states: unpack into a temporary folder and move it into place after the commit.
- Fix: add a table mapping every finding to a proposal or to a one-line reason why none is warranted, as R10 allows.

M9. Appendix A's claim to account "for every ID in the families the follow-up was asked to reconcile" is not met for the review findings (66) and the A1 section 6 conflicts (37). R09 explicitly lists "reviews".
- Review findings with product content have no row: R1-ONT-04 (in-scope suspects without a native check), R1-MOD-04 (V4 and V6 missing from the defect list), R2-02, R2-06, R2-07, R2-08.
- A1 conflicts without a disposition include 21 (`Storage/Module.cls:115` comment against the unfiltered step scan, `:186-195`), 12, 14, 17, 22 and 23.
- Fix: add A.6 (review findings) and A.7 (A1 conflicts) with a disposition per ID: alias of a finding, artifact-only, documentation, or out of scope.

### Minor

- m1. D24 is classified conditional although its entry says it is a defect under every listed Q-10 option. By the file's class table it is supported. Apply the same rule as for D21.
- m2. `history.read.api` is cited at `:247` in OQ (crosswalk Q-28 and the Q-28 section). The tag is at `:246`.
- m3. OQ Q-01 cites `56346977` as the first `-nodev` commit. It is `8e136249` (2022-09-27).
- m4. OQ "A/coverage-map.md:6 attributes Q-21 to Q-23 to round 1" misreads the line. Only the link is wrong there.
- m5. DS S2 cites `HistoryTemp.cls:30-70` for the `Init` write (`Init` is `:9-26`). DS S6 cites `UniversalSettings.cls:150-163` for the seeding (it is at `:178-179`).
- m6. Cause attribution for REPAIR-NAIVE (bugs D1 constraint, DS S3, OQ Q-30, brief L-10). Rolled-back dependencies also read `Committed=1` through their own C3 marker after a nested commit (`Utils/Module.cls:1347-1351`; `loadtx.py` adds `{a}:C3-marker` for every activation). The cause is not only `|| ISOK` and the no-transaction marker. S3's "For a dependency it does change it" is wrong for successful dependencies, which already call `SetCommitted`.
- m7. OQ ranking item 3 says Q-06b changes "more than any other parameter". Q-08 also changes four cases. OQ gives Q-FIN two cases where model-notes lists three. Item 2's "unless Q-22 and Q-23 are settled one way" needs "and the product follows one plan (S8)".
- m8. DS S8 says "a dependency's developer mode is overridden on every reload (L-5)". N-R05-DEVSAME shows it is kept when the installed version satisfies the requirement.
- m9. OBS-L16 calls the legacy path "the one caller that relies on `Finalize`'s 'or OK'". DS S3 says `uninstall` relies on it. Align the two.
- m10. D3's direction "Stamp `lastPurge` only after the delete succeeded" is already product behavior (`General/History.cls:174-176`, as OQ Q-13 says). It should say "after a delete whose per-namespace errors were not swallowed (D23)" or be dropped. D2's trigger-class sentence attributes the key order to the underscore. The deciding comparison is the first character (`P` < `V` < `p`). Widening happens at any key position (N-R04-SEL2). Only the delete-everything case needs the `_` key first. The ledger calls OBS-L5 and OBS-L9 excluded features while bugs keeps them suspected without stating the criterion.
- m11. Misuse: replace `history delete -globally` and `history delete -confirm` in OQ with a description ("an unfiltered global delete"). Rewrite DS S6's sequence as a trigger class, or say it changes an instance-wide setting and must run on a disposable instance with the setting saved and restored.
- m12. Suspected entries OBS-L3, L4, L5, L9, L16, L17, H10, H15, D23 and D30 have no "Decision" line, although "How to read this file" promises one for each finding. Add "Decision: none" or the Q.
- m13. Appendix B's "Not filed" items have no row with class "out of scope", although R09 asks for one row per unique finding. These are the unquoted git URL and `-branch` in a shell command (`Main.cls:2356-2357`), the verify-namespace record and the help and documentation conflicts. The same applies to E-5's "1 passed" for a skipped method, and to repro labels X4 to X7, which have no alias.
- m14. D1, D10, D26 and D27 should carry brief E-3 (terminate-b and sigkill-b helper sources never committed, T and K cases not re-executed) in their missing-evidence fields.
- m15. D14 and Q-22 A quote wiki/00 L155 without its crash qualifier.
- m16. Style: the clause joins, semicolons and labels listed in Check 8.

## Verdict per file

- `bugs_issues.md`: accurate and much more complete than the original. OBS, defect, issue, model and native reconciliation are verified complete. Every finding has a full field set apart from the suspected entries' missing decision lines. It does not yet close R09. Four classifications need correcting (D21, I3 and OBS-H10 remainder, plus D24 as a minor point). It has no linkage to Q-24 to Q-31 and the review-finding and A1-conflict families are unreconciled. Verdict: accept after the M1 to M4 and M9 corrections.
- `open_questions.md`: the strongest of the three. The crosswalk is complete, Q-12 is correctly resolved, sources and git and wiki research are accurate and every question has a real decision. It closes R11 once Q-25's circular reasoning (M1), Q-31's framing (M2), Q-24's type (M3) and the Q-30 evidence statements (M5, M6) are fixed, along with m2 to m4, m7 and m11. Verdict: accept with corrections.
- `design_simplification.md`: R10's specifically named defects are fixed (S1 and Q-17, the intro and variant 103, the S4 pair, the S5 and S6 checks, the new S7 to S9). It still misses R10's field list for six proposals, overstates model evidence for S2 and S8, and gives no disposition for about a dozen findings. Verdict: partially closes R10, revise per M5 to M8.

## Summary

The three files are accurate at the citation level. I found no wrong run directory and no wrong receipt number among more than 150 locators. The 34 of 34 OBS claim and the native-case reconciliation are exactly right. The problems are in classification and in how evidence is read:

- D21, I3 and OBS-H10 (remainder) are filed in the wrong class.
- `bugs_issues.md` was written before Q-24 to Q-31 existed and never caught up, so eight findings carry different decisions in different files, and D12 carries a different class.
- The guarded D1 repair's residual of 141 is measured with a clause that skips unfinalized rows. Most of the violations the guard "removes" reappear as 779 more unfinalized NoJournal-dependency rows with the wrong `Committed`. In addition, 9,216 SIGKILL end states of NoTransaction or NoJournal loads keep the residual the original handoff named.
- The single-plan evidence (M-GLOBAL-MODE) cannot show what DS S8 and Q-30 attribute to it.
- The review-finding and A1-conflict families are not reconciled, contrary to Appendix A's coverage claim.

Fixing M1 to M9 needs no new native run. M5 and M6 are best closed with one more loadtx variant or check, or by stating the limits plainly.
