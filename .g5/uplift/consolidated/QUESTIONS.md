# Consolidated uplift: questions for the IPM owner

New open questions from the consolidation rounds. IDs are `C:Q<n>`. Inherited questions stay in G's
[open_questions.md](../../Verus_verification/open_questions.md) and P's [QUESTIONS.md](../../../playbook-uplift/QUESTIONS.md).
Each question names the interpretations, the evidence for each and the decision needed. The consolidation proceeds
under the stated provisional answer and keeps the alternative visible.

| ID | Question | Interpretations and evidence | Provisional answer | Decision needed | Ontology |
|---|---|---|---|---|---|
| C:Q1 | When another repository offers the same version of a dependency as a load's `.modules` folder, which copy should IPM install? | (a) The older repository's copy: CHANGELOG 0.9.x "Repo check should happen in the order to repo creation, not by repo name"; as built (TR5). (b) The `.modules` copy: commit `2dc933dd` and `TempLocalRepoManager.cls:27` give the temporary repository sort position -1000 "to be the first repo to be found", which has no effect on ties. | (a), as built | Keep creation order and drop the dead sort position, or make the sort position break ties | [load.dependencies.local-repo.precedence](../../ontology/features/load.g5.md#load.dependencies.local-repo.precedence) |
| C:Q2 | Should a `$$$macro` in an attribute value be expanded when the value contains no `{`? | (a) No: `%Evaluate` returns early without `{` (`Storage/Module.cls:1838-1840`); observed E15 and P4. (b) Yes: the expression doc lists `$$$macros` as a supported form without that condition (`Utils/Module.cls:2694`). | (a), as built | Document the `{` condition or expand macros regardless | [module.parameter.expressions](../../ontology/concepts/modules.g5.md#module.parameter.expressions.brace-policy) |
| C:Q3 | Should `2024.1` mean exactly `2024.1.0` while `2024.12` and `1.2` mean a whole minor series? | (a) Yes, as built: a `20NN.N` special case serves IRIS version requirements; IPM's test requires `2022.2` to reject `2022.3` (`Expressions.cls:168-176`). (b) One rule for all partial versions. | (a) | Keep the special case and document it, or apply one rule | [version.range.year](../../ontology/concepts/versions.g5.md#version.range.year) |
| C:Q4 | Should `module-version` also update the `module.xml` file the module was loaded from? | (a) No: it edits the namespace's stored manifest and record only (native MV10: file stays 1.2.3 while the record reads 2.0.0); source control hooks may export it. (b) Yes, at least for developer-mode modules whose folder is the working copy; CHANGELOG #710 says the command "updates the version of a module". | (a) | Document that the file is not touched, or write it for developer-mode modules | [module-version.file](../../ontology/features/developer-tools.g5.md#devtools.authoring.module-version.file) |
| C:Q5 | Should one module with an invalid name block a whole filesystem repository? | (a) As built: yes. The refresh fails and changes nothing, and a load's `.modules` repository is not created (G:D13 path). An unreadable manifest, in contrast, is skipped silently (LK1). (b) Skip it with a warning like an unreadable manifest. (c) Fail on every bad manifest so nothing is hidden. | (b) | Choose one treatment for bad manifests | [repo.filesystem.bad-module](../../ontology/concepts/repositories.g5.md#repo.filesystem.bad-module) |
| C:Q6 | When one of several `-env` files fails, what should the command keep, and how should two arrays merge? | (a) As built: fail and keep nothing (U3-U5); arrays merge position by position, so a later file cannot shorten a list (U2). (b) Keep the files loaded before the failing one. (c) Check every file before changing anything; let a later array replace the earlier one. No source states the intent (plan policy row). | (a) | Pick the failure and array-merge rules and document them | [module.env.failure](../../ontology/concepts/modules.g5.md#module.env.failure) |


| C:Q7 | May deleting an active history row be followed by restoration at close? | Accessible delete remains absent; transaction-locked delete followed by temporary close can restore the original finalized row. The broad no-resurrection claim is withdrawn, not all history behavior. | Preserve both observed schedules. | Maintainers choose skip-active, permitted recreation or another explicit delete contract. | [history.delete.running-records](../../ontology/features/history.g5.md#history.delete.running-records) |
| C:Q8 | How should an uncommitted filesystem cache refresh stay consistent after the default manager closes? | Default immediate release is now observed at return; explicit deferred mode stays locked until commit. No race corruption was established. | Do not infer persistent explicit locking or safe concurrent replacement. | Maintainers choose transaction/database locking, deferred ownership or refusal inside outer transactions. | [repo.filesystem.concurrent-refresh](../../ontology/concepts/repositories.g5.md#repo.filesystem.concurrent-refresh) |
| C:Q9 | Which ORAS tag/layer ordering is authoritative? | Untagged pull takes the provider's last tag, first tar-layer validation differs from first downloaded-file selection. | Keep the mismatch and explicit provider premises. | Registry/IPM maintainers decide sorting, unique archive rules and byte-to-layer binding. | [repo.oras.integrity.policy](../../ontology/concepts/repositories.g5.md#repo.oras.integrity.policy) |
| C:Q10 | When should artifact local content be refreshed? | Existing local folders and same-named temporary files are reused; no accepted identity/expiry rule found. | Preserve observed offline reuse without claiming freshness. | Maintainers/module authors choose explicit refresh, immutable digest names or cached-byte validation. | [artifact.refresh-policy](../../ontology/features/artifact-download.g5.md#artifact.refresh-policy) |
| C:Q11 | Should Artifactory copies refuse protected installation targets? | Activate calls a warning; it does not use FileCopy's refusal guard. Only temporary targets were probed. | Disclose the boundary; do not claim system-folder protection. | Maintainers/administrators choose common refusal, explicit override or warning only. | [artifact.target-policy](../../ontology/features/artifact-download.g5.md#artifact.target-policy) |
| C:Q12 | Must package failure compensate CPF configuration changes? | A valid role survived rollback; invalid merge returned child exit 7 without the role. Other effects/restart actions remain open. | Database rollback alone does not compensate configuration. | Maintainers/administrators choose supported actions, compensation or transactional refusal. | [module.processor.cpf-effects.policy](../../ontology/concepts/modules.g5.md#module.processor.cpf-effects.policy) |

## Extension audit

[Question audit](extension/question-audit.json) retains all inherited aliases. The owning `.g5/ontology` blocks carry settled facts, working assumptions and alternatives. No maintainer decision was inferred from a passing check. The constructor release-mode observation is settled by iteration 2's paired native schedules; its cache-consistency policy remains C:Q8. C:Q7-Q9 and C:Q12 integrate that iteration's owner-linked questions rather than leaving them only in an appendix. C:Q10-Q11 are newly discovered policy gaps. No passing check ratifies a policy.

## Iteration 4 questions

| ID | Question | Interpretations and evidence | Provisional answer | Decision needed | Ontology |
|---|---|---|---|---|---|
| C:Q13 | Should a declared test package with no test classes fail `test` and `verify`? | (a) As built: warn, skip and pass (native T1); the source calls it "not an error in itself". (b) Fail, so a renamed or emptied package cannot turn CI green. (c) Fail only when a whole run executes no tests. A named Suite folder that does not exist already fails (T4). | (a) | Maintainers choose whether CI should be protected from runs with no tests. | [build.test.outcome.empty.policy](../../ontology/features/build-and-publish.g5.md#build.test.outcome.empty.policy) |
| C:Q14 | Should registry credentials go to a download URL on another server? | (a) As built: yes (native R-cross), following commit `d6cc4204`. (b) Only to the repository's own host and port (C:S14), matching `#1115` and `#649`. (c) Refuse cross-host download URLs. Redirect following and production registry layouts are untested. | (a), recorded as suspected defect C:D20 | Registry and IPM maintainers decide whether download URLs may name other hosts and which credentials they receive. | [repo.remote.download.credentials](../../ontology/concepts/repositories.g5.md#repo.remote.download.credentials) |

## Iteration 5 questions

| ID | Question | Interpretations and evidence | Provisional answer | Decision needed | Ontology |
|---|---|---|---|---|---|
| C:Q15 | Should install take over an existing web application IPM did not create, and should uninstall delete it? | (a) As built: yes to both (native W2/W3). (b) Refuse install when an unlisted application of that name exists. (c) Record creation and delete only what IPM created. The duplicate-name refusal (W4) already protects applications another module lists. | (a), disclosed | Maintainers and administrators decide ownership of pre-existing applications. | [module.webapp.ownership](../../ontology/concepts/modules.g5.md#module.webapp.ownership) |

C:Q14 now also covers redirects: whichever answer is chosen must hold for every hop, not only the URL in the metadata ([repo.remote.download.redirect](../../ontology/concepts/repositories.g5.md#repo.remote.download.redirect)).

## Iteration 6 questions

C:Q16: [Should generation skip invalid source?](../../ontology/features/developer-tools.g5.md#devtools.generate.invalid-source) Source inspection settles the skip branch, not whether it is the intended author experience. No owner decision is inferred.

## Iteration 7 question

C:Q17: [Should uninstall retain or restore legacy SystemSetting values?](../../ontology/concepts/modules.g5.md#module.system-setting.cleanup-policy) Observed retention is settled; intended cleanup and treatment of later administrator edits require an owner. No accepted decision was found.

## Iteration 8 question

C:Q18: [How should shared Python packages be removed?](../../ontology/concepts/modules.g5.md#module.python.cleanup-policy) Current Clean deliberately keeps packages; the native offline wheel remains after uninstall. The product TODO recognizes sharing but does not settle manually installed/upgraded package ownership. Keep current behavior pending an owner decision.

## Iteration 9 question

C:Q19: [May modules share update-step identities?](../../ontology/features/update.g5.md#update.steps.shared-ownership) Ordinary activation moves both kinds to the new home while retaining completion. Distinct packages are the working assumption; conflict refusal, module-keyed records and shared ownership are proposals, not owner decisions.

## Iteration 10 question

C:Q20: [Does an omitted IncludeTests field retain or reset its stored value?](../../ontology/features/build-and-publish.g5.md#build.package.include-refresh.policy) Reload retains it, while explicit false clears it and a command-only zero does not persist. The choice remains with the owner; current behavior is the working assumption.

## Iteration 11 questions

C:Q19 strengthened: [Shared step execution](../../ontology/features/update.g5.md#update.steps.shared-execution) now includes two-process entry and late HomeModule overwrite. Distinct update packages remain the working assumption. Serialization/identity choice is still an owner decision.

C:Q21 opened: [Should class-deletion refusal warn and remove metadata, or fail and retain it?](../../ontology/features/uninstall.g5.md#uninstall.class-delete-error.policy) Actual lock refusal and success-with-warning are established. Desired script status/retry metadata remain unconfirmed.

## Iteration 12 disposition

No new question opened or settled. Namespace and lock-file ordering outcomes are source/native-supported inherited findings, not policy ratification. C:Q19–C:Q21 and older decisions remain unchanged. Missing batch/provider-failure behavior and unsupported input boundaries remain verification gaps rather than automatic owner questions.

## Iteration 13 reconciliation

No owner question opened or settled. The partial-batch behavior is documented as observed; no unsupported atomicity obligation is invented. Restart/provider-error policy remains unassessed.

## Iteration 14 question

| ID | Question | Interpretations/evidence | Provisional answer | Decision needed | Ontology |
|---|---|---|---|---|---|
| C:Q22 | Should FileCopy protect a parent containing an unconfigured descendant with an IRIS.DAT marker? | (a) Exact marker plus configured/system root ancestors, as built and bounded by current tests. (b) Detect/protect unknown marker descendants too, consistent with a broad reading of database-directory/ancestor protection; no inspected human decision settles scan scope/cost. Synthetic nested-marker scratch parent is unprotected and removed by Clean. | (a), as built; no owner ratification | Choose and document descendant discovery/protection scope. | [module.processor.file-copy.unknown-descendants](../../ontology/concepts/modules.g5.md#module.processor.file-copy.unknown-descendants) |

C:D29 has explicit replacement intent and stays separate; C:Q22 does not weaken known system/configured-root protection. Earlier owner decisions remain unchanged.

## Iteration 15 reconciliation

No new question ID. C:Q9 gains the deep-build service/display versus manager/install evidence. Keep authoritative ordering policy open; no human ratification inferred. The AllVersions=1 platform-empty row is displayed by SQL but filtered by manager, an API distinction without a new defect/policy claim.

## Iteration 16 reconciliation

No owner question opened or settled. Existing dependency/update source and transaction questions remain unchanged; selected direct local load does not settle those policies. Archive-path scope is unassessed/unpursued by explicit user direction.

## Iteration 17 reconciliation

No owner question opened or settled. C:D31 contradicts the existing selected-run reporting intent; native behavior does not authorize changing that intent. Numeric duration entries explain actual counts without a new counting-policy decision. Archive-path behavior remains unassessed/unpursued, including reproduction and probing.

## Iteration 18 reconciliation

No owner question opened or settled. Existing human intent already permits and reloads the same version; native behavior qualifies a supported source-change case without changing policy. Refused-attempt history, generic cache/provider failures and other versions remain verification limits. Archive-path behavior remains unassessed/unpursued, including investigation, reproduction and probing.

## Iteration 19 reconciliation

No owner question opened or settled. Source explicitly warns which physical duplicate will be used; the checked behavior supports that scoped rule without a portable scan-sort policy. Mutable-source, parser/provider failure and concurrent refresh remain verification gaps or existing C:Q5/C:Q6, not newly ratified policy. Archive-path scope remains unassessed/unpursued, including investigation, reproduction and probing.

## Iteration 20 reconciliation

No owner question opened or settled. Source explicitly describes naming-convention fallback; the selected native behavior supports that rule without broader package/stub or collation policy. General inventory/type/namespace/provider limits remain verification gaps. Archive-path behavior stays unassessed/unpursued including investigation, reproduction and probing.

## Iteration 21 reconciliation

No owner question opened or settled. The explicit version obligation is recovered from product-authored install help; observing its violation does not ratify a new policy. C:Q5 invalid-module behavior and C:Q8 concurrent refresh remain open. Prior19/20 shorthand mentioning C:Q6 in filesystem context does not change that ID: C:Q6 remains environment-file merge/failure policy. Generic parser/provider/concurrency limits remain verification gaps. Archive-path boundary stays unassessed/unpursued including investigation, reproduction and probing.

## Iteration 22 reconciliation

No owner question opened or settled. The existing reinstall.version conflict now has a native available-newer neighbor: the current plain 1.0.0 is reinstalled while 2.0.0 is offered. The provisional current-version interpretation remains; the alternative latest-version interpretation and protected decisions remain unchanged. Missing-name versus delegated failure history are as-built facts under the existing history owners. Archive-path behavior remains unassessed and unpursued, including investigation, reproduction and probing.

## Iteration 23 reconciliation

No owner question opened or settled. Raw status in details is already an established requirement, with earlier defect withdrawal recorded by its owner. Initial 23 planning had not read that subsection; canonical reverse lookup corrected the plan before any new disposition. No generic text/UTF-8 transport promise was found or invented. Existing policy questions remain unchanged. Archive-path behavior remains unassessed and unpursued, including investigation, reproduction and probing.

## Iteration 24 reconciliation

No owner question opened or settled. Default snapshot dependency refresh is an existing product-authored requirement; its checked range-conflict refusal is C:D33, not a new policy choice. The current-versus-latest reinstall question remains open. Explicit `PermitDowngrade=1` is a contrast that reaches the reload path, not an accepted repair or recommendation to change the downgrade policy. Archive-path behavior remains unassessed and unpursued, including investigation, reproduction and probing.

## Iteration 25 reconciliation

No owner question opened or settled. The existing developer-mode requirement and normal overwrite guard govern the checked reinstall cases. Force/load scope, current-versus-latest reinstall, dependency policy and other protected decisions remain unchanged. External source-control plug-in interpretation and trigger scheduling are verification gaps, not new accepted behavior or owner decisions. Declared resource-path metadata and selected fallback import paths are recorded separately without inventing a generic path-discovery obligation. Archive-path behavior remains unassessed/unpursued, including investigation, reproduction and probing.

## Iteration 26 reconciliation

No owner question opened or settled. The existing verify-only skip and aggregate summary behavior are refined with a process-local bookkeeping contract and actual stored readers. No permanent-history, persistence or general callback/routing guarantee is invented. This work follows feedback explicitly disclosed after the22→24 evaluation exit and is tagged as repair; the separate private IRIS sample remains unread. Protected questions and decisions remain unchanged.

## Iteration 27 reconciliation

No new question or owner decision. The existing uninstall purge promise and inherited P:F-UNIN-2 determine this discrepancy. A custom installer receiving a different nested argument is a scoped supported neighbor, not authorization to weaken the intended `-purge` behavior. Existing questions remain unresolved on their recorded terms. The deployed mapping route remains open after metadata-only assessment; archive-path investigation and private evaluator inputs remain excluded. [Handoff](iter27/HANDOFF.md).

## Iteration 28 reconciliation

No new question or owner decision. Public DEC-RES-2 already settles the JSON root shape; inherited P:F-RES-5 records the inaccurate help. Selected fail-stop/preflight behavior is an as-built scope, with no new rollback/continue policy inferred. General parser/JSON/child-command/IO/escaping behavior remains qualified rather than resolved by the small file sample. Existing open questions stay on their recorded terms. [Handoff](iter28/HANDOFF.md).


## Findings audit against iteration 28

All C:Q1–C:Q22 were traced to their actual canonical owners; the source and settlement gate are in [the question review](findings-audit-iter28/QUESTION-REVIEW.md). No passing observation ratifies policy. Opening-arm assumptions and aliases are now reconciled in the G/P crosswalks. C:Q14 and the archive/target/integrity security questions remain outside active reproduction scope.

C:Q23: [Should a higher-version load remove an omitted web application?](../../ontology/concepts/modules.g5.md#module.webapp.upgrade.policy) C:D26 observed retention after load; it did not establish update behavior or owner approval.

C:Q24: [When a phase list mixes standard phases with a custom or unknown name, what should run?](../../ontology/concepts/lifecycle.g5.md#lifecycle.phase-list-policy) Iteration30 observed that such names are dropped without notice (C:D35); the order of a custom phase in a mixed list and refusal of unknown names are not settled by any source.

C:Q25: [Should an unversioned install fall back to the newest offer that satisfies installed requirements?](../../ontology/features/dependencies.g5.md#deps.installed.unversioned-policy) Iteration31 case S4 observed a refusal although an older offer satisfied every requirement.

C:Q26: [Should a directory load refuse or warn when it breaks an installed module's requirement?](../../ontology/features/dependencies.g5.md#deps.installed.directory-policy) Iteration31 case S6 observed a silent success; S7 is the passing neighbour after the requiring module is removed.

C:Q27: [What should the installer's upgrade flag and database name mean?](../../ontology/concepts/lifecycle.g5.md#lifecycle.installer.upgrade-policy) Iteration36 observed that IPM never sets them, even when a newer version replaces an installed one.

C:Q28: [Should a load fail or warn when `LifecycleClass` names a class that is not available?](../../ontology/concepts/modules.g5.md#module.lifecycle-class.unknown-policy) Iteration52 observed a silent fallback to `Module`.
