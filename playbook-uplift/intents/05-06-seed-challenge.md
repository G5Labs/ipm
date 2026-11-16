# Seed challenge — modules 05 (uninstall) and 06 (registries)

Seed: `docs/ipm-uplift/index.md` (human-recovered intents, inspected 2026-11-13). Target: ipm 571e04a1. Each seed
claim in these areas was re-derived from source, tests, changelog and commit or PR history. The seed verdicts below
rest on reading code and docs. The candidate native checks were later run on IRIS 2026.1 (see "Native check results"
at the end); where a run changed a verdict, that table says so.

## Seed verdicts

| Seed claim (paraphrase) | Verdict | Evidence | Ontology |
|---|---|---|---|
| File ownership on removal: a module author can set `KeepOnUninstall` on a `FileCopy` resource so shared files survive clean or uninstall. | Confirmed for uninstall. | `FileCopy.OnPhase("Clean")` returns before deleting when `KeepOnUninstall` is set (src/cls/IPM/ResourceProcessor/FileCopy.cls#L227-L230); PR #1308 / commit f4516c0e; changelog #1135. Test asserts both kept files and the removed `mine.txt` (tests/integration_tests/Test/PM/Integration/FileCopy.cls#L101-L103). | UNIN-D9, UNIN-B18, UNIN-B19 |
| ... "survive clean". | Refined. | The test only runs `uninstall`. For `<module> clean` the module it runs on is at `Clean.Level=0`, where `%Clean` deletes no resources at all (Lifecycle/Base.cls#L325), so its files survive whether or not the flag is set. The flag matters for `clean` only on dependencies that clean removes. | UNIN-B63 |
| Its default is off. | Confirmed. | `Property KeepOnUninstall As %Boolean [ InitialExpression = 0 ]` (FileCopy.cls#L36). | UNIN-B20 |
| The regression case puts retained and ordinary files in the same module. | Confirmed. | Fixture `_data/file-copy-test/fc-keep-on-uninstall/module.xml`: two kept resources (one directory with `Overlay="1"`, one file) and one ordinary file, all under one target folder. | UNIN-B19 |
| (implied) Without the flag only the module's own files go. | Contradicted in part (by design). | Without the flag a directory resource is removed with `RemoveDirectoryTree` on the whole target (FileCopy.cls#L236-L240), even when it was copied with `Overlay="1"` among other modules' files. The property comment admits IPM does not track who owns a shared file (FileCopy.cls#L32-L35). Protected IRIS/IPM directories are never removed (changelog #300; FileCopy.cls#L149 test). | UNIN-B21, UNIN-B22 |
| Registry namespace: a dependency published beneath a configured ORAS registry namespace must still resolve when its parent is installed. | Confirmed. | tests/integration_tests/Test/PM/Integration/Oras.cls#L130-L155 publishes child and parent under namespace `test`, uninstalls both, installs `oras/simplest-module 3.0.0`, and asserts both versions (#L153-L155). Commit 8fd97c0b (PR #1268), changelog #1267. | REPO-B23 |
| The module name presented to a caller omits the configured registry prefix. | Refined. | `ModuleNameFromRepository` strips the namespace only as a whole leading path segment (Oras/PackageService.cls#L14-L23; unit test tests/unit_tests/Test/PM/Unit/Oras.cls#L132-L137 shows `packages/ipmx/module-a` is left whole). Deeper paths below the namespace stay in the name (e.g. `sub/module`); nested lookups are supported (changelog #1269, Oras.cls#L607-L612). No integration test asserts the printed `search` output (Oras.cls#L79-L80 asserts only status). The namespace is lower-cased and trimmed of slashes (unit Oras.cls#L60-L70). | REPO-B21, REPO-B22, REPO-B24 |
| This "namespace" is part of the registry path. | Confirmed. | `-namespace` help (Oras/Definition.cls#L28-L29): "this is not the IRIS namespace". Two repos on one registry may differ only by namespace (Oras.cls#L508-L517); a URL with a path is refused and the path is suggested as namespace (changelog #1283). | REPO-D4, REPO-B19, REPO-B20 |
| Worked example: "Extending the load rule to uninstall would invent a guarantee." | Confirmed; extended. | Uninstall runs no transaction around Clean (Storage/Module.cls#L727-L757); only the final record and manifest delete is transactional (StudioDocument/Module.cls#L205-L227). New: a *blocked* uninstall (dependent present) is logged as a success with the module still installed (F-UNIN-6, DEC-UNIN-1). History-field semantics stay with module 02. | UNIN-B9, UNIN-B43, UNIN-B44, UNIN-B30 |
| Where to continue mining → Public operations: pair each command with the resource it changes and the command that reads it back. | Done for uninstall, orphans, repo, publish, unpublish, search, list-installed, show, module-version, config, default-modifiers. | Pair table in `ontology/06-registries.md` ("Write and read-back pairs"). | 05, 06 |
| Namespace ownership (README): registry settings can stay local to a namespace. | Refined (shared seed; noted here for the settings half). | Registry definitions are per namespace (`^IPM.Repo.DefinitionD`), but `config` settings live in `^IPM.settings` in %SYS and apply to the whole server (UniversalSettings.cls#L1, #L150-L177), and default modifiers live in a percent global (`^%IPM.DefaultModifiers`), which IRIS shares across namespaces by default (F-REPO-5). | REPO-B97, REPO-B106 |

## Behaviours the seed missed (most important)

1. A blocked uninstall still deletes the module's web apps and CSP files, because processors run `OnBeforePhase`
   before the dependent check (F-UNIN-1). Same for `<module> clean` on the module itself.
2. `uninstall -purge` has no reader (F-UNIN-2).
3. `uninstall IPM` / `uninstall ZPM` bypass the IPM self-removal guard and prompt (F-UNIN-3).
4. A dependent whose dependency graph fails to build is ignored by the dependent check (F-UNIN-4).
5. A `.GBL` resource without the `Global` attribute is never deleted (F-UNIN-5).
6. With no publish flag, `publish` and prefix-less `unpublish` fall back to any enabled registry (F-REPO-3, DEC-REPO-1).
7. `-token-auth-method` is ignored for remote registries; `-sslconfig` and `-publish-url` are never read
   (F-REPO-1, F-REPO-2).
8. `config delete` restores the first value ever set for keys with no seeded default (F-REPO-4).
9. Disabled registries are still contacted by `search` and `publish -r` (F-REPO-7, DEC-REPO-2).
10. `repo -info` and the `repo -enable -app ... -publish 1` help example are not implemented (REPO-B107, REPO-B108).

## Candidate native checks

Each check runs in a scratch namespace with IPM installed (`zpm "..."` or `##class(%IPM.Main).Shell("...")`, which
returns a `%Status`). "Expected" is the intended behaviour from the ontology; "Code predicts" is what reading the
code at 571e04a1 suggests when it differs.

| ID | Behaviour | Experiment | Expected | Code predicts |
|---|---|---|---|---|
| NC-UNIN-1 | UNIN-B28, UNIN-B30, UNIN-B31 (DEC-UNIN-1, F-UNIN-1, F-UNIN-6) | Load a module `dep` with a `<WebApplication Url="/nc-dep">` and a module `top` that depends on `dep`. Run `uninstall dep` (no -force). Check `##class(%IPM.Storage.Module).NameExists("dep")`, the returned status, `##class(Security.Applications).Exists("/nc-dep")` in %SYS, and the latest history row. | Module kept; status is an error naming `top` (if DEC-UNIN-1 picks (a)); web app still present. | Status OK, "Clean SKIPPED - required by 1 other module", module kept, web app gone, history says success. |
| NC-UNIN-2 | UNIN-B66 (F-UNIN-1) | Load a module `w` with a web app and no dependencies; run `zpm "w clean"`. | `w` and its web app unchanged. | `w` still installed; web app deleted; installer `OnUnconfigureComponent` called (if any). |
| NC-UNIN-3 | UNIN-B13 (F-UNIN-2) | Module with a persistent class and an installer class whose `OnUnconfigureComponent` records `pPurge` in a global. Insert a row. Run `uninstall m -purge`. Check the recorded `pPurge` and `$data(^<class>D)`. | `pPurge=1`; table data removed. | `pPurge=0`; data global still present. |
| NC-UNIN-4 | UNIN-B62 (F-UNIN-3) | In a namespace where IPM is mapped from another namespace, run `##class(%IPM.Main).Shell("uninstall IPM")` (and separately `uninstall zpm`). | Both refused with the mapping error. | `uninstall zpm` refused; `uninstall IPM` proceeds and deletes IPM classes (do this only in a throwaway instance). |
| NC-UNIN-5 | UNIN-B33 (F-UNIN-4) | Load `a`; load `b` that depends on `a` and on `ghost` (not available in any registry, installed with `-DNoMapping` or by editing the stored dependency after install). Run `uninstall a`. | `a` kept (b needs it). | `a` removed; `b` left with a missing dependency. |
| NC-UNIN-6 | UNIN-B12 (F-UNIN-5) | Module with `<Resource Name="NC.Data.GBL"/>` (no `Global` attribute) and a second module with `<Resource Name="NC.Data2.GBL" Global="^NC.Data2"/>`. Uninstall both; check `$data(^NC.Data)` and `$data(^NC.Data2)`. | Both 0. | `^NC.Data` still defined; `^NC.Data2` gone. |
| NC-UNIN-7 | UNIN-B19, UNIN-B21 | Install the `fc-keep-on-uninstall` fixture plus a second module whose `FileCopy` copies a directory into the same target folder with `Overlay="1"` and no keep flag. Uninstall the second module. | `shared.txt`, `shared-file.txt` from the first module remain. | Whole target folder removed, including the first module's kept files. |
| NC-UNIN-8 | UNIN-B43, UNIN-B44 | Module `p` whose dependency `q`'s installer `OnUnInstallComponent` returns an error. Run `uninstall p -r`. Check `p`'s classes and record. | (No guarantee intended.) Document state. | `p`'s classes deleted, `p`'s record kept, status error. |
| NC-UNIN-9 | UNIN-B69, UNIN-B70 (DEC-UNIN-2) | With two other modules installed, run `zpm "zpm clean"`; capture output; list installed modules. | Per decision. | All modules still installed; output contains "WARNING: 2 module(s) orphaned". |
| NC-UNIN-10 | UNIN-B71-B74 | Create a class `NC.Orphan` with `$system.OBJ.Compile`; run `orphans -type CLS` and `orphans`. | `NC.Orphan.CLS` listed in both; no `%`-classes, no mapped or generated items; no state change. | Same; unknown whether an empty type filter lists anything (spec `""`). |
| NC-REPO-1 | REPO-B56 (DEC-REPO-1, F-REPO-3) | `repo -delete-all`; `repo -r -n a -url http://127.0.0.1:9/ -token t` (no -publish); load a module; `publish m` (no -r) with verbose. | Fails: no registry has the publish flag. | Tries to publish to `a` ("Publish to: http://127.0.0.1:9/"). |
| NC-REPO-2 | REPO-B54, REPO-B82 (DEC-REPO-2, F-REPO-7) | Configure a filesystem repo `f` and an ORAS repo `o` pointed at a local zot; `repo -n o -disable`; run `search` and `publish m -r o`. | Per decision; at least publish refused. | `search` prints `o` and its modules; publish succeeds. `unpublish o/m all -force` fails "Repository 'o' is unavailable." |
| NC-REPO-3 | REPO-B46 (DEC-REPO-3) | `repo -o -n o -url http://hostA -username u -password p`; then `repo -n o -url http://hostB`; open `%IPM.Repo.Oras.Definition` "o" and read `Username`. Repeat with `-r`. | Per decision; remote clears. | ORAS keeps `u`; remote has "". |
| NC-REPO-4 | REPO-B47 (F-REPO-1) | `repo -r -n r -url http://127.0.0.1:9/ -token t -token-auth-method bearer`; read `TokenAuthMethod`. | `bearer`. | `apiKey`. |
| NC-REPO-5 | REPO-B48 (F-REPO-2) | `repo -r -n r -url https://example.invalid/ -sslconfig MyCfg`; inspect `%IPM.Repo.Remote.PackageService` request `SSLConfiguration`. | `MyCfg`. | `example.invalid` (auto-created config). |
| NC-REPO-6 | REPO-B99 (F-REPO-4) | On a fresh install: `config set HistoryRetain 30`, `config set HistoryRetain 1`, `config delete HistoryRetain`, `config get HistoryRetain`. | Empty (factory value). | 30. |
| NC-REPO-7 | REPO-B106 (F-REPO-5) | In USER: `default-modifiers -set -DNcProbe=1`; switch to another IPM-enabled namespace; `default-modifiers -get`; check `$data(^%IPM.DefaultModifiers)` there. | Nothing set in the second namespace. | `-DNcProbe=1` listed in the second namespace. |
| NC-REPO-8 | REPO-B101 (DEC-REPO-4, F-REPO-8) | `set sc=##class(%IPM.Main).Shell("config set NoSuchKey 1")`; `w $system.Status.IsOK(sc)`. Same with `config set TestReportFormat xml`. | Error status. | 1 (OK) in both cases. |
| NC-REPO-9 | REPO-B77 (DEC-REPO-5) | Publish a deployed module to zot (tag `1.0.0<sep><platform>`); `unpublish o/m 1.0.0 -force`; list tags with the ORAS client. | Per decision. | Error "'m' 1.0.0 not found" (plain tag absent); platform tag remains. |
| NC-REPO-10 | REPO-B10 (F-REPO-6) | `repo -type %Library.RegisteredObject -n x`. | Error "Invalid type ... must extend %IPM.Repo.Definition". | A method-does-not-exist error. |
| NC-REPO-11 | REPO-B26, REPO-B27 | `repo -fs -n f -path <dir>` with one module; add a second module folder to `<dir>`; `search`; then re-run `repo -n f -path <dir>`; `search`. | Second module appears only after re-running repo. | Same. |
| NC-REPO-12 | REPO-B22 | Publish `default-name` with ORAS namespace `ns/sub`, then set the repo namespace to `ns`; run `repo -list-modules -n oras`. | Name shown without `ns/` (seed). | Shown as `sub/default-name`. |
| NC-REPO-13 | REPO-B44 (DEC-REPO-6) | `repo -r -n r -url http://127.0.0.1:9/ -username u -password-env PW` with `PW=secret`; `zw ^IPM.Repo.DefinitionD`. | Per decision. | `secret` stored in plain text. |

## Native check results

Run on IRIS Community 2026.1 (build 234U), IPM 0.10.11-SNAPSHOT from source at 571e04a1, container
`ipm-uplift-iris-repo`, ORAS checks against the shared zot `http://oras:5000` (all paths `nc-repo-*`, removed
afterwards: `nc-repo-zot-cleanup`). Probes: `validation/iris/repo/cls/Uplift/Repo/*.cls`; fixtures
`validation/iris/repo/fixtures/`; scripts `validation/iris/repo/7*.script`; logs `validation/logs/7*-repo-*.log`.
Fixture modules are renamed copies of the shapes the checks describe (for example `nc-web-dep`/`nc-web-top` for
`dep`/`top`).

| Check | Result | Observations |
|---|---|---|
| NC-UNIN-1 | As code predicted: status OK, "Clean SKIPPED - required by 1 other module. (nc-web-top)", then "Clean SUCCESS"; module and class kept; web app `/nc-dep` deleted; history `uninstall/success`. | `nc-unin-1-blocked-uninstall` |
| NC-UNIN-2 | As code predicted: `nc-web-w` and its class kept; web app `/nc-w` deleted; installer `OnUnconfigureComponent` and `OnUnInstallComponent` each called once. | `nc-unin-2-clean-own-module` |
| NC-UNIN-3 | Differs in part: data kept with and without `-purge` (as predicted); the hook's `pPurge` argument is 0 (as predicted), but `pVars("Purge")` is 1 with `-purge`. | `nc-unin-3-uninstall-plain`, `nc-unin-3-uninstall-purge`, `nc-unin-env-obj-delete-default`, `nc-unin-env-obj-delete-e-flag` |
| NC-UNIN-4 | Differs: in a namespace that borrows IPM by `enable -map`, `uninstall zpm` is refused with the mapping error, and `uninstall ZPM`, `ipm`, `IPM` fail with "Module ... not found" (no IPM record there); nothing is deleted. The predicted bypass happens only where IPM's record lives: in USER, `uninstall IPM` removed IPM with no module list, question or shell exit. | `nc-unin-4-mapped-setup`, `nc-unin-4-mapped-uninstall-zpm`, `nc-unin-4-mapped-uninstall-ZPM`, `nc-unin-4-mapped-uninstall-ipm`, `nc-unin-4-mapped-uninstall-IPM`, `nc-unin-self-external-name-IPM` |
| NC-UNIN-5 | As code predicted: after `nc-ghost-b` got an unresolvable need, `GetDependentsList` found 0 dependents (1 graph error) and `uninstall nc-ghost-a` removed it; the control run before the edit was skipped. | `nc-unin-5-unresolvable-dependent` |
| NC-UNIN-6 | As code predicted: `^NC.Data` (no `Global` attribute) still defined; `^NC.Data2` gone. | `nc-unin-6-global-resources` |
| NC-UNIN-7 | As code predicted: uninstalling the Overlay module without the keep flag removed the whole target folder, including `shared.txt` and `shared-file.txt`. Control: the keep module alone keeps both and deletes `mine.txt`. | `nc-unin-7-shared-target-folder`, `nc-unin-7-control-keep-module` |
| NC-UNIN-8 | As code predicted: status error ("nc-q refuses to uninstall"); `nc-p`'s class deleted, `nc-p`'s record kept; `nc-q` record and class kept; both history rows are failures. | `nc-unin-8-dependency-hook-fails` |
| NC-UNIN-9 | As code predicted: all modules and IPM kept; output "WARNING: 2 module(s) orphaned" with 2 other modules. | `nc-unin-9-clean-on-ipm` |
| NC-UNIN-10 | Differs in part: `NC.Orphan` listed by both (lower-case `.cls`), no module-owned class, no state change. Without `-type` the empty spec lists every type: 14 non-class items, 10 of them mapped (EnsLib BPL/DTL, HIPAA X12) plus `%IRIS_X12ReplyType.LUT` and CSP pages (F-UNIN-7). | `nc-unin-10-orphans`, `nc-unin-10-orphans-detail` |
| NC-REPO-1 | As code predicted: `PublishingServer` picks `nc-repo-a`; "Publish to: http://127.0.0.1:9/"; fails on the TCP connection. | `nc-repo-1-publish-no-flag` |
| NC-REPO-2 | As code predicted: disabled ORAS registry and its module listed by `search`; `publish -r` to it OK; unpublish refused ("Repository 'nc-repo-o2' is unavailable."); install did not use it. | `nc-repo-2-disabled-registry` |
| NC-REPO-3 | As code predicted: ORAS keeps user and password after a URL change; remote clears both. Also: a remote registry cannot take a URL an ORAS registry already holds. | `nc-repo-3-url-change-credentials` |
| NC-REPO-4 | As code predicted: stored `apiKey`, request header `apikey`, no `Authorization`; ORAS control stores `bearer`. | `nc-repo-4-token-auth-method` |
| NC-REPO-5 | As code predicted: request `SSLConfiguration` = `nc-example.invalid` (auto-created); `NcCfg` not created or stored; `-publish-url` not stored. | `nc-repo-5-sslconfig-publish-url` |
| NC-REPO-6 | As code predicted: default node absent on a fresh install, set to 30 by the first `config set`; after `config delete` the value is 30. | `nc-repo-6-config-delete` |
| NC-REPO-7 | As code predicted: `^%IPM.DefaultModifiers` maps to IRISSYS from USER, NCMAP and %SYS; `default-modifiers -get` in NCMAP lists `-DNcProbe=1`. | `nc-repo-7-default-modifiers-namespaces`, `nc-repo-env-percent-global-shared` |
| NC-REPO-8 | As code predicted: unknown key (set and get) and bad `TestReportFormat` all return OK; the bad value is not stored. | `nc-repo-8-config-errors` |
| NC-REPO-9 | As code predicted: only tag `1.0.0__2026.1` pushed; `unpublish ... 1.0.0 -force` fails "'nc-repo-dpl' 1.0.0 not found in repo 'nc-repo-o9'." and the tag stays; `all` removes it. | `nc-repo-9-unpublish-deployed-version` |
| NC-REPO-10 | Differs from both: `-type` is matched against monikers, so `%Library.RegisteredObject` is dropped and IPM prompts "Which sort of repository do you wish to configure?"; `^` gives "Operation cancelled.", nothing created. No method-not-found error (F-REPO-6 refuted; F-REPO-9). | `nc-repo-10-invalid-type`, `nc-repo-10-control-type-moniker` |
| NC-REPO-11 | As intended (and as predicted): `nc-fs2` added to the folder is not seen by `search` or `install` until `repo -n nc-repo-f -path` runs again; then both work. | `nc-repo-11-folder-cache` |
| NC-REPO-12 | As code predicted (seed refined): listed as `sub/nc-repo-pub` once the namespace is `nc-repo-ns12`. | `nc-repo-12-nested-namespace-name`, `nc-repo-12-cleanup` |
| NC-REPO-13 | As code predicted: the secret values from `-password-env` and `-token-env` are stored in plain text; `repo -list` does not print them. | `nc-repo-13-secret-at-rest` |

Other facts settled natively: `Name` and `ExternalName` lookups are case-blind (`nc-unin-env-name-case`); a non-full
self-uninstall (`uninstall zpm`, answered No) deletes IPM's classes but keeps IPM's own record (ID 1), the other
module records, the registry list and the stored manifests, and a reinstall adopts them again with one IPM record
(`nc-unin-self-zpm-answer-no`, `nc-unin-self-zpm-after-reinstall`; new statement UNIN-B76).

IPM tests run here (all passed; whole classes unless a method is named): `Test.PM.Integration.Uninstall`,
`PkgCaseUninstall`, `Show`, `ModuleVersion`, `FileCopy#TestKeepOnUninstall`, `FileCopy#TestProtectedDirectoryFileCopy`,
`Test.PM.Unit.CLI`, `Test.PM.Unit.Oras` (log `77-repo-ipm-tests.log`). Not run: `Test.PM.Integration.Oras`, the rest
of `FileCopy` and `PublishModuleWithDeps`, because they push non-`nc-repo-` names to the shared zot or need the absent
`registry` service.


Review fix (2026-11-13, review/REVIEW.md finding 8): `Test.PM.Integration.Uninstall` asserts only that `uninstall -all`
leaves IPM alone, not the order, so UNIN-B46 no longer cites it. A new probe settled the order natively: with
`nc-ghost-b` needing `nc-ghost-a`, `uninstall -all` without -force removed `nc-ghost-b` first (output and history
rows), against name order (`nc-repo-fix-unin-all-order`, log `79-repo-fix-unin-all-order.log`, probe
`validation/iris/repo/cls/Uplift/Repo/Fix.cls`). Finding 9: REPO-B59 now states the help's "one per namespace" and
carries F-REPO-3.
