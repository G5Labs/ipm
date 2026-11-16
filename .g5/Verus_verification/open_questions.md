# IPM guidance-arm question crosswalk

The canonical ontology owns the question. This audit corrects stale resolved-by-assumption and no-owner-block labels; it does not manufacture approval. [Full source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md).

| Starting ID | Canonical owner | Current disposition |
|---|---|---|
| G:Q-01 | [dev-mode.sticky](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.sticky) | open |
| G:Q-02 | [load.refusal.update-required.dev-bypass](../ontology/features/load.g5.md#load.refusal.update-required.dev-bypass) | open |
| G:Q-03 | [dev-mode.overwrite-guard.force](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.force) | open |
| G:Q-04 | [transaction.rollback-scope.policy](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope.policy) | rollback mechanics settled; compensation policy remains open |
| G:Q-05 | [history.committed.nested](../ontology/features/history.g5.md#history.committed.nested) | open |
| G:Q-06 | [history.dependencies.parent-rollback](../ontology/features/history.g5.md#history.dependencies.parent-rollback) | settled meaning; inherited attributed source preserved |
| G:Q-07 | [history.committed.side-effects](../ontology/features/history.g5.md#history.committed.side-effects) | settled meaning; inherited attributed source preserved |
| G:Q-08 | [history.record.abnormal-end](../ontology/features/history.g5.md#history.record.abnormal-end) | open |
| G:Q-09 | [history.record.unlogged-refusals](../ontology/features/history.g5.md#history.record.unlogged-refusals) | open |
| G:Q-10 | [history.read.filter.invalid-column](../ontology/features/history.g5.md#history.read.filter.invalid-column) | open |
| G:Q-11 | [history.delete.confirm-with-filter](../ontology/features/history.g5.md#history.delete.confirm-with-filter) | open |
| G:Q-12 | [history.delete.global-unfiltered](../ontology/features/history.g5.md#history.delete.global-unfiltered) | settled meaning; inherited attributed source preserved |
| G:Q-13 | [history.retention.effect](../ontology/features/history.g5.md#history.retention.effect) | open |
| G:Q-14 | [history.phases.returned-errors](../ontology/features/history.g5.md#history.phases.returned-errors) | open |
| G:Q-15 | [history.fields.dependency-source](../ontology/features/history.g5.md#history.fields.dependency-source) | open |
| G:Q-16 | [history.keep-on-ipm-uninstall.other-namespaces](../ontology/features/history.g5.md#history.keep-on-ipm-uninstall.other-namespaces) | open |
| G:Q-17 | [load.refusal.requirements.timing](../ontology/features/load.g5.md#load.refusal.requirements.timing) | open |
| G:Q-18 | [history.read.filter.default-modifiers](../ontology/features/history.g5.md#history.read.filter.default-modifiers) | open |
| G:Q-19 | [reinstall.version](../ontology/features/install.g5.md#reinstall.version) | open |
| G:Q-20 | [history.delete.running-records](../ontology/features/history.g5.md#history.delete.running-records) | open |
| G:Q-21 | [history.locked.unfinished](../ontology/features/history.g5.md#history.locked.unfinished) | open |
| G:Q-22 | [transaction.flag-sources.dependency-defaults](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-defaults) | open |
| G:Q-23 | [transaction.flag-sources.dependency-no-transaction](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-no-transaction) | open |
| G:Q-24 | [dev-mode.overwrite-guard.dependency-policy](../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.dependency-policy) | open |
| G:Q-25 | [transaction.flag-sources.dependency-closure](../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-closure) | open |
| G:Q-26 | [history.committed.escaped-dependency](../ontology/features/history.g5.md#history.committed.escaped-dependency) | open |
| G:Q-27 | [history.delete.global-errors](../ontology/features/history.g5.md#history.delete.global-errors) | open |
| G:Q-28 | [history.read.global-shape](../ontology/features/history.g5.md#history.read.global-shape) | open |
| G:Q-29 | [history.retention.running-policy](../ontology/features/history.g5.md#history.retention.running-policy) | open |
| G:Q-30 | [history.survives-rollback.repair-policy](../ontology/features/history.g5.md#history.survives-rollback.repair-policy) | open |
| G:Q-31 | [history.committed.marking.refusal-policy](../ontology/features/history.g5.md#history.committed.marking.refusal-policy) | open |

G:Q-06b remains an alias of Q-05; G:Q-FIN remains an alias of Q-21. Q-06 and Q-07 no longer appear as unresolved merely because old prose lagged the canonical blocks. Q-12 remains the documented global-delete decision. Original source research remains in [the sealed register](../uplift/consolidated/findings-audit-iter28/sealed/.g5/Verus_verification/open_questions.md).
