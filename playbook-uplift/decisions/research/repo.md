# Intent research: modules 05 (uninstall) and 06 (registries)

Baseline: IPM `571e04a1` (v0.10.11-beta.28). Sources used for every item: code and comments at 571e04a1, `git log -S`,
`git blame`, `git show`, CHANGELOG, README, the IPM wiki clone, GitHub issues and PRs of `intersystems/ipm` (read
only), IPM tests, and IRIS 2026.1 (build 234U) in container `ipm-uplift-iris-repo`. Paths are relative to the IPM
checkout. New probes: `validation/iris/r4/repo/` (scripts `01`–`05`, classes `cls/Uplift/R4/Repo.cls`,
`cls/Uplift/R4/Full.cls`, fixtures `fixtures/nc-sc-*`). New logs: `validation/logs/a3-repo-*.log`. New observation names
start with `r4-repo-`; they become `O:` evidence only after `scripts/collect_evidence.py` is re-run.

Nothing in `ontology/`, `decisions/05-06.md`, `findings/` or `BUGS.md` was edited. Everything below is a proposal.

Verdicts: **RESOLVED-EVIDENCE** = an authoritative source settles intent; **RESOLVED-ASSUMPTION** = no such source, the
most reasonable intent is picked; **STILL-OPEN** = a genuine policy choice.

---

## DEC-UNIN-1 — blocked uninstall: fail or succeed?

**Verdict: RESOLVED-EVIDENCE — option (a).**

**Resolution.** `uninstall M` without `-force`, while a dependent is installed, must return an error status that names
the dependents, and its history row must be a failure. A need that `uninstall X -r` keeps because another module uses
it is documented behaviour of `-r`, so that case must not fail the command (but its row must not claim a removal).

**Evidence.**
- Issue #58 "uninstall allows you to remove dependent class" (https://github.com/intersystems/ipm/issues/58). Comment
  of 2025-09-22 relays maintainer isc-kiyer: default behaviour should be to "block and warn"; `-force` overrides.
  Comment of 2025-09-30 by isc-eneil: once this is fixed, "unskip" `Test.PM.Integration.InstallModule::TestUninstallDependencyDoesError()`.
- That test, `tests/integration_tests/Test/PM/Integration/InstallModule.cls#L132-L159`, added by isc-kiyer in PR #887
  (commit `f83b207e`, merged 2025-10-14), asserts `AssertStatusNotOK` with the message "Can't uninstall a module that is
  a dependency of another installed module" (#L152). It is still skipped at #L135 ("does not yet exist"); the
  coordinator reports the same skip in IPM CI run 42293577988 at 571e04a1 (all 8 image jobs).
- PR #912 (https://github.com/intersystems/ipm/pull/912, merged 2025-10-01, "Fixes #58") made the block work without
  `-verbose` (commit `e7239955`; before it the skip only happened with verbose output) but kept `pSkip=1` with an OK
  status and did not unskip the test.
- Code comment `src/cls/IPM/Lifecycle/Base.cls#L276`: "return an error status if it does". It dates from the initial
  commit `2aaa8cca` (2019); the code has always returned OK.
- Help `Main.cls#L486-L487` and wiki `02.-CLI-commands.md#L1253`: uninstall "will be prevented".
- Wiki `01.-Getting-started.md#L111-L120` (CI section): `Shell()` returns a `%Status`, and a script checks
  `$$$ISERR(sc)` to learn "The command failed."
- Counter-evidence weighed: `PublishModuleWithDeps.cls#L25-L26` asserts success for `uninstall publish-with-deps-dep`
  while `publish-with-deps` needs it. Those lines were written on 2025-02-11 (commit `d79ac23e`, Shuheng Liu), seven
  months **before** the #58 block existed: at that time the uninstall really removed the dependency. The assertion was
  never re-examined (the only later change, PR #1300 `7b407e95`, touched #L6). It records old behaviour, not intent.
- Native: `nc-unin-1-blocked-uninstall` (OK, "Clean SKIPPED ... Clean SUCCESS", `uninstall/success`);
  `nc-repo-model-unin-recurse-skipped-dep` (the `-r` case returns OK).

**Consequence.** Code does **not** match. Defect: **F-UNIN-6** (existing). Under (a) the web-app deletion must also
move after the check (F-UNIN-1), and `PublishModuleWithDeps.cls#L25-L26` must be changed (uninstall the parent first,
or assert NotOK); `TestUninstallDependencyDoesError` should be unskipped.

Ontology carrying the decision: `UNIN-B30` (`OPEN:DEC-UNIN-1`); cells `UNIN-C15`, `UNIN-C17`.
Proposed:
- `UNIN-B30` — text unchanged: "A blocked uninstall reports a failure to the caller." Evidence:
  `F:F-UNIN-6; O!:nc-unin-1-blocked-uninstall` (drop `OPEN:DEC-UNIN-1`). Notes: add issue #58 comments, the skipped
  test `InstallModule.cls#L132-L159` (skipped, so not `X`), and the date of `PublishModuleWithDeps.cls#L25-L26`.
- New `UNIN-B84` (parent UNIN-B30): "The error names the dependents that block it." Evidence: `F:F-UNIN-6`
  (today the names are only printed, Base.cls#L291).
- New `UNIN-B85` (parent UNIN-B39): "Keeping such a need does not make the command fail." Evidence:
  `I; O:nc-repo-model-unin-recurse-skipped-dep`. Source: Main.cls#L502 (help of `-r`).
- Cells `UNIN-C15`, `UNIN-C17`: clear the Decision column (outcome lists stay).

---

## DEC-UNIN-2 — `zpm clean` (Clean on IPM itself)

**Verdict: RESOLVED-EVIDENCE — option (b).**

**Resolution.** `zpm clean` is an ordinary Clean at level 0: it keeps IPM and cleans IPM's needs (it has none). The
orphan warning, the full cleanup and the self-uninstall handling in `ExecutePhases` belong only to a real removal of
IPM (`Clean.Level>0`, reached by `uninstall zpm`). No source supports refusing the command (a) or treating it as an
uninstall (c).

**Evidence.**
- The block's own comment, `Lifecycle/Module.cls#L27`: "Special handling for IPM self-uninstall". `Storage/Module.cls#L630`
  and `#L647`: "Self-uninstall: ... before Clean phase deletes IPM's own classes" and "After Clean deletes IPM's own
  classes". At level 0 Clean deletes nothing (Base.cls#L325), so these premises are false for `zpm clean`.
- PR #1106 (https://github.com/intersystems/ipm/pull/1106, commit `ffba34ec`, "Fix 1057") describes the warning only as
  part of `uninstall zpm`; `git log -L27,81:src/cls/IPM/Lifecycle/Module.cls` shows only #1106 and #1205 (formatting)
  touched it. No test covers `zpm clean`.
- Wiki `00.-Concepts.md#L26`: Clean "Removes the module's resources ... Recurses into dependencies"; wiki
  `02.-CLI-commands.md#L715-L716`: `<module> clean` "deletes all of its dependencies". Neither has an IPM exception.
- Native: `nc-unin-9-clean-on-ipm` (OK, "WARNING: 2 module(s) orphaned", nothing removed).

**Consequence.** Code does **not** match. **New defect F-UNIN-11** (Low): `zpm clean` runs the IPM self-uninstall
block at level 0, so it prints a false "N module(s) orphaned / Reinstalling IPM will ... re-adopt" warning; by
inspection `ExecutePhases` also treats it as a self-uninstall and skips the post-phase callbacks (`OnAfterPhase`,
`Invoke` after-phase hooks, Storage/Module.cls#L631-L655). Fix: guard both on `Clean.Level>0` (or on the self-uninstall
flag). Root cause class RC-6/scope.

Ontology: `UNIN-B70` (`OPEN:DEC-UNIN-2`); cell `UNIN-C41`.
Proposed:
- `UNIN-B70` → "It does not warn that those modules are left without IPM." Evidence:
  `F:F-UNIN-11; O!:nc-unin-9-clean-on-ipm` (drop `I` and `OPEN:DEC-UNIN-2`). Source: Lifecycle/Module.cls#L27-L81.
- New `UNIN-B86` (parent UNIN-B69): "A clean on IPM itself leaves out the steps that only a removal of IPM needs."
  Evidence: `F:F-UNIN-11`. Source: Storage/Module.cls#L630-L655.
- Cell `UNIN-C41`: clear the Decision column.

---

## DEC-UNIN-3 — do phase-scoped needs block uninstall?

**Verdict: RESOLVED-ASSUMPTION — option (b), which is what the code already does.** (Part B: **closed** natively.)

**Resolution.** A need declared only for a phase (`Scope="test"` or `"verify"`) does not make its owner a dependent, so
it does not block `uninstall`. `show`, `list-dependents` and the uninstall check all agree on this today.

**Correction of the decision row.** The row (and `UNIN-B34`'s note) say uninstall counts scoped needs "in any phase".
That is wrong. `GetDependentsList` builds each graph with no phase list (`Utils/Module.cls#L615`, argument 5 empty);
`BuildDependencyGraph` skips a dependency when `HasScope(pPhases, dep.Scope)` is false (`Storage/Module.cls#L955`), and
`HasScope` returns false for any non-empty scope when the phase list is empty (`#L834-L855`). The "two read-backs
disagree" premise is therefore false.

**Native run (closed U23 / DEC-UNIN-3 part).** Log `validation/logs/a3-repo-unin3-scoped.log`, script
`validation/iris/r4/repo/01-unin3-scoped.script`:
- `r4-repo-unin3-test-scoped-dependent`: `nc-sc-user` stores `nc-sc-lib:test`; `GetDependentsList` count 0;
  `list-dependents` and `show nc-sc-lib` do not name `nc-sc-user`; `uninstall nc-sc-lib` → OK, no "SKIPPED", module and
  class gone, `uninstall/success`, `nc-sc-user` still installed.
- `r4-repo-unin3-verify-scoped-dependent`: same for `Scope="verify"`.
- Control `r4-repo-unin3-control-unscoped-dependent`: unscoped need → count 1, `show` names it, uninstall skipped.
- `r4-repo-unin3-recurse-removes-scoped-need`: `uninstall nc-sc-user -r` also removes the test-scoped `nc-sc-lib`
  (the `-r` loop reads every stored dependency, Base.cls#L432, no scope filter).
- `r4-repo-unin3-all-order`: `uninstall -all` removed `nc-sc-user` before `nc-sc-lib` (ordering uses every stored
  dependency, Utils/Module.cls#L725). Cleanup `r4-repo-unin3-cleanup`: only `zpm` left.

**Why (b) is the reasonable intent.**
- A scoped need is loaded on demand when its phase runs: `Scopes.cls#L108-L129` asserts the test-scoped dependency is
  absent after a normal install and is installed by `scope-test test`. So removing it breaks nothing the module needs
  at run time; the next `test` re-fetches it.
- PR #1315 (`show`, https://github.com/intersystems/ipm/pull/1315) explains leaving scoped dependencies out: they
  "usually aren't installed outside dev mode". `Scopes.cls#L26-L41` asserts scoped needs are outside the graph for
  phases that do not include their scope.
- IPM's own cleanup in `Scopes.cls#L132-L133` and `#L166-L167` uninstalls the scoped dependency before its owner, which
  only leaves a clean namespace if scoped needs do not block (no status asserted, so weak).
- Alternatives: (a) count scoped needs — protects a developer whose test dependency came from a folder load and cannot
  be re-fetched; it would need `GetDependentsList` to pass all phases and `show` to list them. (c) adds only display.
  Choosing (a) would turn the current code into a defect in three readers.

**Consequence.** Code matches (b). No defect. Minor inconsistency, not a defect under (b): `-r` and `-all` treat scoped
needs as needs (they remove or order them); this is consistent with "dependency" in the `-r` help.

Ontology: `UNIN-B34` (`I; OPEN:DEC-UNIN-3`). No cell names DEC-UNIN-3.
Proposed:
- `UNIN-B34` → "Needs that apply only to one phase, such as testing, do not block uninstall." Evidence:
  `I; O:r4-repo-unin3-test-scoped-dependent; O:r4-repo-unin3-verify-scoped-dependent` (drop `OPEN:DEC-UNIN-3`).
  Source: Utils/Module.cls#L615; Storage/Module.cls#L834-L855; Storage/Module.cls#L955. Notes: control
  `r4-repo-unin3-control-unscoped-dependent`; `show` (ModuleSummary.cls#L37) and `list-dependents` (Main.cls#L3164)
  agree.
- `UNIN-D2` → "A **dependent** of a module is another installed module that needs it. A need for one phase only does not
  count." (grade 4.8).
- New `UNIN-B87` (parent UNIN-B38): "The recurse flag also removes needs that apply to one phase only." Evidence:
  `I; O:r4-repo-unin3-recurse-removes-scoped-need`. Source: Lifecycle/Base.cls#L431-L433.

---

## DEC-REPO-1 — publish with no `-r` and no publish flag

**Verdict: RESOLVED-EVIDENCE — option (b): keep the fallback (flag, then token, then user and password, then any other
enabled remote/ORAS registry) and document it. The help text is stale.**

**Evidence.**
- PR #332 "Deployment Enabled" (https://github.com/intersystems/ipm/pull/332, merged 2022-08-02). In review, author
  daimor wrote that he did not want "to force users to use publish flag" and ordered the registries: flag, token,
  user and password, "Any other". Maintainer isc-tleavitt replied "I like that solution." Commit `e4dc908d` ("find
  server for deployment by order Deployment, Token, User, all else") added the query with `ORDER BY PublishOrder, ID`.
- Issue #514 (isc-dchui) and PR #569 (merged 2024-09-18) renamed the column to "Default Deployment Registry": the flag
  marks a default, not the only allowed target.
- The help sentence (Main.cls#L56-L57) predates this: `git log -S` finds it in `f97767b3` (2020) and the 2019 initial
  commit.
- Native: `nc-repo-1-publish-no-flag` (token registry chosen).

**Consequence.** Code matches the fallback intent, so the High defect **F-REPO-3 is withdrawn for the fallback**: what
remains is (1) a documentation defect, help Main.cls#L56-L57 omits the fallback, and (2) REPO-B59/B60 (one flag per
kind, not per namespace), which F-REPO-3 also covers. **F-REPO-10 stays and is a regression**: the ordered query of
`e4dc908d` lost its `ORDER BY` in commit `3174d72d` (PR #541, ORAS support, 2024-08-12), which split it per kind and
reads only the first row; the new code's comment "Order by publish order and then name"
(Http/Definition.cls#L156) shows ordering was still meant. Risk note for the record: with the fallback, a configured
community registry with credentials can receive a private module; maintainers accepted that in #332.

Ontology: `REPO-B56` (`OPEN:DEC-REPO-1; F:F-REPO-3; O!:...`), note in `REPO-B67`; cells `REPO-C8`, `REPO-C10`,
`REPO-C24`, `REPO-C48`.
Proposed:
- `REPO-B56` → "With no -repo and no publish flag set, publish still picks a registry." Evidence:
  `I; O:nc-repo-1-publish-no-flag` (drop `OPEN`, `F:F-REPO-3`, `O!`). Source: Remote/Definition.cls#L117-L129;
  Oras/Definition.cls#L143-L155. Notes: PR #332; help Main.cls#L56-L57 is stale (doc defect).
- New `REPO-B109` (parent REPO-B56): "It tries one with a token first. Next it tries one with a user and password, then
  any other." Evidence: `I; F:F-REPO-10; O:nc-repo-1-publish-no-flag`. Notes: within a kind the first row wins
  (F-REPO-10, `nc-repo-model-publish-row-order`); across kinds, order then name.
- `REPO-B67` note: "fallback of REPO-B56" instead of "fallback of DEC-REPO-1".
- Cells `REPO-C8`, `REPO-C10`, `REPO-C24`, `REPO-C48`: clear the Decision column; add `REPO-B109` to C8, C24 and C48.
- `REPO-B59`, `REPO-B60` keep `F:F-REPO-3`; rewrite F-REPO-3 to cover only the one-per-kind rule and the stale help.

---

## DEC-REPO-2 — what may IPM do with a disabled registry?

**Verdict: RESOLVED-EVIDENCE — option (a): refuse it everywhere it would be used; `repo -list` still shows it.**

**Evidence.**
- Help Main.cls#L259: "-disable: For any repository, specifies that it can not be used." (present since at least
  `c91aa57d`, 2020-01-27).
- IPM's own paths already refuse it, including when it is named explicitly: install lookups
  (`Repo/Manager.cls#L69`, `where Enabled = 1 and (? is null or Name = ?)`, so `install R/M` also refuses), the default
  publish pick (`Enabled = 1` in both `PublishingServer` queries), unpublish (`Remote/PublishManager.cls#L26-L37`,
  `Oras/PublishManager.cls#L15-L19`, "Repository '%1' is unavailable."). `search` (Main.cls#L2047) and
  `publish -r` (Lifecycle/Base.cls#L1598-L1600) are the only exceptions; no comment, commit or issue marks them as
  deliberate (`gh search` found no issue on disabled registries).
- Comparable tools: dnf/yum skip a repo with `enabled=0` for both install and search unless it is turned on for the
  command.
- Security: `search` and `publish -r` send stored credentials to a registry the operator turned off.
- Native: `nc-repo-2-disabled-registry`.

**Consequence.** Code does **not** match. Defect **F-REPO-7** (existing).
Ontology: `REPO-B54`, `REPO-B82` (`OPEN:DEC-REPO-2; F:F-REPO-7; O!:nc-repo-2-disabled-registry`); cells `REPO-C29`,
`REPO-C30`. Proposed: keep both texts; evidence `F:F-REPO-7; O!:nc-repo-2-disabled-registry` (drop `OPEN`); clear the
Decision column of `REPO-C29`, `REPO-C30`.

---

## DEC-REPO-3 — ORAS registry address change and stored credentials

**Verdict: RESOLVED-ASSUMPTION — option (a): clear them, as for a remote registry.**

**Evidence.**
- Issue #649 (https://github.com/intersystems/ipm/issues/649, nsolov): credentials "should be cleared automatically
  when the registry URL changes". Fixed by PR #650 (`71552823`, v0.7.x) and carried to main by isc-tleavitt in
  `12740a58` (2025-04-11), only in `Remote/Definition.cls` (now #L81-L86). CHANGELOG#L322: "#649: Clear existing
  credentials when changing URL on remote HTTP registry".
- CHANGELOG#L36 (#1115): credential checks no longer follow redirects "so credentials are not sent to another host" —
  the maintainers treat sending credentials to a host the operator did not pick as a bug.
- `Oras/Definition.cls` was changed by PRs #1131, #1268, #1284 and #1305 (`ce1bd00f`, 2026-11-04, which also edited
  `OnConfigure` #L87-L141) without adding the clearing; no comment or test says ORAS should keep them (no test reads credentials after a URL change).
- Native: `nc-repo-3-url-change-credentials`.
- Why an assumption: #649 and its changelog name only the remote kind; nothing says the ORAS gap is deliberate, and
  the reason given (a new host must not get the old credentials) applies to ORAS unchanged. Alternative (b) "keep and
  warn" would suit an operator who only moves a registry to a new name; it is not what the remote kind does. Docker,
  npm and pip key stored credentials by host, so a new host never gets the old ones.

**Consequence.** Code does **not** match. **New defect F-REPO-11** (Medium): changing an ORAS registry's `-url` keeps the
user, password and token, so the next search, install or publish
sends them to the new host. Fix: the same canonical-URL comparison and clearing as Remote/Definition.cls#L81-L86.
Ontology: `REPO-B46` (`OPEN:DEC-REPO-3`); cell `REPO-C9`. Proposed: keep the text; evidence
`F:F-REPO-11; O!:nc-repo-3-url-change-credentials` (drop `OPEN`); clear the Decision column of `REPO-C9`.

---

## DEC-REPO-4 — `config` with an unknown key or a bad value

**Verdict: RESOLVED-EVIDENCE — option (a): return an error status.**

**Evidence.**
- Wiki `01.-Getting-started.md#L111-L120`: `Shell()` "returns a `%Status`"; CI code checks `$$$ISERR(sc)` to learn the
  command failed.
- CHANGELOG#L35 (#1115): `unpublish` "now fails ... instead of printing the error and reporting success" — the same
  pattern was judged a bug.
- The callee already reports the error: `UniversalSettings:UpdateOne` returns the status of `SetTestReportFormat`
  (#L117-L125) and `ResetToDefault` returns a status (#L87-L107); `Main.Config` drops them by calling with `do`
  (Main.cls#L2063-L2093). Unknown keys print "not found" and return OK (#L79-L83, #L117-L121). The same method also
  returns OK after "Key and Value required" (Main.cls#L2076-L2078). No test asserts the OK.
- Native: `nc-repo-8-config-errors`.

**Consequence.** Code does **not** match. Defect **F-REPO-8** (existing); extend it to the missing key/value case.
Ontology: `REPO-B101` (`OPEN:DEC-REPO-4; F:F-REPO-8; O!:nc-repo-8-config-errors`). Proposed: keep the text; evidence
`F:F-REPO-8; O!:nc-repo-8-config-errors` (drop `OPEN`). No cell names DEC-REPO-4.

---

## DEC-REPO-5 — ORAS unpublish of one version of a deployed module

**Verdict: RESOLVED-ASSUMPTION — option (a): `unpublish R/M V` deletes every tag of version V, plain and per platform.**

**Evidence.**
- Help Main.cls#L202 and wiki `02.-CLI-commands.md#L1318-L1327`: unpublish deletes a "version" of the package; no
  platform argument exists.
- Wiki `05.-Deployed-Packages.md#L80`: the registry "supports multiple platform versions for one specific version of
  the package" — the platform build is a variant of the version, not a separate version.
- The remote registry server (intersystems-community/zpm-registry, `src/cls/ZPM/Registry.cls` `DeletePackage` →
  `ZPM.Package:DeletePackageVersion`) marks the whole name+version deleted, which hides all of its platform builds
  (`ZPM.DeployedPackage` rows hang off name+version). So `unpublish R/M V` on a remote registry removes every platform
  build of V. ORAS should behave the same.
- ORAS tags are `<version>__<platform>` for deployed modules (`Oras/PublishService.cls#L43-L48`, commit `b5abbe7e`,
  2025-03-07, "include platform version in tag"); `Unpublish` deletes only `$$$Semver2Tag(V)` (#L72-L81). The platform
  suffix came after the unpublish code and the unpublish path was never updated.
- Native: `nc-repo-9-unpublish-deployed-version` — today a deployed version cannot be unpublished by version at all.
- Why an assumption: no maintainer statement on ORAS unpublish of deployed versions. Alternative (b) "require the
  platform" is reasonable for removing one bad platform build; it could be added later as an option, but the plain
  form must at least not fail as "not found".

**Consequence.** Code does **not** match. **New defect F-REPO-12** (Medium): ORAS `unpublish R/M V` of a deployed
module fails with "'M' V not found" and leaves every `V__<platform>` tag; only `all` removes them. Fix: delete the tags
`Semver2Tag(V)` and every `Semver2Tag(V)__*` returned by `get_tags`.
Ontology: `REPO-B77` (`OPEN:DEC-REPO-5`). Proposed: keep the text; evidence
`F:F-REPO-12; O!:nc-repo-9-unpublish-deployed-version` (drop `OPEN`). No cell names DEC-REPO-5.

---

## DEC-REPO-6 — registry passwords and tokens in plain text

**Verdict: RESOLVED-EVIDENCE — (a) is the accepted interim; (c) "use IRIS credential storage (the security wallet)"
is the stated direction.**

**Evidence.**
- Issue #628 "Use Security Wallet for credentials once available in IRIS"
  (https://github.com/intersystems/ipm/issues/628, open, labels `enhancement`, `upstream`), opened from maintainer
  isc-kiyer's review of PR #541: "For longer term, we should consider using the security wallet (once available in
  IRIS) to store credentials". isc-tleavitt added `upstream` "as this is dependent on upstream IRIS functionality".
- Issues/PRs #462, #949 (`-password-stdin`), #1178/#1202 (`-password-env`, `-token-env`): all aim to keep the secret off
  the command line and out of logs; none changes storage.
- IRIS fact (this run): IRIS 2026.1 has the wallet API — `%Wallet.KeyValue`, `%Wallet.Collection`, `%Wallet.Secret`,
  `%SYS.Wallet.*` are compiled; `%Wallet.KeyValue` has `Create`, `GetSecretValue`, `ApplyHTTP`; collections have
  `UseResource`/`EditResource` (`r4-repo-env-secure-wallet`, log `validation/logs/a3-repo-wallet-facts.log`).
  `^IPM.Repo.DefinitionD` lives in the USER database (`/usr/irissys/mgr/user/`), readable by anyone who can read that
  database. Which IRIS release first shipped the wallet was not checked here. IPM still supports IRIS `>=2022.1`
  (`module.xml#L35`), so a fallback store is needed on older versions.
- Comparable tools: npm (`.npmrc`), pip/twine (`.pypirc`), cargo and Docker (`config.json`) store tokens in plain or
  base64 text by default and offer an OS keychain or credential helper as the secure option; storing a reference to an
  environment variable (option b) is uncommon for stored settings because the variable must exist in every process,
  including IRIS jobs.
- Native: `nc-repo-13-secret-at-rest`.

**Consequence.** Code matches the interim intent; the planned improvement is an open enhancement, not a defect at
this revision. Note for the owner: the "upstream" blocker of #628 looks lifted on IRIS 2026.1.
Ontology: `REPO-B44` (`I; OPEN:DEC-REPO-6; O:nc-repo-13-secret-at-rest`). Proposed:
- `REPO-B44` — text unchanged; evidence `I; O:nc-repo-13-secret-at-rest` (drop `OPEN`). Notes: accepted interim per
  issue #628.
- New `REPO-B110` (parent REPO-B44): "Where IRIS has a secure wallet, IPM keeps these secrets in it." Evidence: `N`.
  Source: issue #628. Notes: `r4-repo-env-secure-wallet`.

---

## Part B

### DEC-UNIN-3 (U23, scoped needs) — **closed**

See DEC-UNIN-3 above: five observations in `validation/logs/a3-repo-unin3-scoped.log`. Scoped needs never block
uninstall; the decision row's premise was wrong. The DEC-NSP-3 half of U23 (legacy ZPM records) is not in this lane.

### U21 (full cleanup while another namespace borrows IPM) — **closed**, container restored

Run in USER, where NCMAP borrows `%IPM` (package and routines map to USER's database). Logs:
`a3-repo-u21-full-cleanup-a.log` (first attempt; failed at once on a probe bug, `<NAME>` before any change, IPM
verified intact), `a3-repo-u21-full-cleanup-a2.log`, `a3-repo-u21-full-cleanup-b.log`,
`a3-repo-u21-restore-and-mapinfo.log`, `a3-repo-u21-tidy-ncmap.log`; scripts `03`–`05` in `validation/iris/r4/repo/`.

- Prepare (`r4-repo-u21-prepare`): module `nc-sc-lib` loaded in NCMAP; USER has only `zpm`.
- `uninstall zpm -force -DClean.FullCleanup=1` in USER (`r4-repo-u21-full-cleanup-with-mapped-ns`): status OK, full
  cleanup, no prompt, and **no "Will affect the following namespaces" line**. USER: no module records, `%IPM.Main`
  gone. NCMAP: `%IPM.Main` gone too (its mapped code lived in USER's database), the shell fails with
  `<CLASS DOES NOT EXIST>`, but NCMAP's own module record `nc-sc-lib`, its class and the `%IPM` mappings stay.
- Why no warning (`r4-repo-u21-mapinfo-from-user`, `-from-ncmap`): `GetMapInfo` reports NCMAP as mapped from
  `USER-VERIFY`, because USER-VERIFY (made by IPM's verify phase) also has a `zpm` record and resolves `%IPM` to USER's
  database; the last source by name wins. This is **F-NSP-7** (DEC-NSP-6) in a new place: the self-uninstall warning
  (UNIN-B53, Main.cls#L2534-L2550) is silently skipped.
- After `IPM.Installer.setup` in USER (`r4-repo-u21-after-reinstall`): IPM 0.10.11-SNAPSHOT back in USER; NCMAP's
  shell works again and lists `nc-sc-lib` (re-adopted).
- Restore (`r4-repo-u21-restored`, `r4-repo-u21-repair-user`, `r4-repo-u21-tidy-ncmap`, `r4-repo-u21-final`): USER's
  leftover `^IPM.Repo.Filesystem.CacheI/S` (F-UNIN-10) killed and a folder registry add/delete checked; `nc-sc-lib`
  uninstalled from NCMAP; probe leftovers in NCMAP's registry globals removed; backup global removed. Final: USER
  modules `zpm` only, NCMAP shell OK, no registries in either, USER folder-cache globals in the same shape as before
  (`1/10/1`). History rows were added (expected).
- Side fact: deleting a folder registry leaves its `^IPM.Repo.Filesystem.Cache*` data (seen in NCMAP and USER). Not
  investigated further.

Proposed ontology:
- `UNIN-B53` ("If other namespaces borrow IPM from this one, IPM names them and goes on."): add
  `F:F-NSP-7; O!:r4-repo-u21-full-cleanup-with-mapped-ns`; note that it holds only while one namespace with a `zpm`
  record uses that database.
- New `UNIN-B88` (parent UNIN-B56): "A full cleanup does not touch the modules of a namespace that borrows IPM. That
  namespace loses IPM until IPM is put back." Evidence:
  `I; O:r4-repo-u21-full-cleanup-with-mapped-ns; O:r4-repo-u21-after-reinstall`. Source: Main.cls#L2712-L2749;
  wiki `Frequently-Asked-Questions.md#L3-L9` (manual removal of mappings).
- Possible new decision (not opened here): should `uninstall zpm` in a source namespace ask, or refuse, when other
  namespaces borrow IPM? Today it only prints names (and, per F-NSP-7, may print nothing).
- Add F-NSP-7's new impact to `findings/07-09.md` (self-uninstall warning lost).

---

## Summary

| ID | Verdict | Resolution (one line) | Code matches? |
|---|---|---|---|
| DEC-UNIN-1 | RESOLVED-EVIDENCE | Blocked top-level uninstall fails with an error naming dependents; `-r` keeping a shared need still succeeds. | No (F-UNIN-6) |
| DEC-UNIN-2 | RESOLVED-EVIDENCE | `zpm clean` is an ordinary level-0 Clean; orphan warning and self-uninstall handling only on real removal. | No (new F-UNIN-11) |
| DEC-UNIN-3 | RESOLVED-ASSUMPTION | Phase-scoped needs do not block uninstall (native: they already don't; row premise was wrong). | Yes |
| DEC-REPO-1 | RESOLVED-EVIDENCE | Keep fallback flag → token → user/password → any (PR #332); fix the help; restore ORDER BY. | Fallback yes; order no (F-REPO-10); F-REPO-3 becomes doc + one-per-kind |
| DEC-REPO-2 | RESOLVED-EVIDENCE | A disabled registry is not used by any command (search, publish -r included); `repo -list` still shows it. | No (F-REPO-7) |
| DEC-REPO-3 | RESOLVED-ASSUMPTION | Changing an ORAS registry's address clears its credentials, as for remote. | No (new F-REPO-11) |
| DEC-REPO-4 | RESOLVED-EVIDENCE | `config` with an unknown key, bad value or missing argument returns an error status. | No (F-REPO-8) |
| DEC-REPO-5 | RESOLVED-ASSUMPTION | ORAS `unpublish R/M V` deletes the plain tag and every `V__<platform>` tag. | No (new F-REPO-12) |
| DEC-REPO-6 | RESOLVED-EVIDENCE | Plain text is the accepted interim; IRIS security wallet is the planned store (issue #628). | Yes (interim) |
| U23 (DEC-UNIN-3 run) | Closed | Native probe run; scoped needs ignored by uninstall, `show`, `list-dependents`; removed by `-r`. | — |
| U21 | Closed | Full cleanup in the source strips IPM from borrowing namespaces, keeps their modules; warning lost via F-NSP-7; reinstall restores. | Warning: no (F-NSP-7) |

New defects proposed: **F-UNIN-11** (Low, `zpm clean` false orphan warning and self-uninstall path), **F-REPO-11**
(Medium, ORAS keeps credentials on address change), **F-REPO-12** (Medium, ORAS unpublish of a deployed version).
Reclassified: **F-REPO-3** (fallback is intended; keep only the stale help and the one-per-kind flag rule). New impact
for **F-NSP-7** (self-uninstall warning lost).

## Revision

- 2026-11-13: created (lane repo, round 4). Probes and logs listed above; no existing file edited.
