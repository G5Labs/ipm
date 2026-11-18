# Ontology clarification questions

<a id="lifecycle.installer.upgrade-policy"></a>
### What should the installer's upgrade flag and database name mean?

[Q · lifecycle.installer.upgrade-policy]

- Working assumption: as built, IPM never sets them. The upgrade flag is 0 and the database name is empty unless `-DConfigure.IsUpgrade` or `-DConfigure.DatabaseName` supplies them, even when a newer version replaces an installed one.
- Alternative and implications: set the upgrade flag when the module is already installed. That lets installers migrate data but changes the calls existing installers receive.
- This assumption guides review; it is not a maintainer decision.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#lifecycle.installer.upgrade-policy)

```g5-question
schema: 1
id: lifecycle.installer.upgrade-policy
status: open
prompt: What should the installer's upgrade flag and database name mean?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: as built, IPM never sets them. The upgrade flag is 0 and the database name is empty unless `-DConfigure.IsUpgrade` or `-DConfigure.DatabaseName` supplies them, even when a newer version replaces an installed one.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: as built, IPM never sets them. The upgrade flag is 0 and the database name is empty unless `-DConfigure.IsUpgrade` or `-DConfigure.DatabaseName` supplies them, even when a newer version replaces an installed one.
```

<a id="lifecycle.phase-list-policy"></a>
### When a phase list mixes standard phases with a custom or unknown name, what should run?

[Q · lifecycle.phase-list-policy]

- Working assumption: Run the standard chain, then each custom phase in the order named. Refuse the whole command before any phase runs when a name is neither a standard nor a declared custom phase.
- Alternative and implications: Run each custom phase at its position in the command, which needs a rule for where it falls inside a chain. Or refuse any mix of custom and standard phases with a clear error, which is simpler but breaks lists that work alone.
- This assumption guides review; it is not a maintainer decision. The command help ("Space-delimited list of module lifecycle phases to run") shows lists are intended but does not settle order or refusal.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#lifecycle.phase-list-policy)

```g5-question
schema: 1
id: lifecycle.phase-list-policy
status: open
prompt: When a phase list mixes standard phases with a custom or unknown name, what should run?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Run the standard chain, then each custom phase in the order named. Refuse the whole command before any phase runs when a name is neither a standard nor a declared custom phase.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Run the standard chain, then each custom phase in the order named. Refuse the whole command before any phase runs when a name is neither a standard nor a declared custom phase.
```

<a id="dec-sync-4"></a>
<a id="lifecycle.empty-reload-policy"></a>
### Should standalone reload warn or fail when a module has no source root?

[Q · lifecycle.empty-reload-policy]

- Working assumption: A standalone `reload` of a module with no root says that nothing was loaded; the phase stays silent inside other phase lists.
- Alternative and implications: Fail the standalone command (stricter for scripts).
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-SYNC-4 · What should `reload` do for a module that is not in developer mode (no root directory)? · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#lifecycle.empty-reload-policy)

```g5-question
schema: 1
id: lifecycle.empty-reload-policy
status: open
prompt: Should standalone reload warn or fail when a module has no source root?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: A standalone `reload` of a module with no root says that nothing was loaded; the phase stays silent inside other phase lists.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: A standalone `reload` of a module with no root says that nothing was loaded; the phase stays silent inside other phase lists.
```

<a id="module.lifecycle-class.unknown-policy"></a>
### Should a load fail or warn when `LifecycleClass` names a class that is not available when the lifecycle is resolved?

[Q · module.lifecycle-class.unknown-policy]

- Working assumption: as built, fall back to `Module` silently.
- Alternative and implications: fail the load, which catches typos but blocks modules that ship the class as an ordinary resource; or warn and keep the manifest's name.
- This assumption guides review; it is not a maintainer decision.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.lifecycle-class.unknown-policy)

```g5-question
schema: 1
id: module.lifecycle-class.unknown-policy
status: open
prompt: Should a load fail or warn when `LifecycleClass` names a class that is not available when the lifecycle is resolved?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: as built, fall back to `Module` silently.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: as built, fall back to `Module` silently.
```

<a id="module.webapp.upgrade.policy"></a>
### Should a higher-version load remove a web application omitted by the new manifest?

[Q · module.webapp.upgrade.policy]

- Working assumption: disclose that the application remains; do not promise automatic removal without an ownership rule.
- Alternatives: remove resources formerly owned by the module, or require an explicit migration. Automatic removal needs ownership and administrator-edit checks; explicit migration leaves cleanup to the author.
- An owner decision covering dropped resources, administrator changes and later uninstall would settle the policy. C:D26 observed a load, not update.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.webapp.upgrade.policy)

```g5-question
schema: 1
id: module.webapp.upgrade.policy
status: open
prompt: Should a higher-version load remove a web application omitted by the new manifest?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: disclose that the application remains; do not promise automatic removal without an ownership rule.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: disclose that the application remains; do not promise automatic removal without an ownership rule.
```

<a id="c-q15"></a>
<a id="module.webapp.ownership"></a>
### Should install take over an existing web application of the same name, and should uninstall delete one IPM did not create?

[Q · module.webapp.ownership]

- Evidence: `CreateOrUpdateWebApp` updates any existing application and `DeleteWebApp` deletes by name. No changelog, test or discussion addresses applications made outside IPM. The duplicate-name refusal protects only applications another module lists.
- Working assumption: keep the as-built behavior and disclose it. Alternatives are to refuse install when the application exists and no module lists it, or to record creation and delete only what IPM created. Both change today's reinstall-over-manual-setup path.
- Register: C:Q15.

- C:Q15 · Should install take over an existing web application IPM did not create, and should uninstall delete it? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.webapp.ownership)

```g5-question
schema: 1
id: module.webapp.ownership
status: open
prompt: Should install take over an existing web application of the same name, and should uninstall delete one IPM did not create?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep the as-built behavior and disclose it. Alternatives are to refuse install when the application exists and no module lists it, or to record creation and delete only what IPM created. Both change today's reinstall-over-manual-setup path.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep the as-built behavior and disclose it. Alternatives are to refuse install when the application exists and no module lists it, or to record creation and delete only what IPM created. Both change today's reinstall-over-manual-setup path.
```

<a id="c-q6"></a>
<a id="module.env.failure"></a>
### When one of several `-env` files is missing, malformed or an array, what should the command keep, and how should two arrays merge?

[Q · module.env.failure]

- Provisional answer: as built. The command fails and keeps nothing from any file, and a later array overwrites the earlier one position by position, so it cannot remove items.
- Alternatives: keep the files loaded before the failing one, or check every file before changing anything; let a later array replace the earlier one.

- C:Q6 · When one of several `-env` files fails, what should the command keep, and how should two arrays merge? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.env.failure)

```g5-question
schema: 1
id: module.env.failure
status: open
prompt: When one of several `-env` files is missing, malformed or an array, what should the command keep, and how should two arrays merge?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: as built. The command fails and keeps nothing from any file, and a later array overwrites the earlier one position by position, so it cannot remove items.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: as built. The command fails and keeps nothing from any file, and a later array overwrites the earlier one position by position, so it cannot remove items.
```

<a id="c-q2"></a>
<a id="module.parameter.expressions.brace-policy"></a>
### Should a `$$$macro` expand when the attribute contains no opening brace?

[Q · module.parameter.expressions.brace-policy]

- Evidence: `%Evaluate` returns early on no `{`; native E15/P4 leave the macro literal. The expression documentation lists macros without this gate.
- Working assumption: keep the observed brace gate until a maintainer chooses otherwise.
- Alternative: expand macros with or without a brace.
- Affected claims: parameter evaluation, after-install message, file-copy target and Invoke arguments. Alias C:Q2; parent [module.parameter.expressions](../ontology/features/manifest.g5.md#module.parameter.expressions).

- C:Q2 · Should a `$$$macro` in an attribute value be expanded when the value contains no `{`? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.parameter.expressions.brace-policy)

```g5-question
schema: 1
id: module.parameter.expressions.brace-policy
status: open
prompt: Should a `$$macro` expand when the attribute contains no opening brace?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep the observed brace gate until a maintainer chooses otherwise.
    description: Default assumption.
  - label: |-
      expand macros with or without a brace.
      - Affected claims: parameter evaluation, after-install message, file-copy target and Invoke arguments. Alias C:Q2; parent
    description: |-
      expand macros with or without a brace.
      - Affected claims: parameter evaluation, after-install message, file-copy target and Invoke arguments. Alias C:Q2; parent [module.parameter.expressions](../ontology/features/manifest.g5.md#module.parameter.expressions)
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep the observed brace gate until a maintainer chooses otherwise.
```

<a id="c-q12"></a>
<a id="module.processor.cpf-effects.policy"></a>
### Must package failure undo instance configuration changes?

[Q · module.processor.cpf-effects.policy]

- Owner: IPM maintainers and instance administrators; no owner decision was available.
- Working assumption: database rollback alone gives no such promise. Alternatives are compensating selected changes or refusing them inside package transactions.
- Depends on: [rollback policy](#transaction.rollback-scope.policy). A successful native role test settles observation, not this policy.

- C:Q12 · Must package failure compensate CPF configuration changes? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.processor.cpf-effects.policy)

```g5-question
schema: 1
id: module.processor.cpf-effects.policy
status: open
prompt: Must package failure undo instance configuration changes?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: database rollback alone gives no such promise. Alternatives are compensating selected changes or refusing them inside package transactions.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: database rollback alone gives no such promise. Alternatives are compensating selected changes or refusing them inside package transactions.
```

<a id="module.system-setting.cleanup-policy"></a>
### Should uninstall keep a legacy SystemSetting value or restore the value that preceded installation?

[Q · module.system-setting.cleanup-policy]

The implementation keeps the installed value, and that is the working assumption. Restoring the old value would need to track it and account for later administrator edits; blindly deleting it can remove a setting another user now relies on. The owner must choose this policy (C:Q17).



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.system-setting.cleanup-policy)

```g5-question
schema: 1
id: module.system-setting.cleanup-policy
status: open
prompt: Should uninstall keep a legacy SystemSetting value or restore the value that preceded installation?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Q17).
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Q17).
```

<a id="module.python.cleanup-policy"></a>
### How should IPM remove Python packages without removing a package another module or administrator uses?

[Q · module.python.cleanup-policy]

The working assumption preserves the current shared installation on Clean. Tracking package users could permit later removal, but the product TODO does not settle ownership of manually installed or upgraded versions. This needs an owner decision (C:Q18).



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.python.cleanup-policy)

```g5-question
schema: 1
id: module.python.cleanup-policy
status: open
prompt: How should IPM remove Python packages without removing a package another module or administrator uses?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Q18).
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Q18).
```

<a id="c-q22"></a>
<a id="module.processor.file-copy.unknown-descendants"></a>
### Should FileCopy protect a parent containing an unconfigured descendant with an IRIS.DAT marker, or only configured roots/ancestors and a marker at the exact target?

[Q · module.processor.file-copy.unknown-descendants]

The observed rule is the latter. Tests cover exact unconfigured markers and configured-root ancestors; no inspected human decision settles a recursive search requirement. C:Q22 retains both interpretations.

- C:Q22 · Should FileCopy protect a parent containing an unconfigured descendant with an IRIS.DAT marker? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#module.processor.file-copy.unknown-descendants)

```g5-question
schema: 1
id: module.processor.file-copy.unknown-descendants
status: open
prompt: Should FileCopy protect a parent containing an unconfigured descendant with an IRIS.DAT marker, or only configured roots/ancestors and a marker at the exact target?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Protect configured storage roots, their ancestors, and an exact target containing an IRIS.DAT marker. Preserve the observed behavior while a recursive search requirement remains undecided.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Protect configured storage roots, their ancestors, and an exact target containing an IRIS.DAT marker. Preserve the observed behavior while a recursive search requirement remains undecided.
```

<a id="dec-nsp-6"></a>
<a id="namespace.enable.source-ambiguity-policy"></a>
### Which source owns mapped IPM when several namespace records name one code database?

[Q · namespace.enable.source-ambiguity-policy]

- Working assumption: The source of a mapped namespace is the namespace whose own code database holds the IPM code.
- Alternative and implications: Report a tie as an error instead of guessing. Either way F-NSP-7 stays a defect.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-NSP-6 · Two namespaces with a zpm record share one IPM database (IPM's verify-phase namespace USER-VERIFY, or an installer run in a mapped namespace). Which one is the source of the namespaces mapped to that database? · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#namespace.enable.source-ambiguity-policy)

```g5-question
schema: 1
id: namespace.enable.source-ambiguity-policy
status: open
prompt: Which source owns mapped IPM when several namespace records name one code database?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: The source of a mapped namespace is the namespace whose own code database holds the IPM code.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: The source of a mapped namespace is the namespace whose own code database holds the IPM code.
```

<a id="dec-nsp-4"></a>
<a id="namespace.enable.community-failure-policy"></a>
### What status should enable -community return when a component step fails?

[Q · namespace.enable.community-failure-policy]

- Working assumption: If a step of `enable -community` fails, the command stops and returns that error.
- Alternative and implications: Run all three steps and return a combined error (also fine); or keep OK and document: F-NSP-11 becomes documentation.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-NSP-4 · If one of the three steps of `enable -community` fails, should the command fail? · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#namespace.enable.community-failure-policy)

```g5-question
schema: 1
id: namespace.enable.community-failure-policy
status: open
prompt: What status should enable -community return when a component step fails?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: If a step of `enable -community` fails, the command stops and returns that error.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: If a step of `enable -community` fails, the command stops and returns that error.
```

<a id="dec-repo-5"></a>
<a id="repo.oras.unpublish-platform-policy"></a>
### Which platform tags should unpublish remove when only a version is named?

[Q · repo.oras.unpublish-platform-policy]

- Working assumption: ORAS `unpublish R/M V` deletes the plain tag and every `V__<platform>` tag of that version.
- Alternative and implications: Require the platform to be named; the plain form must still not fail as "not found".
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-REPO-5 · Should `unpublish R/M V` in an ORAS registry also delete the platform-specific copies of a deployed module (tags `<version><sep><platform>`)? Code deletes only the plain version tag (Oras/PublishService.cls#L72-L81); deployed copies are pushed with a platform suffix (#L43-L48). · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.oras.unpublish-platform-policy)

```g5-question
schema: 1
id: repo.oras.unpublish-platform-policy
status: open
prompt: Which platform tags should unpublish remove when only a version is named?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: ORAS `unpublish R/M V` deletes the plain tag and every `V__<platform>` tag of that version.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: ORAS `unpublish R/M V` deletes the plain tag and every `V__<platform>` tag of that version.
```

<a id="dec-repo-3"></a>
<a id="repo.oras.credentials-change-policy"></a>
### Should changing an ORAS registry address clear its stored credentials?

[Q · repo.oras.credentials-change-policy]

- Working assumption: Changing an ORAS registry's address clears its stored credentials, as for a remote registry.
- Alternative and implications: Keep and warn (operator only renamed the host): F-REPO-11 becomes a missing warning.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-REPO-3 · Should changing the address of an ORAS registry clear its stored credentials, as a remote registry does (Remote/Definition.cls#L81-L86)? ORAS OnConfigure (Oras/Definition.cls#L87-L141) keeps them, so they are sent to the new host. · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.oras.credentials-change-policy)

```g5-question
schema: 1
id: repo.oras.credentials-change-policy
status: open
prompt: Should changing an ORAS registry address clear its stored credentials?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Changing an ORAS registry's address clears its stored credentials, as for a remote registry.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Changing an ORAS registry's address clears its stored credentials, as for a remote registry.
```

<a id="c-q5"></a>
<a id="repo.filesystem.bad-module"></a>
### Should a module with an invalid name in a filesystem repository's folder block the whole repository?

[Q · repo.filesystem.bad-module]

- Provisional answer: no. Skip it with a warning, like an unreadable manifest, so the other modules stay available.
- As built: yes. The same failure stops a load's temporary `.modules` repository from being created, which then fails the load after its commit.
- Alternative: fail every bad manifest, including unreadable ones, so nothing is silently hidden.

- C:Q5 · Should one module with an invalid name block a whole filesystem repository? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.filesystem.bad-module)

```g5-question
schema: 1
id: repo.filesystem.bad-module
status: open
prompt: Should a module with an invalid name in a filesystem repository's folder block the whole repository?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: no. Skip it with a warning, like an unreadable manifest, so the other modules stay available.
    description: Default assumption.
  - label: fail every bad manifest, including unreadable ones, so nothing is silently hidden
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: no. Skip it with a warning, like an unreadable manifest, so the other modules stay available.
```

<a id="c-q9"></a>
<a id="repo.oras.integrity.policy"></a>
### Should an untagged pull always select the highest semantic version, and must a package have exactly one archive layer?

[Q · repo.oras.integrity.policy]

- Owner: registry maintainers and IPM maintainers; no owner decision was available. The method comment supports highest-version intent, while source behavior depends on provider order.
- Working assumption: expose the mismatch and retain size/digest checking. Alternatives are sorting before selection or requiring a registry ordering contract, plus rejecting ambiguous archives or selecting by declared digest.
- Proposed repairs are not implemented or proved. Preserve deployed platform tags and compare both a valid single archive and ambiguous multi-layer input before a repair.

- C:Q9 · Which ORAS tag/layer ordering is authoritative? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.oras.integrity.policy)

```g5-question
schema: 1
id: repo.oras.integrity.policy
status: open
prompt: Should an untagged pull always select the highest semantic version, and must a package have exactly one archive layer?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: expose the mismatch and retain size/digest checking. Alternatives are sorting before selection or requiring a registry ordering contract, plus rejecting ambiguous archives or selecting by declared digest.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: expose the mismatch and retain size/digest checking. Alternatives are sorting before selection or requiring a registry ordering contract, plus rejecting ambiguous archives or selecting by declared digest.
```

<a id="c-q14"></a>
<a id="repo.remote.download.credentials"></a>
### Should registry credentials be sent to a download URL on a different server?

[Q · repo.remote.download.credentials]

- Evidence: commit `d6cc4204` switched to the registry-supplied URL to "fix downloading packages"; a native provider received the repository's Basic credentials and API key at a different host and port. Elsewhere the maintainers treat credentials as belonging to one host: CHANGELOG 0.10.11 `#1115` stops ORAS credential checks from following redirects "so credentials are not sent to another host", and `#649` clears a remote registry's credentials when its URL changes. No source decides the download case.
- Working assumption: keep following the returned URL, and record the forwarding as suspected defect C:D20. The proposed alternative sends credentials only when the URL's server and port match the repository; a third choice is to refuse such URLs. Either change can break registries that redirect downloads to storage needing the same credentials.
- Register: C:Q14 and proposal C:S14, revised as C:S14b after the [redirect challenge](../ontology/concepts/download-security.g5.md#repo.remote.download.redirect).

- C:Q14 · Should registry credentials go to a download URL on another server? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.remote.download.credentials)

```g5-question
schema: 1
id: repo.remote.download.credentials
status: open
prompt: Should registry credentials be sent to a download URL on a different server?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep following the returned URL, and record the forwarding as suspected defect C:D20. The proposed alternative sends credentials only when the URL's server and port match the repository; a third choice is to refuse such URLs. Either change can break registries that redirect downloads to storage needing the same credentials.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep following the returned URL, and record the forwarding as suspected defect C:D20. The proposed alternative sends credentials only when the URL's server and port match the repository; a third choice is to refuse such URLs. Either change can break registries that redirect downloads to storage needing the same credentials.
```

<a id="c-q8"></a>
<a id="repo.filesystem.concurrent-refresh"></a>
### Does cache replacement need its explicit lock until the caller's transaction ends?

[Q · repo.filesystem.concurrent-refresh]

- Owner: IPM maintainers. The default constructor releases at object close; the deferred constructor is a separate supported mechanism.
- Working assumption: keep the intended consistent cache and investigate competing refreshes under actual database isolation. Alternatives are deferred release, refusing an outer transaction or demonstrating that existing database locks suffice.
- No race corruption or repair is established by the observed lock release. [Withdrawal and paired evidence](../uplift/consolidated/iter2/boundaries/withdrawals.json).

- C:Q8 · How should an uncommitted filesystem cache refresh stay consistent after the default manager closes? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#repo.filesystem.concurrent-refresh)

```g5-question
schema: 1
id: repo.filesystem.concurrent-refresh
status: open
prompt: Does cache replacement need its explicit lock until the caller's transaction ends?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep the intended consistent cache and investigate competing refreshes under actual database isolation. Alternatives are deferred release, refusing an outer transaction or demonstrating that existing database locks suffice.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep the intended consistent cache and investigate competing refreshes under actual database isolation. Alternatives are deferred release, refusing an outer transaction or demonstrating that existing database locks suffice.
```

<a id="g-q-22"></a>
<a id="transaction.flag-sources.dependency-defaults"></a>
### Does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes escape the parent's normal-mode rollback?

[Q · transaction.flag-sources.dependency-defaults]

- Provisional answer: yes. A module's `<Defaults>` apply whenever that module is installed, including as a dependency. A `NoJournal` dependency therefore keeps its changes when the parent rolls back, and its history record must survive and say so.
- Reason: the wiki says flags embedded in `module.xml` are "applied whenever this module is installed", and that with `NoJournal` "changes cannot be rolled back". These statements are specific to the flags, while the whole-load rollback rule is general.
- Alternative: the parent's transaction governs every nested load, so a dependency's `<Defaults>` `NoJournal` is ignored, or the dependency is refused, while a parent transaction is open. This keeps [the whole-load rollback](../ontology/features/load.g5.md#load.outcome).

Unresolved Q-25: [May an escaped NoJournal dependency remain without its own dependencies?](#transaction.flag-sources.dependency-closure)

- G:Q-22 · Q-22: Does a dependency's own `NoJournal` default escape its parent's rollback? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#transaction.flag-sources.dependency-defaults)

```g5-question
schema: 1
id: transaction.flag-sources.dependency-defaults
status: open
prompt: Does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes escape the parent's normal-mode rollback?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: yes. A module's `<Defaults>` apply whenever that module is installed, including as a dependency. A `NoJournal` dependency therefore keeps its changes when the parent rolls back, and its history record must survive and say so.
    description: Default assumption.
  - label: the parent's transaction governs every nested load, so a dependency's `<Defaults>` `NoJournal` is ignored, or the dependency is refused, while a parent transact
    description: the parent's transaction governs every nested load, so a dependency's `<Defaults>` `NoJournal` is ignored, or the dependency is refused, while a parent transaction is open. This keeps [the whole-load rollback](../ontology/features/load.g5.md#load.outcome)
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: yes. A module's `<Defaults>` apply whenever that module is installed, including as a dependency. A `NoJournal` dependency therefore keeps its changes when the parent rolls back, and its history record must survive and say so.
```

<a id="g-q-23"></a>
<a id="transaction.flag-sources.dependency-no-transaction"></a>
### Should a dependency's own `<Defaults>` `NoTransaction` suppress its parent's transaction?

[Q · transaction.flag-sources.dependency-no-transaction]

- Provisional answer: yes. A module that declares `NoTransaction` must not run inside a transaction opened for its parent, so the package change runs without one, and a failure may leave a partial install.
- Reason: the wiki gives `NoTransaction`'s purpose as modules in the install that "conflict with an outer wrapping transaction", and says `<Defaults>` apply "whenever this module is installed". Without suppression, the flag of a dependency would have no effect, because the dependency would still run inside the parent's transaction.
- Alternative: the flag affects only the dependency's own load, so the parent's transaction still wraps it and undoes it on failure. A user whose install contains a conflicting module passes `-DNoTransaction=1` for the whole command.

- G:Q-23 · Q-23: Does a dependency's own `NoTransaction` default suppress its parent's transaction? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#transaction.flag-sources.dependency-no-transaction)

```g5-question
schema: 1
id: transaction.flag-sources.dependency-no-transaction
status: open
prompt: Should a dependency's own `<Defaults>` `NoTransaction` suppress its parent's transaction?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: yes. A module that declares `NoTransaction` must not run inside a transaction opened for its parent, so the package change runs without one, and a failure may leave a partial install.
    description: Default assumption.
  - label: the flag affects only the dependency's own load, so the parent's transaction still wraps it and undoes it on failure. A user whose install contains a conflictin
    description: the flag affects only the dependency's own load, so the parent's transaction still wraps it and undoes it on failure. A user whose install contains a conflicting module passes `-DNoTransaction=1` for the whole command
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: yes. A module that declares `NoTransaction` must not run inside a transaction opened for its parent, so the package change runs without one, and a failure may leave a partial install.
```

<a id="g-q-03"></a>
<a id="dev-mode.overwrite-guard.force"></a>
### May `-force` lift the developer-mode overwrite guard, and does the guard cover `load`?

[Q · dev-mode.overwrite-guard.force]

- Provisional answer: keep the current scope. The guard covers `install` only, and `-force` does not lift it.
- Reason: this follows the product as built since the guard was added, and the `-force` help, which speaks only of newer versions without update steps. The wiki's "Uninstall first, or use `-force`" conflicts with both, and a maintainer must decide which is meant.
- Alternative: let `-force` lift the guard as the wiki says, and decide whether `load` is covered.

- G:Q-03 · Q-03: Scope of the developer-mode overwrite guard and `-force` · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#dev-mode.overwrite-guard.force)

```g5-question
schema: 1
id: dev-mode.overwrite-guard.force
status: open
prompt: May `-force` lift the developer-mode overwrite guard, and does the guard cover `load`?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep the current scope. The guard covers `install` only, and `-force` does not lift it.
    description: Default assumption.
  - label: let `-force` lift the guard as the wiki says, and decide whether `load` is covered
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep the current scope. The guard covers `install` only, and `-force` does not lift it.
```

<a id="g-q-01"></a>
<a id="dec-load-2"></a>
<a id="dev-mode.sticky"></a>
### When a module is already in developer mode, does a plain `load` with neither `-dev` nor `-nodev` keep developer mode?

[Q · dev-mode.sticky]

- Provisional answer: yes, it keeps the stored mode.
- Reason: the fix this follows stopped plain loads from turning developer mode on. It did not ask plain loads to turn it off, and `-nodev` exists for that.
- Alternative: a plain load always uses normal mode, following that fix's wording that a module "should only be DevMode if the -dev flag is included".

- G:Q-01 · Q-01: Does a plain `load` keep developer mode? · open
- DEC-LOAD-2 · Should a plain `load` (no `-dev`, no `-nodev`) over a module installed in developer mode keep developer mode? It does today (O:ipm-dev-then-plain-load); the help text only says `-nodev` disables it. · open; conflicting canonical question takes precedence over the resolved label

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#dev-mode.sticky)

```g5-question
schema: 1
id: dev-mode.sticky
status: open
prompt: When a module is already in developer mode, does a plain `load` with neither `-dev` nor `-nodev` keep developer mode?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: yes, it keeps the stored mode.
    description: Default assumption.
  - label: a plain load always uses normal mode, following that fix's wording that a module "should only be DevMode if the -dev flag is included"
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: yes, it keeps the stored mode.
```

<a id="g-q-04"></a>
<a id="transaction.rollback-scope.policy"></a>
### Should a failed package change compensate external effects that the database cannot roll back?

[Q · transaction.rollback-scope.policy]

- Evidence: copied files and `module-lock.json` survived native database rollback; arbitrary hooks and remote calls can have further effects.
- Working assumption: database rollback is the existing boundary; external effects need separate receipts and must be reported.
- Alternative: add explicit compensating actions for each external writer.
- Affected claims: load failure atomicity, lock-file location and status, file copy, CPF and history `Committed`.

- G:Q-04 · Q-04: What does a rollback promise beyond the database? · rollback mechanics settled; compensation policy remains open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#transaction.rollback-scope.policy)

```g5-question
schema: 1
id: transaction.rollback-scope.policy
status: open
prompt: Should a failed package change compensate external effects that the database cannot roll back?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: database rollback is the existing boundary; external effects need separate receipts and must be reported.
    description: Default assumption.
  - label: |-
      add explicit compensating actions for each external writer.
      - Affected claims: load failure atomicity, lock-file location and status, file copy, CPF and history
    description: |-
      add explicit compensating actions for each external writer.
      - Affected claims: load failure atomicity, lock-file location and status, file copy, CPF and history `Committed`
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: database rollback is the existing boundary; external effects need separate receipts and must be reported.
```

<a id="g-q-24"></a>
<a id="dec-inst-3"></a>
<a id="dev-mode.overwrite-guard.dependency-policy"></a>
### May a parent replace a dependency that is already in developer mode?

[Q · dev-mode.overwrite-guard.dependency-policy]

- Evidence: Source guard checks only the top-level Install parameter; native N-R05-DEVUP replaces the dependency at another version and clears developer mode. Same-version DEVSAME skips it.
- Working assumption: Preserve the stated install overwrite guard for dependencies; load scope follows the parent force/load question.
- Alternative: Allow top-level guard only, with a warning when dependency developer mode is cleared.
- Affected claims: install refusal, dependency replacement and developer working copy preservation. Alias Q-24.

- G:Q-24 · Q-24: May loading or installing a parent replace a dependency that is in developer mode? · open
- DEC-INST-3 · Should install refuse to replace a needed module that is in developer mode, as it does for the named module? Today the dependency is silently replaced and loses developer mode (Utils/Module.cls L109, L308). Observed on IRIS: with complex-c 2.0.0 loaded -dev, `install complex-b 2.0.0` succeeds and leaves complex-c at 3.0.0 without developer mode, with no message (nc-inst-8-dev-dependency-replaced). · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#dev-mode.overwrite-guard.dependency-policy)

```g5-question
schema: 1
id: dev-mode.overwrite-guard.dependency-policy
status: open
prompt: May a parent replace a dependency that is already in developer mode?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Preserve the stated install overwrite guard for dependencies; load scope follows the parent force/load question.
    description: Default assumption.
  - label: |-
      Allow top-level guard only, with a warning when dependency developer mode is cleared.
      - Affected claims: install refusal, dependency replacement and developer w
    description: |-
      Allow top-level guard only, with a warning when dependency developer mode is cleared.
      - Affected claims: install refusal, dependency replacement and developer working copy preservation. Alias Q-24
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Preserve the stated install overwrite guard for dependencies; load scope follows the parent force/load question.
```

<a id="g-q-25"></a>
<a id="transaction.flag-sources.dependency-closure"></a>
### May an escaped NoJournal dependency remain without its own dependencies?

[Q · transaction.flag-sources.dependency-closure]

- Evidence: N-R05-NESTF leaves d1 installed and rolls d2 back because the flat closure loads d2 under parent parameters before d1.
- Working assumption: Require closure consistency as intent; record the observed violating state rather than assuming a repair.
- Alternative: Document temporary inconsistency after failure, or use one load-wide transaction mode.
- Affected claims: dependency satisfaction, mixed-mode rollback and retry. Alias Q-25.

- G:Q-25 · Q-25: If a `NoJournal` dependency escapes, may its own dependencies be rolled back? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#transaction.flag-sources.dependency-closure)

```g5-question
schema: 1
id: transaction.flag-sources.dependency-closure
status: open
prompt: May an escaped NoJournal dependency remain without its own dependencies?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Require closure consistency as intent; record the observed violating state rather than assuming a repair.
    description: Default assumption.
  - label: |-
      Document temporary inconsistency after failure, or use one load-wide transaction mode.
      - Affected claims: dependency satisfaction, mixed-mode rollback and retry
    description: |-
      Document temporary inconsistency after failure, or use one load-wide transaction mode.
      - Affected claims: dependency satisfaction, mixed-mode rollback and retry. Alias Q-25
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Require closure consistency as intent; record the observed violating state rather than assuming a repair.
```

<a id="c-q3"></a>
<a id="version.range.year"></a>
### Should a partial version such as `2024.1` mean exactly `2024.1.0`, as built, while `2024.12` and `1.2` mean a whole minor series?

[Q · version.range.year]

- Provisional answer: keep the special case for IRIS-style versions, because IPM's tests require `2022.2` to match only `2022.2`.
- Alternative: one rule for every partial version. Then `2022.2` would also accept `2022.2.1`.

- C:Q3 · Should `2024.1` mean exactly `2024.1.0` while `2024.12` and `1.2` mean a whole minor series? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#version.range.year)

```g5-question
schema: 1
id: version.range.year
status: open
prompt: Should a partial version such as `2024.1` mean exactly `2024.1.0`, as built, while `2024.12` and `1.2` mean a whole minor series?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep the special case for IRIS-style versions, because IPM's tests require `2022.2` to match only `2022.2`.
    description: Default assumption.
  - label: one rule for every partial version. Then `2022.2` would also accept `2022.2.1`
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep the special case for IRIS-style versions, because IPM's tests require `2022.2` to match only `2022.2`.
```

<a id="c-q10"></a>
<a id="artifact.refresh-policy"></a>
### When should an existing artifact folder or same-named temporary file be refreshed?

[Q · artifact.refresh-policy]

- Owner: IPM maintainers and module authors. The code deliberately reuses a local folder during Activate, but no accepted cache identity or expiry rule was found.
- Working assumption: preserve observed reuse and disclose that it gives no freshness guarantee. Alternatives are immutable digest-addressed files, explicit refresh, or validating the cached bytes against the selected artifact.
- This decision affects reproducible installs and exports. Do not remove local/offline reuse as part of an authentication repair.

- C:Q10 · When should artifact local content be refreshed? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#artifact.refresh-policy)

```g5-question
schema: 1
id: artifact.refresh-policy
status: open
prompt: When should an existing artifact folder or same-named temporary file be refreshed?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: preserve observed reuse and disclose that it gives no freshness guarantee. Alternatives are immutable digest-addressed files, explicit refresh, or validating the cached bytes against the selected artifact.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: preserve observed reuse and disclose that it gives no freshness guarantee. Alternatives are immutable digest-addressed files, explicit refresh, or validating the cached bytes against the selected artifact.
```

<a id="c-q11"></a>
<a id="artifact.target-policy"></a>
### Should artifact copies refuse protected installation folders as FileCopy does?

[Q · artifact.target-policy]

- Owner: IPM maintainers and instance administrators. The processor calls the shared warning before Activate's copy; it does not call FileCopy's deletion guard. No protected system folder was modified in this run.
- Working assumption: disclose the warning boundary without promising protection. Alternatives are a common refusal rule, an explicit operator override, or retaining warning-only behavior.
- Archive path safety, partial copies and compensation remain open. A successful copy under `/tmp` settles none of these policies.

- C:Q11 · Should Artifactory copies refuse protected installation targets? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#artifact.target-policy)

```g5-question
schema: 1
id: artifact.target-policy
status: open
prompt: Should artifact copies refuse protected installation folders as FileCopy does?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: disclose the warning boundary without promising protection. Alternatives are a common refusal rule, an explicit operator override, or retaining warning-only behavior.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: disclose the warning boundary without promising protection. Alternatives are a common refusal rule, an explicit operator override, or retaining warning-only behavior.
```

<a id="c-q13"></a>
<a id="build.test.outcome.empty.policy"></a>
### Should a declared test package with no test classes fail `test` and `verify`?

[Q · build.test.outcome.empty.policy]

- Evidence: the source comment calls it "not an error in itself" and skips it on purpose. No changelog entry, test or maintainer discussion states what CI users should see.
- Working assumption: keep the warning and the passing status. The alternative is to fail, or to fail only when a run executes no tests at all. Either alternative changes current green builds whose test package is intentionally empty.
- Register: C:Q13.

- C:Q13 · Should a declared test package with no test classes fail `test` and `verify`? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#build.test.outcome.empty.policy)

```g5-question
schema: 1
id: build.test.outcome.empty.policy
status: open
prompt: Should a declared test package with no test classes fail `test` and `verify`?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep the warning and the passing status. The alternative is to fail, or to fail only when a run executes no tests at all. Either alternative changes current green builds whose test package is intentionally empty.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep the warning and the passing status. The alternative is to fail, or to fail only when a run executes no tests at all. Either alternative changes current green builds whose test package is intentionally empty.
```

<a id="build.package.include-refresh.policy"></a>
### Should a reloaded manifest that omits IncludeTests keep the previous value or reset it to false?

[Q · build.package.include-refresh.policy]

- Working assumption: preserve the checked behavior and use an explicit false to clear it. Resetting on omission would make a removed declaration disable test packaging, but would change existing reload behavior.
- Evidence: the XML reader reuses the existing module object, does not reset this scalar field and writes the result back. Native package/extract observations distinguish retention from an explicit false. No owner decision found. Register: C:Q20.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#build.package.include-refresh.policy)

```g5-question
schema: 1
id: build.package.include-refresh.policy
status: open
prompt: Should a reloaded manifest that omits IncludeTests keep the previous value or reset it to false?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: preserve the checked behavior and use an explicit false to clear it. Resetting on omission would make a removed declaration disable test packaging, but would change existing reload behavior.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: preserve the checked behavior and use an explicit false to clear it. Resetting on omission would make a removed declaration disable test packaging, but would change existing reload behavior.
```

<a id="deps.installed.unversioned-policy"></a>
### When the newest offer of a module breaks an installed module's requirement, should `install <module>` without a version fall back to the newest offer that satisfies every requirement?

[Q · deps.installed.unversioned-policy]

- Working assumption: keep the as-built refusal, so the user sees the conflict and chooses a version explicitly.
- Alternative and implications: fall back to the newest satisfying offer, which matches how dependency resolution already chooses versions but can silently install an older version than the user expected.
- This assumption guides review; it is not a maintainer decision. The install help says only that the latest available version is installed.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#deps.installed.unversioned-policy)

```g5-question
schema: 1
id: deps.installed.unversioned-policy
status: open
prompt: When the newest offer of a module breaks an installed module's requirement, should `install <module>` without a version fall back to the newest offer that satisfies every requirement?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep the as-built refusal, so the user sees the conflict and chooses a version explicitly.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep the as-built refusal, so the user sees the conflict and chooses a version explicitly.
```

<a id="deps.installed.directory-policy"></a>
### Should `load <directory>` refuse or warn when the loaded module's version breaks a requirement that installed modules declare on it?

[Q · deps.installed.directory-policy]

- Working assumption: as built, the load succeeds without a warning, and installed modules keep requirements the namespace no longer satisfies.
- Alternative and implications: refuse with the same message as a repository install, which protects installed modules but blocks deliberate local overrides; or warn and continue.
- This assumption guides review; it is not a maintainer decision. If the owner requires a check, iteration31 case S6 reproduces the gap and S7 is its passing neighbour.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#deps.installed.directory-policy)

```g5-question
schema: 1
id: deps.installed.directory-policy
status: open
prompt: Should `load <directory>` refuse or warn when the loaded module's version breaks a requirement that installed modules declare on it?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: as built, the load succeeds without a warning, and installed modules keep requirements the namespace no longer satisfies.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: as built, the load succeeds without a warning, and installed modules keep requirements the namespace no longer satisfies.
```

<a id="dec-dep-3"></a>
<a id="deps.lockfile.local-supplier-policy"></a>
### Should a lock file accept a directory-loaded dependency with no registry?

[Q · deps.lockfile.local-supplier-policy]

- Working assumption: A needed module loaded from a folder is written to the lock file with an empty registry.
- Alternative and implications: Fail the command (if a future `ci` needs a registry for every entry): F-DEP-9 changes accordingly.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-DEP-3 · What should -create-lockfile do when a needed module has no registry (it was added with load from a folder)? Today no file is written and an error is printed; the install succeeds. Observed on IRIS: with lock-mod-a loaded from a folder, `install -create-lockfile lock-mod-e-1-dep-0-transient` succeeds, prints "Error creating lock file ... Tried getting repo "" but none found", and leaves the existing module-lock.json untouched (nc-dep-14-lockfile-folder-dependency). · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#deps.lockfile.local-supplier-policy)

```g5-question
schema: 1
id: deps.lockfile.local-supplier-policy
status: open
prompt: Should a lock file accept a directory-loaded dependency with no registry?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: A needed module loaded from a folder is written to the lock file with an empty registry.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: A needed module loaded from a folder is written to the lock file with an empty registry.
```

<a id="dec-sync-3"></a>
<a id="devtools.sync.aggregate-status-policy"></a>
### Should sync without a module name return an error when a module fails?

[Q · devtools.sync.aggregate-status-policy]

- Working assumption: `sync` with no module name reports each failure and a summary, then returns OK.
- Alternative and implications: Return an error when any module fails, like `sync <module>`: a new Low defect.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-SYNC-3 · Should `sync` with no module name return an error status when any module fails? · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#devtools.sync.aggregate-status-policy)

```g5-question
schema: 1
id: devtools.sync.aggregate-status-policy
status: open
prompt: Should sync without a module name return an error when a module fails?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: "`sync` with no module name reports each failure and a summary, then returns OK."
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: "`sync` with no module name reports each failure and a summary, then returns OK."
```

<a id="dec-sync-1"></a>
<a id="devtools.sync.failed-import-policy"></a>
### Should sync retry a failed compile or reload after its source is reverted?

[Q · devtools.sync.failed-import-policy]

- Working assumption: Files that a failed `compile`, `load -dev` or `reload` put on the server are re-detected by the next `sync`.
- Alternative and implications: Retry marks stay sync-only and the docs say "after a failed compile, run compile again": F-SYNC-1 becomes documentation.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-SYNC-1 · Should a failed load or compile outside `sync` (`compile`, `load -dev`, `reload`) flag the files it put on the server, as a failed sync does? · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#devtools.sync.failed-import-policy)

```g5-question
schema: 1
id: devtools.sync.failed-import-policy
status: open
prompt: Should sync retry a failed compile or reload after its source is reverted?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Files that a failed `compile`, `load -dev` or `reload` put on the server are re-detected by the next `sync`.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Files that a failed `compile`, `load -dev` or `reload` put on the server are re-detected by the next `sync`.
```

<a id="c-q4"></a>
<a id="devtools.authoring.module-version.file"></a>
### Should `module-version` also update the `module.xml` file a module was loaded from?

[Q · devtools.authoring.module-version.file]

- Provisional answer: no, as built. The command edits the namespace's copy, and source control hooks, when present, take it from there.
- Alternative: yes for a module loaded in developer mode, whose folder is its working copy.

- C:Q4 · Should `module-version` also update the `module.xml` file the module was loaded from? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#devtools.authoring.module-version.file)

```g5-question
schema: 1
id: devtools.authoring.module-version.file
status: open
prompt: Should `module-version` also update the `module.xml` file a module was loaded from?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: no, as built. The command edits the namespace's copy, and source control hooks, when present, take it from there.
    description: Default assumption.
  - label: yes for a module loaded in developer mode, whose folder is its working copy
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: no, as built. The command edits the namespace's copy, and source control hooks, when present, take it from there.
```

<a id="devtools.generate.invalid-source"></a>
### Should `generate` report or refuse a source file that cannot load?

[Q · devtools.generate.invalid-source]

As built, it skips the file and can produce a manifest without it. The working assumption preserves that behavior; the alternative fails generation or reports each omission so an author can fix it (C:Q16).



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#devtools.generate.invalid-source)

```g5-question
schema: 1
id: devtools.generate.invalid-source
status: open
prompt: Should `generate` report or refuse a source file that cannot load?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Q16).
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Q16).
```

<a id="g-q-09"></a>
<a id="history.record.unlogged-refusals"></a>
### Should IPM record `update`, `reinstall` and `uninstall` attempts it refuses because of their arguments, as it records refused loads and installs?

[Q · history.record.unlogged-refusals]

- Provisional answer: record them. Examples are `update` or `reinstall` of a module that is not installed, and `uninstall` with no name.
- Reason: [recorded actions](../ontology/features/history.g5.md#history.actions) include refused attempts, and the wiki says IPM records every action "it attempts, whether or not the attempt succeeded". `install` with no name is recorded only when a program calls `##class(%IPM.Main).Install` directly. At the command line the parser refuses it before any record, as it does `update` and `reinstall` with no name (iteration74 native cases i4 and i4api).
- Alternative: argument refusals are not attempts, because IPM refuses them before the action starts.

- G:Q-09 · Q-09: Should argument refusals of `update`, `reinstall` and `uninstall` be recorded? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.record.unlogged-refusals)

```g5-question
schema: 1
id: history.record.unlogged-refusals
status: open
prompt: Should IPM record `update`, `reinstall` and `uninstall` attempts it refuses because of their arguments, as it records refused loads and installs?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: record them. Examples are `update` or `reinstall` of a module that is not installed, and `uninstall` with no name.
    description: Default assumption.
  - label: argument refusals are not attempts, because IPM refuses them before the action starts
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: record them. Examples are `update` or `reinstall` of a module that is not installed, and `uninstall` with no name.
```

<a id="g-q-08"></a>
<a id="history.record.abnormal-end"></a>
### What should `Committed` say on the record of an action that never ended?

[Q · history.record.abnormal-end]

- Provisional answer: `Committed` reflects the changes kept, even on an unfinished record. IPM writes it to the permanent record at each commit.
- Reason: `Committed` "should reflect whether the action's changes were actually retained", and the wiki's recipe `-Dcommitted=1 -DSuccess="0*"` is how users find failures that kept changes. A crash that kept changes is that case.
- Alternative: an empty end time means the outcome is unknown, so readers must not trust `Committed=0` on such a record.

- G:Q-08 · Q-08: What should `Committed` say on a record whose action never ended? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.record.abnormal-end)

```g5-question
schema: 1
id: history.record.abnormal-end
status: open
prompt: What should `Committed` say on the record of an action that never ended?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: "`Committed` reflects the changes kept, even on an unfinished record. IPM writes it to the permanent record at each commit."
    description: Default assumption.
  - label: an empty end time means the outcome is unknown, so readers must not trust `Committed=0` on such a record
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: "`Committed` reflects the changes kept, even on an unfinished record. IPM writes it to the permanent record at each commit."
```

<a id="g-q-15"></a>
<a id="history.fields.dependency-source"></a>
### How should the log label and show a dependency that IPM pulled from a repository while running a `load` or `update`, and what source does an `update` from a registry store?

[Q · history.fields.dependency-source]

- Provisional answer: keep the `load` label for dependencies of `load` and `update`, keep their stored source, and keep the rule that listings show a source only for installs.
- Reason: the maintainer test for dependency history expects a `load` record for the dependency of a `load`. The original request asks that dependency installs be logged, which this does.
- Alternative: label them `install` with their source, following the original request's wording "install (including as a dependency)". That changes the maintainer test.
- Alternative: keep the `load` label and show the stored source in listings for any record that has one. That changes the wiki's rule that listings show `n/a` for anything other than install.
- Alternative: clear the source of records labeled `load`, as the field's description says for loads. This was not chosen, because hiding a real source loses useful history.
- `update` is not covered by any source. Provisionally it may store its registry, and listings still show `n/a` for it.

- G:Q-15 · Q-15: How are repository-pulled dependencies labeled and shown? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.fields.dependency-source)

```g5-question
schema: 1
id: history.fields.dependency-source
status: open
prompt: How should the log label and show a dependency that IPM pulled from a repository while running a `load` or `update`, and what source does an `update` from a registry store?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: keep the `load` label for dependencies of `load` and `update`, keep their stored source, and keep the rule that listings show a source only for installs.
    description: Default assumption.
  - label: |-
      label them `install` with their source, following the original request's wording "install (including as a dependency)". That changes the maintainer test.
      - Alte
    description: |-
      label them `install` with their source, following the original request's wording "install (including as a dependency)". That changes the maintainer test.
      - Alternative: keep the `load` label and show the stored source in listings for any record that has one. That changes the wiki's rule that listings show `n/a` for anything other than install.
      - Alternative: clear the source of records labeled `load`, as the field's description says for loads. This was not chosen, because hiding a real source loses useful history.
      - `update` is not covered by any source. Provisionally it may store its registry, and listings still show `n/a` for it
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: keep the `load` label for dependencies of `load` and `update`, keep their stored source, and keep the rule that listings show a source only for installs.
```

<a id="g-q-14"></a>
<a id="history.phases.returned-errors"></a>
### Must every phase that starts also record its end and status, including when an `<Invoke>` returns an error status instead of throwing one?

[Q · history.phases.returned-errors]

- Provisional answer: yes. The docs read a phase with no end time as a hang or crash, so an ordinary error must not look like one.
- Alternative: accept open phases for returned errors and document it.

- G:Q-14 · Q-14: Must every started phase record its end and status? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.phases.returned-errors)

```g5-question
schema: 1
id: history.phases.returned-errors
status: open
prompt: Must every phase that starts also record its end and status, including when an `<Invoke>` returns an error status instead of throwing one?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: yes. The docs read a phase with no end time as a hang or crash, so an ordinary error must not look like one.
    description: Default assumption.
  - label: accept open phases for returned errors and document it
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: yes. The docs read a phase with no end time as a hang or crash, so an ordinary error must not look like one.
```

<a id="g-q-05"></a>
<a id="dec-hist-4"></a>
<a id="history.committed.nested"></a>
### When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?

[Q · history.committed.nested]

- Provisional answer: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit.
- Reason: no source says whether wrapping IPM in a caller's transaction is supported.
- Alternative: compare the transaction level at the end with the level at the start, so any commit nested in a caller transaction reads uncommitted.
- Alternative: treat caller-held transactions as unsupported, so the record may be lost with them.

- Settled observation: the command may return success before the caller later rolls back its module data and permanent history row. The imported model proves this no-trace result under its stated domain. Published caller-transaction use does not decide the independent audit retention policy; the working assumption and alternatives above remain policy proposals.

- G:Q-05 · Q-05: What happens to a record under a caller's own transaction? · open
- DEC-HIST-4 · What should IPM do when a command starts inside a caller transaction (F-HIST-3)? The load and its history then share the caller's fate. · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.committed.nested)

```g5-question
schema: 1
id: history.committed.nested
status: open
prompt: When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit.
    description: Default assumption.
  - label: |-
      compare the transaction level at the end with the level at the start, so any commit nested in a caller transaction reads uncommitted.
      - Alternative: treat calle
    description: |-
      compare the transaction level at the end with the level at the start, so any commit nested in a caller transaction reads uncommitted.
      - Alternative: treat caller-held transactions as unsupported, so the record may be lost with them
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit.
```

<a id="dec-load-1"></a>
<a id="history.committed.terminal-choice-policy"></a>
### What history outcome is required when an operator commits a stopped load at the terminal?

[Q · history.committed.terminal-choice-policy]

- Working assumption: Answering "Commit" at a crash prompt is an operator error to document.
- Alternative and implications: Mark the log committed before each region a halt could commit: F-HIST-4 becomes a defect again.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-LOAD-1 · A crash at an interactive terminal asks the operator to commit or roll back the open transaction. Committing keeps a half-done load that history marks Committed = 0 (F-HIST-4). Is that an operator error to document, or should IPM guard against it? · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.committed.terminal-choice-policy)

```g5-question
schema: 1
id: history.committed.terminal-choice-policy
status: open
prompt: What history outcome is required when an operator commits a stopped load at the terminal?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Answering "Commit" at a crash prompt is an operator error to document.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Answering "Commit" at a crash prompt is an operator error to document.
```

<a id="g-q-10"></a>
<a id="history.read.filter.invalid-column"></a>
### What should happen to a filter on a column such as `Version_Major`, on a column that does not exist, or when the query fails?

[Q · history.read.filter.invalid-column]

- Provisional answer: filters on listed columns, including version parts, must work, and IPM must never silently drop a filter. When `find` cannot run its query, it reports the error instead of showing an empty log.
- Reason: a dropped filter widens `history delete`, which could then delete a whole namespace's records without `-confirm`. An empty listing with a success status hides the problem.
- Alternative: refuse an unknown column with an error, or ignore it with a warning and apply the rest.

- G:Q-10 · Q-10: How must `history` treat unusable filter columns and query errors? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.read.filter.invalid-column)

```g5-question
schema: 1
id: history.read.filter.invalid-column
status: open
prompt: What should happen to a filter on a column such as `Version_Major`, on a column that does not exist, or when the query fails?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: filters on listed columns, including version parts, must work, and IPM must never silently drop a filter. When `find` cannot run its query, it reports the error instead of showing an empty log.
    description: Default assumption.
  - label: refuse an unknown column with an error, or ignore it with a warning and apply the rest
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: filters on listed columns, including version parts, must work, and IPM must never silently drop a filter. When `find` cannot run its query, it reports the error instead of showing an empty log.
```

<a id="g-q-18"></a>
<a id="history.read.filter.default-modifiers"></a>
### Should the namespace's [default modifiers](../ontology/operations/settings.g5.md#settings.default-modifiers) act as `history` filters?

[Q · history.read.filter.default-modifiers]

- Provisional answer: no. Default modifiers are meant for package changes, such as `-DNoTransaction=1`, while `history` filters are explicit requests.
- Reason: a default such as `NoTransaction` names no history column, so it would break or empty every listing.
- Alternative: apply them as filters and document it.

- G:Q-18 · Q-18: Do namespace default modifiers act as `history` filters? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.read.filter.default-modifiers)

```g5-question
schema: 1
id: history.read.filter.default-modifiers
status: open
prompt: Should the namespace's [default modifiers](../ontology/operations/settings.g5.md#settings.default-modifiers) act as `history` filters?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: no. Default modifiers are meant for package changes, such as `-DNoTransaction=1`, while `history` filters are explicit requests.
    description: Default assumption.
  - label: apply them as filters and document it
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: no. Default modifiers are meant for package changes, such as `-DNoTransaction=1`, while `history` filters are explicit requests.
```

<a id="g-q-11"></a>
<a id="history.delete.confirm-with-filter"></a>
### Does `-confirm` together with a filter delete every record, or only the matching ones?

[Q · history.delete.confirm-with-filter]

- Provisional answer: only the matching ones. `-confirm` only permits a delete without a filter.
- Reason: the wiki describes it this way, while the command help says `-confirm` "will delete all records in the current namespace".
- Alternative: follow the command help.

- G:Q-11 · Q-11: Does `-confirm` with a filter delete everything? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.delete.confirm-with-filter)

```g5-question
schema: 1
id: history.delete.confirm-with-filter
status: open
prompt: Does `-confirm` together with a filter delete every record, or only the matching ones?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: only the matching ones. `-confirm` only permits a delete without a filter.
    description: Default assumption.
  - label: follow the command help
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: only the matching ones. `-confirm` only permits a delete without a filter.
```

<a id="g-q-20"></a>
<a id="c-q7"></a>
<a id="dec-hist-6"></a>
<a id="history.delete.running-records"></a>
### May `history delete` or the daily [purge](#history.retention) remove the record of an action that is still running, and what should that action's final write do then?

[Q · history.delete.running-records]

- Provisional answer: a delete or purge may remove any matching record, and the running action must still finish normally whether or not its record comes back.
- Reason: no source covers this case. Deleting is an explicit user choice, and the log must not make a package change fail.
- Alternative: skip records of running actions, or always write the final result back.

- As built, deleting an accessible active twin before final persistence leaves the row absent even when Finalize and PersistToTwin return success.
- A transaction-locked twin has a different checked schedule: delete waits, final persistence runs, the transaction releases its lock, delete completes, then temporary-object close recreates the same ID with its finalized successful result. A successful delete is not cancellation of the remaining writer.
- The [paired final-reader receipts and Verus schedule contract](../uplift/consolidated/iter2/boundaries/REPORT.md) refute a general no-resurrection claim. They cover two schedules, not all lock timeouts, interleavings or cross-namespace behavior. The alternatives above remain an owner policy question.

- G:Q-20 · Q-20: May a delete or purge remove a running action's record? · open
- C:Q7 · May deleting an active history row be followed by restoration at close? · see audited disposition
- DEC-HIST-6 · What should happen when `history delete` removes the record of a command that is still running? By the IRIS contract (re-saving an object re-creates its row, O:env-resave-after-rollback) the closing %OnClose would write it back. Not observed on IRIS for IPM. · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.delete.running-records)

```g5-question
schema: 1
id: history.delete.running-records
status: open
prompt: May `history delete` or the daily [purge](#history.retention) remove the record of an action that is still running, and what should that action's final write do then?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: a delete or purge may remove any matching record, and the running action must still finish normally whether or not its record comes back.
    description: Default assumption.
  - label: skip records of running actions, or always write the final result back
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: a delete or purge may remove any matching record, and the running action must still finish normally whether or not its record comes back.
```

<a id="g-q-21"></a>
<a id="history.locked.unfinished"></a>
### May IPM finalize, and so lock, the record of an action that never ended?

[Q · history.locked.unfinished]

- Provisional answer: no. A finalized record is one whose action has ended, so a record without an end time stays unfinalized.
- Reason: a locked record of an unfinished action could never be corrected.
- Alternative: finalize whenever IPM releases the record, and accept locked records with no end time.

- G:Q-21 · Q-21: May IPM finalize the record of an action that never ended? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.locked.unfinished)

```g5-question
schema: 1
id: history.locked.unfinished
status: open
prompt: May IPM finalize, and so lock, the record of an action that never ended?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: no. A finalized record is one whose action has ended, so a record without an end time stays unfinalized.
    description: Default assumption.
  - label: finalize whenever IPM releases the record, and accept locked records with no end time
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: no. A finalized record is one whose action has ended, so a record without an end time stays unfinalized.
```

<a id="g-q-13"></a>
<a id="history.retention.effect"></a>
### May a failure of the daily purge stop the user's command?

[Q · history.retention.effect]

- Provisional answer: no. The command should still run, which is a proposed change.
- Reason: no source says a purge failure may block other commands, and the purge is housekeeping the user did not ask for in that command.
- Alternative: report the purge error and stop, as a signal that retention is broken.

- G:Q-13 · Q-13: May a failed purge stop the user's command? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.retention.effect)

```g5-question
schema: 1
id: history.retention.effect
status: open
prompt: May a failure of the daily purge stop the user's command?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: no. The command should still run, which is a proposed change.
    description: Default assumption.
  - label: report the purge error and stop, as a signal that retention is broken
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: no. The command should still run, which is a proposed change.
```

<a id="g-q-16"></a>
<a id="history.keep-on-ipm-uninstall.other-namespaces"></a>
### May uninstalling IPM in one namespace delete the in-progress records of other namespaces?

[Q · history.keep-on-ipm-uninstall.other-namespaces]

- Provisional answer: no. History belongs to each namespace.
- Alternative: accept it, because removing IPM is rare.

- G:Q-16 · Q-16: May uninstalling IPM delete other namespaces' in-progress records? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.keep-on-ipm-uninstall.other-namespaces)

```g5-question
schema: 1
id: history.keep-on-ipm-uninstall.other-namespaces
status: open
prompt: May uninstalling IPM in one namespace delete the in-progress records of other namespaces?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: no. History belongs to each namespace.
    description: Default assumption.
  - label: accept it, because removing IPM is rare
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: no. History belongs to each namespace.
```

<a id="g-q-26"></a>
<a id="history.committed.escaped-dependency"></a>
### Should a parent record count writes kept only by its NoJournal dependency?

[Q · history.committed.escaped-dependency]

- Evidence: The property says any action changes kept. N-R05-M5 leaves child data while top-level marker can remain false and the child row can be lost.
- Working assumption: Count surviving child writes in the parent outcome; retain the distinction between parent and child records.
- Alternative: Keep per-activation marker only and rely on independent child records after a repair.
- Affected claims: Committed truth, record existence and aggregate load outcome. Alias Q-26.

- G:Q-26 · Q-26: What `Committed` should a failed load's record show when only a `NoJournal` dependency's changes were kept? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.committed.escaped-dependency)

```g5-question
schema: 1
id: history.committed.escaped-dependency
status: open
prompt: Should a parent record count writes kept only by its NoJournal dependency?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Count surviving child writes in the parent outcome; retain the distinction between parent and child records.
    description: Default assumption.
  - label: |-
      Keep per-activation marker only and rely on independent child records after a repair.
      - Affected claims: Committed truth, record existence and aggregate load ou
    description: |-
      Keep per-activation marker only and rely on independent child records after a repair.
      - Affected claims: Committed truth, record existence and aggregate load outcome. Alias Q-26
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Count surviving child writes in the parent outcome; retain the distinction between parent and child records.
```

<a id="g-q-27"></a>
<a id="history.delete.global-errors"></a>
### Must global delete report namespace errors other than missing permission?

[Q · history.delete.global-errors]

- Evidence: DeleteHistoryGlobally catches all exceptions; a native unknown-column comparison distinguishes local failure from global success.
- Working assumption: Report errors that mean requested deletion was not performed.
- Alternative: Keep best-effort behavior and explicitly list skipped namespaces and errors.
- Affected claims: global delete status and namespace scope. Alias Q-27.

- G:Q-27 · Q-27: Should a global history delete report errors other than missing permission? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.delete.global-errors)

```g5-question
schema: 1
id: history.delete.global-errors
status: open
prompt: Must global delete report namespace errors other than missing permission?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Report errors that mean requested deletion was not performed.
    description: Default assumption.
  - label: |-
      Keep best-effort behavior and explicitly list skipped namespaces and errors.
      - Affected claims: global delete status and namespace scope. Alias Q-27
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Report errors that mean requested deletion was not performed.
```

<a id="g-q-28"></a>
<a id="history.read.global-shape"></a>
### Must programmatic global history rows carry their namespace and ID?

[Q · history.read.global-shape]

- Evidence: CLI output carries namespace; GlobalRecords ROWSPEC omits namespace and ID, and native H10 repeats caller rows.
- Working assumption: Return identities sufficient for a caller to distinguish colliding namespace-local IDs.
- Alternative: Keep the legacy row shape and document a separate namespace-aware API.
- Affected claims: global query scope, identity and details lookup. Alias Q-28.

- G:Q-28 · Q-28: Should the global history readers return each row's namespace and ID? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.read.global-shape)

```g5-question
schema: 1
id: history.read.global-shape
status: open
prompt: Must programmatic global history rows carry their namespace and ID?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Return identities sufficient for a caller to distinguish colliding namespace-local IDs.
    description: Default assumption.
  - label: |-
      Keep the legacy row shape and document a separate namespace-aware API.
      - Affected claims: global query scope, identity and details lookup. Alias Q-28
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Return identities sufficient for a caller to distinguish colliding namespace-local IDs.
```

<a id="g-q-29"></a>
<a id="history.retention.running-policy"></a>
### May automatic retention remove an action that has not ended?

[Q · history.retention.running-policy]

- Evidence: The as-built quoted cutoff deletes nothing. With that defect repaired the filter uses TimeStart only; the Python reader model can then remove active rows.
- Working assumption: Retain the documented age rule while keeping deletion/finalization races explicit; no repair outcome is claimed.
- Alternative: Skip active rows and define how killed actions eventually expire.
- Affected claims: retention fix, active writers and abnormal process ends. Alias Q-29.

- G:Q-29 · Q-29: May the retention purge remove records of actions that have not ended? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.retention.running-policy)

```g5-question
schema: 1
id: history.retention.running-policy
status: open
prompt: May automatic retention remove an action that has not ended?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Retain the documented age rule while keeping deletion/finalization races explicit; no repair outcome is claimed.
    description: Default assumption.
  - label: |-
      Skip active rows and define how killed actions eventually expire.
      - Affected claims: retention fix, active writers and abnormal process ends. Alias Q-29
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Retain the documented age rule while keeping deletion/finalization races explicit; no repair outcome is claimed.
```

<a id="g-q-30"></a>
<a id="history.survives-rollback.repair-policy"></a>
### Which repair should preserve child history without lying about Committed?

[Q · history.survives-rollback.repair-policy]

- Evidence: Naive durable rows can mark rolled-back writes committed. The inherited bounded variants distinguish guarded close, before-image ordering, per-activation NoJournal and SIGKILL residuals; mirroring is untested.
- Working assumption: Keep current survival requirement and report the repair as undecided; no product repair applied.
- Alternative: Use one load-wide mode plus guarded close and mark ordering, or extend the repair to durably mark escaped child writes.
- Affected claims: D1 and aliases, caller rollback, crash cuts and mirrored history. Alias Q-30.

- G:Q-30 · Q-30: How should D1 be repaired? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.survives-rollback.repair-policy)

```g5-question
schema: 1
id: history.survives-rollback.repair-policy
status: open
prompt: Which repair should preserve child history without lying about Committed?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Keep current survival requirement and report the repair as undecided; no product repair applied.
    description: Default assumption.
  - label: |-
      Use one load-wide mode plus guarded close and mark ordering, or extend the repair to durably mark escaped child writes.
      - Affected claims: D1 and aliases, calle
    description: |-
      Use one load-wide mode plus guarded close and mark ordering, or extend the repair to durably mark escaped child writes.
      - Affected claims: D1 and aliases, caller rollback, crash cuts and mirrored history. Alias Q-30
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Keep current survival requirement and report the repair as undecided; no product repair applied.
```

<a id="g-q-31"></a>
<a id="history.committed.marking.refusal-policy"></a>
### Should the no-transaction marker run only after every refusal?

[Q · history.committed.marking.refusal-policy]

- Evidence: LoadNewModule marks before manifest validation; a later refusal can keep no data yet read Committed=1 (G:D12). The property says noncommitted means nothing kept or rolled back.
- Working assumption: Keep the intent that a no-write refusal is uncommitted; classify the early marker as a defect.
- Alternative: Declare every no-transaction attempt committed from its start and change the owning I rule explicitly.
- Affected claims: validation timing, no-transaction history, D12 and requirement refusals. Alias Q-31.

- G:Q-31 · Q-31: May the no-transaction `Committed` marker move after all refusals? · open

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#history.committed.marking.refusal-policy)

```g5-question
schema: 1
id: history.committed.marking.refusal-policy
status: open
prompt: Should the no-transaction marker run only after every refusal?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: Keep the intent that a no-write refusal is uncommitted; classify the early marker as a defect.
    description: Default assumption.
  - label: |-
      Declare every no-transaction attempt committed from its start and change the owning I rule explicitly.
      - Affected claims: validation timing, no-transaction hist
    description: |-
      Declare every no-transaction attempt committed from its start and change the owning I rule explicitly.
      - Affected claims: validation timing, no-transaction history, D12 and requirement refusals. Alias Q-31
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: Keep the intent that a no-write refusal is uncommitted; classify the early marker as a defect.
```

<a id="g-q-19"></a>
<a id="dec-inst-4"></a>
<a id="reinstall.version"></a>
### Does `reinstall` install the version already installed, or the latest available version?

[Q · reinstall.version]

- Provisional answer: the version already installed, as the command summary and its examples say.
- Alternative: the latest version, as the command description and the wiki say.

- G:Q-19 · Q-19: Does `reinstall` install the installed version or the latest one? · open
- DEC-INST-4 · What does reinstall fetch? Summary (Main.cls L448) and code: the installed version. Description (L450): "from the latest version available in a configured repository". Observed on IRIS: with complex-c 2.0.0 installed and 3.0.0 in the registry, `reinstall complex-c` prints "Reinstalling complex-c 2.0.0" and keeps 2.0.0 (nc-inst-9-reinstall-version). · open; conflicting canonical question takes precedence over the resolved label

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#reinstall.version)

```g5-question
schema: 1
id: reinstall.version
status: open
prompt: Does `reinstall` install the version already installed, or the latest available version?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: the version already installed, as the command summary and its examples say.
    description: Default assumption.
  - label: the latest version, as the command description and the wiki say
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: the version already installed, as the command summary and its examples say.
```

<a id="g-q-02"></a>
<a id="dec-inst-1"></a>
<a id="load.refusal.update-required.dev-bypass"></a>
### Should `-dev` also lift the update-required refusal?

[Q · load.refusal.update-required.dev-bypass]

- Provisional answer: yes. A maintainer test expects a newer version to install with `-dev`, and the original request asked IPM to point users to `-dev` or `-force`.
- Reason for doubt: the README and the `-force` help name only `-force`.
- Alternative: only `-force` lifts it, as the README says.
- Related: a `-dev` install from a [read-only repository](../ontology/concepts/repositories.g5.md#repo.read-only) is checked before developer mode is turned off for it.

- G:Q-02 · Q-02: Does `-dev` lift the update-required refusal? · open
- DEC-INST-1 · Is the developer-mode bypass of the update-package guard (install -dev moves to a newer version without running or seeding update steps) part of the public contract? README L62 names only -force; test TestDevMode (InstallModule.cls L178) asserts the bypass; issue #550 mentions dev environments. Observed on IRIS: `install -dev update-simple 3.5.0` over 2.1.1 succeeds and leaves the V3 step with no record at all (nc-inst-2-dev-install-then-update). Review round: the bypass needs a typed -dev; a module merely stored in developer mode does not bypass it (after `load -dev` 1.0.0, a plain `load` of 2.0.0 with an update package is refused with the "use update" message: nc-inst-fix-stored-dev-load-newer), because the guard reads only the DeveloperMode parameter (Utils/Module.cls#L1219). Cell INST-C33 now names this decision. · open; conflicting canonical question takes precedence over the resolved label

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#load.refusal.update-required.dev-bypass)

```g5-question
schema: 1
id: load.refusal.update-required.dev-bypass
status: open
prompt: Should `-dev` also lift the update-required refusal?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: yes. A maintainer test expects a newer version to install with `-dev`, and the original request asked IPM to point users to `-dev` or `-force`.
    description: Default assumption.
  - label: |-
      only `-force` lifts it, as the README says.
      - Related: a `-dev` install from a [read-only repository](../ontology/concepts/repositories.g5.md#repo.read-only) is
    description: |-
      only `-force` lifts it, as the README says.
      - Related: a `-dev` install from a [read-only repository](../ontology/concepts/repositories.g5.md#repo.read-only) is checked before developer mode is turned off for it
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: yes. A maintainer test expects a newer version to install with `-dev`, and the original request asked IPM to point users to `-dev` or `-force`.
```

<a id="g-q-17"></a>
<a id="dec-nsp-5"></a>
<a id="load.refusal.requirements.timing"></a>
### Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?

[Q · load.refusal.requirements.timing]

- Provisional answer: yes. The manifest docs say requirements are checked "before module installation", and a module that fails them was never installable.
- Reason: keeping changes for debugging is meant for failures of a module that could be installed.
- Alternative: accept that a refused developer-mode or `-DNoTransaction=1` load keeps its saved manifest, since developer mode keeps what it did.

- G:Q-17 · Q-17: Must system requirements be checked before anything is written? · open
- DEC-NSP-5 · In developer mode, a load refused by a system need (e.g. `SYSNamespace`) leaves the module's manifest record committed. Is that intended? · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#load.refusal.requirements.timing)

```g5-question
schema: 1
id: load.refusal.requirements.timing
status: open
prompt: Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: yes. The manifest docs say requirements are checked "before module installation", and a module that fails them was never installable.
    description: Default assumption.
  - label: accept that a refused developer-mode or `-DNoTransaction=1` load keeps its saved manifest, since developer mode keeps what it did
    description: ""
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: yes. The manifest docs say requirements are checked "before module installation", and a module that fails them was never installable.
```

<a id="c-q1"></a>
<a id="load.dependencies.local-repo.precedence"></a>
### When another repository offers the same version of a dependency as the `.modules` folder, which copy should IPM install?

[Q · load.dependencies.local-repo.precedence]

- Provisional answer: the copy in the older repository, as built. The changelog says ties go to the repository created first.
- Alternative: the `.modules` copy, because the code that creates the temporary repository says it should be found first and gives it the first sort position.
- Either way, a higher version wins, wherever it is.

- C:Q1 · When another repository offers the same version of a dependency as a load's `.modules` folder, which copy should IPM install? · see audited disposition

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#load.dependencies.local-repo.precedence)

```g5-question
schema: 1
id: load.dependencies.local-repo.precedence
status: open
prompt: When another repository offers the same version of a dependency as the `.modules` folder, which copy should IPM install?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: the copy in the older repository, as built. The changelog says ties go to the repository created first.
    description: Default assumption.
  - label: |-
      the `.modules` copy, because the code that creates the temporary repository says it should be found first and gives it the first sort position.
      - Either way, a
    description: |-
      the `.modules` copy, because the code that creates the temporary repository says it should be found first and gives it the first sort position.
      - Either way, a higher version wins, wherever it is
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: the copy in the older repository, as built. The changelog says ties go to the repository created first.
```

<a id="dec-unin-3"></a>
<a id="uninstall.dependents.scoped-policy"></a>
### Should a phase-scoped dependency block uninstall?

[Q · uninstall.dependents.scoped-policy]

- Working assumption: A need declared only for a phase (test, verify) does not block uninstall.
- Alternative and implications: Count scoped needs: three readers would become defects.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-UNIN-3 · Should phase-scoped needs (for example a test-only dependency) block uninstall? Uninstall counts every module whose full graph contains M, in any phase (Utils/Module.cls#L615); `show` lists only unscoped direct dependents (ModuleSummary.cls#L37). The two read-backs disagree. · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#uninstall.dependents.scoped-policy)

```g5-question
schema: 1
id: uninstall.dependents.scoped-policy
status: open
prompt: Should a phase-scoped dependency block uninstall?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: A need declared only for a phase (test, verify) does not block uninstall.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: A need declared only for a phase (test, verify) does not block uninstall.
```

<a id="uninstall.class-delete-error.policy"></a>
### Should uninstall report an error and keep its metadata when a class cannot be deleted, or warn and remove the metadata as it does now?

[Q · uninstall.class-delete-error.policy]

- Working assumption: preserve and disclose the checked behavior pending an owner decision. Returning an error would make incomplete removal visible to scripts; retaining metadata would preserve a retry route. Earlier external effects may already have occurred.
- The source deliberately prints a warning, but no inspected help, test or change rationale settles the intended script status and metadata policy for this refusal. Register: C:Q21.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#uninstall.class-delete-error.policy)

```g5-question
schema: 1
id: uninstall.class-delete-error.policy
status: open
prompt: Should uninstall report an error and keep its metadata when a class cannot be deleted, or warn and remove the metadata as it does now?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: preserve and disclose the checked behavior pending an owner decision. Returning an error would make incomplete removal visible to scripts; retaining metadata would preserve a retry route. Earlier external effects may already have occurred.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: preserve and disclose the checked behavior pending an owner decision. Returning an error would make incomplete removal visible to scripts; retaining metadata would preserve a retry route. Earlier external effects may already have occurred.
```

<a id="update.steps.shared-ownership"></a>
### Should two modules be allowed to share an update-step class and method, or should the second registration fail?

[Q · update.steps.shared-ownership]

- The checked implementation moves the single record's home and retains its completion. One module's uninstall can therefore remove a shared completion record still relevant to another installed module.
- Working assumption: authors use distinct update packages. Alternatives are to refuse a conflicting home, key records by module, or retain explicit shared owners. Each changes registration and deletion behavior; concurrent writers need a separate check.
- Evidence: ordinary activation, both step stores and deletion readers in iteration 9. No owner decision is inferred. Register: C:Q19.



Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#update.steps.shared-ownership)

```g5-question
schema: 1
id: update.steps.shared-ownership
status: open
prompt: Should two modules be allowed to share an update-step class and method, or should the second registration fail?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: authors use distinct update packages. Alternatives are to refuse a conflicting home, key records by module, or retain explicit shared owners. Each changes registration and deletion behavior; concurrent writers need a separate check.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: authors use distinct update packages. Alternatives are to refuse a conflicting home, key records by module, or retain explicit shared owners. Each changes registration and deletion behavior; concurrent writers need a separate check.
```

<a id="dec-inst-2"></a>
<a id="update.steps.dev-seeding-policy"></a>
### Should a developer-mode install seed update-step records?

[Q · update.steps.dev-seeding-policy]

- Working assumption: A developer-mode install or load seeds update steps like any install; a later update runs only newer steps.
- Alternative and implications: Keep "a dev install leaves all steps due" (so `load -dev` + update exercises every step): F-INST-8 withdrawn.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-INST-2 · Should an install in developer mode seed update steps? Today it does not (Lifecycle/Module.cls L122), so a later update runs every step from V1, including steps older than the installed version. Observed on IRIS: a dev install creates no step records (nc-inst-2b-dev-fresh-then-update, 0 rows); the next update ran every V1 and V2 step (nc-inst-2b) and, after a dev install of 3.5.0 over 2.1.1, ran the new V3 step plus V1 steps whose records did not exist, rerunning a seeded V1 step after them (nc-inst-2-dev-install-then-update). That update also cleared developer mode. Proved in models/verus/update_steps.rs: an OK install in developer mode leaves the step records unchanged (inst_refines), so the next update schedules the steps of every class, older ones included, while a non-developer install leaves none due (inst_witness_dev_install_leaves_steps_due). Review round: seeding tests the module's stored DeveloperMode (Lifecycle/Module.cls#L122), so a plain `load` of a module stored in developer mode seeds nothing either (nc-inst-fix-stored-dev-plain-load: 0 step rows; modelled in update_steps.rs). Cell INST-C15 now names this decision. · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#update.steps.dev-seeding-policy)

```g5-question
schema: 1
id: update.steps.dev-seeding-policy
status: open
prompt: Should a developer-mode install seed update-step records?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: A developer-mode install or load seeds update steps like any install; a later update runs only newer steps.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: A developer-mode install or load seeds update steps like any install; a later update runs only newer steps.
```

<a id="dec-inst-8"></a>
<a id="update.steps.fresh-seeding-policy"></a>
### Should a dependency first installed by update run its old steps or seed them?

[Q · update.steps.fresh-seeding-policy]

- Working assumption: A module that `update` installs for the first time has its steps seeded, as on a fresh install.
- Alternative and implications: Run them all (if the new module adopts data an older parent created): F-INST-10 withdrawn.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-INST-8 · When an update brings in a module that was not installed before, should its update steps run (today, all of them) or be seeded like a fresh install? Observed on IRIS: `update update-deps-added 2.1.1` brought in update-simple 1.2.1 and ran its V1 step (start time set); a plain install of update-simple 1.2.1 seeds it instead (nc-inst-6-dep-added-by-update, nc-inst-6b-fresh-install-control). Proved: with no step records, an update that ends OK runs every step this server may run (inst_fresh_update_runs_all). · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#update.steps.fresh-seeding-policy)

```g5-question
schema: 1
id: update.steps.fresh-seeding-policy
status: open
prompt: Should a dependency first installed by update run its old steps or seed them?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: A module that `update` installs for the first time has its steps seeded, as on a fresh install.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: A module that `update` installs for the first time has its steps seeded, as on a fresh install.
```

<a id="dec-inst-6"></a>
<a id="update.steps.busy-policy"></a>
### Should an update whose step lock is busy fail or report success?

[Q · update.steps.busy-policy]

- Working assumption: If the update lock is busy, `update` fails with an error that says so; it never reports success.
- Alternative and implications: Wait longer, then fail: same contract. Only "keep success" is ruled out. If it were chosen, F-INST-1 (High) would become a documentation defect.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

- DEC-INST-6 · What should update do when the update lock is busy? Today it reports success and runs no step (finding F-INST-1). PR #887 put preventing concurrent updates out of scope. Observed on IRIS: with the lock held by another process, update returned OK after 3.3 s, printed nothing, moved the module to 2.1.1 and ran no step (nc-inst-3-update-lock-busy); a later update to the same version ran them (nc-inst-3b-update-after-lock-free). Proved: with the lock timing out, update ends OK with the new version and no step run (inst_witness_busy_lock_reports_success, F-INST-1). · open; working assumption

Disposition: open. [Source audit](../uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md#update.steps.busy-policy)

```g5-question
schema: 1
id: update.steps.busy-policy
status: open
prompt: Should an update whose step lock is busy fail or report success?
severity: medium
multi_select: false
allow_free_text: true
options:
  - label: If the update lock is busy, `update` fails with an error that says so; it never reports success.
    description: Default assumption.
answer:
  selected: []
  text: ""
  rationale: ""
  answered_by: null
  answered_at: null
default_assumption: If the update lock is busy, `update` fails with an error that says so; it never reports success.
```

<a id="g-q-06"></a>
### Q-06: What `Success` does a dependency record show after its parent's rollback?

G:Q-06 · settled meaning; inherited attributed source preserved

A dependency's `Success` records its own returned status, even if its parent's later rollback undoes its data. `Committed` describes kept database writes.

```g5-question
schema: 1
id: g-q-06
status: answered
prompt: "Q-06: What `Success` does a dependency record show after its parent's rollback?"
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: A dependency's `Success` records its own returned status, even if its parent's later rollback undoes its data. `Committed` describes kept database writes.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: .g5/uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md
```

<a id="g-q-07"></a>
### Q-07: Does `Committed` cover changes outside the database?

G:Q-07 · settled meaning; inherited attributed source preserved

This question is resolved for the field's meaning: `Committed` covers database changes. It makes no filesystem rollback promise.

```g5-question
schema: 1
id: g-q-07
status: answered
prompt: "Q-07: Does `Committed` cover changes outside the database?"
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "This question is resolved for the field's meaning: `Committed` covers database changes. It makes no filesystem rollback promise."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: .g5/uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md
```

<a id="g-q-12"></a>
### Q-12: Global unfiltered delete without `-confirm` (resolved)

G:Q-12 · settled meaning; inherited attributed source preserved

`history delete -globally` does not require `-confirm`. With no filter, it deletes every record on the instance without asking.

```g5-question
schema: 1
id: g-q-12
status: answered
prompt: "Q-12: Global unfiltered delete without `-confirm` (resolved)"
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "`history delete -globally` does not require `-confirm`. With no filter, it deletes every record on the instance without asking."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: .g5/uplift/consolidated/findings-audit-iter28/QUESTION-REVIEW.md
```

<a id="dec-sys-1"></a>
### The playbook says a term may be used if an ancestor or an earlier same-level statement defines it. Should a definition by an earlier sibling of an ancestor (lexical scope) count? The index script assumes yes, so SYS-D* definitions reach every module.

DEC-SYS-1 · method/verification observation; not an unresolved IPM policy decision

(A) lexical scope, as implemented; (B) strict reading: repeat or chain shared definitions under each module root.

Source: [DEC-SYS-1](../../playbook-uplift/decisions/00-02.md)

```g5-question
schema: 1
id: dec-sys-1
status: answered
prompt: The playbook says a term may be used if an ancestor or an earlier same-level statement defines it. Should a definition by an earlier sibling of an ancestor (lexical scope) count? The index script assumes yes, so SYS-D* definitions reach every module.
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(A) lexical scope, as implemented; (B) strict reading: repeat or chain shared definitions under each module root."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/00-02.md
```

<a id="dec-hist-1"></a>
### When a parent load rolls back, should the history records of the dependency loads it ran survive? Today they vanish with the rollback (F-HIST-1), although the class comment says history is kept apart from rollbacks.

DEC-HIST-1 · inherited source-supported interpretation; not new owner approval

(A) keep them: write dependency records outside the parent's transaction (for example from IRISTEMP after the parent's outcome is known); (B) accept: a dependency record exists only when the dependency's changes were kept, and say so in the docs. Recommendation: A, because the top-level record already follows A. Proved design (models/verus/load_general.rs, `fix`): re-persist pending dependency rows after the parent's rollback and at the next command after a crash, with Committed taken from the dependency's installed state (keeping the logged Committed was shown dishonest by the explorer). Sub-question: a dependency that loaded and was then undone by the parent then reads Success = 1, Committed = 0; is that the wanted reading, or should Success also be cleared?

Source: [DEC-HIST-1](../../playbook-uplift/decisions/00-02.md)

```g5-question
schema: 1
id: dec-hist-1
status: answered
prompt: When a parent load rolls back, should the history records of the dependency loads it ran survive? Today they vanish with the rollback (F-HIST-1), although the class comment says history is kept apart from rollbacks.
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(A) keep them: write dependency records outside the parent's transaction (for example from IRISTEMP after the parent's outcome is known); (B) accept: a dependency record exists only when the dependency's changes were kept, and say so in the docs. Recommendation: A, because the top-level record already follows A. Proved design (models/verus/load_general.rs, `fix`): re-persist pending dependency rows after the parent's rollback and at the next command after a crash, with Committed taken from the dependency's installed state (keeping the logged Committed was shown dishonest by the explorer). Sub-question: a dependency that loaded and was then undone by the parent then reads Success = 1, Committed = 0; is that the wanted reading, or should Success also be cleared?"
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/00-02.md
```

<a id="dec-hist-2"></a>
### How should the Committed flag survive a crash after a developer-mode load has committed its manifest (F-HIST-2)?

DEC-HIST-2 · inherited source-supported interpretation; not new owner approval

(A) persist the twin row right after the first TCOMMIT (Utils/Module.cls#L1311-L1313, before the next TSTART) so the durable row says Committed = 1; (B) keep the IRISTEMP record when %OnClose runs at $TLEVEL > 0 and reconcile it at the next command; (C) accept and document. Recommendation: A, a two-line change at a point with no transaction open.

Source: [DEC-HIST-2](../../playbook-uplift/decisions/00-02.md)

```g5-question
schema: 1
id: dec-hist-2
status: answered
prompt: How should the Committed flag survive a crash after a developer-mode load has committed its manifest (F-HIST-2)?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(A) persist the twin row right after the first TCOMMIT (Utils/Module.cls#L1311-L1313, before the next TSTART) so the durable row says Committed = 1; (B) keep the IRISTEMP record when %OnClose runs at $TLEVEL > 0 and reconcile it at the next command; (C) accept and document. Recommendation: A, a two-line change at a point with no transaction open."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/00-02.md
```

<a id="dec-hist-3"></a>
### Must a record name the module whenever IPM has read its manifest (refusals, crashes before SetName)? Today such records have an empty package and version 0.0.0 (F-HIST-5). The property comment only promises a name is "not necessarily required, e.g. when loading a nonexistent directory".

DEC-HIST-3 · inherited source-supported interpretation; not new owner approval

(A) yes: set the name from the manifest read in LoadNewModule before the guards; (B) no: the name is best effort.

Source: [DEC-HIST-3](../../playbook-uplift/decisions/00-02.md)

```g5-question
schema: 1
id: dec-hist-3
status: answered
prompt: Must a record name the module whenever IPM has read its manifest (refusals, crashes before SetName)? Today such records have an empty package and version 0.0.0 (F-HIST-5). The property comment only promises a name is "not necessarily required, e.g. when loading a nonexistent directory".
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(A) yes: set the name from the manifest read in LoadNewModule before the guards; (B) no: the name is best effort."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/00-02.md
```

<a id="dec-hist-5"></a>
### Does Committed describe only database changes? File-system effects are never rolled back: an archive load deletes and recreates `<ModuleRoot>/<name>/<version>/` before any check (Main.cls#L2507-L2513), and FileCopy writes files.

DEC-HIST-5 · inherited source-supported interpretation; not new owner approval

(A) Committed covers database writes only; document it; (B) Committed should also turn on when files change.

Source: [DEC-HIST-5](../../playbook-uplift/decisions/00-02.md)

```g5-question
schema: 1
id: dec-hist-5
status: answered
prompt: "Does Committed describe only database changes? File-system effects are never rolled back: an archive load deletes and recreates `<ModuleRoot>/<name>/<version>/` before any check (Main.cls#L2507-L2513), and FileCopy writes files."
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: (A) Committed covers database writes only; document it; (B) Committed should also turn on when files change.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/00-02.md
```

<a id="dec-sem-1"></a>
### `^IRIS.Temp.IPM.LoadedResourceD` (IPM working data in IRISTEMP) is still set after commands end (O:ws-residue-after-commands). Is it a deliberate per-instance cache, or should each command clear it?

DEC-SEM-1 · method/verification observation; not an unresolved IPM policy decision

(A) deliberate cache: document its lifetime (until IRIS restarts); (B) clear it at the end of each command.

Source: [DEC-SEM-1](../../playbook-uplift/decisions/00-02.md)

```g5-question
schema: 1
id: dec-sem-1
status: answered
prompt: "`^IRIS.Temp.IPM.LoadedResourceD` (IPM working data in IRISTEMP) is still set after commands end (O:ws-residue-after-commands). Is it a deliberate per-instance cache, or should each command clear it?"
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(A) deliberate cache: document its lifetime (until IRIS restarts); (B) clear it at the end of each command."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/00-02.md
```

<a id="dec-sem-2"></a>
### Uninstalling a module that has a web application writes compiled-code globals in IRISSYS (`^oddDEF`, `^oddCOM`, `^rOBJ`; O:ws-uninstall-webapp). Is a system-database code change an accepted side effect of removing the web application, or a frame violation to investigate?

DEC-SEM-2 · method/verification observation; not an unresolved IPM policy decision

(A) accept: IRIS regenerates system class metadata when a security application is deleted; document it; (B) investigate which classes change and why.

Source: [DEC-SEM-2](../../playbook-uplift/decisions/00-02.md)

```g5-question
schema: 1
id: dec-sem-2
status: answered
prompt: Uninstalling a module that has a web application writes compiled-code globals in IRISSYS (`^oddDEF`, `^oddCOM`, `^rOBJ`; O:ws-uninstall-webapp). Is a system-database code change an accepted side effect of removing the web application, or a frame violation to investigate?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(A) accept: IRIS regenerates system class metadata when a security application is deleted; document it; (B) investigate which classes change and why."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/00-02.md
```

<a id="dec-inst-5"></a>
### What delivery promise do update steps have? Rollback undoes step records, so steps that worked in a failed run rerun; effects outside the database (files, $INCREMENT, other systems) can repeat; a kill between the step and its record save reruns that step. Observed on IRIS (nc-inst-4-nondev-failed-update, nc-inst-4b-nondev-retry): after a failed non-dev update the ordinary global set by the step that worked is rolled back, but its $INCREMENT counter and file line stay; the retry runs that step again (counter 2, two file lines). In developer mode (nc-inst-4c, nc-inst-4d) the worked step's record is kept and the retry skips it. Proved: a transactional update whose second step fails rolls the records back while the first step's effects outside the database stay, and the retry schedules that step again (inst_witness_failed_update_repeats_effects). Review round: INST-B67 now states the intent the seed asks about (a step's effects happen once) with this decision open and `O!:nc-inst-4b-nondev-retry`; the witness is cited in Notes, not as P.

DEC-INST-5 · inherited source-supported interpretation; not new owner approval

A) At-least-once; require idempotent steps and document it in VersionBase. B) Exactly-once for database effects only (document the boundary). C) Add a stronger mechanism (per-step commit or idempotency key).

Source: [DEC-INST-5](../../playbook-uplift/decisions/03-04.md)

```g5-question
schema: 1
id: dec-inst-5
status: answered
prompt: "What delivery promise do update steps have? Rollback undoes step records, so steps that worked in a failed run rerun; effects outside the database (files, $INCREMENT, other systems) can repeat; a kill between the step and its record save reruns that step. Observed on IRIS (nc-inst-4-nondev-failed-update, nc-inst-4b-nondev-retry): after a failed non-dev update the ordinary global set by the step that worked is rolled back, but its $INCREMENT counter and file line stay; the retry runs that step again (counter 2, two file lines). In developer mode (nc-inst-4c, nc-inst-4d) the worked step's record is kept and the retry skips it. Proved: a transactional update whose second step fails rolls the records back while the first step's effects outside the database stay, and the retry schedules that step again (inst_witness_failed_update_repeats_effects). Review round: INST-B67 now states the intent the seed asks about (a step's effects happen once) with this decision open and `O!:nc-inst-4b-nondev-retry`; the witness is cited in Notes, not as P."
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: A) At-least-once; require idempotent steps and document it in VersionBase. B) Exactly-once for database effects only (document the boundary). C) Add a stronger mechanism (per-step commit or idempotency key).
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/03-04.md
```

<a id="dec-inst-7"></a>
### Are primary-only update steps promised to run when a mirror member becomes primary at this revision? VersionBase.cls L18 says so; no code does it; PR #887 lists mirrored updates as out of scope and the wiki update guide (not read here) is reported by the seed to call mirroring future work. Native check #20 not run: the IRIS container is not a mirror member (nc-inst-20-no-mirror).

DEC-INST-7 · inherited source-supported interpretation; not new owner approval

A) Not promised at this revision; fix the VersionBase comment. B) Promised; implement a become-primary hook and mirror tests.

Source: [DEC-INST-7](../../playbook-uplift/decisions/03-04.md)

```g5-question
schema: 1
id: dec-inst-7
status: answered
prompt: "Are primary-only update steps promised to run when a mirror member becomes primary at this revision? VersionBase.cls L18 says so; no code does it; PR #887 lists mirrored updates as out of scope and the wiki update guide (not read here) is reported by the seed to call mirroring future work. Native check #20 not run: the IRIS container is not a mirror member (nc-inst-20-no-mirror)."
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: A) Not promised at this revision; fix the VersionBase comment. B) Promised; implement a become-primary hook and mirror tests.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/03-04.md
```

<a id="dec-dep-1"></a>
### Must dependency resolution find a solution whenever one exists? The resolver is greedy (newest first, one-level check, no backtracking past one level), so it can fail on solvable inputs; the method comment says "with backtracking". Observed on IRIS: `install nc-dep-root` (A ^1, B ^1; A 1.1.0 needs C ^2, A 1.0.0 and B need C ^1) fails with a clash on nc-dep-c naming nc-dep-a 1.1.0; installing nc-dep-a 1.0.0 first makes the same install succeed (nc-dep-11-greedy-resolution, nc-dep-11b-solvable-with-pin). Proved in models/verus/dep_resolve.rs: on the nc-dep-11 instance a sound choice exists and the (deterministic) resolver fails (dep_witness_greedy_rejects_solvable, dep_deterministic). Explored (models/explorer/inst/check.py, dep-explore-completeness): 2,040 of 70,656 small instances are solvable but fail; some fail because the quick check ignores the installed-version floor. A resolution that ends Done is always sound (dep_refines; 0 of 59,776 explored). Review round: DEP-B13 now states the intent ("if some choice of versions meets every requirement, IPM finds one") with this decision open and `O!:nc-dep-11-greedy-resolution`; the witness is cited in Notes, not as P. A failure is now proved to name a reason (a loop of recorded requirements, or a requirement with no version allowed by it and the requirements already recorded; DEP-B54); those recorded requirements include earlier greedy choices, which is why option B is not met.

DEC-DEP-1 · inherited source-supported interpretation; not new owner approval

A) Greedy is the contract; fix the comment and document that pins may be needed. B) Complete resolution is the contract; add real backtracking.

Source: [DEC-DEP-1](../../playbook-uplift/decisions/03-04.md)

```g5-question
schema: 1
id: dec-dep-1
status: answered
prompt: 'Must dependency resolution find a solution whenever one exists? The resolver is greedy (newest first, one-level check, no backtracking past one level), so it can fail on solvable inputs; the method comment says "with backtracking". Observed on IRIS: `install nc-dep-root` (A ^1, B ^1; A 1.1.0 needs C ^2, A 1.0.0 and B need C ^1) fails with a clash on nc-dep-c naming nc-dep-a 1.1.0; installing nc-dep-a 1.0.0 first makes the same install succeed (nc-dep-11-greedy-resolution, nc-dep-11b-solvable-with-pin). Proved in models/verus/dep_resolve.rs: on the nc-dep-11 instance a sound choice exists and the (deterministic) resolver fails (dep_witness_greedy_rejects_solvable, dep_deterministic). Explored (models/explorer/inst/check.py, dep-explore-completeness): 2,040 of 70,656 small instances are solvable but fail; some fail because the quick check ignores the installed-version floor. A resolution that ends Done is always sound (dep_refines; 0 of 59,776 explored). Review round: DEP-B13 now states the intent ("if some choice of versions meets every requirement, IPM finds one") with this decision open and `O!:nc-dep-11-greedy-resolution`; the witness is cited in Notes, not as P. A failure is now proved to name a reason (a loop of recorded requirements, or a requirement with no version allowed by it and the requirements already recorded; DEP-B54); those recorded requirements include earlier greedy choices, which is why option B is not met.'
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: A) Greedy is the contract; fix the comment and document that pins may be needed. B) Complete resolution is the contract; add real backtracking.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/03-04.md
```

<a id="dec-dep-2"></a>
### Should a failed install leave a lock file behind? The file is written after dependencies load but before the module's own Activate, outside the transaction. Observed on IRIS: `install -create-lockfile nc-dep-lf-fail` whose Activate fails returns an error, rolls back both modules, and leaves module-lock.json (listing nc-dep-lf-base) in the module folder (nc-dep-15-lockfile-failed-install).

DEC-DEP-2 · inherited source-supported interpretation; not new owner approval

A) Write it only after the whole install succeeds. B) Remove it on failure. C) Keep as is and document.

Source: [DEC-DEP-2](../../playbook-uplift/decisions/03-04.md)

```g5-question
schema: 1
id: dec-dep-2
status: answered
prompt: "Should a failed install leave a lock file behind? The file is written after dependencies load but before the module's own Activate, outside the transaction. Observed on IRIS: `install -create-lockfile nc-dep-lf-fail` whose Activate fails returns an error, rolls back both modules, and leaves module-lock.json (listing nc-dep-lf-base) in the module folder (nc-dep-15-lockfile-failed-install)."
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: A) Write it only after the whole install succeeds. B) Remove it on failure. C) Keep as is and document.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/03-04.md
```

<a id="dec-unin-1"></a>
### When `uninstall M` is blocked because a dependent is installed (no `-force`), should the command report failure? Help (Main.cls#L486-L487) says uninstall "will be prevented"; the code comment (Lifecycle/Base.cls#L276) says "return an error status"; the code returns OK, prints `Clean SUCCESS` and logs a successful uninstall; PublishModuleWithDeps.cls#L25-L26 asserts that success. See F-UNIN-6.

DEC-UNIN-1 · inherited source-supported interpretation; not new owner approval

(a) Fail with an error that names the dependents, and log the uninstall as failed. (b) Keep success, but log it as skipped. (c) Keep current behaviour and document it.

Source: [DEC-UNIN-1](../../playbook-uplift/decisions/05-06.md)

```g5-question
schema: 1
id: dec-unin-1
status: answered
prompt: When `uninstall M` is blocked because a dependent is installed (no `-force`), should the command report failure? Help (Main.cls#L486-L487) says uninstall "will be prevented"; the code comment (Lifecycle/Base.cls#L276) says "return an error status"; the code returns OK, prints `Clean SUCCESS` and logs a successful uninstall; PublishModuleWithDeps.cls#L25-L26 asserts that success. See F-UNIN-6.
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: (a) Fail with an error that names the dependents, and log the uninstall as failed. (b) Keep success, but log it as skipped. (c) Keep current behaviour and document it.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/05-06.md
```

<a id="dec-unin-2"></a>
### What should the Clean phase do when run on IPM itself (`zpm clean`)? Code runs the IPM-only block at every level and prints "WARNING: N module(s) orphaned" while removing nothing (Lifecycle/Module.cls#L27-L81); ExecutePhases also treats it as a self-uninstall (Storage/Module.cls#L631).

DEC-UNIN-2 · inherited source-supported interpretation; not new owner approval

(a) Refuse Clean on IPM. (b) Run it, but only warn when IPM is really removed (Clean.Level>0). (c) Treat it as uninstall of IPM.

Source: [DEC-UNIN-2](../../playbook-uplift/decisions/05-06.md)

```g5-question
schema: 1
id: dec-unin-2
status: answered
prompt: 'What should the Clean phase do when run on IPM itself (`zpm clean`)? Code runs the IPM-only block at every level and prints "WARNING: N module(s) orphaned" while removing nothing (Lifecycle/Module.cls#L27-L81); ExecutePhases also treats it as a self-uninstall (Storage/Module.cls#L631).'
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: (a) Refuse Clean on IPM. (b) Run it, but only warn when IPM is really removed (Clean.Level>0). (c) Treat it as uninstall of IPM.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/05-06.md
```

<a id="dec-repo-1"></a>
### When `publish` (or `unpublish` without a prefix) names no registry and no registry holds the publish flag, which registry is used? Help (Main.cls#L56-L57): "saves that artifact to the repository for which deployment is enabled". Code (Remote/Definition.cls#L117-L129, Oras/Definition.cls#L143-L155) falls back to a registry with a token, then one with user and password, then any enabled remote or ORAS registry, which can be the public community registry. See F-REPO-3.

DEC-REPO-1 · inherited source-supported interpretation; not new owner approval

(a) Fail when no registry has the publish flag. (b) Keep the fallback order and document it. (c) Fall back only to registries with credentials.

Source: [DEC-REPO-1](../../playbook-uplift/decisions/05-06.md)

```g5-question
schema: 1
id: dec-repo-1
status: answered
prompt: 'When `publish` (or `unpublish` without a prefix) names no registry and no registry holds the publish flag, which registry is used? Help (Main.cls#L56-L57): "saves that artifact to the repository for which deployment is enabled". Code (Remote/Definition.cls#L117-L129, Oras/Definition.cls#L143-L155) falls back to a registry with a token, then one with user and password, then any enabled remote or ORAS registry, which can be the public community registry. See F-REPO-3.'
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: (a) Fail when no registry has the publish flag. (b) Keep the fallback order and document it. (c) Fall back only to registries with credentials.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/05-06.md
```

<a id="dec-repo-2"></a>
### What may IPM still do with a registry that is not enabled? Help: "-disable: specifies that it can not be used." Install lookups (Manager.cls#L69) and unpublish (PublishManager classes) refuse it; `search` lists it (Main.cls#L2047, Utils/Module.cls#L916) and `publish -r R` sends to it (Lifecycle/Base.cls#L1598). See F-REPO-7.

DEC-REPO-2 · inherited source-supported interpretation; not new owner approval

(a) Refuse it everywhere. (b) Allow read-only listing (search) but refuse publish. (c) Allow anything that names it explicitly.

Source: [DEC-REPO-2](../../playbook-uplift/decisions/05-06.md)

```g5-question
schema: 1
id: dec-repo-2
status: answered
prompt: 'What may IPM still do with a registry that is not enabled? Help: "-disable: specifies that it can not be used." Install lookups (Manager.cls#L69) and unpublish (PublishManager classes) refuse it; `search` lists it (Main.cls#L2047, Utils/Module.cls#L916) and `publish -r R` sends to it (Lifecycle/Base.cls#L1598). See F-REPO-7.'
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: (a) Refuse it everywhere. (b) Allow read-only listing (search) but refuse publish. (c) Allow anything that names it explicitly.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/05-06.md
```

<a id="dec-repo-4"></a>
### Should `config set` or `config get` with an unknown key, or `config set` with an invalid value (for example a bad TestReportFormat), make the command fail? Code prints "Config key ... not found" or the error text and returns OK (Main.cls#L2063-L2093 ignores the status of `UpdateOne`). Changelog #1115 treated "printing the error and reporting success" as a bug for unpublish. See F-REPO-8.

DEC-REPO-4 · inherited source-supported interpretation; not new owner approval

(a) Return an error status. (b) Keep printing only.

Source: [DEC-REPO-4](../../playbook-uplift/decisions/05-06.md)

```g5-question
schema: 1
id: dec-repo-4
status: answered
prompt: 'Should `config set` or `config get` with an unknown key, or `config set` with an invalid value (for example a bad TestReportFormat), make the command fail? Code prints "Config key ... not found" or the error text and returns OK (Main.cls#L2063-L2093 ignores the status of `UpdateOne`). Changelog #1115 treated "printing the error and reporting success" as a bug for unpublish. See F-REPO-8.'
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: (a) Return an error status. (b) Keep printing only.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/05-06.md
```

<a id="dec-repo-6"></a>
### Is storing registry passwords and tokens as plain text in the namespace (`^IPM.Repo.DefinitionD`, Http/Definition.cls#L17-L21) acceptable? `-password-env` and `-token-env` (changelog #1178) protect only the command line; the value is still stored.

DEC-REPO-6 · inherited source-supported interpretation; not new owner approval

(a) Accept and document. (b) Store a reference to the environment variable instead of the value. (c) Encrypt at rest or use IRIS credential storage.

Source: [DEC-REPO-6](../../playbook-uplift/decisions/05-06.md)

```g5-question
schema: 1
id: dec-repo-6
status: answered
prompt: "Is storing registry passwords and tokens as plain text in the namespace (`^IPM.Repo.DefinitionD`, Http/Definition.cls#L17-L21) acceptable? `-password-env` and `-token-env` (changelog #1178) protect only the command line; the value is still stored."
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: (a) Accept and document. (b) Store a reference to the environment variable instead of the value. (c) Encrypt at rest or use IRIS credential storage.
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/05-06.md
```

<a id="dec-nsp-1"></a>
### A namespace gets IPM code and repo settings by mapping, then gets its own IPM (installer or `load`). What should happen to the repo settings mapping?

DEC-NSP-1 · inherited source-supported interpretation; not new owner approval

(a) Remove it with the code mapping, since README#L77 says repos are only mapped together with code from the same namespace. (b) Keep it (current code: installer touches only `%IPM` package/routine mapping, Installer.cls#L25-29). Note that afterwards `unmap -ns X` reports "No mapping found" (Main.cls#L3968) so the repo mapping can no longer be removed by `unmap`. Native (N7): after the installer ran in NCD (code and repos mapped from USER), NCD kept its repo mapping and also its code mapping, and `unmap -ns NCD` then said "No mapping found for NCD" and removed nothing (O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-N7-unmap-after-local-install; F-NSP-5).

Source: [DEC-NSP-1](../../playbook-uplift/decisions/07-09.md)

```g5-question
schema: 1
id: dec-nsp-1
status: answered
prompt: A namespace gets IPM code and repo settings by mapping, then gets its own IPM (installer or `load`). What should happen to the repo settings mapping?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: '(a) Remove it with the code mapping, since README#L77 says repos are only mapped together with code from the same namespace. (b) Keep it (current code: installer touches only `%IPM` package/routine mapping, Installer.cls#L25-29). Note that afterwards `unmap -ns X` reports "No mapping found" (Main.cls#L3968) so the repo mapping can no longer be removed by `unmap`. Native (N7): after the installer ran in NCD (code and repos mapped from USER), NCD kept its repo mapping and also its code mapping, and `unmap -ns NCD` then said "No mapping found for NCD" and removed nothing (O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-N7-unmap-after-local-install; F-NSP-5).'
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/07-09.md
```

<a id="dec-nsp-2"></a>
### May IPM be installed locally in a namespace that currently gets IPM by mapping?

DEC-NSP-2 · inherited source-supported interpretation; not new owner approval

(a) Yes: help for `enable -map -globally` says "Namespace-specific installation will override this" (Main.cls#L743) and the installer / `load` have no guard. (b) No: `install`, `reinstall` and `uninstall` of `zpm` refuse with "Cannot install 'zpm' in namespace X because it is mapped from namespace Y" (Main.cls#L2534-2539). The two paths disagree. Native (N7): `install zpm` in NCD was refused with the mapped-from error; the installer run returned OK but did not give NCD its own code: the mappings to USER stayed, so the install compiles through them into USER's database (re-point probe), while NCD gained a `zpm` record (O:nc-nsp-N7-install-zpm-mapped, O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-installer-repoint; F-NSP-5).

Source: [DEC-NSP-2](../../playbook-uplift/decisions/07-09.md)

```g5-question
schema: 1
id: dec-nsp-2
status: answered
prompt: May IPM be installed locally in a namespace that currently gets IPM by mapping?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(a) Yes: help for `enable -map -globally` says \"Namespace-specific installation will override this\" (Main.cls#L743) and the installer / `load` have no guard. (b) No: `install`, `reinstall` and `uninstall` of `zpm` refuse with \"Cannot install 'zpm' in namespace X because it is mapped from namespace Y\" (Main.cls#L2534-2539). The two paths disagree. Native (N7): `install zpm` in NCD was refused with the mapped-from error; the installer run returned OK but did not give NCD its own code: the mappings to USER stayed, so the install compiles through them into USER's database (re-point probe), while NCD gained a `zpm` record (O:nc-nsp-N7-install-zpm-mapped, O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-installer-repoint; F-NSP-5)."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/07-09.md
```

<a id="dec-nsp-3"></a>
### When `enable -map` (or `-community`) reaches a namespace that still holds legacy ZPM module records, should those records be migrated?

DEC-NSP-3 · inherited source-supported interpretation; not new owner approval

(a) Yes, run the same `MigrateZPMToIPM` that an IPM load runs (Migration.cls#L6-35). (b) No (current code: EnableIPM never calls RunAll, Main.cls#L3611-3708); the operator must re-load IPM. No doc covers this case. Native: not run (no legacy ZPM in these containers). Side fact: `MigrateRepoDefinitions` returns OK when there is nothing to copy (O:nc-nsp-migrate-repo-defs-success).

Source: [DEC-NSP-3](../../playbook-uplift/decisions/07-09.md)

```g5-question
schema: 1
id: dec-nsp-3
status: answered
prompt: When `enable -map` (or `-community`) reaches a namespace that still holds legacy ZPM module records, should those records be migrated?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(a) Yes, run the same `MigrateZPMToIPM` that an IPM load runs (Migration.cls#L6-35). (b) No (current code: EnableIPM never calls RunAll, Main.cls#L3611-3708); the operator must re-load IPM. No doc covers this case. Native: not run (no legacy ZPM in these containers). Side fact: `MigrateRepoDefinitions` returns OK when there is nothing to copy (O:nc-nsp-migrate-repo-defs-success)."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/07-09.md
```

<a id="dec-res-1"></a>
### Should a namespace switch done inside `exec` persist after the command?

DEC-RES-1 · inherited source-supported interpretation; not new owner approval

(a) No, isolate (PR #996, changelog 0.10.6 at CHANGELOG.md#L142, code `new $namespace` Main.cls#L2435); then the help example "set the namespace to MYAPP" (Main.cls#L515-516) is stale. (b) Yes, as the help example shows. Native (N16): the switch holds inside the command (INSIDE:%SYS) and ends with it; the caller stays in USER (O:nc-res-N16-exec-namespace-v2).

Source: [DEC-RES-1](../../playbook-uplift/decisions/07-09.md)

```g5-question
schema: 1
id: dec-res-1
status: answered
prompt: Should a namespace switch done inside `exec` persist after the command?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: '(a) No, isolate (PR #996, changelog 0.10.6 at CHANGELOG.md#L142, code `new $namespace` Main.cls#L2435); then the help example "set the namespace to MYAPP" (Main.cls#L515-516) is stale. (b) Yes, as the help example shows. Native (N16): the switch holds inside the command (INSIDE:%SYS) and ends with it; the caller stays in USER (O:nc-res-N16-exec-namespace-v2).'
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/07-09.md
```

<a id="dec-res-2"></a>
### What is the JSON format of a `run-from-file` command file?

DEC-RES-2 · inherited source-supported interpretation; not new owner approval

(a) A root object with a `commands` array (help example Main.cls#L364-404, code Main.cls#L2003-2009). (b) A single root array, as the help description says (Main.cls#L352). The code rejects (b). Native (N17): a root array is refused ("Expected JSON object at root level"); a `commands` object runs (O:nc-res-N17-run-from-file-v2).

Source: [DEC-RES-2](../../playbook-uplift/decisions/07-09.md)

```g5-question
schema: 1
id: dec-res-2
status: answered
prompt: What is the JSON format of a `run-from-file` command file?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: '(a) A root object with a `commands` array (help example Main.cls#L364-404, code Main.cls#L2003-2009). (b) A single root array, as the help description says (Main.cls#L352). The code rejects (b). Native (N17): a root array is refused ("Expected JSON object at root level"); a `commands` object runs (O:nc-res-N17-run-from-file-v2).'
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/07-09.md
```

<a id="dec-res-3"></a>
### Packaging a module that has `Deploy="true"` classes runs MakeDeployed in the current namespace, which removes those classes' source even in developer mode. Is that intended?

DEC-RES-3 · inherited source-supported interpretation; not new owner approval

(a) Yes: the verbose message says "unless packaging, in which case items WILL be deployed" (Base.cls#L1837). (b) No: packaging should deploy into the artifact only and leave the developer's namespace editable. Native (N12): `package` of a `-dev` module made DeployMapA.First Deployed=1 in the namespace and its method source unreadable (O:nc-res-N12b-package-dev).

Source: [DEC-RES-3](../../playbook-uplift/decisions/07-09.md)

```g5-question
schema: 1
id: dec-res-3
status: answered
prompt: Packaging a module that has `Deploy="true"` classes runs MakeDeployed in the current namespace, which removes those classes' source even in developer mode. Is that intended?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: "(a) Yes: the verbose message says \"unless packaging, in which case items WILL be deployed\" (Base.cls#L1837). (b) No: packaging should deploy into the artifact only and leave the developer's namespace editable. Native (N12): `package` of a `-dev` module made DeployMapA.First Deployed=1 in the namespace and its method source unreadable (O:nc-res-N12b-package-dev)."
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/07-09.md
```

<a id="dec-sync-2"></a>
### When a module has no baseline (after `load -dev -skip-sync`, or from before sync existed), should the first `sync` load files edited since the load?

DEC-SYNC-2 · inherited source-supported interpretation; not new owner approval

(a) Yes: help for `-skip-sync` says "The next sync of the module records them instead" (Main.cls#L315), and sync help promises changes since the last load. (b) No (current, and asserted by Sync.cls#L126-133 for the pre-feature case): the first sync only stamps; edits made before it are absorbed. Native (N19): after `load -dev -skip-sync` and an edit, the first sync only set the baseline, the second said "Nothing to sync", and the server kept the old code (O:nc-sync-N19-skip-sync-first-sync).

Source: [DEC-SYNC-2](../../playbook-uplift/decisions/07-09.md)

```g5-question
schema: 1
id: dec-sync-2
status: answered
prompt: When a module has no baseline (after `load -dev -skip-sync`, or from before sync existed), should the first `sync` load files edited since the load?
severity: medium
multi_select: false
allow_free_text: true
options: []
answer:
  selected: []
  text: '(a) Yes: help for `-skip-sync` says "The next sync of the module records them instead" (Main.cls#L315), and sync help promises changes since the last load. (b) No (current, and asserted by Sync.cls#L126-133 for the pre-feature case): the first sync only stamps; edits made before it are absorbed. Native (N19): after `load -dev -skip-sync` and an edit, the first sync only set the baseline, the second said "Nothing to sync", and the server kept the old code (O:nc-sync-N19-skip-sync-first-sync).'
  rationale: Existing source interpretation retained from the findings audit.
  answered_by: null
  answered_at: null
resolution_source: playbook-uplift/decisions/07-09.md
```
