# Data policy

IPM records what is installed in each namespace and keeps a history of the actions taken there. These rules govern what identifies an installed item, which records are local, and how to distinguish the current supplier from the source of an earlier install.

## Definitions

A module is a versioned package described by a manifest (`module.xml`). A resource is an item the module installs. Its processor can supply an ownership key different from its manifest name. A namespace is the IRIS environment in which the module is installed. A mapping makes code or data from another database visible there.

## Module names and resource ownership

<a id="module"></a>
[I · module]

A module is a named, versioned package of code and data that IPM installs into a namespace. Its manifest, `module.xml`, describes its resources, dependencies, settings and install steps.

<a id="module.identity"></a>
[I · module.identity]

A manifest gives the module a `Name`, a [semantic version](versions.g5.md#version) and descriptive fields.

- The manifest is a `<Document>` named `<Name>.ZPM`.
- An optional `ExternalName` can be used in commands in place of the name. It must be unique and must not match another module's name.

<a id="module.resource"></a>
[I · module.resource]

A module lists its resources, and IPM picks a resource processor for each one from its type or element.

- Resource attributes include `Deploy`, `Directory`, `Preload` and `Scope`.
- Loading a manifest that no longer lists a resource or dependency updates the stored record but leaves the dropped resource's code and the dropped dependency module installed. Whether IPM should remove them is the open upgrade-cleanup policy (C:D26).
- Processors with special behavior are listed under [resource processors](../features/resources.g5.md#module.processor).

<a id="module.resource.identity"></a>
[D · module.resource.identity]

When saving a module, IPM refuses a new resource reference if its nonempty unique name is already stored, even for the same module. Reusing the stored reference passes this check; the other save checks still apply.

- The check uses the resource's stored identity and unique name, not just the module name. It runs before a deployed-project installer can be called.
- A refused save can leave changed object fields that were not written to the row. Read the database separately when deciding what survived.

## Namespace boundaries

<a id="namespace"></a>
[I · namespace]

IPM 0.9 and later is installed per namespace. Each namespace can have its own IPM version, repositories, installed modules and [history](#history.namespace).

<a id="history.namespace"></a>
[I · history.namespace]

Each namespace has its own log. IPM writes a record in the namespace where the action runs, and record IDs are unique only within a namespace.

## Current supplier and historical source

<a id="repo.origin.installed"></a>
[D · repo.origin.installed]

A repository install writes its configured repository name into the module record. `show` and `list -repository` read that saved name, even after the repository definition is deleted or reused for a different location.

- In the checked fresh filesystem installs, qualified alpha and beta requests select their respective payloads despite identical module names and versions. Recreating alpha at a new folder changes the next fresh install, while the earlier module and [history source snapshot](#history.fields.source-snapshot) retain their old values.
- A later local `load` clears the module's repository name. `show` omits its Repository field and neither repository-filtered list includes it. Earlier installation history still describes the past installs, not the current payload.
- Repository settings stay stable during each checked command. [Same-version supplier changes](#repo.origin.same-version) have a separate contract; concurrent reconfiguration and general cache lifetimes remain unassessed.
- Installed supplier names differ from project-repository metadata in available-module searches. That separate catalog distinction is source-inspected here, not a newly executed provider case.

<a id="repo.origin.same-version"></a>
[D · repo.origin.same-version]

Reinstalling the same plain `1.0.0` from a different qualified filesystem repository loads that supplier's files again. The saved supplier, root and callable payload change even though the version string stays the same.

- The checked sequence installs alpha, then beta at the same version without uninstalling, then alpha again. Each successful action gets a new source snapshot; earlier alpha and beta history still describe their past suppliers.
- A request for that version from a missing repository fails before loading. The installed beta payload and current supplier stay beta, and earlier successful source snapshots remain available. The installed copy does not satisfy a request for a missing qualifier.
- This is the supported [same-version reload](../features/load.g5.md#load.same-version), not a new product defect. The domain has one valid package per configured root, stable definitions and successful load/compile/save providers. Other versions, provider failure effects and refused-attempt history details remain unassessed. Logical successful-record labels are separate from database IDs, which can include failed attempts.

<a id="history.fields.source-snapshot"></a>
[D · history.fields.source-snapshot]

For a retained finalized install record, `history details <ID>` reads the saved repository name, type and details. Deleting or reconfiguring that repository later does not replace those fields with its current definition.

- The checked filesystem sequence installs from alpha, reuses alpha for another folder, and then installs from beta. Each permanent record retains the supplier and scan-location details captured for that action; ordinary `show`, repository-filtered lists and actual history output were checked separately.
- A subsequent local load has an empty source triple, and its details show `n/a` because its action is `load`. Its command string identifies the local path. Older install history remains historical and must not be mistaken for current origin after that reload.
- The model uses logical record labels mapped to independently captured permanent IDs. It does not equate temporary and permanent IDs, or cover history deletion, caller rollback, concurrency or refused-attempt source records.

## Protection and retention of history

<a id="history.locked"></a>
[I · history.locked]

Once a record is finalized, it cannot be edited through SQL `UPDATE` or by saving the object. Records can only be added or deleted, which protects the log from tampering.

- Working assumption: no. A finalized record is one whose action has ended, so a record without an end time stays unfinalized.

<a id="history.retention"></a>
[I · history.retention]

`config set HistoryRetain <days>` makes IPM delete records that started more than that many days ago. `0`, the default, turns this off, and the log then grows without limit.

- The purge runs on any IPM command, at most once per calendar day.
- It applies to every namespace, not just the current one.

- As built, the quoted cutoff can keep an expired row while `SetHistoryLastPurge` advances the daily marker (P:F-HIST-6; native P1–P4). The marker proves an attempt, not successful deletion.

- Preserve record identities when checking retention.

- Working assumption: no. The command should still run, which is a proposed change.
- Working assumption: Retain the documented age rule while keeping deletion/finalization races explicit; no repair outcome is claimed.

<a id="history.keep-on-ipm-uninstall"></a>
[I · history.keep-on-ipm-uninstall]

The log survives uninstalling IPM itself. Full cleanup removes all modules and IPM's metadata except the history log.

- Working assumption: no. History belongs to each namespace.

## Metadata

### module

- human-provided
- user-facing-behavior
- Children: [identity](#module.identity), [resources](#module.resource), [dependencies](../features/manifest.g5.md#module.dependency), [parameters](../features/manifest.g5.md#module.parameter), [requirements](../features/manifest.g5.md#module.requirement), [invokes](../architecture/lifecycle.g5.md#module.invoke), [processors](../features/resources.g5.md#module.processor), [placement](../features/resources.g5.md#module.placement), [deployed code](../features/packaging-deployed-code.g5.md#module.deployed), [Python](../features/resources.g5.md#module.python), [environment files](../features/manifest.g5.md#module.env).

Sources: README L1-6; `wiki/03`; `wiki/00` L1-10.

### module.identity

- human-provided
- user-facing-behavior
- Expands: [Module](#module).

Sources: `wiki/03` L99-115, L181-186; CHANGELOG 0.10.0 (`#755`, `#756`), 0.10.5 (`#959`, `#958`). Inventory: CAT-01. Test: `ExternalNameModuleAction.cls`. Observed: the manifest is validated on save, refusing an external-name clash or a resource owned by another module (`Storage/Module.cls:1510-1591`, OP-LOAD-09).

### module.resource

- design-verified

- human-provided
- user-facing-behavior
- Expands: [Module](#module).
- Children: [scope](testing-policy.g5.md#module.resource.scope), [stored identity](#module.resource.identity).

Sources: `wiki/03` L279-343; `wiki/00` L5-7. Inventory: CAT-02. `wiki/03` L302 misdescribes `Deploy` as not shipped. It means shipped as compiled code ([deployed code](../features/packaging-deployed-code.g5.md#module.deployed)).


Iteration52 changed manifest: [record.rs](../../Verus_verification/consolidated/iter52/record.rs) `changed_manifest_keeps_old_code`.

State machines: [record · iter52](../../Verus_verification/models/iter52-record/manifest.json)

### module.resource.identity

- agent
- Expands: [Resources](#module.resource).

Sources: `Storage/Module.cls::%OnValidateObject`; `Storage/ResourceReference.cls` unique-name property/index. Verus `resources.rs::{resource_save_allowed,project_database,resource_identity_and_database_reader}`. Native reused-reference corrupt-install error leaves stored Deployed=true; a fresh duplicate reference instead fails validation with object=true and SQL row=false. These probes distinguish the two actual error origins. Other module-save guards remain separate obligations.

### namespace

- human-provided
- user-facing-behavior
- Children: [enable](../operations/namespaces.g5.md#namespace.enable), [unmap](../operations/namespaces.g5.md#namespace.unmap), [mapped refusal](../operations/namespaces.g5.md#namespace.mapped-refusal), [legacy upgrade](../operations/namespaces.g5.md#namespace.legacy).

Sources: README L23-30, L64-70. Inventory: CAT-29. Observed: IPM settings (`config`) and default modifiers are stored in `%SYS` or `^%` globals and may apply to the whole instance ([settings](../operations/settings.g5.md#settings)).

### history.namespace

- human-provided
- architecture-decision
- test-coverage
- Expands: [History](../features/history.g5.md#history).

Sources: class doc `General/History.cls:3`; `wiki/09` L20, L169; PR #578 comment by isc-dchui, 2025-11-24 ("History is now stored per namespace"); review r1925176116 by isc-kiyer. Inventory: INT-HIST-16; OP-HIST-02. Test: `TestMultipleNamespaces`. This supersedes the 2024 decision in PR #578 to keep one shared table, and the `Namespace` field proposed in issue #366. Observed: a record is written only where IPM's history classes exist.

### repo.origin.installed

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Repositories](repositories.g5.md#repo).
- Children: [same-version supplier changes](#repo.origin.same-version).

[Supplier and reader contract](../../Verus_verification/consolidated/iter16/provenance.rs): `select`, `apply_install`, `install`, `local_load`, `show_repository`, `listed_for_repository`, `history_detail`. The model derives source identity from a configured supplier and its selected fixture package. Fresh installs require actual module/code absence; `successful_uninstall` is an observed-success projection, not general removal or compensation. Existing-installed behavior is unassessed by this fresh-install contract and covered separately below. [Correspondence and runbook](../../uplift/consolidated/iter16/HANDOFF.md).

State machines: [provenance · iter16](../../Verus_verification/models/iter16-provenance/manifest.json)

### repo.origin.same-version

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Installed supplier](#repo.origin.installed).

[Equal-version source and reader contract](../../Verus_verification/consolidated/iter18/reloading.rs) extends the unchanged fresh-install model with the plain equal-version admission and selected load writers. [Iteration18](../../uplift/consolidated/iter18/HANDOFF.md) records actual callable bytes, stored metadata, unique permanent successful records and rendered current/history readers. Rejected-attempt history and generic provider mechanics are outside the new guarantee; earlier proof and native receipts remain unchanged.

State machines: [reloading · iter18](../../Verus_verification/models/iter18-reloading/manifest.json)

### history.fields.source-snapshot

- design-verified

- agent; user-facing-behavior; source-inspected; native-tested; Verus-scoped
- Expands: [Recorded fields](../features/history.g5.md#history.fields).

[Provenance contract](../../Verus_verification/consolidated/iter16/provenance.rs): `qualified_receipt`, `persist`, `history_row`, `history_detail`, `source_view`, using the unchanged imported history source/finalization/action contracts. The selected supplier produces the snapshot; the final reader consumes saved fields. [Native ID mapping, renderer checks and limits](../../uplift/consolidated/iter16/HANDOFF.md). Dependency/update source policy remains separately open under [How should the log label and show a dependency that IPM pulled from a repository while running a `load` or `update`, and what source does an `update` from a registry store?](../../Verus_verification/open_questions.md#history.fields.dependency-source).

State machines: [provenance · iter16](../../Verus_verification/models/iter16-provenance/manifest.json)

### history.locked

- uncertain
- human-provided
- user-facing-behavior, security
- test-coverage
- Expands: [History](../features/history.g5.md#history).
- Children: [May IPM finalize, and so lock, the record of an action that never ended?](../../Verus_verification/open_questions.md#history.locked.unfinished).

Sources: `wiki/09` L197; PR #578 reviews r2583818448 by isc-kiyer ("To protect history from tampering ... only inserts and deletes are allowed?"), r2586415526, r2587636282; trigger `General/History.cls:13-22`. Inventory: INT-HIST-14; OP-HIST-12; premise P-8. Test: `TestFinalizedRecordsLocked` (object save and SQL `UPDATE`, SQLCODE -132). Observed: unfinalized records stay editable, and deletes are allowed by design. Native evidence: N-P13 confirmed (`181129Z-premises`). Object save and SQL `UPDATE` of a finalized record fail with SQLCODE -132, unfinalized records stay editable, and `DELETE` is allowed.

Open questions: [May IPM finalize, and so lock, the record of an action that never ended?](../../Verus_verification/open_questions.md#history.locked.unfinished)

### history.retention

- uncertain
- human-provided
- user-facing-behavior
- Expands: [History](../features/history.g5.md#history).
- Children: [May a failure of the daily purge stop the user's command?](../../Verus_verification/open_questions.md#history.retention.effect).

Sources: `Main.cls:840-842`; `wiki/02` L113; `wiki/09` L173-185; CHANGELOG 0.10.7 (`#992`); issue #977. Inventory: INT-HIST-13; OP-HIST-21, OP-HIST-22. Test: `Unit/CLI.cls` `TestAutoPurgeOldHistory` runs the purge but does not check that anything was deleted, so no coverage tag. Observed: the purge runs only through `%IPM.Main:Shell`, and the retention setting and last-purge date are shared by the whole instance. The opt-in default answers the audit-trail concern raised on PR #578 by isc-dchui, 2025-12-02. Consolidated round 3 (`20261114T032524Z-r3-purge`, ARM64): the first command of the day purges and writes the marker (P1); a later command the same day does not purge (P2); with retention `0` the marker is left as it was (P3). The 2000-01-01 record survived every purge, reproducing P F-HIST-6. Note the stored key is `history_retain` (`Repo/UniversalSettings.cls:46`); `config set HistoryRetain` maps to it.

Confirmed product defect B2 D3: the purge deletes nothing and still records that it ran, so no purge happens that day (case F1; `182845Z-cases-ef`, `184230Z-replay-fast`, `184626Z-repro-autopurge-noop`). In a control, the same cutoff without its embedded quote marks deleted exactly the two old rows (`General/History.cls:172-173`).

Extension Verus: [history.rs::purge](../../Verus_verification/consolidated/verus/history.rs). These hand translations represent only the manifest-scoped property under its named premises; native receipt links and open inputs are in [the bidirectional index](../../uplift/consolidated/extension/bindings.json). They do not mechanically verify the ObjectScript body.

Findings: [G:D3 — e2e reproduced](../../Verus_verification/bugs_issues.md#g-d3). The linked entry qualifies the caller, reader and providers.

Open questions: [May a failure of the daily purge stop the user's command?](../../Verus_verification/open_questions.md#history.retention.effect), [May automatic retention remove an action that has not ended?](../../Verus_verification/open_questions.md#history.retention.running-policy)

### history.keep-on-ipm-uninstall

- uncertain
- human-provided
- user-facing-behavior
- Expands: [History](../features/history.g5.md#history).
- Children: [May uninstalling IPM in one namespace delete the in-progress records of other namespaces?](../../Verus_verification/open_questions.md#history.keep-on-ipm-uninstall.other-namespaces).

Sources: prompt `Main.cls:2735` ("fully remove all modules and IPM metadata (except history log)"); maintainer comment `Lifecycle/Module.cls:54` ("Preserve ^IPM.General.HistoryD for audit trail"); CHANGELOG 0.10.7 (`#1057`). Inventory: INT-HIST-15; OP-HIST-23. Observed: the uninstall record of IPM itself is finalized as successful before its Clean phase runs (OBS-H15). No test. Not run by B2: self-uninstall (OBS-H15). Only its persist step was probed. A second persist on a finalized record fails with -132 and changes nothing (N-P16, `181129Z-premises`).

Open questions: [May uninstalling IPM in one namespace delete the in-progress records of other namespaces?](../../Verus_verification/open_questions.md#history.keep-on-ipm-uninstall.other-namespaces)
