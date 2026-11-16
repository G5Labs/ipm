# Question and source audit

All 63 current canonical Q blocks were traced (49 inherited and 14 newly restored/created). The 44 playbook decision rows include 20 assumptions: 17 concern product policy and three concern method or an unconfirmed observation. An assumption is open even when a builder called it resolved. The inherited 31 G IDs and 22 C IDs remain indexed. C:Q23 records the dropped-resource policy rather than upgrading C:D26 into an accepted obligation.

Source review reads the canonical block, its same-ID metadata, source/comments, repository tests/docs and pinned git history listed for each entry. Relevant public discussion was checked separately in UPSTREAM.md. Current code demonstrates behavior; merge/test history can support inferred intent without proving a maintainer chose an unstated edge-case policy. No new owner approval was found. Security questions receive defensive source/existing-evidence review only.

<a id="lifecycle.empty-reload-policy"></a>
### lifecycle.empty-reload-policy

Canonical: [lifecycle.empty-reload-policy](../../../ontology/concepts/lifecycle.g5.md#lifecycle.empty-reload-policy)

Should standalone reload warn or fail when a module has no source root?

- Working assumption: A standalone `reload` of a module with no root says that nothing was loaded; the phase stays silent inside other phase lists.
- Alternative and implications: Fail the standalone command (stricter for scripts).
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [lifecycle](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Sources: [DEC-SYNC-4](../../../../playbook-uplift/decisions/07-09.md); Help: reload "pulls module source code into the namespace from disk"; the code's "For now" no-op.


Findings: [P:F-SYNC-3 — e2e reproduced](../../../../playbook-uplift/BUGS.md#p-f-sync-3). The linked entry qualifies the caller, reader and providers.

<a id="module.webapp.upgrade.policy"></a>
### module.webapp.upgrade.policy

Canonical: [module.webapp.upgrade.policy](../../../ontology/concepts/modules.g5.md#module.webapp.upgrade.policy)

Should a higher-version load remove a web application omitted by the new manifest?

- Working assumption: disclose that the application remains; do not promise automatic removal without an ownership rule.
- Alternatives: remove resources formerly owned by the module, or require an explicit migration. Automatic removal needs ownership and administrator-edit checks; explicit migration leaves cleanup to the author.
- An owner decision covering dropped resources, administrator changes and later uninstall would settle the policy. C:D26 observed a load, not update.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Storage/Module.cls`, `src/cls/IPM/Storage/ModuleSetting/Default.cls`, `src/cls/IPM/ResourceProcessor/WebApplication.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
acea30ff ORAS repos automatically filter deployed code modules with the current IRIS version (#1231)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Upgrade behavior](../../../ontology/concepts/modules.g5.md#module.webapp.upgrade)

Sources: C:D26; [starting evidence](sealed/.g5/uplift/consolidated/BUGS.md).


<a id="module.webapp.ownership"></a>
### module.webapp.ownership

Canonical: [module.webapp.ownership](../../../ontology/concepts/modules.g5.md#module.webapp.ownership)

Should install take over an existing web application of the same name, and should uninstall delete one IPM did not create?

- Evidence: `CreateOrUpdateWebApp` updates any existing application and `DeleteWebApp` deletes by name. No changelog, test or discussion addresses applications made outside IPM. The duplicate-name refusal protects only applications another module lists.
- Working assumption: keep the as-built behavior and disclose it. Alternatives are to refuse install when the application exists and no module lists it, or to record creation and delete only what IPM created. Both change today's reinstall-over-manual-setup path.
- Register: C:Q15.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Storage/Module.cls`, `src/cls/IPM/Storage/ModuleSetting/Default.cls`, `src/cls/IPM/ResourceProcessor/WebApplication.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
acea30ff ORAS repos automatically filter deployed code modules with the current IRIS version (#1231)
```

Existing attributable evidence:

- agent
- uncertain
- Expands: [Web applications](../../../ontology/concepts/modules.g5.md#module.webapp).

No authorized policy decision. Register entry C:Q15.


<a id="module.env.failure"></a>
### module.env.failure

Canonical: [module.env.failure](../../../ontology/concepts/modules.g5.md#module.env.failure)

When one of several `-env` files is missing, malformed or an array, what should the command keep, and how should two arrays merge?

- Provisional answer: as built. The command fails and keeps nothing from any file, and a later array overwrites the earlier one position by position, so it cannot remove items.
- Alternatives: keep the files loaded before the failing one, or check every file before changing anything; let a later array replace the earlier one.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Storage/Module.cls`, `src/cls/IPM/Storage/ModuleSetting/Default.cls`, `src/cls/IPM/ResourceProcessor/WebApplication.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
acea30ff ORAS repos automatically filter deployed code modules with the current IRIS version (#1231)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Environment files](../../../ontology/concepts/modules.g5.md#module.env).

Sources: the plan's policy row "A failing environment file" (no source states the intent); `General/EnvironmentConfig.cls:27-30` (clear on any error), `:118-126` (arrays merged by index). Native U2, U2r, U3-U5. Question C:Q6.


<a id="module.parameter.expressions.brace-policy"></a>
### module.parameter.expressions.brace-policy

Canonical: [module.parameter.expressions.brace-policy](../../../ontology/concepts/modules.g5.md#module.parameter.expressions.brace-policy)

Should a `$$$macro` expand when the attribute contains no opening brace?

- Evidence: `%Evaluate` returns early on no `{`; native E15/P4 leave the macro literal. The expression documentation lists macros without this gate.
- Working assumption: keep the observed brace gate until a maintainer chooses otherwise.
- Alternative: expand macros with or without a brace.
- Affected claims: parameter evaluation, after-install message, file-copy target and Invoke arguments. Alias C:Q2; parent [module.parameter.expressions](../../../ontology/concepts/modules.g5.md#module.parameter.expressions).

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Storage/Module.cls`, `src/cls/IPM/Storage/ModuleSetting/Default.cls`, `src/cls/IPM/ResourceProcessor/WebApplication.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
acea30ff ORAS repos automatically filter deployed code modules with the current IRIS version (#1231)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [expressions](../../../ontology/concepts/modules.g5.md#module.parameter.expressions).

Evidence: C:Q2; Storage/Module.cls:1833-1841; E15/P4; new attr_evaluator.rs.


<a id="module.processor.cpf-effects.policy"></a>
### module.processor.cpf-effects.policy

Canonical: [module.processor.cpf-effects.policy](../../../ontology/concepts/modules.g5.md#module.processor.cpf-effects.policy)

Must package failure undo instance configuration changes?

- Owner: IPM maintainers and instance administrators; no owner decision was available.
- Working assumption: database rollback alone gives no such promise. Alternatives are compensating selected changes or refusing them inside package transactions.
- Depends on: [rollback policy](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope.policy). A successful native role test settles observation, not this policy.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Storage/Module.cls`, `src/cls/IPM/Storage/ModuleSetting/Default.cls`, `src/cls/IPM/ResourceProcessor/WebApplication.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
acea30ff ORAS repos automatically filter deployed code modules with the current IRIS version (#1231)
```

Existing attributable evidence:

- agent
- uncertain
- Expands: [CPF effects](../../../ontology/concepts/modules.g5.md#module.processor.cpf-effects).

Authority remains open; source observation and checked alternatives are in the parent. Arbitrary CPF compensation has not been designed or implemented.


<a id="module.system-setting.cleanup-policy"></a>
### module.system-setting.cleanup-policy

Canonical: [module.system-setting.cleanup-policy](../../../ontology/concepts/modules.g5.md#module.system-setting.cleanup-policy)

Should uninstall keep a legacy SystemSetting value or restore the value that preceded installation?

The implementation keeps the installed value, and that is the working assumption. Restoring the old value would need to track it and account for later administrator edits; blindly deleting it can remove a setting another user now relies on. The owner must choose this policy (C:Q17).

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Storage/Module.cls`, `src/cls/IPM/Storage/ModuleSetting/Default.cls`, `src/cls/IPM/ResourceProcessor/WebApplication.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
acea30ff ORAS repos automatically filter deployed code modules with the current IRIS version (#1231)
```

Existing attributable evidence:

- agent; uncertain; user-facing-behavior
- Expands: [Legacy setting](../../../ontology/concepts/modules.g5.md#module.system-setting).

Source/native: S7-10, `SystemSetting.cls::OnPhase` has only Compile writes and no Clean action. This is distinct from CPF external child effects (C:Q12), and no new defect is asserted.


- uncertain

<a id="module.python.cleanup-policy"></a>
### module.python.cleanup-policy

Canonical: [module.python.cleanup-policy](../../../ontology/concepts/modules.g5.md#module.python.cleanup-policy)

How should IPM remove Python packages without removing a package another module or administrator uses?

The working assumption preserves the current shared installation on Clean. Tracking package users could permit later removal, but the product TODO does not settle ownership of manually installed or upgraded versions. This needs an owner decision (C:Q18).

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Storage/Module.cls`, `src/cls/IPM/Storage/ModuleSetting/Default.cls`, `src/cls/IPM/ResourceProcessor/WebApplication.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
acea30ff ORAS repos automatically filter deployed code modules with the current IRIS version (#1231)
```

Existing attributable evidence:

- agent; uncertain; user-facing-behavior
- Expands: [Explicit wheel](../../../ontology/concepts/modules.g5.md#module.python.explicit-wheel).

Source: `PythonWheel.cls::OnPhase` Clean TODO explicitly identifies shared-package risk. Native uninstall retains the local lune 1.6.2 distribution. This is not a new defect claim.


- uncertain

<a id="module.processor.file-copy.unknown-descendants"></a>
### module.processor.file-copy.unknown-descendants

Canonical: [module.processor.file-copy.unknown-descendants](../../../ontology/concepts/modules.g5.md#module.processor.file-copy.unknown-descendants)

Should FileCopy protect a parent containing an unconfigured descendant with an IRIS.DAT marker, or only configured roots/ancestors and a marker at the exact target? The observed rule is the latter. Tests cover exact unconfigured markers and configured-root ancestors; no inspected human decision settles a recursive search requirement. C:Q22 retains both interpretations.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Storage/Module.cls`, `src/cls/IPM/Storage/ModuleSetting/Default.cls`, `src/cls/IPM/ResourceProcessor/WebApplication.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
acea30ff ORAS repos automatically filter deployed code modules with the current IRIS version (#1231)
```

Existing attributable evidence:

- agent; unresolved; provisional-as-built
- Expands: [Protection](../../../ontology/concepts/modules.g5.md#module.processor.file-copy.protection).

Source comments protect database directories including an unconfigured exact marker; unit tests exercise that case without an ancestor scan. The synthetic nested-marker native neighbor and `whole_segment_neighbors` formal witness expose the distinction. No real database-loss claim or accepted recursive policy is inferred. [C:Q22](../QUESTIONS.md).


- uncertain

<a id="namespace.enable.source-ambiguity-policy"></a>
### namespace.enable.source-ambiguity-policy

Canonical: [namespace.enable.source-ambiguity-policy](../../../ontology/concepts/namespaces.g5.md#namespace.enable.source-ambiguity-policy)

Which source owns mapped IPM when several namespace records name one code database?

- Working assumption: The source of a mapped namespace is the namespace whose own code database holds the IPM code.
- Alternative and implications: Report a tie as an error instead of guessing. Either way F-NSP-7 stays a defect.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [namespace.enable](../../../ontology/concepts/namespaces.g5.md#namespace.enable)

Sources: [DEC-NSP-6](../../../../playbook-uplift/decisions/07-09.md); `-map` help and issue #470; "last by name" has no source.


<a id="namespace.enable.community-failure-policy"></a>
### namespace.enable.community-failure-policy

Canonical: [namespace.enable.community-failure-policy](../../../ontology/concepts/namespaces.g5.md#namespace.enable.community-failure-policy)

What status should enable -community return when a component step fails?

- Working assumption: If a step of `enable -community` fails, the command stops and returns that error.
- Alternative and implications: Run all three steps and return a combined error (also fine); or keep OK and document: F-NSP-11 becomes documentation.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [namespace.enable](../../../ontology/concepts/namespaces.g5.md#namespace.enable)

Sources: [DEC-NSP-4](../../../../playbook-uplift/decisions/07-09.md); Scripts use `zpm "...":1` to stop on errors; every other `enable` path returns its error.


<a id="repo.oras.unpublish-platform-policy"></a>
### repo.oras.unpublish-platform-policy

Canonical: [repo.oras.unpublish-platform-policy](../../../ontology/concepts/repositories.g5.md#repo.oras.unpublish-platform-policy)

Which platform tags should unpublish remove when only a version is named?

- Working assumption: ORAS `unpublish R/M V` deletes the plain tag and every `V__<platform>` tag of that version.
- Alternative and implications: Require the platform to be named; the plain form must still not fail as "not found".
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Repo/Manager.cls`, `src/cls/IPM/Repo/Filesystem/Definition.cls`, `src/cls/IPM/Repo/Oras/Definition.cls`.

Pinned history:

```text
ce1bd00f Fix three ORAS bugs (#1305)
d3815fa3 Multi-byte characters in module.xml parse properly (#1296)
82ea5166 Improve ORAS error reporting (#1284)
8fd97c0b Fix namespaced ORAS repos not creating dependency path correctly (#1268)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [repo.oras](../../../ontology/concepts/repositories.g5.md#repo.oras)

Sources: [DEC-REPO-5](../../../../playbook-uplift/decisions/05-06.md); Today a deployed version cannot be unpublished by version at all, which no source intends.


<a id="repo.oras.credentials-change-policy"></a>
### repo.oras.credentials-change-policy

Canonical: [repo.oras.credentials-change-policy](../../../ontology/concepts/repositories.g5.md#repo.oras.credentials-change-policy)

Should changing an ORAS registry address clear its stored credentials?

- Working assumption: Changing an ORAS registry's address clears its stored credentials, as for a remote registry.
- Alternative and implications: Keep and warn (operator only renamed the host): F-REPO-11 becomes a missing warning.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Repo/Manager.cls`, `src/cls/IPM/Repo/Filesystem/Definition.cls`, `src/cls/IPM/Repo/Oras/Definition.cls`.

Pinned history:

```text
ce1bd00f Fix three ORAS bugs (#1305)
d3815fa3 Multi-byte characters in module.xml parse properly (#1296)
82ea5166 Improve ORAS error reporting (#1284)
8fd97c0b Fix namespaced ORAS repos not creating dependency path correctly (#1268)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [repo.oras](../../../ontology/concepts/repositories.g5.md#repo.oras)

Sources: [DEC-REPO-3](../../../../playbook-uplift/decisions/05-06.md); Standard practice (credentials belong to a host); the remote registry already does it (`Remote/Definition.cls#L81-L86`).


<a id="repo.filesystem.bad-module"></a>
### repo.filesystem.bad-module

Canonical: [repo.filesystem.bad-module](../../../ontology/concepts/repositories.g5.md#repo.filesystem.bad-module)

Should a module with an invalid name in a filesystem repository's folder block the whole repository?

- Provisional answer: no. Skip it with a warning, like an unreadable manifest, so the other modules stay available.
- As built: yes. The same failure stops a load's temporary `.modules` repository from being created, which then fails the load after its commit.
- Alternative: fail every bad manifest, including unreadable ones, so nothing is silently hidden.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Repo/Manager.cls`, `src/cls/IPM/Repo/Filesystem/Definition.cls`, `src/cls/IPM/Repo/Oras/Definition.cls`.

Pinned history:

```text
ce1bd00f Fix three ORAS bugs (#1305)
d3815fa3 Multi-byte characters in module.xml parse properly (#1296)
82ea5166 Improve ORAS error reporting (#1284)
8fd97c0b Fix namespaced ORAS repos not creating dependency path correctly (#1268)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Filesystem repository](../../../ontology/concepts/repositories.g5.md#repo.filesystem).

Sources: `Repo/Filesystem/Definition.cls:184-241` (unreadable manifests skipped at `:196-200`; a save error returned at `:234-237` and aggregated at `:170-176`). Native LK1 (skipped), TR7 and Dtxn (blocked). G's attempt 1 for case C29 also found that an unparseable manifest is skipped (`native/make_replay.py:248`). Question C:Q5.

Extension Verus: [owners.rs::add_cache](../../../Verus_verification/consolidated/verus/owners.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../extension/bindings.json). They do not mechanically verify the ObjectScript body.


<a id="repo.oras.integrity.policy"></a>
### repo.oras.integrity.policy

Canonical: [repo.oras.integrity.policy](../../../ontology/concepts/repositories.g5.md#repo.oras.integrity.policy)

Should an untagged pull always select the highest semantic version, and must a package have exactly one archive layer?

- Owner: registry maintainers and IPM maintainers; no owner decision was available. The method comment supports highest-version intent, while source behavior depends on provider order.
- Working assumption: expose the mismatch and retain size/digest checking. Alternatives are sorting before selection or requiring a registry ordering contract, plus rejecting ambiguous archives or selecting by declared digest.
- Proposed repairs are not implemented or proved. Preserve deployed platform tags and compare both a valid single archive and ambiguous multi-layer input before a repair.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Repo/Manager.cls`, `src/cls/IPM/Repo/Filesystem/Definition.cls`, `src/cls/IPM/Repo/Oras/Definition.cls`.

Pinned history:

```text
ce1bd00f Fix three ORAS bugs (#1305)
d3815fa3 Multi-byte characters in module.xml parse properly (#1296)
82ea5166 Improve ORAS error reporting (#1284)
8fd97c0b Fix namespaced ORAS repos not creating dependency path correctly (#1268)
```

Existing attributable evidence:

- agent
- uncertain
- Expands: [ORAS integrity](../../../ontology/concepts/repositories.g5.md#repo.oras.integrity).

No authorized policy decision. The parent states source authority, alternatives, preservation conditions and remaining premises.


<a id="repo.remote.download.credentials"></a>
### repo.remote.download.credentials

Canonical: [repo.remote.download.credentials](../../../ontology/concepts/repositories.g5.md#repo.remote.download.credentials)

Should registry credentials be sent to a download URL on a different server?

- Evidence: commit `d6cc4204` switched to the registry-supplied URL to "fix downloading packages"; a native provider received the repository's Basic credentials and API key at a different host and port. Elsewhere the maintainers treat credentials as belonging to one host: CHANGELOG 0.10.11 `#1115` stops ORAS credential checks from following redirects "so credentials are not sent to another host", and `#649` clears a remote registry's credentials when its URL changes. No source decides the download case.
- Working assumption: keep following the returned URL, and record the forwarding as suspected defect C:D20. The proposed alternative sends credentials only when the URL's server and port match the repository; a third choice is to refuse such URLs. Either change can break registries that redirect downloads to storage needing the same credentials.
- Register: C:Q14 and proposal C:S14, revised as C:S14b after the [redirect challenge](../../../ontology/concepts/repositories.g5.md#repo.remote.download.redirect).

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Repo/Manager.cls`, `src/cls/IPM/Repo/Filesystem/Definition.cls`, `src/cls/IPM/Repo/Oras/Definition.cls`.

Pinned history:

```text
ce1bd00f Fix three ORAS bugs (#1305)
d3815fa3 Multi-byte characters in module.xml parse properly (#1296)
82ea5166 Improve ORAS error reporting (#1284)
8fd97c0b Fix namespaced ORAS repos not creating dependency path correctly (#1268)
```

Existing attributable evidence:

- agent
- uncertain
- Expands: [Remote download](../../../ontology/concepts/repositories.g5.md#repo.remote.download).

No authorized policy decision. Register entries C:Q14 and C:S14; the proposed request is proved only at the abstract request boundary and was not applied.


<a id="repo.filesystem.concurrent-refresh"></a>
### repo.filesystem.concurrent-refresh

Canonical: [repo.filesystem.concurrent-refresh](../../../ontology/concepts/repositories.g5.md#repo.filesystem.concurrent-refresh)

Does cache replacement need its explicit lock until the caller's transaction ends?

- Owner: IPM maintainers. The default constructor releases at object close; the deferred constructor is a separate supported mechanism.
- Working assumption: keep the intended consistent cache and investigate competing refreshes under actual database isolation. Alternatives are deferred release, refusing an outer transaction or demonstrating that existing database locks suffice.
- No race corruption or repair is established by the observed lock release. [Withdrawal and paired evidence](../iter2/boundaries/withdrawals.json).

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Repo/Manager.cls`, `src/cls/IPM/Repo/Filesystem/Definition.cls`, `src/cls/IPM/Repo/Oras/Definition.cls`.

Pinned history:

```text
ce1bd00f Fix three ORAS bugs (#1305)
d3815fa3 Multi-byte characters in module.xml parse properly (#1296)
82ea5166 Improve ORAS error reporting (#1284)
8fd97c0b Fix namespaced ORAS repos not creating dependency path correctly (#1268)
```

Existing attributable evidence:

- agent
- uncertain
- Expands: [Filesystem cache](../../../ontology/concepts/repositories.g5.md#repo.filesystem).

Source owner and constructor binding: [boundary ledger](../iter2/boundaries/bindings.json). The prior lock correspondence is withdrawn; other cache properties retain their evidence.


<a id="transaction.flag-sources.dependency-defaults"></a>
### transaction.flag-sources.dependency-defaults

Canonical: [transaction.flag-sources.dependency-defaults](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-defaults)

Does a dependency's own `<Defaults>` `NoJournal` apply inside its parent's transaction, so that the dependency's changes escape the parent's normal-mode rollback?

- Provisional answer: yes. A module's `<Defaults>` apply whenever that module is installed, including as a dependency. A `NoJournal` dependency therefore keeps its changes when the parent rolls back, and its history record must survive and say so.
- Reason: the wiki says flags embedded in `module.xml` are "applied whenever this module is installed", and that with `NoJournal` "changes cannot be rolled back". These statements are specific to the flags, while the whole-load rollback rule is general.
- Alternative: the parent's transaction governs every nested load, so a dependency's `<Defaults>` `NoJournal` is ignored, or the dependency is refused, while a parent transaction is open. This keeps [the whole-load rollback](../../../ontology/features/load.g5.md#load.outcome).

Unresolved Q-25: [May an escaped NoJournal dependency remain without its own dependencies?](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-closure)

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- architecture-decision
- Expands: [Flag sources](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources).

Sources: new question Q-22 from stage C round 2 finding R2-03. Round 3 finding R3-03 limited it to `NoJournal`, the flag that can escape a parent's rollback, and moved `NoTransaction` to [Q-23](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-no-transaction). `wiki/00` L147 (whole-load rollback), L149 (`NoTransaction` "when some modules in the install conflict with an outer wrapping transaction"), L155 (with `NoJournal`, "changes cannot be rolled back"), L175 (`<Defaults>` "applied whenever this module is installed"); maintainer doc comment `Utils/Module.cls:1033-1040` (example `<Defaults>` with `NoTransaction`); [load outcome](../../../ontology/features/load.g5.md#load.outcome). No maintainer statement on dependencies inside a parent transaction was found. The provisional answer follows the sources specific to `<Defaults>` and keeps the record-survival rule of [survives rollback](../../../ontology/features/history.g5.md#history.survives-rollback).

Observed (stage C cases M5 and M5N, `20261113T204405Z-stagec2`). In M5 a normal-mode parent fails in Activate, and its dependency's `<Defaults>` set `NoJournal=1`. The parent is rolled back, while the dependency stays installed with its class and `.ZPM` document. No `<ROLLFAIL>` occurs, `$TLEVEL` returns to 0 and the journaling flag is restored. The dependency's history record is lost, because it was journaled and rolled back (D1). M5N, the same without `<Defaults>`, rolls everything back and also loses the dependency record (D1). Code: the dependency's `<Defaults>` are merged under the parent's parameters (`Utils/Module.cls:1166-1169`) and turn journaling off inside the open transaction (`:1191-1193`; premise N-P07). B2 files this as D14, "confirmed, decision pending". The model cannot represent this case (`B/model-notes.md`).


<a id="transaction.flag-sources.dependency-no-transaction"></a>
### transaction.flag-sources.dependency-no-transaction

Canonical: [transaction.flag-sources.dependency-no-transaction](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-no-transaction)

Should a dependency's own `<Defaults>` `NoTransaction` suppress its parent's transaction?

- Provisional answer: yes. A module that declares `NoTransaction` must not run inside a transaction opened for its parent, so the package change runs without one, and a failure may leave a partial install.
- Reason: the wiki gives `NoTransaction`'s purpose as modules in the install that "conflict with an outer wrapping transaction", and says `<Defaults>` apply "whenever this module is installed". Without suppression, the flag of a dependency would have no effect, because the dependency would still run inside the parent's transaction.
- Alternative: the flag affects only the dependency's own load, so the parent's transaction still wraps it and undoes it on failure. A user whose install contains a conflicting module passes `-DNoTransaction=1` for the whole command.

## Developer mode

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- architecture-decision
- Expands: [Flag sources](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources).

Sources: new question Q-23 from stage C round 3 finding R3-03. Main source: `wiki/00` L149 ("Use when some modules in the install conflict with an outer wrapping transaction"). Also L147 (whole-load rollback), L175 (`<Defaults>` "applied whenever this module is installed") and the maintainer doc comment `Utils/Module.cls:1033-1040`, whose example `<Defaults>` sets `NoTransaction`. Checked together with [Q-22](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-defaults): both provisional answers honor a module's own transaction flags when it is installed as a dependency.

Observed by code reading: a dependency whose `<Defaults>` set only `NoTransaction` skips its own `tstart`, but its writes stay journaled inside the parent's open transaction and are undone with it (`Utils/Module.cls:1191-1196`; premise N-P01). Its no-transaction marker first sets `Committed=1` (`:1226-1228`), and its record is then lost with the parent's rollback (D1). The `NoTransaction` variant of stage C case M5 was not run natively.


Findings: [G:D28 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d28). The linked entry qualifies the caller, reader and providers.

<a id="dev-mode.overwrite-guard.force"></a>
### dev-mode.overwrite-guard.force

Canonical: [dev-mode.overwrite-guard.force](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.force)

May `-force` lift the developer-mode overwrite guard, and does the guard cover `load`?

- Provisional answer: keep the current scope. The guard covers `install` only, and `-force` does not lift it.
- Reason: this follows the product as built since the guard was added, and the `-force` help, which speaks only of newer versions without update steps. The wiki's "Uninstall first, or use `-force`" conflicts with both, and a maintainer must decide which is meant.
- Alternative: let `-force` lift the guard as the wiki says, and decide whether `load` is covered.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Overwrite guard](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard).

Sources: A1 Q-03; `wiki/00` L129 (maintainer-authored, isc-dchui, 2026-05-13); `wiki/05` (names only uninstalling); `-force` help `Main.cls:441`; `Utils/Module.cls:103-114`. The guard has never checked `-force` since it was introduced in commit `d541c5b4` (2022-08-23). Round 1 R1-ONT-05: the wiki line is a conflict for a maintainer, not a documentation error. Not run by B2.


<a id="dev-mode.sticky"></a>
### dev-mode.sticky

Canonical: [dev-mode.sticky](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.sticky)

When a module is already in developer mode, does a plain `load` with neither `-dev` nor `-nodev` keep developer mode?

- Provisional answer: yes, it keeps the stored mode.
- Reason: the fix this follows stopped plain loads from turning developer mode on. It did not ask plain loads to turn it off, and `-nodev` exists for that.
- Alternative: a plain load always uses normal mode, following that fix's wording that a module "should only be DevMode if the -dev flag is included".

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Developer mode](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode).

Sources: A1 Q-01; CHANGELOG 0.10.4 (`#892`: "no longer sets Developer Mode unless -dev flag is set"); issue #892; commit `f83b207e`. Observed: a load without `-dev` or `-nodev` keeps the stored mode and does not save `Root` or `Repository` (`Utils/Module.cls:1298-1307`, OBS-L3). Affects [load options](../../../ontology/features/load.g5.md#load.options). Native evidence: g8 (plain `load` of a developer-mode module) keeps developer mode, consistent with the provisional answer, and g9 (`-nodev`) turns it off (`20261113T182832Z-cases-dg`).


<a id="transaction.rollback-scope.policy"></a>
### transaction.rollback-scope.policy

Canonical: [transaction.rollback-scope.policy](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope.policy)

Should a failed package change compensate external effects that the database cannot roll back?

- Evidence: copied files and `module-lock.json` survived native database rollback; arbitrary hooks and remote calls can have further effects.
- Working assumption: database rollback is the existing boundary; external effects need separate receipts and must be reported.
- Alternative: add explicit compensating actions for each external writer.
- Affected claims: load failure atomicity, lock-file location and status, file copy, CPF and history `Committed`.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [transaction.rollback-scope](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.rollback-scope).

Evidence: Native held-out FileCopy/load receipts in ../../uplift/consolidated/extension/REPORT.md. No product repair applied.


<a id="dev-mode.overwrite-guard.dependency-policy"></a>
### dev-mode.overwrite-guard.dependency-policy

Canonical: [dev-mode.overwrite-guard.dependency-policy](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.dependency-policy)

May a parent replace a dependency that is already in developer mode?

- Evidence: Source guard checks only the top-level Install parameter; native N-R05-DEVUP replaces the dependency at another version and clears developer mode. Same-version DEVSAME skips it.
- Working assumption: Preserve the stated install overwrite guard for dependencies; load scope follows the parent force/load question.
- Alternative: Allow top-level guard only, with a warning when dependency developer mode is cleared.
- Affected claims: install refusal, dependency replacement and developer working copy preservation. Alias Q-24.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [dev-mode.overwrite-guard](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-24, source/native cases named in the block; [extension evidence](../extension/REPORT.md).


Findings: [G:I3 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-i3). The linked entry qualifies the caller, reader and providers.

Findings: [P:F-INST-9 — e2e reproduced](../../../../playbook-uplift/BUGS.md#p-f-inst-9). The linked entry qualifies the caller, reader and providers.

<a id="transaction.flag-sources.dependency-closure"></a>
### transaction.flag-sources.dependency-closure

Canonical: [transaction.flag-sources.dependency-closure](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-closure)

May an escaped NoJournal dependency remain without its own dependencies?

- Evidence: N-R05-NESTF leaves d1 installed and rolls d2 back because the flat closure loads d2 under parent parameters before d1.
- Working assumption: Require closure consistency as intent; record the observed violating state rather than assuming a repair.
- Alternative: Document temporary inconsistency after failure, or use one load-wide transaction mode.
- Affected claims: dependency satisfaction, mixed-mode rollback and retry. Alias Q-25.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [transaction.flag-sources.dependency-defaults](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-defaults).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-25, source/native cases named in the block; [extension evidence](../extension/REPORT.md).


Findings: [G:D21 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d21). The linked entry qualifies the caller, reader and providers.

<a id="version.range.year"></a>
### version.range.year

Canonical: [version.range.year](../../../ontology/concepts/versions.g5.md#version.range.year)

Should a partial version such as `2024.1` mean exactly `2024.1.0`, as built, while `2024.12` and `1.2` mean a whole minor series?

- Provisional answer: keep the special case for IRIS-style versions, because IPM's tests require `2022.2` to match only `2022.2`.
- Alternative: one rule for every partial version. Then `2022.2` would also accept `2022.2.1`.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/SemanticVersionExpression/Range.cls`.

Pinned history:

```text
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
dfdda468 Fix dependency resolution bug, semver intersection and bug and have better error message for dependency incompatibilities (#1194)
02189183 Fix v0.10.5 release blockers (#1016)
4cace764 BuildDependencyGraph performance enhancements (#975)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Partial versions](../../../ontology/concepts/versions.g5.md#version.range.partial).

Sources: `General/SemanticVersionExpression/Range.cls:88-91` (pattern `20NN.N`); tests `Expressions.cls:168-176` (`TestIRISVersions`). Native X35 (`2024.1` is `=2024.1.0`) and X36 (`2024.12` is `>=2024.12.0 <2024.13.0-0`). Question C:Q3.


<a id="artifact.refresh-policy"></a>
### artifact.refresh-policy

Canonical: [artifact.refresh-policy](../../../ontology/features/artifact-download.g5.md#artifact.refresh-policy)

When should an existing artifact folder or same-named temporary file be refreshed?

- Owner: IPM maintainers and module authors. The code deliberately reuses a local folder during Activate, but no accepted cache identity or expiry rule was found.
- Working assumption: preserve observed reuse and disclose that it gives no freshness guarantee. Alternatives are immutable digest-addressed files, explicit refresh, or validating the cached bytes against the selected artifact.
- This decision affects reproducible installs and exports. Do not remove local/offline reuse as part of an authentication repair.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/ResourceProcessor/ArtifactoryTarball.cls`.

Pinned history:

```text
15093bbf Warn when resource processors target the install directory (#1321)
92953235 ArtifactoryTarball: download anonymously when no credentials are set (#1263)
2e4ee368 The `load` command also unpacks to the standard directory (#1147)
1dde4efc Adding new processor class for artifactory (#932)
```

Existing attributable evidence:

- agent
- Expands: [Artifact download](../../../ontology/features/artifact-download.g5.md#artifact).

Evidence: local reuse and same-filename native contrasts; [C:Q10](../QUESTIONS.md). No owner ratification or refresh implementation.


- uncertain

<a id="artifact.target-policy"></a>
### artifact.target-policy

Canonical: [artifact.target-policy](../../../ontology/features/artifact-download.g5.md#artifact.target-policy)

Should artifact copies refuse protected installation folders as FileCopy does?

- Owner: IPM maintainers and instance administrators. The processor calls the shared warning before Activate's copy; it does not call FileCopy's deletion guard. No protected system folder was modified in this run.
- Working assumption: disclose the warning boundary without promising protection. Alternatives are a common refusal rule, an explicit operator override, or retaining warning-only behavior.
- Archive path safety, partial copies and compensation remain open. A successful copy under `/tmp` settles none of these policies.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/ResourceProcessor/ArtifactoryTarball.cls`.

Pinned history:

```text
15093bbf Warn when resource processors target the install directory (#1321)
92953235 ArtifactoryTarball: download anonymously when no credentials are set (#1263)
2e4ee368 The `load` command also unpacks to the standard directory (#1147)
1dde4efc Adding new processor class for artifactory (#932)
```

Existing attributable evidence:

- agent
- Expands: [Artifact download](../../../ontology/features/artifact-download.g5.md#artifact).

Evidence: `ArtifactoryTarball.cls::OnAfterPhase`, `Utils/File.cls::WarnIfInInstallDirectory`; [C:Q11](../QUESTIONS.md). Only temporary directories were used by the probes.


- uncertain

<a id="build.test.outcome.empty.policy"></a>
### build.test.outcome.empty.policy

Canonical: [build.test.outcome.empty.policy](../../../ontology/features/build-and-publish.g5.md#build.test.outcome.empty.policy)

Should a declared test package with no test classes fail `test` and `verify`?

- Evidence: the source comment calls it "not an error in itself" and skips it on purpose. No changelog entry, test or maintainer discussion states what CI users should see.
- Working assumption: keep the warning and the passing status. The alternative is to fail, or to fail only when a run executes no tests at all. Either alternative changes current green builds whose test package is intentionally empty.
- Register: C:Q13.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- Expands: [Empty test package](../../../ontology/features/build-and-publish.g5.md#build.test.outcome.empty).

No authorized policy decision. Register entry C:Q13 in [QUESTIONS.md](../QUESTIONS.md).


<a id="build.package.include-refresh.policy"></a>
### build.package.include-refresh.policy

Canonical: [build.package.include-refresh.policy](../../../ontology/features/build-and-publish.g5.md#build.package.include-refresh.policy)

Should a reloaded manifest that omits IncludeTests keep the previous value or reset it to false?

- Working assumption: preserve the checked behavior and use an explicit false to clear it. Resetting on omission would make a removed declaration disable test packaging, but would change existing reload behavior.
- Evidence: the XML reader reuses the existing module object, does not reset this scalar field and writes the result back. Native package/extract observations distinguish retention from an explicit false. No owner decision found. Register: C:Q20.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- Expands: [Reloaded inclusion field](../../../ontology/features/build-and-publish.g5.md#build.package.include-refresh).

[Formal alternatives and observed behavior](../../../Verus_verification/consolidated/iter10/scope.rs). Neither runtime behavior nor an agent proposal is owner approval.


- uncertain

<a id="deps.lockfile.local-supplier-policy"></a>
### deps.lockfile.local-supplier-policy

Canonical: [deps.lockfile.local-supplier-policy](../../../ontology/features/dependencies.g5.md#deps.lockfile.local-supplier-policy)

Should a lock file accept a directory-loaded dependency with no registry?

- Working assumption: A needed module loaded from a folder is written to the lock file with an empty registry.
- Alternative and implications: Fail the command (if a future `ci` needs a registry for every entry): F-DEP-9 changes accordingly.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [deps.lockfile](../../../ontology/features/dependencies.g5.md#deps.lockfile)

Sources: [DEC-DEP-3](../../../../playbook-uplift/decisions/03-04.md); The writer already does this for the base module (`LockFile.cls#L74-L78`).


Findings: [P:F-DEP-9 — e2e reproduced](../../../../playbook-uplift/BUGS.md#p-f-dep-9). The linked entry qualifies the caller, reader and providers.

Supporting context: issue #962 concerns future lock-file installation; it does not decide local-supplier serialization.

<a id="devtools.sync.aggregate-status-policy"></a>
### devtools.sync.aggregate-status-policy

Canonical: [devtools.sync.aggregate-status-policy](../../../ontology/features/developer-tools.g5.md#devtools.sync.aggregate-status-policy)

Should sync without a module name return an error when a module fails?

- Working assumption: `sync` with no module name reports each failure and a summary, then returns OK.
- Alternative and implications: Return an error when any module fails, like `sync <module>`: a new Low defect.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/General/Sync/Pipeline.cls`, `tests/integration_tests/Test/PM/Integration/Sync.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
04519bf0 Fix tarball load issue and cleanup (#1307)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [devtools.sync](../../../ontology/features/developer-tools.g5.md#devtools.sync)

Sources: [DEC-SYNC-3](../../../../playbook-uplift/decisions/07-09.md); PR #1205: it "collects failures without throwing" (says the loop continues; silent on the final status).


<a id="devtools.sync.failed-import-policy"></a>
### devtools.sync.failed-import-policy

Canonical: [devtools.sync.failed-import-policy](../../../ontology/features/developer-tools.g5.md#devtools.sync.failed-import-policy)

Should sync retry a failed compile or reload after its source is reverted?

- Working assumption: Files that a failed `compile`, `load -dev` or `reload` put on the server are re-detected by the next `sync`.
- Alternative and implications: Retry marks stay sync-only and the docs say "after a failed compile, run compile again": F-SYNC-1 becomes documentation.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/General/Sync/Pipeline.cls`, `tests/integration_tests/Test/PM/Integration/Sync.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
04519bf0 Fix tarball load issue and cleanup (#1307)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [devtools.sync](../../../ontology/features/developer-tools.g5.md#devtools.sync)

Sources: [DEC-SYNC-1](../../../../playbook-uplift/decisions/07-09.md); Sync help "since the last load/sync"; issue #1224's expected behaviour for the same symptom inside sync.


Findings: [P:F-SYNC-1 — e2e reproduced](../../../../playbook-uplift/BUGS.md#p-f-sync-1). The linked entry qualifies the caller, reader and providers.

<a id="devtools.authoring.module-version.file"></a>
### devtools.authoring.module-version.file

Canonical: [devtools.authoring.module-version.file](../../../ontology/features/developer-tools.g5.md#devtools.authoring.module-version.file)

Should `module-version` also update the `module.xml` file a module was loaded from?

- Provisional answer: no, as built. The command edits the namespace's copy, and source control hooks, when present, take it from there.
- Alternative: yes for a module loaded in developer mode, whose folder is its working copy.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/General/Sync/Pipeline.cls`, `tests/integration_tests/Test/PM/Integration/Sync.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
04519bf0 Fix tarball load issue and cleanup (#1307)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [module-version](../../../ontology/features/developer-tools.g5.md#devtools.authoring.module-version).

Sources: native MV10; `StudioDocument/Module.cls:403-420` rewrites the stored `.ZPM` document only. Question C:Q4.

Extension Verus: [owners.rs::update_version](../../../Verus_verification/consolidated/verus/owners.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../extension/bindings.json). They do not mechanically verify the ObjectScript body.


<a id="devtools.generate.invalid-source"></a>
### devtools.generate.invalid-source

Canonical: [devtools.generate.invalid-source](../../../ontology/features/developer-tools.g5.md#devtools.generate.invalid-source)

Should `generate` report or refuse a source file that cannot load?

As built, it skips the file and can produce a manifest without it. The working assumption preserves that behavior; the alternative fails generation or reports each omission so an author can fix it (C:Q16).

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/General/Sync/Pipeline.cls`, `tests/integration_tests/Test/PM/Integration/Sync.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
04519bf0 Fix tarball load issue and cleanup (#1307)
```

Existing attributable evidence:

- agent; uncertain; user-facing-behavior
- Expands: [Generated resources](../../../ontology/features/developer-tools.g5.md#devtools.generate.resources).

Source: `ReadItems` continues after a failed `$system.OBJ.Load`. C:Q16 is a proposal for owner review; no defect is asserted without an accepted policy.


- uncertain

<a id="history.record.unlogged-refusals"></a>
### history.record.unlogged-refusals

Canonical: [history.record.unlogged-refusals](../../../ontology/features/history.g5.md#history.record.unlogged-refusals)

Should IPM record `update`, `reinstall` and `uninstall` attempts it refuses because of their arguments, as it records refused loads and installs?

- Provisional answer: record them. Examples are `update` or `reinstall` of a module that is not installed, and `uninstall` with no name.
- Reason: [recorded actions](../../../ontology/features/history.g5.md#history.actions) include refused attempts, and the wiki says IPM records every action "it attempts, whether or not the attempt succeeded". `install` with no name is already recorded.
- Alternative: argument refusals are not attempts, because IPM refuses them before the action starts.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Record](../../../ontology/features/history.g5.md#history.record).

Sources: A1 Q-09; `wiki/09` L18; issue #366 ("we still want to log failed installations"). Round 1 R1-ONT-06 changed the provisional answer to "record them", following [recorded actions](../../../ontology/features/history.g5.md#history.actions). Observed: `update` refuses `PermitDowngrade` and a module that is not installed before creating a record (`Main.cls:4415-4423`). `uninstall` with no name and `reinstall` of a module that is not installed are refused the same way (`Main.cls:2685-2688`, `:2709-2712`). Not run by B2.


Findings: [G:D29 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d29). The linked entry qualifies the caller, reader and providers.

<a id="history.record.abnormal-end"></a>
### history.record.abnormal-end

Canonical: [history.record.abnormal-end](../../../ontology/features/history.g5.md#history.record.abnormal-end)

What should `Committed` say on the record of an action that never ended?

- Provisional answer: `Committed` reflects the changes kept, even on an unfinished record. IPM writes it to the permanent record at each commit.
- Reason: `Committed` "should reflect whether the action's changes were actually retained", and the wiki's recipe `-Dcommitted=1 -DSuccess="0*"` is how users find failures that kept changes. A crash that kept changes is that case.
- Alternative: an empty end time means the outcome is unknown, so readers must not trust `Committed=0` on such a record.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Record](../../../ontology/features/history.g5.md#history.record).

Sources: A1 Q-08; issue #1237 ("should reflect whether the action's changes were actually retained"); `wiki/09` L215-219 (recipe for failures that kept changes), L229-233. Round 1 R1-ONT-02 changed the provisional answer to follow issue #1237, and "unknown outcome" is now the alternative.

Native evidence: C4R, a developer-mode load SIGKILLed in Activate after both commits, keeps its module and classes while the permanent record reads `Committed=0`. That deviates from the provisional answer, and the `wiki/09` recipe misses it (`183827Z-cases-c4r`). C2, ended by `$SYSTEM.Process.Terminate`, reads `Committed=1` (`182903Z-cases-c-terminate`). Delay: IRIS rolls back a SIGKILLed process's transaction only after slot cleanup ([load outcome](../../../ontology/features/load.g5.md#load.outcome)). Not run: instance restart (premise P-3). Approximate case B1-C49 (debugger stall between the final commit and its `Committed` mark): the module is fully installed while the record reads `Committed=0`, and after Terminate the record is also finalized (`205218Z-replay-approx-c`; approximate, see `B/native-replay.md` section 5).


<a id="history.fields.dependency-source"></a>
### history.fields.dependency-source

Canonical: [history.fields.dependency-source](../../../ontology/features/history.g5.md#history.fields.dependency-source)

How should the log label and show a dependency that IPM pulled from a repository while running a `load` or `update`, and what source does an `update` from a registry store?

- Provisional answer: keep the `load` label for dependencies of `load` and `update`, keep their stored source, and keep the rule that listings show a source only for installs.
- Reason: the maintainer test for dependency history expects a `load` record for the dependency of a `load`. The original request asks that dependency installs be logged, which this does.
- Alternative: label them `install` with their source, following the original request's wording "install (including as a dependency)". That changes the maintainer test.
- Alternative: keep the `load` label and show the stored source in listings for any record that has one. That changes the wiki's rule that listings show `n/a` for anything other than install.
- Alternative: clear the source of records labeled `load`, as the field's description says for loads. This was not chosen, because hiding a real source loses useful history.
- `update` is not covered by any source. Provisionally it may store its registry, and listings still show `n/a` for it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Fields](../../../ontology/features/history.g5.md#history.fields).

Sources: A1 Q-15; `General/AbstractHistory.cls:20-27` ("For load and uninstall this will be empty"); `wiki/09` L76; issue #366 ("install (including as a dependency)", source "for install only"). Reconstruction check R9 asked for one provisional answer. Observed: dependencies of `load` and `update` are labeled `load` but carry a repository source (`Utils/Module.cls:63-69`). The listing shows `n/a` as the source for every record that is not `install`, including a top-level `update` from a registry (`General/History.cls:461-463`, OBS-H7). Round 1 R1-ONT-07: `History.cls` `TestWithDependencies` (L752-770) expects a `load` record for the dependency of a `load`, and the provisional answer now follows it. Native evidence: the A0 dependency record has `Action=load` and `SourceName=ipm-temp-modules-1` (`182759Z-cases-a`). Round 3 R3-09 restored the two draft options, "show the source in the listing" and "clear the source", as alternatives.


<a id="history.phases.returned-errors"></a>
### history.phases.returned-errors

Canonical: [history.phases.returned-errors](../../../ontology/features/history.g5.md#history.phases.returned-errors)

Must every phase that starts also record its end and status, including when an `<Invoke>` returns an error status instead of throwing one?

- Provisional answer: yes. The docs read a phase with no end time as a hang or crash, so an ordinary error must not look like one.
- Alternative: accept open phases for returned errors and document it.

## Committed and Success

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Phases](../../../ontology/features/history.g5.md#history.phases).

Sources: A1 Q-14; `wiki/09` L161. Confirmed product defect B2 D6: an `<Invoke>` bound to Activate that returns an error leaves the Activate phase open in a finalized error record (`Storage/Module.cls:669-671`; `VV/native/repro/activate-invoke-phase-open.cos`, run ID not listed in the B2 report). Invokes on the default Configure phase finalize Activate correctly (cases A1, A5).


Findings: [G:D6 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d6). The linked entry qualifies the caller, reader and providers.

<a id="history.committed.nested"></a>
### history.committed.nested

Canonical: [history.committed.nested](../../../ontology/features/history.g5.md#history.committed.nested)

When a caller wraps an IPM command in its own transaction, must the record survive the caller's rollback, and what should `Committed` say?

- Provisional answer: the record survives, because the original request says "if install rolls back we still need the history". `Committed` is exact only for actions started outside any caller transaction, because IPM cannot know the caller's later outcome. Record this limit.
- Reason: no source says whether wrapping IPM in a caller's transaction is supported.
- Alternative: compare the transaction level at the end with the level at the start, so any commit nested in a caller transaction reads uncommitted.
- Alternative: treat caller-held transactions as unsupported, so the record may be lost with them.

- Settled observation: the command may return success before the caller later rolls back its module data and permanent history row. The imported model proves this no-trace result under its stated domain. Published caller-transaction use does not decide the independent audit retention policy; the working assumption and alternatives above remain policy proposals.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- architecture-decision
- Expands: [Committed](../../../ontology/features/history.g5.md#history.committed).

Sources: A1 Q-05; issue #366; issue #1237 option 1; PR #1322. Round 1 R1-ONT-01 narrowed this question to caller-held transactions and added record survival. The dependency case is now settled by [survives rollback](../../../ontology/features/history.g5.md#history.survives-rollback) and [committed](../../../ontology/features/history.g5.md#history.committed). The provisional answer keeps the survival rule of issue #366, and it was checked together with those two blocks.

Native evidence: case D1 (`tstart`, `load`, `trollback`) leaves no record. One ID was allocated and no row persisted, although in flight the record read `Committed=1` and `Finalized=1`. That deviates from the provisional answer (`182832Z-cases-dg`, `184230Z-replay-fast`, `184632Z-repro-caller-tx-history-lost`). D0 (`tcommit`) conforms. Premise N-P03, that an inner commit is not durable under an outer rollback, is confirmed (`181129Z-premises`). Installer Invokes that run nested shell commands inside IPM's own load are the same case ([installer](../../../ontology/features/ipm-setup.g5.md#setup.installer)).


Findings: [G:D25 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d25). The linked entry qualifies the caller, reader and providers.

<a id="history.committed.terminal-choice-policy"></a>
### history.committed.terminal-choice-policy

Canonical: [history.committed.terminal-choice-policy](../../../ontology/features/history.g5.md#history.committed.terminal-choice-policy)

What history outcome is required when an operator commits a stopped load at the terminal?

- Working assumption: Answering "Commit" at a crash prompt is an operator error to document.
- Alternative and implications: Mark the log committed before each region a halt could commit: F-HIST-4 becomes a defect again.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [history.committed](../../../ontology/features/history.g5.md#history.committed)

Sources: [DEC-LOAD-1](../../../../playbook-uplift/decisions/00-02.md); It is an IRIS terminal choice; guarding it costs writes on every load.


<a id="history.read.filter.invalid-column"></a>
### history.read.filter.invalid-column

Canonical: [history.read.filter.invalid-column](../../../ontology/features/history.g5.md#history.read.filter.invalid-column)

What should happen to a filter on a column such as `Version_Major`, on a column that does not exist, or when the query fails?

- Provisional answer: filters on listed columns, including version parts, must work, and IPM must never silently drop a filter. When `find` cannot run its query, it reports the error instead of showing an empty log.
- Reason: a dropped filter widens `history delete`, which could then delete a whole namespace's records without `-confirm`. An empty listing with a success status hides the problem.
- Alternative: refuse an unknown column with an error, or ignore it with a warning and apply the rest.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior, security
- Expands: [Filter](../../../ontology/features/history.g5.md#history.read.filter).

Sources: A1 Q-10; `wiki/09` L112. Round 1 R1-ONT-10 added query errors in `find`. Confirmed product defect B2 D2: a column name containing `_` is dropped together with every later filter, so `find` widens (E1, E2, E8), and `history delete -DVersion_Major=99` without `-confirm` deleted every record (E4) (`182845Z-cases-ef`, `184230Z-replay-fast`, `184621Z-repro-history-filter-widened`, `184623Z-repro-history-delete-underscore`). An unknown column gives `find` an empty listing with a success status (E3), but makes `delete` fail with SQLCODE -29 (E7). Observed: `GetHistory` does not check `%SQLCODE` (`General/History.cls:132-150`). Consolidated round 3 (`20261114T032747Z-r3-delete`, ARM64), three seeded records each: `history delete -DVersion_Major=99` deleted all three though none has major 99 (G D2, now on ARM64 with exact identities); `-DSucess=0`, the misspelling used in the command's own help example (`Main.cls:832`), failed with SQLCODE -29 and deleted nothing; `-DPackage=del-b` and `-DSuccess=0` deleted exactly the matching record.


Findings: [G:D24 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d24). The linked entry qualifies the caller, reader and providers.

<a id="history.read.filter.default-modifiers"></a>
### history.read.filter.default-modifiers

Canonical: [history.read.filter.default-modifiers](../../../ontology/features/history.g5.md#history.read.filter.default-modifiers)

Should the namespace's [default modifiers](../../../ontology/concepts/settings.g5.md#settings.default-modifiers) act as `history` filters?

- Provisional answer: no. Default modifiers are meant for package changes, such as `-DNoTransaction=1`, while `history` filters are explicit requests.
- Reason: a default such as `NoTransaction` names no history column, so it would break or empty every listing.
- Alternative: apply them as filters and document it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Filter](../../../ontology/features/history.g5.md#history.read.filter).

Sources: new question from A2 OBS-H2; `Main.cls:820` ("By default, all entries will be affected"); `Main.cls:617-630` (default-modifiers help). Confirmed as D18, decision pending under this question: stored default modifiers are merged into every command, and `history` reads them as filters (`Main.cls:1067-1069`, `:4136`; `CLI.cls:815-819`). Stage C case H2 (`20261113T204152Z-stagec`): after the product's own example `default-modifiers -set -DNoTransaction=1 -DUpdateSnapshots=1`, `history find` shows no rows with a success status, and `history delete -confirm` fails with SQLCODE -29.


Findings: [G:D18 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d18). The linked entry qualifies the caller, reader and providers.

<a id="history.delete.confirm-with-filter"></a>
### history.delete.confirm-with-filter

Canonical: [history.delete.confirm-with-filter](../../../ontology/features/history.g5.md#history.delete.confirm-with-filter)

Does `-confirm` together with a filter delete every record, or only the matching ones?

- Provisional answer: only the matching ones. `-confirm` only permits a delete without a filter.
- Reason: the wiki describes it this way, while the command help says `-confirm` "will delete all records in the current namespace".
- Alternative: follow the command help.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Delete](../../../ontology/features/history.g5.md#history.delete).

Sources: A1 Q-11; `Main.cls:825`; `wiki/09` L43-44, L187; guard `Main.cls:4165-4167`.


<a id="history.delete.running-records"></a>
### history.delete.running-records

Canonical: [history.delete.running-records](../../../ontology/features/history.g5.md#history.delete.running-records)

May `history delete` or the daily [purge](../../../ontology/features/history.g5.md#history.retention) remove the record of an action that is still running, and what should that action's final write do then?

- Provisional answer: a delete or purge may remove any matching record, and the running action must still finish normally whether or not its record comes back.
- Reason: no source covers this case. Deleting is an explicit user choice, and the log must not make a package change fail.
- Alternative: skip records of running actions, or always write the final result back.

- As built, deleting an accessible active twin before final persistence leaves the row absent even when Finalize and PersistToTwin return success.
- A transaction-locked twin has a different checked schedule: delete waits, final persistence runs, the transaction releases its lock, delete completes, then temporary-object close recreates the same ID with its finalized successful result. A successful delete is not cancellation of the remaining writer.
- The [paired final-reader receipts and Verus schedule contract](../iter2/boundaries/REPORT.md) refute a general no-resurrection claim. They cover two schedules, not all lock timeouts, interleavings or cross-namespace behavior. The alternatives above remain an owner policy question.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Delete](../../../ontology/features/history.g5.md#history.delete).

Sources: new question Q-20 from reconstruction check R12; A1 3.4 last row ("unspecified"). Observed: `history delete` and the purge are not limited to finalized records (OP-HIST-17, OP-HIST-21, OBS-H12). What the running action's end copy then does depended on premise P-9, which B2 observed as N-P15 (below). Affects [retention](../../../ontology/features/history.g5.md#history.retention). Native evidence (N-P15, `181129Z-premises`): deleting the permanent record of an action before its end copy makes the record disappear completely. The end copy raises nothing and does not recreate it.


<a id="history.locked.unfinished"></a>
### history.locked.unfinished

Canonical: [history.locked.unfinished](../../../ontology/features/history.g5.md#history.locked.unfinished)

May IPM finalize, and so lock, the record of an action that never ended?

- Provisional answer: no. A finalized record is one whose action has ended, so a record without an end time stays unfinalized.
- Reason: a locked record of an unfinished action could never be corrected.
- Alternative: finalize whenever IPM releases the record, and accept locked records with no end time.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Locked](../../../ontology/features/history.g5.md#history.locked).

Sources: new question from round 1 R1-ONT-02; this page's definition of Finalized; `wiki/09` L197. Native evidence: case C2, a developer-mode load ended by `$SYSTEM.Process.Terminate`, was finalized by `%OnClose` with no end time and an open Activate phase. It deviates from the provisional answer, and the record can no longer be corrected (`182903Z-cases-c-terminate`). Observed: `PersistToTwin` sets `Finalized` without checking the end time (`General/HistoryTemp.cls:53`).


<a id="history.retention.effect"></a>
### history.retention.effect

Canonical: [history.retention.effect](../../../ontology/features/history.g5.md#history.retention.effect)

May a failure of the daily purge stop the user's command?

- Provisional answer: no. The command should still run, which is a proposed change.
- Reason: no source says a purge failure may block other commands, and the purge is housekeeping the user did not ask for in that command.
- Alternative: report the purge error and stop, as a signal that retention is broken.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Retention](../../../ontology/features/history.g5.md#history.retention).

Sources: A1 Q-13. Reconstruction check R10 narrowed it to the failure case, because [retention](../../../ontology/features/history.g5.md#history.retention) already requires deleting old records. Not run by B2: a purge failure (`Main.cls:862-865`, OBS-H12) remains a code reading. Confirmed product defect B2 D5, owned by [settings](../../../ontology/concepts/settings.g5.md#settings): `config delete HistoryRetain` restores the first value ever set, not `0` (case F0; `184628Z-repro-config-delete-retain`; also after `182514Z-suite-test-CLI`).


<a id="history.keep-on-ipm-uninstall.other-namespaces"></a>
### history.keep-on-ipm-uninstall.other-namespaces

Canonical: [history.keep-on-ipm-uninstall.other-namespaces](../../../ontology/features/history.g5.md#history.keep-on-ipm-uninstall.other-namespaces)

May uninstalling IPM in one namespace delete the in-progress records of other namespaces?

- Provisional answer: no. History belongs to each namespace.
- Alternative: accept it, because removing IPM is rare.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [IPM uninstall](../../../ontology/features/history.g5.md#history.keep-on-ipm-uninstall).

Sources: A1 Q-16. Observed: full cleanup kills `^IRIS.Temp.HistoryTempD`, which every namespace shares (`Lifecycle/Module.cls:51`, OBS-H13, P-1). Native evidence: history IRISTEMP rows form one instance-wide store, readable from another namespace (N-P06, N-P17, `181129Z-premises`). A self-uninstall during another namespace's load was not run.


<a id="history.committed.escaped-dependency"></a>
### history.committed.escaped-dependency

Canonical: [history.committed.escaped-dependency](../../../ontology/features/history.g5.md#history.committed.escaped-dependency)

Should a parent record count writes kept only by its NoJournal dependency?

- Evidence: The property says any action changes kept. N-R05-M5 leaves child data while top-level marker can remain false and the child row can be lost.
- Working assumption: Count surviving child writes in the parent outcome; retain the distinction between parent and child records.
- Alternative: Keep per-activation marker only and rely on independent child records after a repair.
- Affected claims: Committed truth, record existence and aggregate load outcome. Alias Q-26.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [history.committed](../../../ontology/features/history.g5.md#history.committed).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-26, source/native cases named in the block; [extension evidence](../extension/REPORT.md).


Findings: [G:D22 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d22). The linked entry qualifies the caller, reader and providers.

<a id="history.delete.global-errors"></a>
### history.delete.global-errors

Canonical: [history.delete.global-errors](../../../ontology/features/history.g5.md#history.delete.global-errors)

Must global delete report namespace errors other than missing permission?

- Evidence: DeleteHistoryGlobally catches all exceptions; a native unknown-column comparison distinguishes local failure from global success.
- Working assumption: Report errors that mean requested deletion was not performed.
- Alternative: Keep best-effort behavior and explicitly list skipped namespaces and errors.
- Affected claims: global delete status and namespace scope. Alias Q-27.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [history.delete](../../../ontology/features/history.g5.md#history.delete).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-27, source/native cases named in the block; [extension evidence](../extension/REPORT.md).


Findings: [G:D23 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d23). The linked entry qualifies the caller, reader and providers.

<a id="history.read.global-shape"></a>
### history.read.global-shape

Canonical: [history.read.global-shape](../../../ontology/features/history.g5.md#history.read.global-shape)

Must programmatic global history rows carry their namespace and ID?

- Evidence: CLI output carries namespace; GlobalRecords ROWSPEC omits namespace and ID, and native H10 repeats caller rows.
- Working assumption: Return identities sufficient for a caller to distinguish colliding namespace-local IDs.
- Alternative: Keep the legacy row shape and document a separate namespace-aware API.
- Affected claims: global query scope, identity and details lookup. Alias Q-28.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [history.read.api](../../../ontology/features/history.g5.md#history.read.api).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-28, source/native cases named in the block; [extension evidence](../extension/REPORT.md).


<a id="history.retention.running-policy"></a>
### history.retention.running-policy

Canonical: [history.retention.running-policy](../../../ontology/features/history.g5.md#history.retention.running-policy)

May automatic retention remove an action that has not ended?

- Evidence: The as-built quoted cutoff deletes nothing. With that defect repaired the filter uses TimeStart only; the Python reader model can then remove active rows.
- Working assumption: Retain the documented age rule while keeping deletion/finalization races explicit; no repair outcome is claimed.
- Alternative: Skip active rows and define how killed actions eventually expire.
- Affected claims: retention fix, active writers and abnormal process ends. Alias Q-29.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [history.retention](../../../ontology/features/history.g5.md#history.retention).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-29, source/native cases named in the block; [extension evidence](../extension/REPORT.md).


<a id="history.survives-rollback.repair-policy"></a>
### history.survives-rollback.repair-policy

Canonical: [history.survives-rollback.repair-policy](../../../ontology/features/history.g5.md#history.survives-rollback.repair-policy)

Which repair should preserve child history without lying about Committed?

- Evidence: Naive durable rows can mark rolled-back writes committed. The inherited bounded variants distinguish guarded close, before-image ordering, per-activation NoJournal and SIGKILL residuals; mirroring is untested.
- Working assumption: Keep current survival requirement and report the repair as undecided; no product repair applied.
- Alternative: Use one load-wide mode plus guarded close and mark ordering, or extend the repair to durably mark escaped child writes.
- Affected claims: D1 and aliases, caller rollback, crash cuts and mirrored history. Alias Q-30.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [history.survives-rollback](../../../ontology/features/history.g5.md#history.survives-rollback).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-30, source/native cases named in the block; [extension evidence](../extension/REPORT.md).


<a id="history.committed.marking.refusal-policy"></a>
### history.committed.marking.refusal-policy

Canonical: [history.committed.marking.refusal-policy](../../../ontology/features/history.g5.md#history.committed.marking.refusal-policy)

Should the no-transaction marker run only after every refusal?

- Evidence: LoadNewModule marks before manifest validation; a later refusal can keep no data yet read Committed=1 (G:D12). The property says noncommitted means nothing kept or rolled back.
- Working assumption: Keep the intent that a no-write refusal is uncommitted; classify the early marker as a defect.
- Alternative: Declare every no-transaction attempt committed from its start and change the owning I rule explicitly.
- Affected claims: validation timing, no-transaction history, D12 and requirement refusals. Alias Q-31.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/History.cls`, `src/cls/IPM/General/HistoryTemp.cls`, `src/cls/IPM/General/AbstractHistory.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
f522900e Fix 1097: Nested Tests (#1107)
ffba34ec Fix 1057: IPM cleanly uninstalls itself now (#1106)
2cef3073 feat(history): implement automatic history purge logic (#992)
```

Existing attributable evidence:

- uncertain
- agent-provided
- user-facing-behavior
- Expands: [history.committed.marking](../../../ontology/features/history.g5.md#history.committed.marking).

Sources: inherited `.g5/Verus_verification/open_questions.md` Q-31, source/native cases named in the block; [extension evidence](../extension/REPORT.md).


<a id="reinstall.version"></a>
### reinstall.version

Canonical: [reinstall.version](../../../ontology/features/install.g5.md#reinstall.version)

Does `reinstall` install the version already installed, or the latest available version?

- Provisional answer: the version already installed, as the command summary and its examples say.
- Alternative: the latest version, as the command description and the wiki say.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Reinstall](../../../ontology/features/install.g5.md#reinstall).

Sources: A1 conflict 27; summary `Main.cls:448`; examples `Main.cls:455-458`; description `Main.cls:450-451`; `wiki/02` L880; `wiki/01` L40-44. Observed: the code pins the installed version (`Main.cls:2695-2697`). [Iteration 22](../iter22/HANDOFF.md) also observes a `1.0.0` reinstall while `2.0.0` is available. This establishes the checked as-built choice, not an owner decision between the conflicting descriptions.


<a id="load.refusal.update-required.dev-bypass"></a>
### load.refusal.update-required.dev-bypass

Canonical: [load.refusal.update-required.dev-bypass](../../../ontology/features/load.g5.md#load.refusal.update-required.dev-bypass)

Should `-dev` also lift the update-required refusal?

- Provisional answer: yes. A maintainer test expects a newer version to install with `-dev`, and the original request asked IPM to point users to `-dev` or `-force`.
- Reason for doubt: the README and the `-force` help name only `-force`.
- Alternative: only `-force` lifts it, as the README says.
- Related: a `-dev` install from a [read-only repository](../../../ontology/concepts/repositories.g5.md#repo.read-only) is checked before developer mode is turned off for it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Update required](../../../ontology/features/load.g5.md#load.refusal.update-required).

Sources: A1 Q-02; test `InstallModule.cls` `TestDevMode` ("succeeds in dev mode and fails otherwise"); issue #903 by isc-eneil; README L62. Observed: the check skips `-dev` (`Utils/Module.cls:1219`), and a `-dev` install leaves the update steps pending for a later `update`. A read-only repository forces normal mode only after this check (`Utils/Module.cls:1277-1288`). Native evidence: g5 (`load -dev` of 3.x over 2.1.1) loads, consistent with the provisional answer (`182832Z-cases-dg`).


<a id="load.refusal.requirements.timing"></a>
### load.refusal.requirements.timing

Canonical: [load.refusal.requirements.timing](../../../ontology/features/load.g5.md#load.refusal.requirements.timing)

Must IPM check system requirements before it writes anything, even in developer mode or without a transaction?

- Provisional answer: yes. The manifest docs say requirements are checked "before module installation", and a module that fails them was never installable.
- Reason: keeping changes for debugging is meant for failures of a module that could be installed.
- Alternative: accept that a refused developer-mode or `-DNoTransaction=1` load keeps its saved manifest, since developer mode keeps what it did.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [Requirements](../../../ontology/features/load.g5.md#load.refusal.requirements).

Sources: A1 Q-17; `wiki/03` L260-261; issue #363 comment by isc-tleavitt. Confirmed as B2 D4, pending this question: refused `-dev`, `-DNoTransaction=1` and `-DNoJournal=1` loads keep the module record, with no classes, and read `Committed=1`. `list-installed` shows the module (cases B2, B3, B4; `182820Z-cases-b`, `184230Z-replay-fast`, `184630Z-repro-sysreq-dev-manifest-left`). B0 and B1 conform. Observed cause: the check runs after the manifest is saved and after the developer-mode first commit (`Utils/Module.cls:1251-1321`, OBS-L10).


Findings: [G:D4 — e2e reproduced](../../../Verus_verification/bugs_issues.md#g-d4). The linked entry qualifies the caller, reader and providers.

<a id="load.dependencies.local-repo.precedence"></a>
### load.dependencies.local-repo.precedence

Canonical: [load.dependencies.local-repo.precedence](../../../ontology/features/load.g5.md#load.dependencies.local-repo.precedence)

When another repository offers the same version of a dependency as the `.modules` folder, which copy should IPM install?

- Provisional answer: the copy in the older repository, as built. The changelog says ties go to the repository created first.
- Alternative: the `.modules` copy, because the code that creates the temporary repository says it should be found first and gives it the first sort position.
- Either way, a higher version wins, wherever it is.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- human-verified
- uncertain
- user-facing-behavior
- Expands: [Local repository](../../../ontology/features/load.g5.md#load.dependencies.local-repo).

Sources: CHANGELOG 0.9.x "Repo check should happen in the order to repo creation, not by repo name" (HSIEO-9269, HSIEO-9404); comment `General/TempLocalRepoManager.cls:27` and commit `2dc933dd` (temporary repository "first/last repo to be found"); `Repo/Manager.cls:150` (ties keyed by repository ID). Consolidated round 1 case TR5: an older filesystem repository and `.modules` both offer `tr5-dep` 1.0.0, and the older repository's copy is installed. TR5b and TR5c: the higher version wins from either side. The temporary repository's sort position never changed a result in these runs (surviving control `create-sort-last`). Native run `20261114T025102Z-r1-temp-repo`; model `Verus_verification/consolidated/models/temp_repo.py`. Question C:Q1 in [consolidated QUESTIONS](../QUESTIONS.md).


<a id="uninstall.dependents.scoped-policy"></a>
### uninstall.dependents.scoped-policy

Canonical: [uninstall.dependents.scoped-policy](../../../ontology/features/uninstall.g5.md#uninstall.dependents.scoped-policy)

Should a phase-scoped dependency block uninstall?

- Working assumption: A need declared only for a phase (test, verify) does not block uninstall.
- Alternative and implications: Count scoped needs: three readers would become defects.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [uninstall.dependents](../../../ontology/features/uninstall.g5.md#uninstall.dependents)

Sources: [DEC-UNIN-3](../../../../playbook-uplift/decisions/05-06.md); The question's premise was wrong: uninstall, `show` and `list-dependents` already agree on this (IRIS `r4-repo-unin3-*`).


<a id="uninstall.class-delete-error.policy"></a>
### uninstall.class-delete-error.policy

Canonical: [uninstall.class-delete-error.policy](../../../ontology/features/uninstall.g5.md#uninstall.class-delete-error.policy)

Should uninstall report an error and keep its metadata when a class cannot be deleted, or warn and remove the metadata as it does now?

- Working assumption: preserve and disclose the checked behavior pending an owner decision. Returning an error would make incomplete removal visible to scripts; retaining metadata would preserve a retry route. Earlier external effects may already have occurred.
- The source deliberately prints a warning, but no inspected help, test or change rationale settles the intended script status and metadata policy for this refusal. Register: C:Q21.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/Main.cls`, `src/cls/IPM/Utils/Module.cls`, `src/cls/IPM/Lifecycle/Base.cls`.

Pinned history:

```text
571e04a1 Fix and improve the `generate` command (#1324)
0492fea3 Add `show` command (#1315)
5d27e15b Fix 'committed' in history log (#1322)
7cae0b28 Fix two bugs with deployed code (#1317)
```

Existing attributable evidence:

- agent
- Expands: [Class deletion error](../../../ontology/features/uninstall.g5.md#uninstall.class-delete-error).

[Scoped comparison](../../../Verus_verification/consolidated/iter11/deletion.rs): `refuse_on_error` preserves the successful neighbor for this single class; it is not an implemented product repair or compensation for earlier effects. No owner decision is inferred.


- uncertain

<a id="update.steps.shared-ownership"></a>
### update.steps.shared-ownership

Canonical: [update.steps.shared-ownership](../../../ontology/features/update.g5.md#update.steps.shared-ownership)

Should two modules be allowed to share an update-step class and method, or should the second registration fail?

- The checked implementation moves the single record's home and retains its completion. One module's uninstall can therefore remove a shared completion record still relevant to another installed module.
- Working assumption: authors use distinct update packages. Alternatives are to refuse a conflicting home, key records by module, or retain explicit shared owners. Each changes registration and deletion behavior; concurrent writers need a separate check.
- Evidence: ordinary activation, both step stores and deletion readers in iteration 9. No owner decision is inferred. Register: C:Q19.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/Update/VersionBase.cls`, `src/cls/IPM/Storage/Module.cls`, `tests/integration_tests/Test/PM/Integration/Update.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
82ea5166 Improve ORAS error reporting (#1284)
```

Existing attributable evidence:

- agent
- Expands: [Step records](../../../ontology/features/update.g5.md#update.steps.records).

Observed sequential sharing is not owner approval. Proposed repairs and cross-module concurrency remain unchecked. [Record contract](../../../Verus_verification/consolidated/iter9/ownership.rs).


- uncertain

<a id="update.steps.dev-seeding-policy"></a>
### update.steps.dev-seeding-policy

Canonical: [update.steps.dev-seeding-policy](../../../ontology/features/update.g5.md#update.steps.dev-seeding-policy)

Should a developer-mode install seed update-step records?

- Working assumption: A developer-mode install or load seeds update steps like any install; a later update runs only newer steps.
- Alternative and implications: Keep "a dev install leaves all steps due" (so `load -dev` + update exercises every step): F-INST-8 withdrawn.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/Update/VersionBase.cls`, `src/cls/IPM/Storage/Module.cls`, `tests/integration_tests/Test/PM/Integration/Update.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
82ea5166 Improve ORAS error reporting (#1284)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [update.steps](../../../ontology/features/update.g5.md#update.steps)

Sources: [DEC-INST-2](../../../../playbook-uplift/decisions/03-04.md); Update help: steps run "from the current version to the newer version"; `Update.cls#L467` asserts seeded steps do not rerun.


Findings: [P:F-INST-8 — e2e reproduced](../../../../playbook-uplift/BUGS.md#p-f-inst-8). The linked entry qualifies the caller, reader and providers.

<a id="update.steps.fresh-seeding-policy"></a>
### update.steps.fresh-seeding-policy

Canonical: [update.steps.fresh-seeding-policy](../../../ontology/features/update.g5.md#update.steps.fresh-seeding-policy)

Should a dependency first installed by update run its old steps or seed them?

- Working assumption: A module that `update` installs for the first time has its steps seeded, as on a fresh install.
- Alternative and implications: Run them all (if the new module adopts data an older parent created): F-INST-10 withdrawn.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/Update/VersionBase.cls`, `src/cls/IPM/Storage/Module.cls`, `tests/integration_tests/Test/PM/Integration/Update.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
82ea5166 Improve ORAS error reporting (#1284)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [update.steps](../../../ontology/features/update.g5.md#update.steps)

Sources: [DEC-INST-8](../../../../playbook-uplift/decisions/03-04.md); Update help's "from the current version"; issue #550 separates updates from fresh installs.


Findings: [P:F-INST-10 — e2e reproduced](../../../../playbook-uplift/BUGS.md#p-f-inst-10). The linked entry qualifies the caller, reader and providers.

Supporting context: CHANGELOG #1128 motivates the alternative, without deciding this fresh-install policy.

<a id="update.steps.busy-policy"></a>
### update.steps.busy-policy

Canonical: [update.steps.busy-policy](../../../ontology/features/update.g5.md#update.steps.busy-policy)

Should an update whose step lock is busy fail or report success?

- Working assumption: If the update lock is busy, `update` fails with an error that says so; it never reports success.
- Alternative and implications: Wait longer, then fail: same contract. Only "keep success" is ruled out. If it were chosen, F-INST-1 (High) would become a documentation defect.
- This assumption guides review; it is not a maintainer decision. An explicit owner policy or an attributable source addressing this exact case would settle it.

Disposition: open. The working assumption and competing outcomes above remain separate from an owner decision. A maintainer statement addressing the exact competing outcomes, or a directly applicable authoritative specification, would settle policy. A new native check can settle only the unobserved behavior.

Inspected production/test paths: `src/cls/IPM/General/Update/VersionBase.cls`, `src/cls/IPM/Storage/Module.cls`, `tests/integration_tests/Test/PM/Integration/Update.cls`.

Pinned history:

```text
5d27e15b Fix 'committed' in history log (#1322)
ce1bd00f Fix three ORAS bugs (#1305)
b34e6a67 Add MAXLEN for all properties and a test to check (#1309)
82ea5166 Improve ORAS error reporting (#1284)
```

Existing attributable evidence:

- agent
- uncertain
- user-facing-behavior
- Expands: [update.steps](../../../ontology/features/update.g5.md#update.steps)

Sources: [DEC-INST-6](../../../../playbook-uplift/decisions/03-04.md); The PR #887 authors wrote `TestSimultaneousUpdateFails` asserting exactly this (commit `d6d346728`), removed it four days later (`e232e5056`) and put concurrency out of scope; the update help promises the steps run.


Findings: [P:F-INST-1 — e2e reproduced](../../../../playbook-uplift/BUGS.md#p-f-inst-1). The linked entry qualifies the caller, reader and providers.

## Opening-arm decision audit

<a id="dec-sys-1"></a>
### DEC-SYS-1

Disposition: method/verification observation; not an unresolved IPM policy decision.

| DEC-SYS-1 | 00 | The playbook says a term may be used if an ancestor or an earlier same-level statement defines it. Should a definition by an earlier sibling of an ancestor (lexical scope) count? The index script assumes yes, so SYS-D* definitions reach every module. | (A) lexical scope, as implemented; (B) strict reading: repeat or chain shared definitions under each module root. | Human | resolved (assumption) |

<a id="dec-hist-1"></a>
### DEC-HIST-1

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-HIST-1 | 02 | When a parent load rolls back, should the history records of the dependency loads it ran survive? Today they vanish with the rollback (F-HIST-1), although the class comment says history is kept apart from rollbacks. | (A) keep them: write dependency records outside the parent's transaction (for example from IRISTEMP after the parent's outcome is known); (B) accept: a dependency record exists only when the dependency's changes were kept, and say so in the docs. Recommendation: A, because the top-level record already follows A. Proved design (models/verus/load_general.rs, `fix`): re-persist pending dependency rows after the parent's rollback and at the next command after a crash, with Committed taken from the dependency's installed state (keeping the logged Committed was shown dishonest by the explorer). Sub-question: a dependency that loaded and was then undone by the parent then reads Success = 1, Committed = 0; is that the wanted reading, or should Success also be cleared? | Human | resolved (evidence) |

<a id="dec-hist-2"></a>
### DEC-HIST-2

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-HIST-2 | 02 | How should the Committed flag survive a crash after a developer-mode load has committed its manifest (F-HIST-2)? | (A) persist the twin row right after the first TCOMMIT (Utils/Module.cls#L1311-L1313, before the next TSTART) so the durable row says Committed = 1; (B) keep the IRISTEMP record when %OnClose runs at $TLEVEL > 0 and reconcile it at the next command; (C) accept and document. Recommendation: A, a two-line change at a point with no transaction open. | Human | resolved (evidence) |

<a id="dec-hist-3"></a>
### DEC-HIST-3

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-HIST-3 | 02 | Must a record name the module whenever IPM has read its manifest (refusals, crashes before SetName)? Today such records have an empty package and version 0.0.0 (F-HIST-5). The property comment only promises a name is "not necessarily required, e.g. when loading a nonexistent directory". | (A) yes: set the name from the manifest read in LoadNewModule before the guards; (B) no: the name is best effort. | Human | resolved (evidence) |

<a id="dec-hist-4"></a>
### DEC-HIST-4

Disposition: open; working assumption.

| DEC-HIST-4 | 01, 02 | What should IPM do when a command starts inside a caller transaction (F-HIST-3)? The load and its history then share the caller's fate. | (A) refuse with an error when $TLEVEL > 0; (B) allow, and document that history is not kept if the caller rolls back; (C) write the record outside the caller's transaction. | Human | resolved (assumption) |

Canonical: [history.committed.nested](../../../ontology/features/history.g5.md#history.committed.nested).

<a id="dec-hist-5"></a>
### DEC-HIST-5

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-HIST-5 | 01, 02 | Does Committed describe only database changes? File-system effects are never rolled back: an archive load deletes and recreates `<ModuleRoot>/<name>/<version>/` before any check (Main.cls#L2507-L2513), and FileCopy writes files. | (A) Committed covers database writes only; document it; (B) Committed should also turn on when files change. | Human | resolved (evidence) |

<a id="dec-hist-6"></a>
### DEC-HIST-6

Disposition: open; working assumption.

| DEC-HIST-6 | 02 | What should happen when `history delete` removes the record of a command that is still running? By the IRIS contract (re-saving an object re-creates its row, O:env-resave-after-rollback) the closing %OnClose would write it back. Not observed on IRIS for IPM. | (A) the delete wins; (B) a running record cannot be deleted; (C) accept that it reappears. | Human | resolved (assumption) |

Canonical: [history.delete.running-records](../../../ontology/features/history.g5.md#history.delete.running-records).

<a id="dec-load-1"></a>
### DEC-LOAD-1

Disposition: open; working assumption.

| DEC-LOAD-1 | 01 | A crash at an interactive terminal asks the operator to commit or roll back the open transaction. Committing keeps a half-done load that history marks Committed = 0 (F-HIST-4). Is that an operator error to document, or should IPM guard against it? | (A) document; (B) mark the log committed before each region a terminal halt could commit; (C) run loads in a job. | Human | resolved (assumption) |

Canonical: [history.committed.terminal-choice-policy](../../../ontology/features/history.g5.md#history.committed.terminal-choice-policy).

<a id="dec-load-2"></a>
### DEC-LOAD-2

Disposition: open; conflicting canonical question takes precedence over the resolved label.

| DEC-LOAD-2 | 01 | Should a plain `load` (no `-dev`, no `-nodev`) over a module installed in developer mode keep developer mode? It does today (O:ipm-dev-then-plain-load); the help text only says `-nodev` disables it. | (A) keep, as now, and document; (B) a plain load should switch to normal mode. | Human | resolved (evidence) |

Canonical: [dev-mode.sticky](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.sticky).

<a id="dec-sem-1"></a>
### DEC-SEM-1

Disposition: method/verification observation; not an unresolved IPM policy decision.

| DEC-SEM-1 | 3.3a | `^IRIS.Temp.IPM.LoadedResourceD` (IPM working data in IRISTEMP) is still set after commands end (O:ws-residue-after-commands). Is it a deliberate per-instance cache, or should each command clear it? | (A) deliberate cache: document its lifetime (until IRIS restarts); (B) clear it at the end of each command. | Human | resolved (assumption) |

<a id="dec-sem-2"></a>
### DEC-SEM-2

Disposition: method/verification observation; not an unresolved IPM policy decision.

| DEC-SEM-2 | 3.3a | Uninstalling a module that has a web application writes compiled-code globals in IRISSYS (`^oddDEF`, `^oddCOM`, `^rOBJ`; O:ws-uninstall-webapp). Is a system-database code change an accepted side effect of removing the web application, or a frame violation to investigate? | (A) accept: IRIS regenerates system class metadata when a security application is deleted; document it; (B) investigate which classes change and why. | Human | resolved (assumption) |

<a id="dec-inst-1"></a>
### DEC-INST-1

Disposition: open; conflicting canonical question takes precedence over the resolved label.

| DEC-INST-1 | 03-install-update | Is the developer-mode bypass of the update-package guard (install -dev moves to a newer version without running or seeding update steps) part of the public contract? README L62 names only -force; test TestDevMode (InstallModule.cls L178) asserts the bypass; issue #550 mentions dev environments. Observed on IRIS: `install -dev update-simple 3.5.0` over 2.1.1 succeeds and leaves the V3 step with no record at all (nc-inst-2-dev-install-then-update). Review round: the bypass needs a typed -dev; a module merely stored in developer mode does not bypass it (after `load -dev` 1.0.0, a plain `load` of 2.0.0 with an update package is refused with the "use update" message: nc-inst-fix-stored-dev-load-newer), because the guard reads only the DeveloperMode parameter (Utils/Module.cls#L1219). Cell INST-C33 now names this decision. | A) Keep it and document it in README and the -dev help. B) Remove it so only -force bypasses the guard. | human | resolved (evidence) |

Canonical: [load.refusal.update-required.dev-bypass](../../../ontology/features/load.g5.md#load.refusal.update-required.dev-bypass).

<a id="dec-inst-2"></a>
### DEC-INST-2

Disposition: open; working assumption.

| DEC-INST-2 | 03-install-update | Should an install in developer mode seed update steps? Today it does not (Lifecycle/Module.cls L122), so a later update runs every step from V1, including steps older than the installed version. Observed on IRIS: a dev install creates no step records (nc-inst-2b-dev-fresh-then-update, 0 rows); the next update ran every V1 and V2 step (nc-inst-2b) and, after a dev install of 3.5.0 over 2.1.1, ran the new V3 step plus V1 steps whose records did not exist, rerunning a seeded V1 step after them (nc-inst-2-dev-install-then-update). That update also cleared developer mode. Proved in models/verus/update_steps.rs: an OK install in developer mode leaves the step records unchanged (inst_refines), so the next update schedules the steps of every class, older ones included, while a non-developer install leaves none due (inst_witness_dev_install_leaves_steps_due). Review round: seeding tests the module's stored DeveloperMode (Lifecycle/Module.cls#L122), so a plain `load` of a module stored in developer mode seeds nothing either (nc-inst-fix-stored-dev-plain-load: 0 step rows; modelled in update_steps.rs). Cell INST-C15 now names this decision. | A) Seed on dev installs too, like non-dev installs. B) Keep as is and document that dev installs leave all steps due. C) Seed only steps of versions at or below the installed one. | human | resolved (assumption) |

Canonical: [update.steps.dev-seeding-policy](../../../ontology/features/update.g5.md#update.steps.dev-seeding-policy).

<a id="dec-inst-3"></a>
### DEC-INST-3

Disposition: open; working assumption.

| DEC-INST-3 | 03-install-update | Should install refuse to replace a needed module that is in developer mode, as it does for the named module? Today the dependency is silently replaced and loses developer mode (Utils/Module.cls L109, L308). Observed on IRIS: with complex-c 2.0.0 loaded -dev, `install complex-b 2.0.0` succeeds and leaves complex-c at 3.0.0 without developer mode, with no message (nc-inst-8-dev-dependency-replaced). | A) Refuse, naming the dev-mode dependency. B) Allow but warn. C) Keep silent replacement. | human | resolved (assumption) |

Canonical: [dev-mode.overwrite-guard.dependency-policy](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.dependency-policy).

<a id="dec-inst-4"></a>
### DEC-INST-4

Disposition: open; conflicting canonical question takes precedence over the resolved label.

| DEC-INST-4 | 03-install-update | What does reinstall fetch? Summary (Main.cls L448) and code: the installed version. Description (L450): "from the latest version available in a configured repository". Observed on IRIS: with complex-c 2.0.0 installed and 3.0.0 in the registry, `reinstall complex-c` prints "Reinstalling complex-c 2.0.0" and keeps 2.0.0 (nc-inst-9-reinstall-version). | A) Installed version (fix the description). B) Latest available version (change the code). | human | resolved (evidence) |

Canonical: [reinstall.version](../../../ontology/features/install.g5.md#reinstall.version).

<a id="dec-inst-5"></a>
### DEC-INST-5

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-INST-5 | 03-install-update | What delivery promise do update steps have? Rollback undoes step records, so steps that worked in a failed run rerun; effects outside the database (files, $INCREMENT, other systems) can repeat; a kill between the step and its record save reruns that step. Observed on IRIS (nc-inst-4-nondev-failed-update, nc-inst-4b-nondev-retry): after a failed non-dev update the ordinary global set by the step that worked is rolled back, but its $INCREMENT counter and file line stay; the retry runs that step again (counter 2, two file lines). In developer mode (nc-inst-4c, nc-inst-4d) the worked step's record is kept and the retry skips it. Proved: a transactional update whose second step fails rolls the records back while the first step's effects outside the database stay, and the retry schedules that step again (inst_witness_failed_update_repeats_effects). Review round: INST-B67 now states the intent the seed asks about (a step's effects happen once) with this decision open and `O!:nc-inst-4b-nondev-retry`; the witness is cited in Notes, not as P. | A) At-least-once; require idempotent steps and document it in VersionBase. B) Exactly-once for database effects only (document the boundary). C) Add a stronger mechanism (per-step commit or idempotency key). | human | resolved (evidence) |

<a id="dec-inst-6"></a>
### DEC-INST-6

Disposition: open; working assumption.

| DEC-INST-6 | 03-install-update | What should update do when the update lock is busy? Today it reports success and runs no step (finding F-INST-1). PR #887 put preventing concurrent updates out of scope. Observed on IRIS: with the lock held by another process, update returned OK after 3.3 s, printed nothing, moved the module to 2.1.1 and ran no step (nc-inst-3-update-lock-busy); a later update to the same version ran them (nc-inst-3b-update-after-lock-free). Proved: with the lock timing out, update ends OK with the new version and no step run (inst_witness_busy_lock_reports_success, F-INST-1). | A) Fail with a clear error. B) Wait longer, then fail. C) Keep skipping but print a warning and leave the command status not OK. | human | resolved (assumption) |

Canonical: [update.steps.busy-policy](../../../ontology/features/update.g5.md#update.steps.busy-policy).

<a id="dec-inst-7"></a>
### DEC-INST-7

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-INST-7 | 03-install-update | Are primary-only update steps promised to run when a mirror member becomes primary at this revision? VersionBase.cls L18 says so; no code does it; PR #887 lists mirrored updates as out of scope and the wiki update guide (not read here) is reported by the seed to call mirroring future work. Native check #20 not run: the IRIS container is not a mirror member (nc-inst-20-no-mirror). | A) Not promised at this revision; fix the VersionBase comment. B) Promised; implement a become-primary hook and mirror tests. | human | resolved (evidence) |

<a id="dec-inst-8"></a>
### DEC-INST-8

Disposition: open; working assumption.

| DEC-INST-8 | 03-install-update | When an update brings in a module that was not installed before, should its update steps run (today, all of them) or be seeded like a fresh install? Observed on IRIS: `update update-deps-added 2.1.1` brought in update-simple 1.2.1 and ran its V1 step (start time set); a plain install of update-simple 1.2.1 seeds it instead (nc-inst-6-dep-added-by-update, nc-inst-6b-fresh-install-control). Proved: with no step records, an update that ends OK runs every step this server may run (inst_fresh_update_runs_all). | A) Seed them, as a fresh install would. B) Run them all (current behaviour), and document it. | human | resolved (assumption) |

Canonical: [update.steps.fresh-seeding-policy](../../../ontology/features/update.g5.md#update.steps.fresh-seeding-policy).

<a id="dec-dep-1"></a>
### DEC-DEP-1

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-DEP-1 | 04-dependencies | Must dependency resolution find a solution whenever one exists? The resolver is greedy (newest first, one-level check, no backtracking past one level), so it can fail on solvable inputs; the method comment says "with backtracking". Observed on IRIS: `install nc-dep-root` (A ^1, B ^1; A 1.1.0 needs C ^2, A 1.0.0 and B need C ^1) fails with a clash on nc-dep-c naming nc-dep-a 1.1.0; installing nc-dep-a 1.0.0 first makes the same install succeed (nc-dep-11-greedy-resolution, nc-dep-11b-solvable-with-pin). Proved in models/verus/dep_resolve.rs: on the nc-dep-11 instance a sound choice exists and the (deterministic) resolver fails (dep_witness_greedy_rejects_solvable, dep_deterministic). Explored (models/explorer/inst/check.py, dep-explore-completeness): 2,040 of 70,656 small instances are solvable but fail; some fail because the quick check ignores the installed-version floor. A resolution that ends Done is always sound (dep_refines; 0 of 59,776 explored). Review round: DEP-B13 now states the intent ("if some choice of versions meets every requirement, IPM finds one") with this decision open and `O!:nc-dep-11-greedy-resolution`; the witness is cited in Notes, not as P. A failure is now proved to name a reason (a loop of recorded requirements, or a requirement with no version allowed by it and the requirements already recorded; DEP-B54); those recorded requirements include earlier greedy choices, which is why option B is not met. | A) Greedy is the contract; fix the comment and document that pins may be needed. B) Complete resolution is the contract; add real backtracking. | human | resolved (evidence) |

<a id="dec-dep-2"></a>
### DEC-DEP-2

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-DEP-2 | 04-dependencies | Should a failed install leave a lock file behind? The file is written after dependencies load but before the module's own Activate, outside the transaction. Observed on IRIS: `install -create-lockfile nc-dep-lf-fail` whose Activate fails returns an error, rolls back both modules, and leaves module-lock.json (listing nc-dep-lf-base) in the module folder (nc-dep-15-lockfile-failed-install). | A) Write it only after the whole install succeeds. B) Remove it on failure. C) Keep as is and document. | human | resolved (evidence) |

<a id="dec-dep-3"></a>
### DEC-DEP-3

Disposition: open; working assumption.

| DEC-DEP-3 | 04-dependencies | What should -create-lockfile do when a needed module has no registry (it was added with load from a folder)? Today no file is written and an error is printed; the install succeeds. Observed on IRIS: with lock-mod-a loaded from a folder, `install -create-lockfile lock-mod-e-1-dep-0-transient` succeeds, prints "Error creating lock file ... Tried getting repo "" but none found", and leaves the existing module-lock.json untouched (nc-dep-14-lockfile-folder-dependency). | A) Write the entry with an empty registry. B) Fail the command. C) Keep as is. | human | resolved (assumption) |

Canonical: [deps.lockfile.local-supplier-policy](../../../ontology/features/dependencies.g5.md#deps.lockfile.local-supplier-policy).

<a id="dec-unin-1"></a>
### DEC-UNIN-1

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-UNIN-1 | 05 | When `uninstall M` is blocked because a dependent is installed (no `-force`), should the command report failure? Help (Main.cls#L486-L487) says uninstall "will be prevented"; the code comment (Lifecycle/Base.cls#L276) says "return an error status"; the code returns OK, prints `Clean SUCCESS` and logs a successful uninstall; PublishModuleWithDeps.cls#L25-L26 asserts that success. See F-UNIN-6. | (a) Fail with an error that names the dependents, and log the uninstall as failed. (b) Keep success, but log it as skipped. (c) Keep current behaviour and document it. | human | resolved (evidence) |

<a id="dec-unin-2"></a>
### DEC-UNIN-2

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-UNIN-2 | 05 | What should the Clean phase do when run on IPM itself (`zpm clean`)? Code runs the IPM-only block at every level and prints "WARNING: N module(s) orphaned" while removing nothing (Lifecycle/Module.cls#L27-L81); ExecutePhases also treats it as a self-uninstall (Storage/Module.cls#L631). | (a) Refuse Clean on IPM. (b) Run it, but only warn when IPM is really removed (Clean.Level>0). (c) Treat it as uninstall of IPM. | human | resolved (evidence) |

<a id="dec-unin-3"></a>
### DEC-UNIN-3

Disposition: open; working assumption.

| DEC-UNIN-3 | 05 | Should phase-scoped needs (for example a test-only dependency) block uninstall? Uninstall counts every module whose full graph contains M, in any phase (Utils/Module.cls#L615); `show` lists only unscoped direct dependents (ModuleSummary.cls#L37). The two read-backs disagree. | (a) Count scoped needs (current uninstall). (b) Ignore scoped needs. (c) Count them but say so in `show`. | human | resolved (assumption) |

Canonical: [uninstall.dependents.scoped-policy](../../../ontology/features/uninstall.g5.md#uninstall.dependents.scoped-policy).

<a id="dec-repo-1"></a>
### DEC-REPO-1

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-REPO-1 | 06 | When `publish` (or `unpublish` without a prefix) names no registry and no registry holds the publish flag, which registry is used? Help (Main.cls#L56-L57): "saves that artifact to the repository for which deployment is enabled". Code (Remote/Definition.cls#L117-L129, Oras/Definition.cls#L143-L155) falls back to a registry with a token, then one with user and password, then any enabled remote or ORAS registry, which can be the public community registry. See F-REPO-3. | (a) Fail when no registry has the publish flag. (b) Keep the fallback order and document it. (c) Fall back only to registries with credentials. | human | resolved (evidence) |

<a id="dec-repo-2"></a>
### DEC-REPO-2

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-REPO-2 | 06 | What may IPM still do with a registry that is not enabled? Help: "-disable: specifies that it can not be used." Install lookups (Manager.cls#L69) and unpublish (PublishManager classes) refuse it; `search` lists it (Main.cls#L2047, Utils/Module.cls#L916) and `publish -r R` sends to it (Lifecycle/Base.cls#L1598). See F-REPO-7. | (a) Refuse it everywhere. (b) Allow read-only listing (search) but refuse publish. (c) Allow anything that names it explicitly. | human | resolved (evidence) |

<a id="dec-repo-3"></a>
### DEC-REPO-3

Disposition: open; working assumption.

| DEC-REPO-3 | 06 | Should changing the address of an ORAS registry clear its stored credentials, as a remote registry does (Remote/Definition.cls#L81-L86)? ORAS OnConfigure (Oras/Definition.cls#L87-L141) keeps them, so they are sent to the new host. | (a) Clear them, as for remote. (b) Keep them, and warn. (c) Keep them silently (current). | human | resolved (assumption) |

Canonical: [repo.oras.credentials-change-policy](../../../ontology/concepts/repositories.g5.md#repo.oras.credentials-change-policy).

<a id="dec-repo-4"></a>
### DEC-REPO-4

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-REPO-4 | 06 | Should `config set` or `config get` with an unknown key, or `config set` with an invalid value (for example a bad TestReportFormat), make the command fail? Code prints "Config key ... not found" or the error text and returns OK (Main.cls#L2063-L2093 ignores the status of `UpdateOne`). Changelog #1115 treated "printing the error and reporting success" as a bug for unpublish. See F-REPO-8. | (a) Return an error status. (b) Keep printing only. | human | resolved (evidence) |

<a id="dec-repo-5"></a>
### DEC-REPO-5

Disposition: open; working assumption.

| DEC-REPO-5 | 06 | Should `unpublish R/M V` in an ORAS registry also delete the platform-specific copies of a deployed module (tags `<version><sep><platform>`)? Code deletes only the plain version tag (Oras/PublishService.cls#L72-L81); deployed copies are pushed with a platform suffix (#L43-L48). | (a) Delete every tag of that version. (b) Require the platform to be named. (c) Keep current behaviour and document it. | human | resolved (assumption) |

Canonical: [repo.oras.unpublish-platform-policy](../../../ontology/concepts/repositories.g5.md#repo.oras.unpublish-platform-policy).

<a id="dec-repo-6"></a>
### DEC-REPO-6

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-REPO-6 | 06 | Is storing registry passwords and tokens as plain text in the namespace (`^IPM.Repo.DefinitionD`, Http/Definition.cls#L17-L21) acceptable? `-password-env` and `-token-env` (changelog #1178) protect only the command line; the value is still stored. | (a) Accept and document. (b) Store a reference to the environment variable instead of the value. (c) Encrypt at rest or use IRIS credential storage. | human | resolved (evidence) |

<a id="dec-nsp-1"></a>
### DEC-NSP-1

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-NSP-1 | 07-namespace | A namespace gets IPM code and repo settings by mapping, then gets its own IPM (installer or `load`). What should happen to the repo settings mapping? | (a) Remove it with the code mapping, since README#L77 says repos are only mapped together with code from the same namespace. (b) Keep it (current code: installer touches only `%IPM` package/routine mapping, Installer.cls#L25-29). Note that afterwards `unmap -ns X` reports "No mapping found" (Main.cls#L3968) so the repo mapping can no longer be removed by `unmap`. Native (N7): after the installer ran in NCD (code and repos mapped from USER), NCD kept its repo mapping and also its code mapping, and `unmap -ns NCD` then said "No mapping found for NCD" and removed nothing (O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-N7-unmap-after-local-install; F-NSP-5). | human (IPM maintainers) | resolved (evidence) |

<a id="dec-nsp-2"></a>
### DEC-NSP-2

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-NSP-2 | 07-namespace | May IPM be installed locally in a namespace that currently gets IPM by mapping? | (a) Yes: help for `enable -map -globally` says "Namespace-specific installation will override this" (Main.cls#L743) and the installer / `load` have no guard. (b) No: `install`, `reinstall` and `uninstall` of `zpm` refuse with "Cannot install 'zpm' in namespace X because it is mapped from namespace Y" (Main.cls#L2534-2539). The two paths disagree. Native (N7): `install zpm` in NCD was refused with the mapped-from error; the installer run returned OK but did not give NCD its own code: the mappings to USER stayed, so the install compiles through them into USER's database (re-point probe), while NCD gained a `zpm` record (O:nc-nsp-N7-install-zpm-mapped, O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-installer-repoint; F-NSP-5). | human (IPM maintainers) | resolved (evidence) |

<a id="dec-nsp-3"></a>
### DEC-NSP-3

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-NSP-3 | 07-namespace | When `enable -map` (or `-community`) reaches a namespace that still holds legacy ZPM module records, should those records be migrated? | (a) Yes, run the same `MigrateZPMToIPM` that an IPM load runs (Migration.cls#L6-35). (b) No (current code: EnableIPM never calls RunAll, Main.cls#L3611-3708); the operator must re-load IPM. No doc covers this case. Native: not run (no legacy ZPM in these containers). Side fact: `MigrateRepoDefinitions` returns OK when there is nothing to copy (O:nc-nsp-migrate-repo-defs-success). | human (IPM maintainers) | resolved (evidence) |

<a id="dec-nsp-4"></a>
### DEC-NSP-4

Disposition: open; working assumption.

| DEC-NSP-4 | 07-namespace | If one of the three steps of `enable -community` fails, should the command fail? | (a) Fail with an error status so scripts (`zpm "...":1`) stop. (b) Keep current behaviour: print the error in red and continue, returning success (Main.cls#L3565-3574). Help and README are silent. Native (N4, repo extent lock held by another job): each of the three steps printed a red error, the next step still ran, and the command returned OK (O:nc-nsp-N4-community-step-fails). | human (IPM maintainers) | resolved (assumption) |

Canonical: [namespace.enable.community-failure-policy](../../../ontology/concepts/namespaces.g5.md#namespace.enable.community-failure-policy).

<a id="dec-nsp-5"></a>
### DEC-NSP-5

Disposition: open; working assumption.

| DEC-NSP-5 | 07-namespace | In developer mode, a load refused by a system need (e.g. `SYSNamespace`) leaves the module's manifest record committed. Is that intended? | (a) Yes: `-dev` deliberately keeps the manifest so it can be fixed (Utils/Module.cls#L1309-1314, help Main.cls#L306 "transactions will not be rolled back"). (b) No: changelog #106 says the rule "prevents installation in non-%SYS namespaces"; move `CheckSystemRequirements` (L1321) before the dev-mode commit (L1311). Native (N8): plain `load` refused and left no record; `load -dev` refused with the same error but `sys-ns-module` 1.0.0 stayed, listed, with developer mode on (O:nc-nsp-N8-sysns-dev). | human (IPM maintainers) | resolved (assumption) |

Canonical: [load.refusal.requirements.timing](../../../ontology/features/load.g5.md#load.refusal.requirements.timing).

<a id="dec-nsp-6"></a>
### DEC-NSP-6

Disposition: open; working assumption.

| DEC-NSP-6 | 07-namespace | Two namespaces with a zpm record share one IPM database (IPM's verify-phase namespace USER-VERIFY, or an installer run in a mapped namespace). Which one is the source of the namespaces mapped to that database? | (a) The namespace whose own routine database it is (USER for USER's database); a namespace whose code comes from another namespace's database is never a source. (b) Current code: the last one by name (Main.cls#L1628-L1654), so `enable -map -repos` from USER skips every target and `version` names USER-VERIFY (F-NSP-7). Native: O:nc-nsp-model-mapinfo, O:nc-nsp-model-repos-premapped. | human (IPM maintainers) | resolved (assumption) |

Canonical: [namespace.enable.source-ambiguity-policy](../../../ontology/concepts/namespaces.g5.md#namespace.enable.source-ambiguity-policy).

<a id="dec-res-1"></a>
### DEC-RES-1

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-RES-1 | 08-resources | Should a namespace switch done inside `exec` persist after the command? | (a) No, isolate (PR #996, changelog 0.10.6 at CHANGELOG.md#L142, code `new $namespace` Main.cls#L2435); then the help example "set the namespace to MYAPP" (Main.cls#L515-516) is stale. (b) Yes, as the help example shows. Native (N16): the switch holds inside the command (INSIDE:%SYS) and ends with it; the caller stays in USER (O:nc-res-N16-exec-namespace-v2). | human (IPM maintainers) | resolved (evidence) |

<a id="dec-res-2"></a>
### DEC-RES-2

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-RES-2 | 08-resources | What is the JSON format of a `run-from-file` command file? | (a) A root object with a `commands` array (help example Main.cls#L364-404, code Main.cls#L2003-2009). (b) A single root array, as the help description says (Main.cls#L352). The code rejects (b). Native (N17): a root array is refused ("Expected JSON object at root level"); a `commands` object runs (O:nc-res-N17-run-from-file-v2). | human (IPM maintainers) | resolved (evidence) |

<a id="dec-res-3"></a>
### DEC-RES-3

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-RES-3 | 08-resources | Packaging a module that has `Deploy="true"` classes runs MakeDeployed in the current namespace, which removes those classes' source even in developer mode. Is that intended? | (a) Yes: the verbose message says "unless packaging, in which case items WILL be deployed" (Base.cls#L1837). (b) No: packaging should deploy into the artifact only and leave the developer's namespace editable. Native (N12): `package` of a `-dev` module made DeployMapA.First Deployed=1 in the namespace and its method source unreadable (O:nc-res-N12b-package-dev). | human (IPM maintainers) | resolved (evidence) |

<a id="dec-sync-1"></a>
### DEC-SYNC-1

Disposition: open; working assumption.

| DEC-SYNC-1 | 09-sync | Should a failed load or compile outside `sync` (`compile`, `load -dev`, `reload`) flag the files it put on the server, as a failed sync does? | (a) Yes: sync help promises changes "since the last load/sync" (Main.cls#L111), and issue #1224's expected behaviour is that a reverted file is reloaded. (b) No: retry marks are a sync-only mechanism (Pipeline.cls#L210-223); the operator should run `compile` again. Native (N18): a failed `compile` left SyncFailed=0 and the old hash; after an exact-byte revert `sync` said "Nothing to sync" and the broken class stayed (O:nc-sync-N18-compile-fail-revert). | human (IPM maintainers) | resolved (assumption) |

Canonical: [devtools.sync.failed-import-policy](../../../ontology/features/developer-tools.g5.md#devtools.sync.failed-import-policy).

<a id="dec-sync-2"></a>
### DEC-SYNC-2

Disposition: inherited source-supported interpretation; not new owner approval.

| DEC-SYNC-2 | 09-sync | When a module has no baseline (after `load -dev -skip-sync`, or from before sync existed), should the first `sync` load files edited since the load? | (a) Yes: help for `-skip-sync` says "The next sync of the module records them instead" (Main.cls#L315), and sync help promises changes since the last load. (b) No (current, and asserted by Sync.cls#L126-133 for the pre-feature case): the first sync only stamps; edits made before it are absorbed. Native (N19): after `load -dev -skip-sync` and an edit, the first sync only set the baseline, the second said "Nothing to sync", and the server kept the old code (O:nc-sync-N19-skip-sync-first-sync). | human (IPM maintainers) | resolved (evidence) |

<a id="dec-sync-3"></a>
### DEC-SYNC-3

Disposition: open; working assumption.

| DEC-SYNC-3 | 09-sync | Should `sync` with no module name return an error status when any module fails? | (a) Yes, like `sync <module>` (Main.cls#L2381). (b) No (current): print each error and a summary, return success (Main.cls#L2396-2410). Help is silent. Native (N20): `sync` with one broken module printed the error and a summary naming it, synced the other module, and returned OK; `sync sync-test` returned the error (O:nc-sync-N20-sync-all-one-fails). | human (IPM maintainers) | resolved (assumption) |

Canonical: [devtools.sync.aggregate-status-policy](../../../ontology/features/developer-tools.g5.md#devtools.sync.aggregate-status-policy).

<a id="dec-sync-4"></a>
### DEC-SYNC-4

Disposition: open; working assumption.

| DEC-SYNC-4 | 09-sync | What should `reload` do for a module that is not in developer mode (no root directory)? | (a) Fail or warn, since help says it "pulls module source code into the namespace from disk" (Main.cls#L42). (b) Keep the silent no-op marked "For now" (Base.cls#L633-634). Native (N21): `reload` of a non-dev module printed "Reload START ... SUCCESS", returned OK, and did not load the edited file (O:nc-sync-N21-reload-non-dev). | human (IPM maintainers) | resolved (assumption) |

Canonical: [lifecycle.empty-reload-policy](../../../ontology/concepts/lifecycle.g5.md#lifecycle.empty-reload-policy).
