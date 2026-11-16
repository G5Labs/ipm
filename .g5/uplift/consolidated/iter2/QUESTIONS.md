# Owner questions and remaining design choices

These questions are proposals; none has an owner-ratified answer in this iteration.

| Owner role | Canonical question | Working assumption and material alternative |
|---|---|---|
| IPM maintainers and operators | history.delete.running-records | An active action must finish even when its log is deleted; choose whether delete skips active rows or later writers may restore them. Two schedules have different native results. |
| IPM maintainers | repo.filesystem.concurrent-refresh | Preserve consistent cache replacement. Decide whether deferred explicit lock release is needed, outer transactions should be refused, or database locks suffice. No race corruption was established. |
| Registry and IPM maintainers | repo.oras.integrity.policy | Comment suggests highest semantic version. Sort or require an ordering contract; also choose whether multiple archive layers are valid and how the selected bytes bind to the selected layer. |
| Instance administrators and IPM maintainers | module.processor.cpf-effects.policy | Database rollback is not configuration compensation. Choose supported CPF actions/compensation/refusal policy before promising atomic install. |

Canonical blocks contain authority, as-built outcomes, alternatives and dependencies. Existing questions about child journaling, caller transactions, rollback policy, history selection and invalid repository manifests remain open. No permission request or policy decision blocks the evidence delivered here.
