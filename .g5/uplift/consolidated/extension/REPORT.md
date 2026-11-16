# IPM Verus extension at 571e04a1

Working checkout: `consolidated_uplift`, HEAD `0ac7b931`. This report measures the staged/untracked consolidation, not HEAD alone. Product files and both inherited foundations remain unchanged from the initial session state; [preservation receipt](preservation.json) checks 4,828 initial file fingerprints and the exact git index. Nothing was committed.

## Coverage

Every percentage below retains the frozen 1,216 member denominator. A represented member has at least one scoped observable distinction in a resolved Verus symbol. This does not mean every branch or the ObjectScript body was proved. Roles and artifacts overlap, so their counts must not be summed.

| Measure | Before | After | Gain / withdrawal |
|---|---:|---:|---|
| Original Python/native evidence rule, session fixtures |149/1,216 (12.25%)|155/1,216 (12.75%)|+6 /0|
| Same rule, independent environment probes included |141/1,216 (11.60%)|155/1,216 (12.75%)|+14 /0|
| Verus observable representation, audited inherited baseline |100/1,216 (8.22%)|174/1,216 (14.31%)|+74 /0|
| Explicit machine-readable member links, any listed artifact |301/1,216|304/1,216|+3 /0|
| ObjectScript bodies mechanically proved |0/1,216|0/1,216|0|

The inherited representation baseline is an established lower bound of100 members:72 from formerly qualified formal bindings plus28 recovered by [independent representation audit](inherited-representation-audit.json). Those28 failed the stricter old native-evidence rule but have scoped existing state actions, projections or justified shared contracts. They are baseline corrections, not new proofs or coverage gains. Their missing inputs, proof/native gaps and premises remain explicit; unresolved members may contain further undiscovered inherited representation.

The original 149 and 141 both reproduce in [before measurements](before-measurements.json). The extended suite withdrew eight environment credits because object elements in arrays lost earlier keys. Correct recursive merge restores those eight. A new scalar-path neighbor adds `EnvironmentConfig.GetArg`; six native-controlled thin-surface bindings add five distinct members because FileCopy cleanup was already inherited. [Old-rule additions](old-rule-additions.json) and [counts](../coverage/counts.json) show the exact union. The 304-member planning checkpoint remains 149 members away. No credit was added for the withdrawn LockManager constructor distinction.

| Subsystem | Denominator | Verus before | Verus after | Added | Linked | Unresolved representation |
|---|---:|---:|---:|---:|---:|---:|
|(root)|100|19|21|2|45|79|
|CLI|1|0|0|0|0|1|
|DataType|13|0|0|0|0|13|
|ExtensionBase|69|0|0|0|0|69|
|General|213|14|62|48|82|151|
|JSON|6|0|0|0|0|6|
|Lifecycle|72|10|11|1|22|61|
|Repo|184|13|17|4|36|167|
|ResourceProcessor|157|4|9|5|20|148|
|Storage|134|21|26|5|50|108|
|StudioDocument|49|3|4|1|4|45|
|Test|29|0|0|0|0|29|
|Utils|189|16|24|8|45|165|

Direct representation: 165/1,216; shared-contract-only: 9/1,216. [All member dispositions](member-dispositions.json) label every row; 1042 remain unresolved. This is 100% census disposition, not 100% behavioral audit or critical representation.

The extension ledger has 78 selected scoped properties: represented 77/78, proved under stated premises 50/78, refuted by a reachable native/model witness 19/78, and open/partial 12/78. Flags can overlap when a counterexample is proved or only a restricted premise is discharged. Among 75 selected critical properties: represented 74/75, proved 48/75, refuted 18/75, open 12/75. These are selected-property counts; a complete critical universe has not been established.

The existing 595 behavior and 16 exception rows are preserved in [intent reconciliation](intent-reconciliation.json), with source-member/canonical-owner candidates; all 389 command/state cells are joined in [cell reconciliation](command-state-reconciliation.json). Exact text deduplication found 611 distinct rows. Semantic decomposition by actor/precondition/event/result/preserved state remains unresolved for that inventory, so no global deduplicated-obligation percentage is claimed. The original 25 cases are retained as a regression panel, not used as the universe.

[46 command write-set joins](command-write-owners.json) connect observations to command entry members and existing store mechanisms. [18 explicit transaction starts](transaction-starts.json) resolve to frozen owners. [Competing-writer candidates](writer-candidates.json) and caller candidates expose lexical discoveries; dynamic dispatch, generated registrations, callback writers and exclusion audits are not mechanically complete. Exclusions are not subtracted from 1,216.

## Checked Verus artifacts

| Family | New model verification items, imports removed | Native correspondence | Proof controls |
|---|---:|---|---:|
|Environment and attribute evaluation|50|42+74 Python/native fields;13 native-bridge proof items|10/10|
|Semantic versions/resolver inputs|37|2,130 native assertions|4/4|
|Owners, stored manifest/cache/locks/routine edits|15 (52 reported includes semver 37)|127 native assertions|5/5|
|Temporary repository|7|116 oracle fields;9 formal native projections shared with parent runner|2/2|
|Partial compilation|5|31 oracle fields;28 formal native projections|2/2|
|History identity/filter/readers/purge|14|82 oracle fields; final-reader/delete witnesses retained|2/2|
|Per-activation flags/prefix/process ends/retry|13|bounded explorer;6 CLI flag cases/90 fields;42 native-to-Verus projections|2/2|
|Thin file/lockfile/registry/report/inspection surfaces|22|24/24 native fields; fresh native replay passed|8/8|

Total 163 distinct new model verification items, with 0 logical errors and 35/35 proof controls rejected. Generated receipt-to-Verus assertions/bridges are separate: 2,130 semver,127 owners,13 environment/attribute,2 parent native-projection lemmas, and 1 wrapper reusing the imported homogeneous theorem. A further flag-override bridge proves42 assertions in1 additional proof item, reusing the existing priority mutant rather than counting it twice. Raw summaries include imports and must not be summed without this subtraction. [Final gate](final-run/results.json) records exact commands and logs; [formal sources](../../../Verus_verification/consolidated/verus/) and per-family manifests/TRUST files state bounds and premises.

The seven independent Python/native oracles compare 579/579 fields with 0 mismatches; 127/128 wrong variants are rejected. The surviving temporary-repository sort mutant is documented and earns no behavioral distinction. Thin-surface proof controls are8/8;7/8 also differ from matched native observations. The protected-directory native branch remains open. Six new real flag-override loads match90 native fields and 42 formal projections; explicit parent zero overrides the child default, with different journaling and transaction depth ([receipt](flag-overrides-native/REPORT.md)). [Oracle details](after-native-oracles.json).

Inherited results: G 263 verified,0 errors ([raw log](before-G-verus.log)); P 227 verified, 0 errors; 195/195 controls rejected. The 20-minute incomplete batch was not counted as a pass; its completed file receipts and final-file recovery are recorded separately. The P homogeneous-mode theorem is imported directly in `inherited_bridge.rs`, retaining P1c/P2/P3/PC/PI and its folded dependency/total-order domain. Mixed flags, same-key durability and real semver inputs are not silently added to its premises.

The preserved G follow-up run again explores 1,692 configurations and 62,764 as-built end states with 0 retry failures, under its named modes/cuts. The four P explorer suites also reproduce their recorded verdicts (245 check records); known counterexamples and not-expressed observations remain non-pass records. The install/resolver replay used Python 3.11 after the local Python 3.14 XML extension failed to load; the failed attempt was not counted. [Companion results](P-explorer-rerun/results.json) retain exact commands and failures.

## Witnesses and boundaries

- Environment: object-in-array merge keeps both x and y; primitive neighbor yields `[3,2]`; later missing file clears prior values. `$` and backslash replacement differ from literal insertion. Observed behavior is separate from C:Q6 policy.
- Semver: `<2` can install 2.5.0; `>=1.2` rejects 1.3; caret partials narrow unexpectedly; prerelease-bound order and dependency rename change a diamond. Loading 1.0.0-rc can replace 2.0.0-beta while release downgrade is refused. The imported exact-ISet resolver theorem remains valid only on its justified domain.
- Postcommit and resource state: message/cleanup error can leave module/history committed. Two descriptors do not imply two callable classes; ordinary rollback removes both, while no-transaction/developer neighbors retain only A as callable.
- Repositories/owners: reuse preserves a foreign repository; equal versions choose older repository id; failed cache creation may warn without a row. Same-version module-version refusal precedes unchanged; forced stored record/manifest updates leave disk module.xml unchanged. Constructor release-mode credit remains open.
- History: unfinished status renders ERROR#00; underscore filtering can delete all three seeded identities; quoted purge cutoff can keep expired rows while marker advances. CLI global and programmatic GlobalRecords are distinct. A real two-process delete/finalize schedule returned successful finalization with the row absent, narrowing the resurrection hypothesis without proving all lock schedules.
- Thin consumers: immediate copy and `module-lock.json` survive a later failing load and database rollback; deferred-copy neighbor leaves no target. First plain load writes lockfile relative to cwd because stored Root is empty (C:D16 alias G:OBS-L3); rooted developer neighbor writes correctly. Lock-file IO error can be logged while load returns OK. Report JSON exists after an unsupported summary format returns error. ORAS 401 needs a challenge; that availability result does not prove authenticated pull/publish.

These are proofs of hand translations with finite correspondence. Full ICU/ObjectScript evaluation, IRIS compiler internals, arbitrary nested same-key crash refinement, all history renderers and cursor/error/permission branches, concurrent writers, filesystem paths/partial IO, CPF merge effects, mirroring, registry digests/retries and verify namespace cleanup remain open. [Per-claim boundaries](open-boundaries.json) names missing inputs/outcomes, including history helpers; [independent review](INDEPENDENT-REVIEW.md) does not sign off the entire product.

## Five passes, ontology and freshness

Five cumulative integration passes selected an uncovered behavior and challenged a credited claim each time. Lane checks ran in parallel; the table is integration accounting at one product pin, not five historical product revisions. [Inputs, predictions, wrong variants and receipts](five-passes.json):

|Pass|Verus represented before → after|Added / withdrawn|Old-rule extended reach|
|---|---:|---:|---:|
|E1|100 → 109|9 / 0|150/1,216|
|E2|109 → 129|20 / 0|150/1,216|
|E3|129 → 138|9 / 0|150/1,216|
|E4|138 → 166|28 / 0|150/1,216|
|E5|166 → 174|8 / 0|155/1,216|

Canonical `.g5/ontology` rules now carry late-commit exceptions, per-activation flag facts, filter stop order, global-reader shape, database-only Committed meaning, recursive array merge, lock-file placement/status and report ordering. C:Q1–Q6 and inherited Q24–Q31 have canonical child owners; settled field meanings retain aliases. [Question audit](question-audit.json), [finding aliases](reconciled-findings.json), and [10 ontology-only precision answers](precision-after.json) link directly to owning block text. Policies are working assumptions, not maintainer decisions. Product repair/simplification proposals remain separate and unapplied.

[Bidirectional bindings](bindings.json) and [reverse index](reverse-bindings.json) validate pin, member id, exact contained slice, content hash, ontology endpoint, actual symbol, property, role, premises and receipt. [Input fingerprints](check-inputs.json) include transitive imported models, check drivers, native receipts/fixtures, runtime/configuration and tools. [Twelve disposable Git base/head probes](drift-results.json) reopen product correspondence for an internal member edit, select dependent checks after formal/fixture edits, preserve results on unrelated/line-shift edits, and flag renamed/new writers, registration changes, property initializers and unbound include files for investigation. Formal reruns are not substitutes for reopened product bindings.

Residual work, ranked by consequence: destructive history selection and concurrent writers; complete mixed-mode/same-key crash refinement and restart/replication premises; registry fetch/publish integrity and retries; filesystem/CPF partial effects and path overlap; remaining reporting/inspection/verify behaviors; full 611-row semantic deduplication and caller/exclusion audit. 100% critical representation and a proof or validated counterexample for every critical property were not reached.

Reproduce new checks from IPM root: `python3 .g5/uplift/consolidated/extension/run_all.py`. Add `--inherited` for disposable inherited proof/control reruns. Fresh native held-out replay: `python3 .g5/Verus_verification/consolidated/verus/run_thin_native.py`; it uses named test fixtures and does not reset the shared instance. Future shared-guidance proposals are assessed against the five mechanism principles in the independent report; shared guidance was not edited.
