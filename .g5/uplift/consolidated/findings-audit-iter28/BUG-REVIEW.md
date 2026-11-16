# Findings review against the sealed iteration 28 register

Starting inventory: 165 primary IDs (C 31, G 54, P 80). Referenced-only retired IDs and raw builder labels remain in their inherited ID registry; they are not invented new findings. The frozen archive and all starting entries are saved in `sealed/`; the live pre-edit files are in `before/`.

The scope decision is distinct from truth: inconclusive entries remain possible defects, and historical observations remain evidence. No ordinary untested claim is retained as an active functional defect. A component entry states its missing caller. Security entries have only defensive source/existing-evidence review. No protected Need fix, Defer or Irrelevant entry status appeared in the nine starting registers; source-supported human determinations and withdrawals remain recorded.

<a id="c-d2"></a>
### C:D2

Disposition: active · Reproduction: e2e reproduced

TR7 missing-row cleanup warning; TR1 succeeds

[Canonical ontology](../../../ontology/features/load.g5.md#load.dependencies.local-repo)

Starting evidence and claim:

| C:D2 | Low | When IPM cannot create a load's temporary `.modules` repository, it prints "Failed to clean up repository 'ipm-temp-modules-N'. You may need to manually delete it" for a repository that does not exist. The cache build rolled the row back, but the in-memory object keeps its ID, so `CleanUp` deletes a missing row. The operator is told to delete a repository that is not there. | Cleanup reports only repositories it failed to remove (the message's own wording). | native TR7 (top level), Dtxn and Ddev (dependency level): message printed, `repos.count=0` afterwards (`20261114T025102Z-r1-temp-repo`, `20261114T025551Z-r1-eval-postcommit`); model-checked `temp_repo.py` (`cleanup`, variant `cleanup-quiet` rejected) | `General/TempLocalRepoManager.cls:65-67`, `:73-85`; `Repo/Filesystem/Definition.cls:124`, `:180` |

<a id="c-d3"></a>
### C:D3

Disposition: active · Reproduction: e2e reproduced

cyclic Default reaches ordinary load error; acyclic neighbor succeeds

[Canonical ontology](../../../ontology/concepts/modules.g5.md#module.parameter)

Starting evidence and claim:

| C:D3 | Low | Parameters that refer to each other in a loop are never reported. The 20-level guard that throws "Circular reference or too many levels in placeholders" is unreachable, because the `while` condition already stops at zero. The loop leaves `${a}` unfilled, and using it then fails with `<REGULAR EXPRESSION>` (see C:D4) instead of the named error. | The product's own error message and guard (`Storage/ModuleSetting/Default.cls:66-75`). | native E30 (`a=${a} b=${a}`, no error), E18 and E35 (`<REGULAR EXPRESSION>`) (`20261114T025636Z-r1-evaluator`); model-checked `attr_evaluator.py` (variant `resolve-raises-on-cycle` rejected) | `Storage/ModuleSetting/Default.cls:62-110` |

<a id="c-d4"></a>
### C:D4

Disposition: active · Reproduction: e2e reproduced

literal dollar Default makes load fail; five succeeds

[Canonical ontology](../../../ontology/concepts/modules.g5.md#module.parameter)

Starting evidence and claim:

| C:D4 | Medium | A parameter value that contains `$` cannot be used. `%RegExReplace` passes the value as ICU replacement text and escapes only backslashes, so `$5` or an unfilled `${x}` is read as a group reference and raises `<REGULAR EXPRESSION>`. Any attribute that uses the parameter (file target, invoke argument, message) fails the phase that evaluates it. | Values are inserted literally; the backslash escaping shows the intent (`Utils/Module.cls:2811`, `:2813`). | native E32 (`price=$5`, `{$price}` raises) with neighbors E33 (`{$namespace}` in the same module works) and E34 (backslashes kept) (`20261114T025636Z-r1-evaluator`); model-checked `attr_evaluator.py` (variant `regex-escape-dollar` rejected) | `Utils/Module.cls:2804-2815` |

<a id="c-d5"></a>
### C:D5

Disposition: active · Reproduction: e2e reproduced

a.b Default changes {$axb} in the command output; exact ordinary value neighbor

[Canonical ontology](../../../ontology/concepts/modules.g5.md#module.parameter)

Starting evidence and claim:

| C:D5 | Low | A parameter name is placed into a regular expression unescaped, so a dotted name such as `a.b` also fills `{$axb}`. Dotted names are the documented way to nest parameters. | A name matches only itself ([module.parameter](../../../ontology/concepts/modules.g5.md#module.parameter)). | native E21 (`{$axb}` -> `DOT`) (`20261114T025636Z-r1-evaluator`); model-checked (variant `regex-literal-name` rejected) | `Utils/Module.cls:2810`, `:2812` |

<a id="c-d6"></a>
### C:D6

Disposition: active · Reproduction: e2e reproduced

ordinary dependency load installs 2.5.0 for <2; valid dependency neighbor Dok

[Canonical ontology](../../../ontology/concepts/versions.g5.md#version.range)

Starting evidence and claim:

| C:D6 | High | An operator on a partial version is dropped. `<2` means `2.x` (`>=2.0.0 <3.0.0-0`), `>=1.2` and `>1.2` mean `1.2.x`, `<=1` means `1.x`. A dependency declared `<2` installs `2.5.0`; `>=1.2` refuses when only `1.3.0` is offered and picks `1.2.0` over `1.5.0`. Only four-digit-year versions with a one-digit minor escape this, by a special case. | Comparing with the missing parts as zero: IPM's test `TestIRISVersions` requires `>=2022.1` to accept `2022.2`; the range code quotes node-semver; P's ontology DEP-D2 gives `>=2.0` as a range. | native X26-X29, S1, S2, S3 (`20261114T030608Z-r2-semver`, `20261114T031048Z-r2-resolve`); model-checked `semver.py` (variant `range-partial-keeps-operator`, the intended reading, rejected by every one of those fields) | `General/SemanticVersionExpression/Range.cls:84-97`, `:166-183` |

<a id="c-d7"></a>
### C:D7

Disposition: active-component-only · Reproduction: partial reproduced

real range parser witness and valid caret neighbors; ordinary resolver caller with ^1.2 not re-executed

[Canonical ontology](../../../ontology/concepts/versions.g5.md#version.range)

Starting evidence and claim:

| C:D7 | Medium | `^1.2` (a caret on a two-part version) means `1.2.x` instead of `>=1.2.0 <2.0.0-0`. The branch written for it is unreachable, because a two-part version is always flagged as an x-range first. | IPM's test asserts `^1.2.x` equals `>=1.2.0 <2.0.0-0`; the node-semver caret rule quoted in the code. | native X20 (and neighbors X17, X21, X22 matching the tests) (`20261114T030608Z-r2-semver`); model-checked (variant `range-caret-major` rejected) | `General/SemanticVersionExpression/Range.cls:125-137` |

<a id="c-d8"></a>
### C:D8

Disposition: active · Reproduction: e2e reproduced

named diamond load refusal with source-selected dependency bounds; successful neighbor

[Canonical ontology](../../../ontology/concepts/versions.g5.md#version.range)

Starting evidence and claim:

| C:D8 | Medium | Combining two requirements depends on their order when two bounds differ only by a prerelease part: the first is kept. `^1.0.0` AND `<2.0.0` gives `<2.0.0-0`, the reverse gives `<2.0.0`. Because the resolver visits dependencies by name and combines the current requirement with earlier ones, a solvable diamond (`1.5.0` meets `^1.0.0` and `<2.0.0`) is refused when the parents are named `dep-a`/`dep-b`, and installs `1.5.0` when they are named `dep-1`/`dep-0`. | AND is intersection (doc comment `SemanticVersionExpression.cls:120-128`); P's resolver proof assumes it (premise D2). | native A1-A4, S4ab, S4ba, S5 (`20261114T030608Z-r2-semver`, `20261114T031048Z-r2-resolve`, `20261114T031514Z-r2-resolve-order`); model-checked (variants `compare-prerelease-aware`, `intersect-last-wins-tie` rejected) | `General/SemanticVersionExpression/Range.cls:270-292`, `:349-389`; `Storage/Module.cls:1095-1106` |

<a id="c-d9"></a>
### C:D9

Disposition: active · Reproduction: e2e reproduced

ordinary module load stores the malformed parsed build version; ordinary 1.0.0 neighbor

[Canonical ontology](../../../ontology/concepts/versions.g5.md#version)

Starting evidence and claim:

| C:D9 | Low | Build metadata containing a hyphen is parsed as a prerelease: `1.2.3+build-x` becomes `1.2.3-x+build-x`, which sorts before `1.2.3` and matches only itself as a requirement. | SemVer 2.0.0 item 10 (build identifiers may contain hyphens), which the version class cites. | native V9, X45; model-checked (variant `parse-build-hyphen-correct` rejected) | `General/SemanticVersion.cls:50`; `SemanticVersionExpression/Comparator.cls:55` |

<a id="c-d10"></a>
### C:D10

Disposition: active-component-only · Reproduction: partial reproduced

real AND helper error text; full conflicting-dependency command message path remains

[Canonical ontology](../../../ontology/concepts/versions.g5.md#version.range)

Starting evidence and claim:

| C:D10 | Low | The `&&` conflict error prints literal placeholders: "No overlapping versions in && operation for %1 and %2". `$$$ERROR($$$GeneralError, ...)` takes one argument, so the two ranges are dropped. Two disagreeing exact versions also bypass `And`'s module-and-constraints message and report "Exact version matches do not agree". | The message's own placeholders; the `And` message format at `:156-160`. | native X33, X34, A5 | `General/SemanticVersionExpression.cls:37-39`; `Range.cls:247-249` |

<a id="c-d12"></a>
### C:D12

Disposition: active · Reproduction: e2e reproduced

history find displays ERROR #00 for an unfinished record; completed history neighbor

[Canonical ontology](../../../ontology/features/history.g5.md#history.record)

Starting evidence and claim:

| C:D12 | Low | `history find` shows the result of an action that has not ended (still running or killed) as `ERROR #00: (no error description)`, so an operator reads it as a failure with no reason. `SuccessToString` passes the initial value `0` to `GetErrorText`. | The wiki identifies such records by their missing duration, not as errors (`wiki/09` L72). | native H4d, H4f (`20261114T032054Z-r3-history.partial-purge-failed`); model-checked `history_display.py` (variant `success-unfinished-blank` rejected) | `General/History.cls:474-483` |

<a id="c-d13"></a>
### C:D13

Disposition: active · Reproduction: e2e reproduced

module-version unchanged refusal; patch bump succeeds

[Canonical ontology](../../../ontology/features/developer-tools.g5.md#devtools.authoring.module-version)

Starting evidence and claim:

| C:D13 | Low | `module-version <module>` with no version, or with the current version, fails with "Cannot set version to X as it is not strictly higher than the current version X" instead of reporting it unchanged. The "Version is unchanged - Skipping." branch comes after the higher-version check, so it is reachable only with `-force`. Changing only the build (`-build`) is refused the same way. | The code's own unchanged branch (`Main.cls:4067-4072`). | native MV5 (`20261114T033125Z-r4-owners`); model-checked `owners.py` (variant `mv-unchanged-skips` rejected) | `Main.cls:4055-4072` |

<a id="c-d14"></a>
### C:D14

Disposition: active · Reproduction: e2e reproduced

ordinary -env load feeds altered dollar value to its real Invoke reader; five neighbor

[Canonical ontology](../../../ontology/concepts/modules.g5.md#module.env)

Starting evidence and claim:

| C:D14 | Medium | An OS environment value used in an `-env` file through `${VAR}` is inserted as ICU replacement text: `a$1b` becomes `aCONS_DOLLARb` (the variable's name), `a\b` becomes `ab`, and `a$9b` raises `<REGULAR EXPRESSION>` and empties the whole configuration. Passwords and Windows paths are typical values with these characters. | Values are inserted as written (`General/EnvironmentConfig.cls:55-58`: "evaluate ${myVarName} to the value of the environment variable"). | native U7, U8 (`20261114T033831Z-r5-env`, container env `CONS_DOLLAR=a$1b`, `CONS_BS=a\b`, `CONS_DOLLAR9=a$9b`); model-checked `env_config.py` (variant `evaluate-literal-value` rejected) | `General/EnvironmentConfig.cls:69-77` |

<a id="c-d15"></a>
### C:D15

Disposition: active · Reproduction: e2e reproduced

ordinary prerelease downgrade accepted; release and mixed-release downgrades refused

[Canonical ontology](../../../ontology/features/load.g5.md#load.refusal.downgrade)

Starting evidence and claim:

| C:D15 | Medium | The downgrade refusal misses prereleases of different versions: with `2.0.0-beta` installed, `load` of `1.0.0-rc` succeeds and replaces it. `Follows` treats the two as unordered, so neither "installed follows new" nor the reverse holds. Both inherited models encode versions as totally ordered values and predict a refusal here. | "IPM never replaces an installed module with an older version" ([load.refusal.downgrade](../../../ontology/features/load.g5.md#load.refusal.downgrade); PR #887 review). | native HB1-HB2 with neighbors HR1-HR2 and HM1-HM2 refused (`20261114T034428Z-r5-heldback`); model-checked `semver.py` (`_heldback`) | `Utils/Module.cls:1212-1217`; `General/SemanticVersion.cls:153-160` |

<a id="c-d17"></a>
### C:D17

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/features/artifact-download.g5.md#artifact.authentication)

Starting evidence and claim:

| C:D17 | Medium | Artifactory's metadata request uses credentials, but its new byte request sends none. Protected bytes fail after successful authenticated search; public bytes succeed. Owner: [artifact.authentication](../../../ontology/features/artifact-download.g5.md#artifact.authentication). | Resource processor's documented authenticated download, and the deliberate metadata token/Basic support. | Native TLS requests: Bearer metadata -> 200, unauthenticated GET -> 401; authenticated developer-mode failure remains an error. Verus `artifacts.rs::credentials_cross_endpoint` derives both results from requests and a same-credential provider. Proposed request repair checked at the abstract boundary only. | `ArtifactoryTarball.cls::{GetArtifactPath,DownloadArtifact,RetrieveArtifact}`; [iter3 index](../iter3/INDEX.json) |

<a id="c-d18"></a>
### C:D18

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/features/artifact-download.g5.md#artifact.archive-admission)

Starting evidence and claim:

| C:D18 | High | A malformed non-gzip artifact can be reported as successfully extracted and activated with an empty folder. The stream read reports an error and returns empty data; the tar reader ignores the status and treats the empty name as completion. Owner: [artifact.archive-admission](../../../ontology/features/artifact-download.g5.md#artifact.archive-admission). | A reported read error must fail, as other extraction exceptions and checksum errors do. This does not require rejecting a valid empty archive. | Native failed gzip read, successful wrapper return, ordinary/developer artifact activation, valid-archive success and bad-checksum refusal. Verus `header_action` uses the observed header port; proposed read-status check preserves successful reads and rejects failed ones. Gzip decoding and general archive safety remain unproved. | `General/Archive.cls::Extract`; `Utils/FileBinaryTar.cls::{ExtractFile,ExtractStream}`; [native receipt](../iter3/native/receipt.json) |

<a id="c-d19"></a>
### C:D19

Disposition: active-component-only · Reproduction: partial reproduced

existing allowed deployed-helper override shows local status suppression; no ordinary default-helper failure/full package entry established; no new package/archive run

[Canonical ontology](../../../ontology/features/build-and-publish.g5.md#build.package.deployed-status)

Starting evidence and claim:

| C:D19 | Medium | Document export overwrites the deployed-item helper's error before its error guard. A controlled failing override followed by a real source export returns OK. Owner: [build.package.deployed-status](../../../ontology/features/build-and-publish.g5.md#build.package.deployed-status). | The helper returns status and the following error guard is meant to propagate it. | Confirmed local suppression with an allowed processor override; an ordinary default-helper failure in a full package command is still unobserved. Final `%Studio.Project.Check` can prune missing items and return OK, so it is not a universal backstop. Verus as-built and repaired status flow checked separately. | `Default/Document.cls::{OnExportItem,OnExportDeployedItem}`; `Lifecycle/Base.cls::{ExportSingleModule,%Export}`; [iter3 index](../iter3/INDEX.json) |

<a id="c-d16"></a>
### C:D16

Disposition: active · Reproduction: e2e reproduced

first ordinary load -lock writes valid audit-lock JSON in process cwd instead of source root; Root is populated by return, so the final record is not evidence of its earlier availability to the writer; developer-mode neighbor writes at source

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps.lockfile)

Starting evidence and claim:

| C:D16; G:OBS-L3 lockfile-location item | Medium | First plain `load -lock` uses an empty stored Root and writes relative `module-lock.json` in process cwd. A developer-mode or direct nonempty-Root neighbor writes at the module root. A later failed phase can roll database rows back while keeping the lock file. | [deps.lockfile](../../../ontology/features/dependencies.g5.md#deps.lockfile); source General/LockFile.cls:29-85 and Utils/Module.cls:1416-1424; [held-out native/formal receipts](../extension/INDEPENDENT-REVIEW.md), including correct JSON module name. |

<a id="c-d20"></a>
### C:D20

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo.remote.download.credentials)

Starting evidence and claim:

| C:D20 | Medium (suspected, policy-dependent) | A remote registry's download request goes to whatever URL the registry returns and carries the repository's Basic credentials and API key there, even on a different host and port. Owner: [repo.remote.download](../../../ontology/concepts/repositories.g5.md#repo.remote.download). | Credentials belong to one host: CHANGELOG 0.10.11 `#1115` (ORAS credential checks stop following redirects "so credentials are not sent to another host") and `#649` (remote credentials cleared on URL change). Commit `d6cc4204` deliberately follows the returned URL, so the decision is C:Q14. | Native R-cross: provider on `127.0.0.1:18502` recorded `Authorization: Basic ...` and `apikey` after metadata from `localhost:18501`. Verus `registry.rs::lemma_cross_origin_carries_credentials`; proposed repair C:S14 checked at the abstract request only. | `Repo/Remote/PackageService.cls::GetModule`; `Repo/Http/PackageService.cls::GetHttpRequest` |

<a id="c-d21"></a>
### C:D21

Disposition: active-component-only · Reproduction: partial reproduced

real private converter exposed by subclass and valid inside-path neighbor; sync routing reachability remains untraced

[Canonical ontology](../../../ontology/features/developer-tools.g5.md#devtools.sync.test-paths)

Starting evidence and claim:

| C:D21 | Low | The test path converter strips the resource folder as raw leading text. A path outside the folder keeps its folders in the class name, although the method comment promises "", and a sibling folder sharing the prefix (`tests/unitX` for `tests/unit`) yields a wrong class (`X.A.B`). Owner: [devtools.sync.test-paths](../../../ontology/features/developer-tools.g5.md#devtools.sync.test-paths). | The method's own comment. | Native C cases on the real private method through a subclass; Verus `repairs4.rs::rel_path_neighbors`. Whether sync routing ever hands this method such a path is not traced. | `ResourceProcessor/Test.cls::RelPathToClassName` |

<a id="c-d22"></a>
### C:D22

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo.remote.download.integrity)

Starting evidence and claim:

| C:D22 | Low | A registry hash in uppercase hex is refused as a mismatch even when it is the right digest, because only the computed SHA-1 is lowercased. Owner: [repo.remote.download.integrity](../../../ontology/concepts/repositories.g5.md#repo.remote.download.integrity). | A hex digest names the same bytes in either case; the code lowercases one side, which shows the intent to normalize. | Native R-upper; Verus control `hash-compared-case-insensitively` rejected. Whether any registry sends uppercase is unknown. | `Repo/Remote/PackageService.cls::GetModule` |

<a id="c-d23"></a>
### C:D23

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/concepts/modules.g5.md#module.webapp.validation)

Starting evidence and claim:

| C:D23 | Low | A web application attribute that is not a `Security.Applications` property is accepted silently, so a misspelled setting has no effect and no error. The processor's unknown-attribute check is never called, because the lifecycle's Validate step does not dispatch to processors. Owner: [module.webapp.validation](../../../ontology/concepts/modules.g5.md#module.webapp.validation). | The processor's own check and message; CHANGELOG 0.10.11 `#806` quotes that message as user-visible. | Native W9; Verus `webapp.rs::refused_at_validate`, control `validate-dispatches`. | `ResourceProcessor/WebApplication.cls::OnPhase`; `Lifecycle/Base.cls::%Validate` |

<a id="c-d24"></a>
### C:D24

Disposition: active · Reproduction: e2e reproduced

$zpm(version) throws after command; Shell(version) returns success

[Canonical ontology](../../../ontology/root.g5.md#ipm.shell.forms)

Starting evidence and claim:

| C:D24 | Medium | The `$zpm("<command>")` function form raises `<COMMAND> Function must return a value` after running the command, pass or fail, so scripts cannot read the status through it. Commit `8eb520fd` lowercased the extension body to `do … quit`; the generator still rewrites only lines containing `Do ##class`. Owner: [ipm.shell.forms](../../../ontology/root.g5.md#ipm.shell.forms). | The generator's rewrite to `Quit ##class(...)` shows the function form must return the status; it did before `8eb520fd`. The root page claimed it does. | Native Z4/Z5 and the installed `%ZLANGF00` text; Verus `entry_forms.rs::function_returns_value`, controls `matcher-case-insensitive`, `pinned-body-capitalized`. | `Main.cls::{LanguageExtensions,UpdateLanguageExtensions}` |

<a id="c-d25"></a>
### C:D25

Canonical finding/question: [P:F-UNIN-1](../../../../playbook-uplift/BUGS.md#p-f-unin-1).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain this ID as a forwarding record. C:D25 also shares P:F-UNIN-6 for its returned-status projection.

[Canonical ontology](../../../ontology/root.g5.md#ipm)

Starting evidence and claim:

| C:D25 = P:F-UNIN-1 + P:F-UNIN-6 (G:OBS-L17) | High | A blocked uninstall deletes the module's web applications, keeps the module and returns success. Promoted to canonical ownership, not new. Owner: [uninstall.dependents.as-built](../../../ontology/features/uninstall.g5.md#uninstall.dependents.as-built). | Help text, `#058`, issue #58 and the skipped test `TestUninstallDependencyDoesError`. | P native `nc-unin-1-blocked-uninstall` and P proof; iteration 5 native W6/W7 and Verus `webapp.rs::lemma_blocked_uninstall_deletes`. | `Storage/Module.cls::ExecutePhases`; `Lifecycle/Base.cls::{%Clean,CheckBeforeClean}` |

<a id="c-d26"></a>
### C:D26

Disposition: inconclusive · Reproduction: none newly qualified

Observed dropped declaration is narrower than update, and the source does not settle upgrade resource ownership. Keep current facts under module.webapp.upgrade; request policy at module.webapp.upgrade.policy. No accepted cleanup obligation inferred.

[Canonical ontology](../../../ontology/concepts/modules.g5.md#module.webapp.upgrade)

Starting evidence and claim:

| C:D26 | Medium (suspected) | Upgrading to a version that no longer declares a web application leaves it, and uninstall then never removes it. Owner: [module.webapp.upgrade](../../../ontology/concepts/modules.g5.md#module.webapp.upgrade). | "`uninstall <module>` removes an installed module's resources" ([uninstall](../../../ontology/features/uninstall.g5.md#uninstall)); the app was created for this module. Whether an upgrade should remove dropped resources is not stated anywhere. | Native W10; Verus `upgrade_drops_declaration`, control `upgrade-removes-app`. Tested with `load` of a higher version, not `update`. | `Storage/Module.cls::ExecutePhases`; `Lifecycle/Base.cls::%Clean` |

<a id="c-d27"></a>
### C:D27

Disposition: active · Reproduction: e2e reproduced

supported error-returning resource extension; uninstall deletes metadata while class remains callable; successful extension neighbor

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall.child-error)

Starting evidence and claim:

| C:D27 | Medium, reachable extension-error defect | A supported processor returns an error from OnResolveChildren. Package refuses, but uninstall discards the status, deletes module/manifest metadata and leaves the class callable. | [Owning rule](../../../ontology/features/uninstall.g5.md#uninstall.child-error); fresh custom-processor runtime with called counters and valid neighbor; [formal distinction](../../../Verus_verification/consolidated/iter10/errors.rs). General uninstall removal is the governing obligation. | Unfixed. Preserve returned error before metadata deletion. Earlier removed resources require a separate partial-effects contract. Default processor failure is not asserted. |

<a id="c-d28"></a>
### C:D28

Disposition: active · Reproduction: e2e reproduced

real class edit lock; uninstall succeeds with callable class retained; unlocked uninstall removes it

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall.class-delete-error)

Starting evidence and claim:

| C:D28 | Medium, reachable cleanup discrepancy; intended warning policy open | A supported editing lock makes OBJ.Delete refuse and keep a class. Uninstall warns, returns success and deletes module/manifest metadata; class remains callable. | [Owner](../../../ontology/features/uninstall.g5.md#uninstall.class-delete-error), direct API calibration + real command + unlocked neighbor, [Verus](../../../Verus_verification/consolidated/iter11/deletion.rs). The general removal rule is inferred from product help; warning code is deliberate, with no inspected human rationale. | Unfixed. C:Q21 asks whether refusal should preserve metadata and fail the command. Keep the observable result separate from accepted policy. |

<a id="c-d29"></a>
### C:D29

Disposition: active · Reproduction: e2e reproduced

ordinary FileCopy Overlay=0 retains unrelated sentinel and writes new payload; real Unix filesystem provider

[Canonical ontology](../../../ontology/concepts/modules.g5.md#module.processor.file-copy.unix-overlay)

Starting evidence and claim:

| C:D29 | Medium, pinned Unix replacement discrepancy | FileCopy Overlay=0 loads new files but retains unrelated old entries instead of replacing the directory. Direct CopyDir flags0/1 both keep a sentinel and replace the same-name payload. | Explicit Overlay property documentation, Unix argument slots, observed processor Overlay0 and full load; [owner](../../../ontology/concepts/modules.g5.md#module.processor.file-copy.unix-overlay), [model](../../../Verus_verification/consolidated/iter14/protection.rs). | Unfixed. Preserve intended replacement. Correct current Unix provider correspondence; no product repair or Windows native result. |

<a id="c-d30"></a>
### C:D30

Disposition: inconclusive · Reproduction: none newly qualified

Existing ORAS service/install ordering evidence was read. A new tag-to-installed-byte witness would enter excluded archive delivery; no new ORAS install/packaging run was attempted.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo.oras.build-readers)

Starting evidence and claim:

| C:D30 | Low, reachable ordering/listing discrepancy; policy C:Q9 open | Two valid deep-build tags collapse to equal encoded comparison fields, so normal service/SQL listing reports+a.1.a while decoded total order and actual manager/install select+a.1.z. | Aggregate docs promise latest-first; IPM comparator deliberately orders Build alphabetically. Actual source manifests, packages, raw tags, helper fields and final readers checked; [owner](../../../ontology/concepts/repositories.g5.md#repo.oras.build-readers), [model](../../../Verus_verification/consolidated/iter15/oras.rs). | Unfixed. No install failure/corruption claim. An isolated decode-only change is not a validated complete repair because manager key omits Build. |

<a id="c-d31"></a>
### C:D31

Disposition: active · Reproduction: e2e reproduced

actual outer/child Shell test runs, final file/device identities and alone neighbors

[Canonical ontology](../../../ontology/features/build-and-publish.g5.md#build.test-output.run-identity)

Starting evidence and claim:

| C:D31 | Medium, reachable test-report identity defect | A failing outer test runs a passing child through ordinary Shell. Outer device/status identify the failed outer run, but the outer JSON output file contains the passing child's results. | Test output promises full results for the selected run; ReportTestResults already supplies its tracked ID to the device but omits it from ToFile. Actual outer3/child4 allocation, raw rows, file/device JSON, alone neighbors and explicit-ID API comparison are retained in [iteration17](../iter17/HANDOFF.md); [owner](../../../ontology/features/build-and-publish.g5.md#build.test-output.run-identity), [model](../../../Verus_verification/consolidated/iter17/report_identity.rs). | Unfixed. Forward the selected testIndex to ToFile; C:S21 checks this scoped proposal. Command still fails correctly. Other export formats and general nested/provider failures are not claimed as natively qualified. |

<a id="c-d32"></a>
### C:D32

Disposition: active · Reproduction: e2e reproduced

ordinary explicit-version install reads changed directory bytes; stable/rebuilt/refusal neighbors

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo.install.version-request)

Starting evidence and claim:

| C:D32 | Medium, reachable filesystem requested-version defect | Cache a valid1.0.0 directory, then replace its manifest/code with2.0.0 without rebuilding the cache. Search/API stilloffer1, but ordinary explicit `install temporal/iter21-cache 1.0.0` succeeds and installs/calls2.0.0 payload202. | Product install help defines the requested version of the module to install; pinned-install source comment confirms requested filtering. Initial unchanged1/payload101, changed-source2, rebuilt-cache2, old-request refusal and valid2 neighbors are natively qualified in [iteration21](../iter21/HANDOFF.md). [Intent](../../../ontology/concepts/repositories.g5.md#repo.install.version-request), [as-built owner](../../../ontology/concepts/repositories.g5.md#repo.filesystem.mutable-source), [model](../../../Verus_verification/consolidated/iter21/temporal.rs). | Unfixed. Cache snapshot and directory pointer diverge from current source; no archive, concurrent-edit, parser-error or other-delivery claim. No product repair was applied or certified. |

<a id="c-d33"></a>
### C:D33

Disposition: active · Reproduction: e2e reproduced

default snapshot reinstall refuses; shallow/release/explicit-permission neighbors

[Canonical ontology](../../../ontology/concepts/versions.g5.md#version.range.snapshot-intersection)

Starting evidence and claim:

| C:D33 | Medium, reachable default snapshot-reinstall refusal | Install a parent with exact dependency `1.0.0+snapshot`, change that dependency's source payload, then ordinarily reinstall the parent. The forced refresh adds `>=1.0.0+snapshot`; exact-expression intersection drops build metadata and throws NoOverlap, leaving callable payload 1. Both constraints individually accept the snapshot. | Product-authored reinstall/shallow help and [snapshot intent](../../../ontology/features/install.g5.md#reinstall). [Range owner](../../../ontology/concepts/versions.g5.md#version.range.snapshot-intersection), [iteration24](../iter24/HANDOFF.md), [composition](../../../Verus_verification/consolidated/iter24/snapshot_dependencies.rs). Shallow retains1; explicit `-DPermitDowngrade=1` reaches reload and parent/dependency both return2; release default retains1. | Unfixed. One exact dependency per parent and ordinary filesystem sources. No product repair or generic range/graph/provider/rollback claim. |

<a id="g-d1"></a>
### G:D1

Disposition: active · Reproduction: e2e reproduced

dependency row absent after parent rollback; successful load neighbor

[Canonical ontology](../../../ontology/features/history.g5.md#history.survives-rollback)

Starting evidence and claim:

### D1: A dependency's history record is lost when its parent's transaction rolls back

- Expected: a dependency load gets its own record. The record survives any rollback that undoes the dependency, reading `Committed=0`.
- Observed: the record's ID is allocated and no row persists after the parent's rollback. This holds in normal mode when the parent or the dependency fails, in developer mode when a dependency fails inside the second transaction and at a process end inside the parent's transaction. A failed dependency load leaves no trace at all.
- Governing source: `history.survives-rollback` (`features/history.g5.md:67`, "This includes a parent load's rollback of a dependency it loaded"), `history.dependencies` (`:171`), `history.committed` (`:125`, "A dependency undone that way reads uncommitted"). Issue #366: "'install' (including as a dependency)" and "if install rolls back we still need the history". Round 1 disposition R1-ONT-01.
- Mechanism: `LoadModuleReference` creates the dependency's log inside the parent's transaction (`Utils/Module.cls:64-68` → `General/HistoryTemp.cls:21`, the durable twin insert). Its `%OnClose` persist (`HistoryTemp.cls:62-73`) runs inside the same transaction and deletes the IRISTEMP row. The parent's rollback loop (`Utils/Module.cls:1358-1362`) then removes the twin.
- Evidence:
  - Native A1, A2, A3 (`RN/20261113T182759Z-cases-a`, replay `RN/20261113T184230Z-replay-fast`) and reproduction repro X1 (`RN/20261113T184617Z-repro-dep-history-lost`).
  - B1-C31, C32, C33, C36, C37 (`RN/20261113T193952Z-replay-return-b`), B1-C45T, C47T (`RN/20261113T194122Z-replay-terminate-b`), B1-C45K, C47K (`RN/20261113T194155Z-replay-sigkill-b`).
  - M5N and the dependency row of M5 (`RN/20261113T204405Z-stagec2`). Follow-up N-R05-Q23F and N-R05-NESTF (`FRN/20261113T234817Z-r05-modes-nesting`).
  - Model V1 and V2: lemmas `case_c31_dep_record_lost`, `case_c33_dev_dep_record_lost`, `case_c45_kill_dep_record_lost`, `case_c45_terminate_dep_record_lost` (`RV/20261113T222933Z-asbuilt-proofs-r3`, rerun `FR/20261113T232529Z-r12-asbuilt-proofs`: 263 verified, 0 errors). Explorer violations rerun on the final inputs: `FR/20261113T233033Z-asbuilt-r3-dep`, `FR/20261113T233032Z-asbuilt-dep-clauses`. The final load model loses dependency rows as built (`FR/20261114T004842Z-final-loadtx-model`).
  - The corrected replay evaluator changed no verdict (`FR/20261114T002646Z-final-replay-reeval/delta.md`).
- Impact: after a failed install or load, history does not show which dependencies were loaded or which one failed. The audit trail issue #366 asked for is lost exactly when it is needed.
- Decision: none for the defect. Q-30 decides the repair design. Its option A is one transaction plan per load (S8) plus variant 103 with the guarded close, the ordering rule and S3. Its option B keeps per-activation modes (Q-22 A) and extends variant 103 so that a dependency whose own `NoJournal` applies marks its row at its own close. Q-06 (`history.dependencies.parent-rollback`, `:176`) decides which `Success` the surviving record shows, which could not be observed because the record is lost. Q-08 decides what an unfinished dependency row must say (see the residual below).
- Missing evidence: every dependency case used a `.modules` filesystem repository. Remote and ORAS registries as the dependency source were not run, because they need registry services. Dependencies of `install` and `update` were not run natively. The driver helper sources of the uplift's final replays (`replay-return-b`, `replay-terminate-b`, `replay-sigkill-b`, `replay-c51-c52`) were never committed and the follow-up did not re-execute those cases (brief E-3).
- Direction: write the dependency's durable row so that no rollback removes it. Set `Committed=1` on it only when the dependency's writes are durable. This is the round 3 mechanism (disposition R3-02), with the corrections from the final load model (`FR/20261114T004842Z-final-loadtx-model`). That model also checks `Committed` on records that exist but have not ended (clause COMMITTED-UNENDED, Q-08's provisional answer), which earlier counts skipped (brief R-1).
  1. Insert the row with process journaling off for that save only (`%IPM.Utils.JournalManager(0)` around the `History` save in `HistoryTemp.Init`), with `Committed=0`. Premise R-01 is confirmed natively: such a row survives a full rollback, `TROLLBACK 1` and SIGKILL, with no `<ROLLFAIL>` (N-P20, `RN/20261113T214135Z-premise-p20-r01`).
  2. Write the facts that must survive (package, version, phases, `Success`, `TimeEnd`) with unjournaled updates as they become known.
  3. Set `Committed=1` with a journaled update as the row's last write before the commit that returns the load to its entry `$TLEVEL` (C2 in developer mode, C3 in normal mode). When no IPM transaction is open, write it at the dependency's close. A rollback then undoes the journaled update and the row reads `Committed=0` again (N-P20C).
  - Constraints and residuals found by the follow-up (brief L-9 to L-12 and R-1 to R-4):
    - No unjournaled write may touch the row between the journaled mark and the commit. A rollback restores a journaled write only if the node still holds the value that write set (premise P-14). Natively, an unjournaled insert, a journaled `Committed=1` and an unjournaled `Finalized=1` left both values at 1 after the rollback (N-P21A and N-P21R in `FRN/20261113T234716Z-p21-ordering-r2`). The first run `FRN/20261113T234651Z-p21-ordering` is INCOMPLETE for N-P21B.
    - Finalize dependency rows at the top-level close only when no transaction of the load is open. A Terminate between the mark and the commit runs the top-level `%OnClose` before IRIS rolls back (P-06b). The repair design finalizes dependency rows at that close with unjournaled writes, so the mark survives the rollback and a rolled-back dependency reads `Committed=1` (60 end states in a window two statements wide). The crate's variant 103 models the mark as atomic with the commit and cannot show this. The guarded close removes exactly these 60 end states. The mutant that writes the rows unjournaled between the mark and the commit is detected.
    - A dependency whose own `<Defaults>` set `NoJournal` keeps its writes when the parent rolls back (D14). If Q-22 keeps that behavior, its row must read `Committed=1`, so it has to be marked durably at the dependency's own close and not at the parent's commit. Variant 103 marks it at the parent's commit. It leaves finalized dependency rows wrong in 902 end states (842 with a dependency's own `NoJournal` and the 60 window states) and unfinished dependency rows wrong in 10,775 end states (1,559 with a dependency's own `NoJournal` and the 9,216 of the next item).
    - The guarded close does not fix the `NoJournal` case. Those rows stay unfinalized and still read the wrong `Committed`. The unfinished violations with a dependency's own `NoJournal` rise from 1,559 to 2,338. Finalized dependency violations drop to 81, all with a dependency's own `NoJournal`.
    - Every repair variant keeps the residual the original handoff named (`uplift/ipm-verus-20261113/handoff.md:80`: "In NoTransaction and NoJournal, residual violations come from D12 and the Q-08 deviation"). A SIGKILL of a load that runs without a transaction, before the dependency's close, leaves dependency rows reading `Committed=0` with kept writes (9,216 end states). As built, where such rows exist, 10,368 unfinished rows read `Committed=0` with kept writes (9,216 at SIGKILL and 1,152 at Terminate).
    - Only REPAIR-103-GUARDED-GLOBAL has no other dependency residual. It adds one transaction plan per load, in which dependencies ignore their own transaction defaults (Q-30 option A with S8), to the guarded close. The earlier single-plan evidence, variant M-GLOBAL-MODE, loses dependency rows as built and cannot show this (brief R-4).
    - The dependency repair does not address the top-level row. Unfinished top rows read `Committed=0` with kept writes in 29,042 process-end states as built and in REPAIR-103. REPAIR-103-GUARDED-GLOBAL leaves 28,141. That is D26 (Q-08). The top-row part of S2 is not modeled.
    - Do not keep the current `Committed` logic with a surviving row. Rolled-back dependencies would then read 1 through `Finalize`'s "committed or OK", the no-transaction marker and their own C3 marker after a nested commit (`Utils/Module.cls:1347-1351`). Variant REPAIR-NAIVE has 2,712 `Committed` violations on finalized rows.
    - Never a journaled insert followed by unjournaled updates (N-P20C2: the unjournaled value survives the rollback).
    - A write-then-correct policy and a deferral of dependency rows to the top-level close both fail at process ends (round 3 variant 16; B1-C45K and B1-C45T).
  - Writers: `HistoryTemp.Init`, `PersistToTwin`, `%OnClose`, `Utils.Module:LoadModuleReference`, the `LoadNewModule` error path. Readers: `history find` and `details`, `%IPM_General.History`, `Records`.
  - Preserve: a top-level failure reads `Committed=0`. A committed dependency of a successful parent reads `Committed=1`. ID order follows start order. Finalized rows stay immutable.
  - Distinguishing checks: A1 expects one dependency row with `Committed=0`. A3 expects a dependency row with an error `Success`. A5 and B1-C38 expect the dependency row with `Committed=1`. B1-C45K and C45T expect the dependency row to survive with `Committed=0`. A0 keeps two committed rows. The N-P21R write order must not occur on any row.
  - Design: S2 and S3, with S8 for the per-dependency mode.
- Aliases: OBS-L1, OBS-H5, V1, V2, repro X1, B1-C31, B1-C32, B1-C33, B1-C36, B1-C37, B1-C45K, B1-C45T, B1-C47K, B1-C47T (dependency part), native A1, A2, A3, M5N, the dependency row of M5, N-R05-Q23F (record part), N-R05-NESTF (record part).



<a id="g-d2"></a>
### G:D2

Disposition: active · Reproduction: e2e reproduced

history delete removes three seeded identities despite unmatched Version_Major; Package filter preserves nonmatches

[Canonical ontology](../../../ontology/features/history.g5.md#history.read.filter)

Starting evidence and claim:

### D2: A filter column with `_` drops every later filter, so `history find` widens and a filtered `history delete` can remove every record

- Expected: several filters must all match. Versions are filtered by part, such as `Version_Major`. Filters work the same way for `find` and `delete`. `history delete` deletes the records that match its filters.
- Observed: `ConstructSQLWhere` walks the filter keys in collation order and stops at the first key whose name fails `^[a-zA-Z][a-zA-Z0-9]*$` (`General/History.cls:103-105`). That key and every later key are dropped. The delete gate in `Main:History` passes because a filter was given (`Main.cls:4165`). A filtered `find` then returns rows the filter excludes. A filtered delete whose first key is such a column runs with no condition.
- Trigger class: any filter on a column whose name contains `_`, which includes every version-part column that `history schema` lists and wiki/09 L112 recommends. The keys are walked in collation order, where the first character decides (`P` before `V` before `p`). A `Package` key sorts before `Version_Major` and keeps its condition (N-R04-SEL2), while a lowercase `package` key sorts after it and is dropped (N-R04-SEL). Widening happens wherever such a column falls. A delete removes every record only when that column is the first key.
- Governing source: `history.read.filter` (`features/history.g5.md:204`), `history.delete` (`:253`), wiki/09 L112.
- Evidence:
  - Archived E1, E2, E8 and E4 (`RN/20261113T182845Z-cases-ef`, replay `RN/20261113T184230Z-replay-fast`). Reproductions X2 (`RN/20261113T184621Z-repro-history-filter-widened`) and X3 (`RN/20261113T184623Z-repro-history-delete-underscore`, final `RN/20261113T211000Z-repro-history-delete-underscore`). The X3 script backs up and restores the history it deletes.
  - Follow-up, read-only: N-R04-SEL returned all 9 rows where the contract selects 1 and the deliberately wrong model "skip invalid keys" predicted 2. N-R04-SEL2 returned 2 rows where the contract selects 1 (`FRN/20261113T234810Z-r04-history-readers`).
  - Model `FR/20261114T002647Z-final-history-readers-model`: 24 local and 12 global delete commands remove rows they should keep. All 69 plain-column commands match the contract.
- Impact: data loss. A user who follows the documented advice to filter by version part can delete a namespace's whole history without `-confirm`. With `-globally` the same holds for every namespace.
- Decision: none for the defect. Q-10 (`history.read.filter.invalid-column`, `:214`) decides how an unknown column is treated. Every option it lists rejects silent dropping.
- Missing evidence: the widening delete was not re-created in the follow-up, by its ground rules. The global variant rests on the model and the code.
- Direction: accept `_` in names and check each name against the table's columns, as `history schema` lists them. Reject any other name with an error instead of quitting the loop. Apply this in `GetHistory`, `DeleteHistory` and `DeleteHistoryGlobally`.
  - Preserve: E5 (no filter and no `-confirm` is refused), E6 (a single-package delete removes one row), the SQL-injection guard on column names.
  - Distinguishing checks: N-R04-SEL returns 1 row. E2 lists 0 rows. E4 deletes nothing.
  - Design: S4.
- Aliases: OBS-H1, E1, E2, E4, E8, X2, X3, N-R04-SEL, N-R04-SEL2, H-1.



<a id="g-d3"></a>
### G:D3

Disposition: active · Reproduction: e2e reproduced

ordinary list triggers retention; old row survives with marker today; retention-off neighbor

[Canonical ontology](../../../ontology/features/history.g5.md#history.retention)

Starting evidence and claim:

### D3: The retention purge deletes nothing and still marks the day's purge as done

- Expected: with `HistoryRetain` set to N days, IPM deletes records that started more than N days ago, on any IPM command at most once per calendar day, in every namespace.
- Observed: `AutoPurgeOldHistory` builds the filter value `"<='" _ cutoff _ "'"` (`General/History.cls:173`). `ConstructSQLWhere` strips the `<=` and binds the rest, quotes included (`:107-109`), so no row matches. `SetHistoryLastPurge` (`:176`) then stamps today, so the purge does not retry that day.
- Governing source: `history.retention` (`features/history.g5.md:299`).
- Evidence: F1 (`RN/20261113T182845Z-cases-ef`, replay) and reproduction `RN/20261113T184626Z-repro-autopurge-noop`, where an adapter control without the quotes deleted exactly the two old rows. Follow-up read-only N-R04-CUTOFF: the cutoff is `2026-10-14 00:00:00`, the quoted comparison selects 0 rows and the unquoted one selects the 40-day-old row (`FRN/20261113T234810Z-r04-history-readers`). Model variant M-NO-QUOTES (`FR/20261114T002647Z-final-history-readers-model`).
- Impact: retention never works, so history grows without bound although the user configured a limit. The setting is instance-wide (`%SYS` `^IPM.settings`) and the purge runs before every shell command in every namespace.
- Decision: none for the defect. Once it is fixed, Q-29 decides whether the purge may remove the record of an action that started before the cutoff and has not ended. Option A (purge by start time only): the fixed purge removes such a record, as the readers model's M-NO-QUOTES variant shows. Option B (skip records whose action has not ended): the purge filter needs an end-time condition. Q-20 (`history.delete.running-records`, `:276`) covers explicit `history delete` only. Purge failures are OBS-H12 (Q-13).
- Missing evidence: a purge that deletes was not re-created in the follow-up.
- Direction: bind `"<="_cutoffDate` without quotes. Stamp `lastPurge` only after a delete whose per-namespace errors were reported, which needs D23's fix. Today the stamp is skipped only when `DeleteHistoryGlobally` itself throws (`General/History.cls:174-179`). Under Q-29 option B, skip records whose action has not ended.
  - Preserve: retention 0, the default, deletes nothing. At most one purge per day.
  - Distinguishing checks: F1 deletes both 40-day-old rows and keeps the 1-day-old row. N-R04-CUTOFF selects 1 row.
  - Design: S4. Related: D5 (a reset can leave retention on) and D23 (the purge runs through `DeleteHistoryGlobally`, which hides errors).
- Aliases: OBS-H3, F1, N-R04-CUTOFF, H-2, H-7 (the running-record consequence).



<a id="g-d4"></a>
### G:D4

Disposition: active · Reproduction: e2e reproduced

SYSNamespace refusal retains developer module; ordinary-mode refusal leaves none; policy remains open

[Canonical ontology](../../../ontology/features/load.g5.md#load.refusal.requirements.timing)

Starting evidence and claim:

### D4 (Q-17): Refused `-dev`, NoTransaction or NoJournal loads keep the module and read committed when system requirements fail

- Expected under the provisional answer: IPM checks system requirements before it writes anything, in every mode, so the refusal writes no module data and reads uncommitted.
- Observed: the manifest is imported and saved. In developer mode the first commit follows (`Utils/Module.cls:1309-1314`). Only then does `CheckSystemRequirements` run (`:1321`). Without a transaction, `SetCommitted` runs at `:1227` before any write. The module stays listed as installed with `Committed=1`.
- Governing source: `load.refusal.requirements` (`features/load.g5.md:124`) and the open question `load.refusal.requirements.timing` (`:129`). wiki/03 L261: requirements "must be validated before module installation".
- Evidence: B2, B3, B4 (`RN/20261113T182820Z-cases-b`, replay) and reproduction `RN/20261113T184630Z-repro-sysreq-dev-manifest-left`. B1-C23 and C24 (`RN/20261113T193952Z-replay-return-b`). Model V3, lemma `case_c23_sysreq_dev_keeps_manifest`. Neighbours B0 and B1 conform. A2 OBS-L10 predicted it from the code and called it consistent with the comment at `:1310`.
- Impact: a module that cannot run in this namespace appears installed and has to be uninstalled by hand.
- Decision: Q-17. Option A (provisional, check before writing): a defect. Option B (a refused developer-mode or NoTransaction load keeps its manifest, as developer mode keeps what it did, #363): intended. `Committed=1` is then correct.
- Missing evidence: requirement kinds other than `SYSNamespace` were not run.
- Direction (option A): check the requirements on the manifest object that `GetModuleObjectFromPath` returns (`:1164`), before the transaction starts and before `SetCommitted`.
  - Preserve: B0 (met requirements in developer mode install and commit). B1 (a normal-mode refusal rolls back). Developer mode keeps the entries of an installable module that fails later (#363).
  - Distinguishing check: B2, B3 and B4 leave no module record and read `Committed=0`.
  - Design: S1, which chooses the provisional answer.
- Aliases: OBS-L10, V3, B2, B3, B4, B1-C23, B1-C24.



<a id="g-d5"></a>
### G:D5

Canonical finding/question: [P:F-REPO-4](../../../../playbook-uplift/BUGS.md#p-f-repo-4).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D5: `config delete HistoryRetain` restores the first value ever set, not the default 0

- Expected: `config delete` restores a setting's default and the default of `HistoryRetain` is 0.
- Observed: `ResetToDefault` reads the stored default node (`Repo/UniversalSettings.cls:101`). `SetValue` seeds that node with the first value ever set (`:178-179`) and `HistoryRetain` has no install-time default.
- Governing source: `settings` (`concepts/settings.g5.md:13`), `history.retention` (`features/history.g5.md:299`).
- Evidence: F0 and reproduction `RN/20261113T184628Z-repro-config-delete-retain`. Also seen after the product's own `TestAutoPurgeOldHistory`, which ends with `HistoryRetain=30` (`RN/20261113T182514Z-suite-test-CLI`).
- Impact: a user who deletes the setting to turn retention off leaves it on for the whole instance. D3 hides this today. Once D3 is fixed, records would be purged by a setting the user believes is gone.
- Decision: none.
- Missing evidence: other settings without an install-time default were not checked natively.
- Direction: give `HistoryRetain` an explicit factory default of 0 in `ResetToDefault`, as `ModuleRoot` has (`:97-99`), or seed the default node at install. Do not seed it from a user's first `SetValue`.
  - Preserve: settings that do seed their default at install.
  - Distinguishing check: F0 returns 0.
  - Design: S6.
- Aliases: OBS-H4.



<a id="g-d6"></a>
### G:D6

Disposition: active · Reproduction: e2e reproduced

ordinary returned-error Activate Invoke leaves last phase OPEN; successful C01 phase is OK; intent remains Q-14

[Canonical ontology](../../../ontology/features/history.g5.md#history.phases.returned-errors)

Starting evidence and claim:

### D6 (Q-14): An Invoke that returns an error leaves its phase open in a finalized record

- Expected under the provisional answer: every phase that starts records its end and status, including when an `<Invoke>` returns an error status.
- Observed: the before-phase and after-phase Invoke loops in `ExecutePhases` exit with `quit:$$$ISERR(tSC)` (`Storage/Module.cls:608-610` and `:669-671`). That leaves the try block without entering the catch (`:692-697`). `$$$ThrowOnError(tSC)` (`:707`) then throws before `FinalizePhase` (`:710`). This applies to every phase with an Invoke, not only Activate.
- Governing source: `history.phases` (`features/history.g5.md:107`, wiki/09 L161: a phase with no end time "never finished") and the open question `history.phases.returned-errors` (`:115`).
- Evidence: reproduction `RN/20261113T184634Z-repro-activate-invoke-phase-open` (an Invoke bound to Activate). B1-C28 (`RN/20261113T193952Z-replay-return-b`). Model V5, lemma `case_c28_phase_left_open`. Invokes on the default Configure phase, whose errors are thrown, finalize Activate correctly (A1, A5).
- Impact: an ordinary error looks like a hang or a crash in history.
- Decision: Q-14. Option A (provisional): a defect. Option B (accept open phases for returned errors and document it): documented behavior. I1's display then matters more.
- Missing evidence: phases other than Activate were not reproduced natively. The code shows the same exit for each.
- Direction (option A): call `pLog.FinalizePhase(tOnePhase, tSC)` whenever the phase ends with an error, returned or thrown, before `$$$ThrowOnError`.
  - Preserve: phase timings of successful phases.
  - Distinguishing check: the reproduction shows Activate with an end time and the Invoke's error.
  - Design: S5.
- Aliases: OBS-H14, V5, B1-C28.



<a id="g-d9"></a>
### G:D9

Disposition: active · Reproduction: e2e reproduced

ordinary downgrade refusal followed by history details; named successful history neighbor

[Canonical ontology](../../../ontology/features/history.g5.md#history.fields)

Starting evidence and claim:

### D9: A refused load records an empty package although the manifest named the module

- Expected: `Package` is empty only if the action failed before any name was known. A refusal after IPM read the manifest records the name.
- Observed: `Main.cls:2420` starts the log with `LoadInit("")`. The name is set only by `pLog.SetName` (`Utils/Module.cls:1293`), which runs after the downgrade and update-required refusals (`:1213-1222`) and after the manifest checks (`:1251-1264`). The refusal message itself names the module.
- Governing source: `history.fields` (`features/history.g5.md:79`), wiki/09 L69. Round 1 disposition R1-ONT-03.
- Evidence: g3, g6, g7 (`RN/20261113T182832Z-cases-dg`, replay) and R4 (`RN/20261113T184444Z-cases-hr`: a package filter finds 14 of the 20 rows whose command names the module). B1-C09 to C15 and C18 (`RN/20261113T193952Z-replay-return-b`), B1-C51 and C52 (`RN/20261113T205137Z-replay-c51-c52`). Model V7. The g-cases' CONFORMS verdict predates the package expectation that R1-ONT-03 added.
- Impact: filtering history by package misses refused attempts, so a user cannot see that a downgrade or update of that module was refused.
- Decision: none.
- Missing evidence: none material.
- Direction: call `pLog.SetName` and `SetVersion` right after `GetModuleObjectFromPath` succeeds (`Utils/Module.cls:1164`).
  - Preserve: a malformed manifest still records an empty package (`TestDevMode`).
  - Distinguishing check: g3 records the module name. B1-C11 sets the package.
  - Design: S1.
- Aliases: V7, B1-C09 (package part), B1-C10 (package part), B1-C11, B1-C12, B1-C13, B1-C14, B1-C15, B1-C18, B1-C51, B1-C52 (package part), g3, g6, g7, R4.



<a id="g-d10"></a>
### G:D10

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed the persisted/temp history writers and the old SIGKILL/Terminate receipts. The safe return cohorts do not exercise process death; an unmodified running command plus a post-death durable-row reader remains unqualified in this audit.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D10: The durable record of an action that never ended keeps only its start values

- Expected: after an action stops without ending, the log lists its record with `Success` 0, no end time, the phases it started and the package it had named.
- Observed: every setter after `Init` updates only the IRISTEMP row (`General/AbstractHistory.cls:86-173`). The durable twin is written at `Init` (`HistoryTemp.cls:21`) and at `%OnClose` (`:62-73`). After a SIGKILL no `%OnClose` runs, so the twin keeps an empty package, the placeholder version and no phases. The IRISTEMP row is orphaned. After a Terminate of a normal-mode load, `%OnClose` runs before IRIS rolls the transaction back, so its twin update is undone and the IRISTEMP row is deleted.
- Contrary evidence: A2 OBS-H6 predicted the start-value row for a Terminate too. Premise N-P10 refuted that, because `%OnClose` does run at a Terminate. For a developer-mode load at `$TLEVEL` 0, a Terminate writes the real values and finalizes them (C2, see D27). The ontology renamed OBS-H6 to D10 (`features/history.g5.md:362`).
- Governing source: `history.record` (`features/history.g5.md:41`), `history.fields` (`:79`). Round 1 disposition R1-MOD-03.
- Evidence: C1 (`RN/20261113T182903Z-cases-c-terminate`, replay), C3 (`RN/20261113T182927Z-cases-c-sigkill`), C4R (`RN/20261113T183827Z-cases-c4r`). B1-C44T, C45T, C47T (`RN/20261113T194122Z-replay-terminate-b`), B1-C44K, C45K, C46K, C47K, C48K (`RN/20261113T194155Z-replay-sigkill-b`), B1-C49K approximate (`RN/20261113T205218Z-replay-approx-c`). Model V11; `repair103_c45` shows the D1 repair leaves this defect in place.
- Impact: after a crash, history cannot tell which module or which phase was running.
- Decision: none.
- Missing evidence: IRISTEMP at an instance restart (premise P-12) was not run. No new process end was run in the follow-up. The driver helper sources of the uplift's T and K replays (`replay-terminate-b`, `replay-sigkill-b`) were never committed and the follow-up did not re-execute those cases (brief E-3).
- Direction: use D1's mechanism for every log. At each `SetName`, `SetVersion` and phase boundary, write the name, version and phases to the durable row with an unjournaled update, so the copy survives the load's own rollback and a process end at any `$TLEVEL`. Write `Committed` with a journaled update as the row's last write before each commit.
  - The ordering constraints under D1 apply to the top-level row too. The follow-up model checked the Terminate window between the mark and the commit only for dependency rows (L-12), so the top-level close needs the same guard. The top-row part of this repair (S2) is not modeled (brief R-3).
  - Preserve: finalized rows stay immutable. `Success` stays 0 and `TimeEnd` empty for an unfinished record. A rolled-back normal-mode load reads `Committed=0`.
  - Distinguishing checks: C3 lists Initialize to Compile ended and Activate open, with the package set and `Committed=0`. C4R shows the same phases with `Committed=1`.
  - Design: S2.
- Aliases: OBS-H6 (renamed), V11, C1, C3, C4R (phases and package), B1-C44K, B1-C44T, B1-C45K, B1-C45T, B1-C46K, B1-C47K, B1-C47T, B1-C48K, B1-C49K (top-record fields).



<a id="g-d11"></a>
### G:D11

Disposition: active · Reproduction: e2e reproduced

NoJournal manifest refusal leaves the imported ZPM document at 2.0.0; successful NoJournal neighbor; broader cases remain in starting evidence

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D11: Manifest refusals under `-DNoTransaction=1` or `-DNoJournal=1` keep module data

- Expected: a refusal happens before IPM writes any module data, so in every mode the namespace stays as it was. IPM refuses a manifest with more than one document or no module definition before it touches any resource.
- Observed: under NoJournal, `StudioDocument/Module.cls` `ImportStream` saves the `.ZPM` document and rolls back its own nested transaction when validation refuses the module, but the save was not journaled, so it stays (B1-C10). Under NoTransaction, a manifest with several documents or no module definition is refused after `$system.OBJ.Load` has saved its documents (`Utils/Module.cls:1251-1260`), which keeps the module record, the `.ZPM` document and the extra class.
- Governing source: `load.refusal` (`features/load.g5.md:83`), `load.refusal.manifest` (`:99`), wiki/09 L133 ("rejected before any resource was touched").
- Evidence: B1-C10 (`RN/20261113T193952Z-replay-return-b`: document version 2.0.0 kept, `Committed=1`), B1-C52 (`RN/20261113T205137Z-replay-c51-c52`), S2 and S2B (`RN/20261113T204152Z-stagec`), with neighbour S2N conforming. Model V12, lemma `case_c52_multiple_documents_kept`.
- Impact: a refused load leaves a partial module record or a stray class. Its record reads committed, so a user may believe nothing was installed.
- Decision: none.
- Missing evidence: other validation refusals under NoJournal were not enumerated.
- Direction: validate the manifest before any write, or delete the saved documents on refusal when no transaction is active.
  - Preserve: valid NoJournal loads (B1-C04, which replaces the original file's "B4 neighbour", itself a D4 case).
  - Distinguishing checks: B1-C10, B1-C52, S2 and S2B leave no module data and read `Committed=0`.
  - Design: S1.
- Aliases: V12, B1-C10 (namespace part), B1-C52 (namespace part), S2, S2B.



<a id="g-d12"></a>
### G:D12

Disposition: active · Reproduction: e2e reproduced

no-module-definition refusal under NoTransaction keeps no module and reports committed=1; ordinary successful NoJournal neighbor

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D12 (Q-31): A NoTransaction or NoJournal load refused after the no-transaction marker reads `Committed=1` with nothing kept

- Expected under option A of Q-31: a refusal before anything is written is not committed, in any mode.
- Observed: `SetCommitted` runs at `Utils/Module.cls:1226-1228`, before the manifest import (`:1251`), so any refusal after that point reads `Committed=1`.
- Governing source: `history.committed` (`features/history.g5.md:125`), last bullet "A refusal before anything is written is not committed, in any mode" and the property comment `General/AbstractHistory.cls:41-42`. `open_questions.md` Q-31 reads the D block `history.committed.marking` (`:136`, PR #1322: mark "before writing when there is no transaction") and CHANGELOG 0.10.11 #1237 ("one run with `-DNoTransaction=1` or `-DNoJournal=1` reads committed") as authority for the opposite reading.
- Evidence: B1-C08 ("No module definition found.", nothing written) and B1-C09 (`RN/20261113T193952Z-replay-return-b`). Model V4, lemmas `case_c08_committed_without_change` and `case_c09_committed_without_change`, explorer `FR/20261113T233033Z-asbuilt-commit-exact`. The evaluator rerun kept the verdict (`FR/20261114T002646Z-final-replay-reeval`).
- Impact: the wiki's recipe for "failed but committed" records (wiki/09 L218) returns false positives and a user may try to undo changes that do not exist.
- Decision: Q-31. Q-31 has no provisional answer.
  - Option A (a refusal that wrote nothing reads uncommitted in every mode, with every refusal moved ahead of the marker): a defect. `open_questions.md` names A as the most reasonable assumption and the I block `history.committed` states it.
  - Option B (keep the marker where it is): D12 is withdrawn as documented behavior.
  - The independent review argued that the I block alone decides D12. This file keeps it conditional while Q-31 offers option B.
- Missing evidence: none material.
- Direction (option A): validate the manifest before the no-transaction marker, so that no refusal follows it. Refine the D block to "after validation, before the first write". Moving the marker after `OBJ.Load` alone is not safe until D11 is fixed: B1-C10 keeps the `.ZPM` document and would then read `Committed=0`, which breaks the proved rule "changed implies `Committed=1`" (`theorem_commit_sound`).
  - Preserve: changed implies `Committed=1` (B1-C10, B1-C52 and S2 while D11 stands). B3 and B4 while D4 is undecided. A0.
  - Distinguishing check: B1-C08 reads `Committed=0`.
  - Design: S1 and S3.
- Aliases: V4, B1-C08, B1-C09 (`Committed` part).



<a id="g-d13"></a>
### G:D13

Disposition: active · Reproduction: e2e reproduced

ordinary directory load hits failed temporary repository creation after commit; module remains; valid .modules neighbor

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D13: A load that committed reports `<INVALID OREF>` when the temporary `.modules` repository could not be created

- Expected: the returned status and the namespace agree. A failed normal-mode load keeps no module data, so an error must mean nothing was kept.
- Observed: when `TempLocalRepoManager:%OnNew` fails, `%New` returns `""` (`General/TempLocalRepoManager.cls:63-68`). After `LoadNewModule` has committed, `tmpRepoMgr.CleanUp()` (`Main.cls:2530`) throws `<INVALID OREF>`.
- Trigger class: a module whose `.modules` folder holds a manifest the filesystem repository cache rejects.
- Governing source: `load.outcome` (`features/load.g5.md:145`), `load.dependencies.local-repo` (`:189`).
- Evidence: B1-C29 (`RN/20261113T193952Z-replay-return-b`: `ERROR #5002: <INVALID OREF>LoadInternal+83^%IPM.Main.1`, module and classes installed, `Committed=1`). Model V6, lemma `case_c29_error_after_commit`.
- Impact: automation sees a failure and may retry or roll back by other means while the module is installed. The record reads failed but committed.
- Decision: none. If maintainers accept an error after the final commit, that becomes a Q under `load.outcome`.
- Missing evidence: the same pattern exists for archive installs from a repository (`Utils/Module.cls:256-259`) and was not run.
- Direction: check the repository manager before use and report its creation error before the transaction starts. Treat cleanup errors after the commit as warnings.
  - Preserve: the normal-mode rollback of real failures.
  - Distinguishing check: B1-C29 either refuses before writing or succeeds with a warning.
  - Design: S7.
- Aliases: OBS-L12, V6, B1-C29.



<a id="g-d14"></a>
### G:D14

Disposition: policy-only/as-built-observed · Reproduction: e2e reproduced

Ordinary nested load confirms that the dependency NoJournal default escapes parent rollback. This agrees with Q-22 working assumption A; it is a defect only under alternative B. Runtime is settled for the checked pair, policy remains open.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D14 (Q-22): A dependency whose own `<Defaults>` set `NoJournal` escapes its parent's normal-mode rollback

- Expected: two documented rules conflict. `load.outcome` says a failed normal-mode load rolls back the whole load, "including dependencies loaded for it". `transaction.flag-sources` and wiki/00 L175 say `<Defaults>` are "applied whenever this module is installed". wiki/00 L155 says that with `NoJournal`, "if the process crashes mid-install, changes cannot be rolled back".
- Observed: the dependency's `LoadNewModule` merges its own `<Defaults>` under the parameters it receives (`Utils/Module.cls:1164-1169`). The parent never set `NoJournal`, so the dependency's value applies and creates `JournalManager(0)` inside the parent's open transaction (`:1191-1193`). Its unjournaled writes survive the parent's rollback (N-P07). The parent is rolled back and the dependency stays installed with its class and `.ZPM` document, with no `<ROLLFAIL>` and `$TLEVEL` 0.
- Governing source: `load.outcome` (`features/load.g5.md:145`), `transaction.flag-sources` (`concepts/transactions-and-dev-mode.g5.md:43`) and the open question `transaction.flag-sources.dependency-defaults` (`:50`).
- Evidence: M5 with neighbour M5N (`RN/20261113T204405Z-stagec2`). Follow-up N-R05-NESTF and N-R05-PARDEF (`FRN/20261113T234817Z-r05-modes-nesting`). In the loadtx model the as-built product violates the effect-prefix clause in 2,440 end states under option B and in none under option A. A model with one global mode cannot produce M5 (`FR/20261114T004842Z-final-loadtx-model`). Command-line flags override a parent's defaults, which override the dependency's own (L-1).
- Impact: under option B, a load reported as rolled back leaves a dependency installed.
- Decision: Q-22. Option A (provisional, a dependency's own defaults apply): intended behavior. What remains are D1 (the lost record), D21 (the broken dependency closure, decided with Q-25) and D22 (the top record's `Committed`, decided with Q-26). Option B (the parent's transaction governs): a defect, to be repaired by making a nested load inherit its parent's mode or refusing it. D21 is then an alias of D14.
- Missing evidence: none for the behavior. The precedence is a maintainer choice.
- Direction: under option B, ignore `NoTransaction` and `NoJournal` from a dependency's `<Defaults>` while a parent transaction is open, or refuse such a dependency. Under option A, document that the parent's rollback cannot undo such a dependency and settle D1, D21 and D22.
  - Distinguishing check under option B: M5 leaves the dependency uninstalled.
  - Design: S8.
- Aliases: M5 (namespace part), round 1 finding R1-MOD-05.



<a id="g-d15"></a>
### G:D15

Disposition: inconclusive/execution-scope-blocked · Reproduction: none in this audit

Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D15: `load` refuses `.tar.gz` archives

- Expected: the path may be a `.tgz` or `.tar.gz` archive, as the help example `load C:\module\root\path\module-0.0.1.tar.gz` shows (`Main.cls:282`, CHANGELOG 0.10.0 #474).
- Observed: `$piece(path,".",*)` yields `gz`, so `extension="tar.gz"` never matches (`Main.cls:2466-2467`). The path is treated as a folder and the load fails with "File not found to obtain module object: module.xml". This regressed in `f83b207e` (#887), which replaced a two-piece check.
- Governing source: `load.path` (`features/load.g5.md:42`).
- Evidence: S1, with neighbour S1N (`.tgz`) conforming (`RN/20261113T204152Z-stagec`).
- Impact: a documented archive format does not load.
- Decision: none.
- Missing evidence: none material.
- Direction: test the last piece for `tgz` and the last two pieces for `tar.gz`.
  - Preserve: `.tgz` and folder loads.
  - Distinguishing check: S1 loads.
- Aliases: OBS-L2, S1.



<a id="g-d16"></a>
### G:D16

Disposition: inconclusive/execution-scope-blocked · Reproduction: none in this audit

Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D16: `update -path` with another module's archive crashes instead of giving the mismatch refusal

- Expected: IPM refuses an update whose `-path` archive holds a different module, keeps no module data and records the refusal as not committed.
- Observed: `LoadNewModule` opens the installed module by the archive's module name (`Utils/Module.cls:1207`, `""` when that module is not installed) and dereferences it at `:1232`, before the mismatch check (`:1269-1275`) can run. The update fails with `<INVALID OREF>LoadNewModule+67`. Under NoTransaction the record reads `Committed=1` and the archive is left extracted under the `ModuleRoot` folder.
- Contrary evidence: A2 OBS-L6 predicted that the check runs after the manifest was saved and that the other module's manifest stays. S3 refuted that, because the load crashes before the manifest import.
- Governing source: `update.refusal` (`features/update.g5.md:20`), `history.committed` (`features/history.g5.md:125`).
- Evidence: S3 and S3N1 (`RN/20261113T204152Z-stagec`). S3N2, an update from the module's own archive, conforms.
- Impact: an internal error instead of the documented message, a false `Committed=1` under NoTransaction and a leftover extracted folder (I6).
- Decision: none.
- Missing evidence: the case where the other module is also installed, so that the check is reachable, was not run.
- Direction: run the mismatch check on the manifest object before the version checks.
  - Distinguishing check: S3 refuses with the mismatch message and `Committed=0`.
  - Design: S1.
- Aliases: OBS-L6 (prediction refuted), S3, S3N1 (message part).



<a id="g-d17"></a>
### G:D17

Disposition: inconclusive · Reproduction: none newly qualified

Source and earlier witness establish acceptance of relative paths. They do not establish loading an unintended module or an authoritative refusal requirement; retain fact without an active defect verdict.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D17: `load` accepts an existing relative path

- Expected: `load` accepts only absolute paths and git URLs (CHANGELOG 0.10.4 #706: "load now only accepts absolute paths").
- Observed: `Main.cls:2447-2452` refuses a path only when it does not exist, is not absolute and is not a URL. A relative path that `%File` resolves against the process's current directory passes.
- Governing source: `load.refusal.path` (`features/load.g5.md:94`).
- Evidence: S4R, with neighbours S4N1 and S4N2 conforming (`RN/20261113T204405Z-stagec2`). The first binding S4 is INCOMPLETE (the path did not exist for `%File`).
- Impact: the same command loads different modules depending on the process's directory.
- Decision: none.
- Missing evidence: none material.
- Direction: refuse any path that is not absolute and not a URL before the existence check.
  - Distinguishing check: S4R is refused.
- Aliases: S4R, S4.



<a id="g-d18"></a>
### G:D18

Disposition: active · Reproduction: e2e reproduced

stored NoTransaction modifier makes ordinary history find hide seeded row while returning OK; deleting default restores the same row

[Canonical ontology](../../../ontology/features/history.g5.md#history.read.filter.default-modifiers)

Starting evidence and claim:

### D18 (Q-18): Namespace default modifiers become `history` filters

- Expected under the provisional answer: default modifiers apply to package changes and are not `history` filters. `history find` lists records.
- Observed: defaults are merged into every command's data (`Main.cls:1067-1069`). Non-dotted `-D` names are duplicated under `data("zpm",…)` (`CLI.cls:815-819`), which `History` uses as its filter (`Main.cls:4136`). After the product's own help example for `default-modifiers` (`Main.cls:628-629`), which stores `NoTransaction` and `UpdateSnapshots`, `history find` shows no rows with an OK status (D24) and an unfiltered confirmed delete fails with SQLCODE -29.
- Governing source: `history.read.find` (`features/history.g5.md:196`) and the open question `history.read.filter.default-modifiers` (`:223`).
- Evidence: H2 (`RN/20261113T204152Z-stagec`, with the defaults and history restored afterwards). Model `FR/20261114T002647Z-final-history-readers-model` (STATUS violations with a stored default).
- Impact: after following the documented example, history looks empty. With D19 this happens in every namespace.
- Decision: Q-18. Option A (provisional): a defect. Option B (apply defaults as filters and document it): documented behavior, though D24 still hides the resulting error.
- Missing evidence: none material.
- Direction (option A): build the `history` filter only from the command line's own `-D` values.
  - Distinguishing check: H2 lists the 5 rows.
  - Design: S4.
- Aliases: OBS-H2, H2 (filter part).



<a id="g-d19"></a>
### G:D19

Canonical finding/question: [P:F-REPO-5](../../../../playbook-uplift/BUGS.md#p-f-repo-5).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D19: "Namespace" default modifiers are shared by every namespace

- Expected: `default-modifiers` stores flags that IPM adds to every command in the current namespace.
- Observed: `$$$ZPMDefaultModifiers` is `^%IPM.DefaultModifiers` (`src/inc/IPM/Common.inc:31`), a `^%` global stored in IRISSYS and visible from every namespace.
- Governing source: `settings.default-modifiers` (`concepts/settings.g5.md:30`) and the command description in `Main.cls`.
- Evidence: X1 (`RN/20261113T204152Z-stagec`): defaults set in USER are visible in USER-VERIFY.
- Impact: storing `-DNoTransaction=1` in one namespace makes every load in every namespace non-transactional and stored defaults become history filters everywhere (D18).
- Decision: none.
- Missing evidence: none material.
- Direction: store default modifiers in a namespace global, or subscript the global by namespace, with a migration for existing values.
  - Distinguishing check: X1 shows nothing in USER-VERIFY.
  - Design: S9.
- Aliases: OBS-X1, X1, A2 premise P-10 (now observed).



<a id="g-d20"></a>
### G:D20

Disposition: active · Reproduction: e2e reproduced

public GlobalRecords repeats USER history and misses seeded other namespace; local Records returns exactly one correct row

[Canonical ontology](../../../ontology/features/history.g5.md#history.read.api)

Starting evidence and claim:

### D20: The `GlobalRecords` query returns the current namespace's rows once per namespace

- Expected: the class query `GlobalRecords` returns the records of all namespaces.
- Observed: `GlobalRecordsFetch` (`General/History.cls:253-281`) fetches result sets prepared for other namespaces without switching `$namespace`, unlike `DisplayArray` (`:528`).
- Governing source: `history.read.api` (`features/history.g5.md:246`). Issue #366 asked for a "cross-namespace class query".
- Evidence: H10 (`RN/20261113T204152Z-stagec`): `history find -globally` is correct, while the query returned 458 rows, twice USER's 229, with none of USER-VERIFY's 173. Model `FR/20261114T002647Z-final-history-readers-model` (`globalrecords:ROWS`).
- Impact: programs that use the documented API get duplicated rows of one namespace and miss the others.
- Decision: none.
- Missing evidence: the follow-up container has no second IPM namespace, so the follow-up relies on H10 and the model.
- Direction: switch to each result set's namespace while fetching, as `DisplayArray` does. Add a namespace column (I4).
  - Distinguishing check: H10 returns 229 + 173 rows.
  - Design: S9.
- Aliases: OBS-H10 (wrong-rows part), H10.



<a id="g-d21"></a>
### G:D21

Disposition: active · Reproduction: e2e reproduced

ordinary parent failure leaves NoJournal middle dependency while its own child is gone; successful NESTS neighbor

[Canonical ontology](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-closure)

Starting evidence and claim:

### D21 (new, Q-22, Q-25): A failed normal-mode load can leave an installed module without its dependency

- Expected: it depends on two questions. Under Q-22 option B the parent's transaction governs, so a failed normal-mode load keeps no module data (`load.outcome`). Under Q-22 option A (provisional) a dependency whose own `<Defaults>` set `NoJournal` keeps its changes. No I block then says whether that dependency's own dependencies must stay too and `load.outcome` leaves dependency defaults open. That is Q-25.
- Observed: in a chain m → d1 → d2 where d1's own `<Defaults>` set `NoJournal`, the top load's `LoadDependencies` loads the whole closure in dependency order with m's parameters. d2 is loaded before d1 inside m's transaction and does not inherit d1's `NoJournal`. When m fails, d2 is rolled back and d1 stays installed with its class. Neither dependency has a record (D1).
- Mechanism: a module's own defaults are merged under the parameters it receives (`Utils/Module.cls:1164-1169`), the top load passes its parameters to `LoadDependencies` (`:1328`, `:1368-1413`) and an installed version that satisfies the requirement is skipped (`Storage/Module.cls:1139-1154`). Follow-up L-3.
- Governing source: `load.outcome` (`features/load.g5.md:145`) under Q-22 option B. Under Q-22 option A, the open question Q-25. `load.dependencies` (`:179`) loads dependencies before the module and `uninstall.dependents` (`features/uninstall.g5.md:20`) protects installed modules' dependencies. They lean against leaving a module without its dependency but do not decide this case.
- Evidence: N-R05-NESTF (expectation written before the run) and its success neighbour N-R05-NESTS (`FRN/20261113T234817Z-r05-modes-nesting`). The loadtx model relies on this reduction, argued from the source in its docstring (`VV/followup/models/loadtx.py`, receipt `FR/20261114T004842Z-final-loadtx-model`).
- Impact: d1 is listed as installed while its dependency is missing, so its code can fail at run time. The protection that `uninstall.dependents` gives is bypassed.
- Decision: Q-22, with Q-25 if Q-22 is answered A. Q-25 has no provisional answer.
  - Q-22 option B: the same cause as D14, which is then a defect. D21 is an alias of D14 and is fixed with it.
  - Q-22 option A with Q-25 option A (accept it and document it): documented behavior. A failed load can leave an escaped module without its dependencies until the next load.
  - Q-22 option A with Q-25 option B (load a `NoJournal` dependency's own dependencies under its `NoJournal`): a defect.
  - Q-25 option C (avoid the case by answering Q-22 with B): as for Q-22 option B.
  - `open_questions.md` names "not A" as the most reasonable assumption for Q-25.
  - The follow-up brief (L-4) and the first revision of this file called D21 decision-independent. The independent review corrected that, because no I block decides the Q-22 A case.
- Missing evidence: deeper chains, a developer-mode parent and remote dependency sources were not run.
- Direction: decide one transaction plan per load.
  - Under Q-22 option B, nested loads inherit the parent's mode, which fixes D14 and D21 together.
  - Under Q-22 option A with Q-25 option B, load a `NoJournal` dependency's own dependencies with the same `NoJournal`, or refuse the combination, so that what escapes the rollback is closed under dependencies.
  - Distinguishing check: N-R05-NESTF leaves neither d1 nor d2 (Q-22 B), or both (Q-22 A with Q-25 B).
  - Design: S8.
- Aliases: L-4, N-R05-NESTF (namespace part). Under Q-22 option B also D14.



<a id="g-d22"></a>
### G:D22

Disposition: active · Reproduction: e2e reproduced

same parent failure records committed=0 while middle dependency survives; successful parent neighbor

[Canonical ontology](../../../ontology/features/history.g5.md#history.committed.escaped-dependency)

Starting evidence and claim:

### D22 (new, Q-22, Q-26): The top record reads `Committed=0` although a dependency's `NoJournal` changes were kept

- Expected under Q-22 option A (provisional) with Q-26 option A: a dependency with its own `NoJournal` keeps its changes when the parent rolls back and the top record's `Committed` counts those changes as kept by the load.
- Observed: the parent fails and rolls back while the dependency stays installed. The top record reads `Committed=0`.
- Governing source: `history.committed` (`features/history.g5.md:125`) with `transaction.flag-sources.dependency-defaults` (`concepts/transactions-and-dev-mode.g5.md:50`). Whether a parent's `Committed` covers its dependencies' writes is Q-26. The sources define `Committed` per action and do not say.
- Evidence: M5 (`RN/20261113T204405Z-stagec2`: parent `Committed=0`, dependency installed) and N-R05-NESTF (`FRN/20261113T234817Z-r05-modes-nesting`: top `Committed=0`, d1 installed). Loadtx model: 30 end states, all with a dependency's own `NoJournal` (`FR/20261114T004842Z-final-loadtx-model`).
- Impact: the wiki's recipe for "failed but committed" records misses loads that changed the namespace.
- Decision: Q-22 and Q-26. Q-26 has no provisional answer.
  - Q-22 option A with Q-26 option A (mark the top record committed when any dependency's changes were kept): a defect.
  - Q-22 option A with Q-26 option B (keep `Committed` per module and rely on the dependency's own record): D22 reduces to D1, because that record is lost today.
  - Q-22 option B: once D14 is fixed nothing is kept and `Committed=0` is correct.
- Missing evidence: a developer-mode parent was not run.
- Direction (Q-22 A with Q-26 A): when an activation of the load writes outside the parent's transaction, mark the top record durably as committed at that point, with the same write rules as D1.
  - Distinguishing check: M5's parent record reads `Committed=1`.
  - Design: S3 and S8.
- Aliases: L-8, M5 (parent record part), N-R05-NESTF (top record part).



<a id="g-d23"></a>
### G:D23

Disposition: active · Reproduction: e2e reproduced

ordinary global delete with unknown SQL column reports OK and retains sentinel; valid Package-filtered global delete removes it; nonpermission-error policy remains Q-27

[Canonical ontology](../../../ontology/features/history.g5.md#history.delete.global-errors)

Starting evidence and claim:

### D23 (new, Q-27): The global history delete hides every error, not only permission errors

- Expected: `-verbose` shows which namespaces were skipped for lack of permission (`history.delete`, `features/history.g5.md:253`). The code comment says the catch is for permission errors (`General/History.cls:213`, `:221`). A local delete with an unknown column is refused with SQLCODE -29 (E7).
- Observed (code and model): `DeleteHistoryGlobally` catches every exception in each namespace and continues (`General/History.cls:214-225`), so a global delete with an unusable column reports "Deleted 0 record(s)" without an error. The purge goes through the same method.
- Evidence: history readers model, `delete-global:STATUS` in 52 commands (`FR/20261114T002647Z-final-history-readers-model`).
- Impact: a user believes no record matched while the delete never ran.
- Decision: Q-27. Option A (report errors other than missing permission per namespace and return an error): a defect, which the code comments and wiki/09 L195 support. Option B (keep skipping every failing namespace and document it): documented behavior.
- Missing evidence: not run natively. The follow-up container has no second IPM namespace and its ground rules exclude deletes beyond test rows.
- Direction: catch only permission errors and report every other error. Design: S9.
- Aliases: H-4.



<a id="g-d24"></a>
### G:D24

Disposition: active · Reproduction: e2e reproduced

misspelled history column returns OK; valid Success query neighbor

[Canonical ontology](../../../ontology/features/history.g5.md#history.read.filter.invalid-column)

Starting evidence and claim:

### D24: `history find` shows an empty log with an OK status when its query fails

- Expected: filters work the same way for `find` and `delete`. `delete` refuses an unknown column with SQLCODE -29 (E7). Every option of Q-10 requires `find` to report a query it cannot run instead of showing an empty log.
- Observed: `GetHistory` returns the result set of `%ExecDirect` without checking `%SQLCODE` (`General/History.cls:146-147`), unlike `DeleteHistory` (`:157`). A filter on an unknown column, or a stored default modifier (D18), makes `find` print nothing with an OK status.
- Governing source: `history.read.filter` (`features/history.g5.md:204`, "Filters work the same way for `find` and `delete`") and `history.read.find` (`:196`). The open question `history.read.filter.invalid-column` (Q-10, `:214`), extended by round 1 finding R1-ONT-10, decides only how the problem is reported.
- Evidence: E3 (`RN/20261113T182845Z-cases-ef`, whose CONFORMS verdict used the phase 1 expectation "error also acceptable"), the find part of H2 (`RN/20261113T204152Z-stagec`) and follow-up N-R04-UNKNOWN (`FRN/20261113T234810Z-r04-history-readers`: SQLCODE -29 inside the result set, no exception). Model `FR/20261114T002647Z-final-history-readers-model`: `find:STATUS` in 104 commands and `find-global:STATUS` in 52. E7, a delete with an unknown column, is refused correctly.
- Impact: a typing error in a filter column looks like an empty history.
- Decision: none for the defect. Q-10's option A (provisional) has two variants: refuse an unknown column with an error or ignore it with a warning. Both report the problem. The independent review moved D24 from conditional to supported for this reason.
- Missing evidence: none material.
- Direction: check `%SQLCODE` in `GetHistory` and in each namespace of `GetHistoryGlobally` and report the error.
  - Distinguishing check: E3 reports an error.
  - Design: S4.
- Aliases: E3, H-3, N-R04-UNKNOWN, H2 (find part).



<a id="g-d25"></a>
### G:D25

Disposition: active · Reproduction: e2e reproduced

supported caller transaction rollback removes the row/module; the ordinary no-caller successful return retains a finalized row; policy remains Q-05

[Canonical ontology](../../../ontology/features/history.g5.md#history.committed.nested)

Starting evidence and claim:

### D25 (Q-05/Q-06b): A caller's rollback removes the record of a load it wrapped

- Expected under the provisional answer: the record of a load run inside a caller's transaction survives the caller's rollback. `Committed` is exact only for actions started outside a caller transaction.
- Observed: the record's insert, every IPM commit and the final persist nest in the caller's transaction, so the caller's rollback removes the record. In flight the record read `Committed=1` and `Finalized=1`.
- Governing source: the open question `history.committed.nested` (`features/history.g5.md:151`), issue #366 ("if install rolls back we still need the history").
- Evidence: native case D1 (`RN/20261113T182832Z-cases-dg`, replay) and reproduction `RN/20261113T184632Z-repro-caller-tx-history-lost`. B1-C42, C43 (`RN/20261113T193952Z-replay-return-b`), B1-C50T (`RN/20261113T194122Z-replay-terminate-b`), B1-C50K (`RN/20261113T194155Z-replay-sigkill-b`). Model V8, lemma `case_c42_caller_rollback_record_lost`. Neighbour D0 conforms. B1-C41 (`Committed=1` while nothing is durable yet) is permitted under the provisional answer.
- Impact: automation that wraps IPM in its own transaction loses the audit trail when it rolls back. The same happens when a module's Invoke runs a load inside another module's normal-mode load (OBS-H16).
- Decision: Q-05 and its model parameter Q-06b. Option A (provisional, the record survives): a defect. Option C (caller-held transactions are unsupported, so the record may be lost): a documented limitation. Option B (compare `$TLEVEL` at the end with its value at the start) changes only `Committed` and leaves the record as under A.
- Missing evidence: none for the behavior.
- Direction (option A): write the top-level row with D1's mechanism, so its unjournaled insert survives the caller's rollback (R-01 was confirmed at `$TLEVEL` 1 and 2). Document what `Committed` means under a caller transaction.
  - Distinguishing check: native case D1 keeps one record.
  - Design: S2.
- Aliases: the original deviation row "Q-05", OBS-H16, V8, native case D1, B1-C42, B1-C43, B1-C50K, B1-C50T.



<a id="g-d26"></a>
### G:D26

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed old SIGKILL evidence and Q-08. A fresh unfinished action that kept writes, its durable Committed reader and a normal-return neighbor were not run; the existing abnormal-end requirement is preserved.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D26 (Q-08): The record of an action that never ended reads `Committed=0` although its changes were kept

- Expected under the provisional answer: `Committed` reflects the changes kept, even on an unfinished record.
- Observed: `SetCommitted` writes only the IRISTEMP row (`General/AbstractHistory.cls:120-125`) and the durable twin keeps `Committed=0` until `%OnClose` (N-P14). After a SIGKILL of a developer-mode load after its commits, the manifest and classes stay and the durable record reads `Committed=0`.
- Governing source: the open question `history.record.abnormal-end` (`features/history.g5.md:58`), issue #1237 ("should reflect whether the action's changes were actually retained").
- Evidence: C4R (`RN/20261113T183827Z-cases-c4r`), B1-C46K and C48K (`RN/20261113T194155Z-replay-sigkill-b`). B1-C49K and C49T approximate (`RN/20261113T205218Z-replay-approx-c`): a process end between the C3 commit and its `SetCommitted` (`Utils/Module.cls:1348-1350`) leaves an installed module whose record reads `Committed=0`. Model V9, lemma `case_c46_dev_end_inside_activate`. The final load model, which checks `Committed` on unfinished records, finds unfinished top rows reading `Committed=0` with kept writes in 29,042 process-end states (`FR/20261114T004842Z-final-loadtx-model`, clause COMMITTED-UNENDED).
- Impact: the wiki's recipe for "failed but committed" records misses crashed loads that kept changes.
- Decision: Q-08. Option A (provisional): a defect. Option B (an empty end time means the outcome is unknown, so readers must not trust `Committed=0`): a documented limitation.
- Missing evidence: B1-C49T and C49K stay APPROXIMATE (a debugger stall). An exact witness needs a hook between the commit and `SetCommitted`, which the product does not have. No new process end was run in the follow-up. The driver helper sources of the uplift's T and K replays (`replay-terminate-b`, `replay-sigkill-b`) were never committed and the follow-up did not re-execute those cases (brief E-3).
- Direction (option A): D10's mechanism, with the journaled `Committed=1` written before each commit, which also closes the window between the commit and the mark. The D1 repair does not fix this. In the model, its variants leave the same top-row count (29,042 in REPAIR-103 and 28,141 in REPAIR-103-GUARDED-GLOBAL).
  - Distinguishing check: C4R reads `Committed=1`.
  - Design: S2.
- Aliases: the original deviation row "Q-08", V9, C4R (`Committed` part), B1-C46K, B1-C48K, B1-C49K, B1-C49T (`Committed` part).



<a id="g-d27"></a>
### G:D27

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed Terminate close/finalization code, old receipt and Q-21. A fresh termination schedule is not qualified by the return-only cohorts; its desired finalized/end-time relationship remains an owner question.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D27 (Q-21): A Terminate finalizes (locks) the record of an action that never ended

- Expected under the provisional answer: a record without an end time stays unfinalized.
- Observed: at a Terminate or `HALT`, `%OnClose` calls `PersistToTwin`, which sets `Finalized=1` regardless of the end time (`General/HistoryTemp.cls:53`). A developer-mode load ended in Activate at `$TLEVEL` 0 gets a finalized record with `Success` 0, no end time and Activate open.
- Governing source: `history.locked` (`features/history.g5.md:285`) and the open question `history.locked.unfinished` (`:290`).
- Evidence: C2 (`RN/20261113T182903Z-cases-c-terminate`, replay), with reader case R2. B1-C46T and C48T (`RN/20261113T194122Z-replay-terminate-b`), B1-C49T approximate. Model V10.
- Impact: the record can never be corrected, because finalized records reject edits.
- Decision: Q-21 (the model calls it Q-FIN and the original file called it "new Q (round 1)"). Option A (provisional): a defect. Option B (finalize whenever IPM releases the record): intended.
- Missing evidence: the driver helper sources of the uplift's T and K replays (`replay-terminate-b`, `replay-sigkill-b`) were never committed and the follow-up did not re-execute those cases (brief E-3).
- Direction (option A): `PersistToTwin` sets `Finalized` only when `Finalize` has run.
  - Distinguishing check: C2's record is unfinalized.
  - Design: S2.
- Aliases: the original deviation row "new Q (round 1)", V10, C2, B1-C46T, B1-C48T, B1-C49T (`Finalized` part).



<a id="g-d28"></a>
### G:D28

Disposition: active · Reproduction: e2e reproduced

own NoTransaction dependency still rolls back with parent; successful Q23S neighbor

[Canonical ontology](../../../ontology/concepts/transactions-and-dev-mode.g5.md#transaction.flag-sources.dependency-no-transaction)

Starting evidence and claim:

### D28 (Q-23): A dependency's own `<Defaults>` `NoTransaction` does not keep it out of its parent's transaction

- Expected under the provisional answer: a module that declares `NoTransaction` does not run inside a transaction opened for its parent, so a failure may leave it partly installed.
- Observed: the dependency only skips its own `tstart` (`Utils/Module.cls:1172`, `:1194-1196`). It stays inside the parent's transaction, is undone with it (`:1358-1362`) and loses its record (D1). On success it is installed with `Committed=1`.
- Governing source: the open question `transaction.flag-sources.dependency-no-transaction` (`concepts/transactions-and-dev-mode.g5.md:59`), wiki/00 L149 (`NoTransaction` is for modules that "conflict with an outer wrapping transaction").
- Evidence: first native observation, follow-up N-R05-Q23F and its success neighbour N-R05-Q23S (`FRN/20261113T234817Z-r05-modes-nesting`, expectations written before the run). Loadtx model: 5,780 end states violate the effect-prefix clause under option A and none under option B (`FR/20261114T004842Z-final-loadtx-model`).
- Impact: under option A, a module that cannot live inside an outer transaction still runs inside one.
- Decision: Q-23. Option A (provisional): a defect. Option B (the flag affects only the dependency's own load and users pass `-DNoTransaction=1` for the whole command): intended.
- Missing evidence: neither a dependency with both `NoTransaction` and `NoJournal` nor a developer-mode parent was run.
- Direction (option A): resolve each load's transaction plan before the first `tstart`, so a dependency's `NoTransaction` suppresses the parent's transaction or is refused with a clear message.
  - Distinguishing check: N-R05-Q23F keeps the dependency installed after the parent fails.
  - Design: S8.
- Aliases: L-2, N-R05-Q23F (namespace part).



<a id="g-d29"></a>
### G:D29

Disposition: active · Reproduction: e2e reproduced

ordinary update/reinstall/uninstall argument refusals allocate no history ID; successful/failed named load neighbors elsewhere; intent remains Q-09

[Canonical ontology](../../../ontology/features/history.g5.md#history.record.unlogged-refusals)

Starting evidence and claim:

### D29 (new, Q-09): Refused `update`, `reinstall` and `uninstall` attempts leave no record

- Expected: under the provisional answer of `history.record.unlogged-refusals` (`features/history.g5.md:49`), IPM records these refused attempts, as `history.actions` (`:32`) records refused loads and installs.
- Observed (code): `update` refuses `PermitDowngrade` and a module that is not installed (`Main.cls:4415-4424`) before it creates its log (`:4438`). `reinstall` refuses a module that is not installed (`Main.cls:2686-2687`) and `uninstall` refuses a missing name (`Main.cls:2710-2711`). Both happen before any log exists.
- Impact: refused updates leave no trace in history.
- Decision: Q-09. Option A (provisional): a defect. Option B (argument refusals are not attempts): intended.
- Missing evidence: not run.
- Direction (option A): create the log before the argument checks, as `load` and `install` do.



<a id="g-d30"></a>
### G:D30

Canonical finding/question: [P:F-UNIN-2](../../../../playbook-uplift/BUGS.md#p-f-unin-2).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D30 (new): `uninstall -purge` sets a key nothing reads

- Expected: `-purge` deletes the data in the module's tables during uninstall (`uninstall.purge` at `features/uninstall.g5.md:40` and the help at `Main.cls:505`).
- Observed (code): the modifier sets `data("Purge")` (`Main.cls:505`), while the only reader takes `pParams("Configure","Purge")` (`Lifecycle/Base.cls:554`) and passes it to the installer class (`:561`). A1 conflict 28.
- Impact: the module's table data is kept although the user asked for it to be purged.
- Decision: none.
- Missing evidence: not run.
- Direction: map `-purge` to `Configure.Purge`, or read `Purge` in `%Unconfigure`.



<a id="g-d31"></a>
### G:D31

Canonical finding/question: [P:F-INST-9](../../../../playbook-uplift/BUGS.md#p-f-inst-9).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### D31 (new, Q-03, Q-24): `install` replaces a dependency in developer mode without the overwrite guard

- Expected: `dev-mode.overwrite-guard` (`concepts/transactions-and-dev-mode.g5.md:100`) and `install.refusal` (`features/install.g5.md:23`) state the guard for installs with no exception for dependencies. The history page counts a dependency install that IPM starts as an install (Definitions, `features/history.g5.md:18`).
- Observed (code): `install` sets `params("Install")` for the top-level module only. `LoadModuleFromDirectory` removes it before the top module's `LoadNewModule` (`Utils/Module.cls:318-319`), so the guard at `:109` never runs for the dependencies that load installs. They get `DeveloperMode` 0 (`:309-311`).
- Impact: as for I3. A developer-mode dependency is replaced by the repository version during an `install` of its parent.
- Decision: Q-03 and Q-24. Q-03's provisional answer is "keep the current scope", which `open_questions.md` reads as top-level `install` only. Under that reading the I blocks do not reach dependency installs and Q-24 decides, as for I3. If maintainers read the guard as covering every install, D31 is a defect whatever Q-24 says.
- Missing evidence: not run. It needs a registry serving the dependency.
- Direction: as for I3.
- Aliases: the `install` part of L-5.



<a id="g-i1"></a>
### G:I1

Canonical finding/question: [C:D12](../BUGS.md#c-d12).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### I1: An unfinished phase is shown as `ERROR #00: (no error description)`

- Expected: no source requires a display. wiki/09 L161 explains how to read a phase with no end time.
- Observed: `General/History.cls:420` applies `GetErrorText` to the placeholder status 0 of an open phase.
- Evidence: reader case R2 (`RN/20261113T184444Z-cases-hr`).
- Impact: an open phase reads like an error with no description.
- Decision: none. Round 1 disposition R1-MOD-12 moved it from defect D8 to this issue.
- Missing evidence: none.
- Direction: print "not finished" when the phase has no end time. Distinguishing check: R2 shows "not finished".
- Aliases: D8, R2.



<a id="g-i2"></a>
### G:I2

Disposition: inconclusive · Reproduction: none newly qualified

The later-resolution-first consequence is narrowed by TR5 creation-order ties. The interrupted-load registration itself needs a process-end fixture; no fresh interruption was performed and no universal priority claim is retained.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### I2: A load ended mid-way leaves its temporary `.modules` repository registered at first priority

- Expected: `load.dependencies.local-repo` (`features/load.g5.md:189`) says IPM removes the temporary repository when loading finishes, even after an error. No source covers a process end.
- Observed: the repository is created before the transaction and removed only by the explicit `CleanUp()` (`Main.cls:2528-2530`), which a process end skips.
- Evidence: after the uplift's Terminate and SIGKILL replays, six `ipm-temp-modules-N` repositories with `OverriddenSortOrder=-1000` still pointed at the cases' `.modules` folders (`RN/20261113T210229Z-temp-repos-leftover`). They were recorded and then removed.
- Impact: later installs in the namespace resolve dependencies from those folders first.
- Decision: none, unless maintainers extend `load.dependencies.local-repo` to process ends.
- Missing evidence: none.
- Direction: register the temporary repository in a form that does not outlive the process, or remove stale `ipm-temp-modules-*` entries at the start of the next load. Design: S7.
- Aliases: none.



<a id="g-i3"></a>
### G:I3

Disposition: active · Reproduction: e2e reproduced

ordinary parent load upgrades developer dependency and clears developer mode; same-version neighbor retains it

[Canonical ontology](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.dependency-policy)

Starting evidence and claim:

### I3 (Q-03, Q-24): Loading a parent replaces a dependency that is in developer mode and clears its developer mode

- Expected: `dev-mode.overwrite-guard` (`concepts/transactions-and-dev-mode.g5.md:100`) says a module installed in developer mode cannot be overwritten by a normal-mode install and `install.refusal` (`features/install.g5.md:23`) lists the guard among install refusals. Neither I block has an exception for dependencies. Two questions decide whether they reach this case. Whether the guard covers `load` is the open part of Q-03 and whether it protects a dependency is Q-24.
- Observed: when a load needs another version of a dependency that is installed in developer mode, the dependency is replaced and its `DeveloperMode` cleared, with an OK status. Dependencies get `DeveloperMode` set to 0 (`Utils/Module.cls:309-311`). The guard at `:109` needs `params("Install")`, which `load` never sets (`Main.cls:2457-2460`). At the required version the dependency is skipped and keeps developer mode. The same path under `install` is D31.
- Governing source: the two I blocks above, with the open questions `dev-mode.overwrite-guard.force` (Q-03, `concepts/transactions-and-dev-mode.g5.md:107`) and Q-24.
- Evidence: N-R05-DEVUP and the neighbour N-R05-DEVSAME (`FRN/20261113T234817Z-r05-modes-nesting`).
- Impact: a module a developer is changing in this namespace is replaced by a packaged version and loses its developer setup without a warning.
- Decision: Q-03 and Q-24. Q-24 has no provisional answer.
  - Q-24 option A (extend the guard to dependencies, so the parent's load fails unless the developer uninstalls or forces): a defect.
  - Q-24 option C (replace it but warn and name the module): a defect, because no warning is given.
  - Q-24 option B (replace it in normal mode as today and document it): intended, unless Q-03 is decided so that the guard covers `load`. Then the dependency install inside the load is a normal-mode install over a developer-mode module and I3 is a defect.
  - Q-03 option A (provisional) keeps the guard to top-level `install`, so on its own it leaves the `load` case to Q-24.
  - The first revision of this file filed I3 as an issue without a governing requirement. The independent review pointed to the two I blocks, so I3 keeps its ID and is now conditional.
- Missing evidence: remote dependency sources were not run.
- Direction: apply the guard, or at least a warning, when a dependency load would replace a developer-mode module.
  - Distinguishing check: N-R05-DEVUP is refused or warns.
  - Design: S8 (the rule belongs in the load plan).
- Aliases: L-5, N-R05-DEVUP. The `install` variant is D31.



<a id="g-i4"></a>
### G:I4

Disposition: policy-only/source-reviewed · Reproduction: source review; historical observations preserved

The public ROWSPEC lacks namespace/ID. Desired API shape remains Q-28; current query duplication is separately reproduced as G:D20, not a decision to add columns.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### I4 (new): The history class queries return no ID and no namespace

- Expected: no source states the columns. `history.namespace` (`features/history.g5.md:184`) says record IDs are unique only within a namespace.
- Observed: `Records` declares `CONTAINID = 1` while its first column is `Action` (`General/History.cls:26-28`). `GlobalRecords` has neither a namespace nor an ID column (`:233-235`).
- Evidence: static reading. The history readers model notes it.
- Impact: a program cannot open a row's details from the query result. Even with D20 fixed it cannot tell which namespace a `GlobalRecords` row came from.
- Decision: Q-28. Option A (add `Namespace` and `ID` to `GlobalRecords` and make the ID column that `Records` declares real): the missing columns become a change to make. Option B (keep the documented shape and state that rows cannot be attributed): documented behavior.
- Missing evidence: a client that honors `CONTAINID` was not run.
- Direction: add the ID and a namespace column and point `CONTAINID` at the ID column. Design: S9.
- Aliases: H-5, OBS-H11.



<a id="g-i5"></a>
### G:I5

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed Compile status dropping and inherited extension witness. A supported failing processor followed through the complete compile command/external reader is not supplied by C:D27, whose hook is ResolveChildren during uninstall.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### I5: Compile ignores a resource processor's error status

- Expected: no source states it for resource processors. For `<Invoke>`, `module.invoke` (`concepts/modules.g5.md:65`) says an error fails the phase.
- Observed: `%Compile` calls `do tResource.Processor.OnPhase("Compile",.pParams,.tHandled)` and discards the returned status (`Lifecycle/Base.cls:1081`).
- Evidence: code reading only.
- Impact: a processor's compile failure is reported as Compile SUCCESS.
- Decision: none.
- Missing evidence: not run.
- Direction: check the returned status and fail the phase on error, as other processor hooks do (`Storage/Module.cls:683-684`).
- Aliases: OBS-L8.



<a id="g-i6"></a>
### G:I6

Disposition: policy-only/source-reviewed · Reproduction: source review; historical observations preserved

Database rollback does not promise filesystem compensation. The failed-lockfile path is reproduced under P:F-DEP-8; the broader external-compensation choice remains transaction.rollback-scope.policy.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### I6: Files a load writes outside the database outlive its rollback

- Expected: under the provisional answer of Q-04 (`transaction.rollback-scope`, `concepts/transactions-and-dev-mode.g5.md:24`), a rollback promises only database writes. Files, the git clone folder and IRISTEMP data stay. So no source requires the cleanup below.
- Observed (code):
  - Archive loads delete and replace `<ModuleRoot>/<name>/<version>` before any check (`Main.cls:2507-2513`; `Utils/Module.cls:236-242` for repository installs). A same-version archive load that then fails rolls the database back and leaves the failed archive's files in the installed module's folder.
  - The lock file is written at the end of `LoadDependencies` (`Utils/Module.cls:1416-1422`), before the module's phases and commit, so it survives a rollback.
  - Git URL loads clone into `mgr/Temp/<timestamp>/` and never remove it (`Main.cls:2351-2372`). `RunCommandViaZF` returns OK whatever git returns (`Utils/Module.cls:2387-2416`), so a failed clone surfaces later as "File not found to obtain module object".
- Evidence: code reading. S3 observed an archive left extracted after a failed update (D16).
- Impact: the installed module's files can disagree with its database state. A lock file can describe an install that was rolled back. A failed clone gives a misleading error.
- Decision: Q-04. Under option B (restore the whole namespace), each item becomes a suspected defect.
- Missing evidence: none of these paths was run natively. Git URL loads have no native or formal evidence.
- Direction: unpack into a temporary folder and move it into place only after the commit. Write the lock file after the commit. Remove the clone folder when the load ends and fail on a non-zero git exit code.
- Aliases: OBS-L7, OBS-L11, OBS-L13.



<a id="g-i7"></a>
### G:I7

Canonical finding/question: [P:F-DEP-1](../../../../playbook-uplift/BUGS.md#p-f-dep-1).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### I7: `PermitDowngrade` reaches the dependency graph and one guard but not the downgrade refusal

- Expected: `load.refusal.downgrade` (`features/load.g5.md:104`) says nothing lifts the downgrade refusal. `PermitDowngrade` is undocumented except in `update`'s refusal (A3 disagreement 1, CAT-28).
- Observed: `BuildDependencyGraph` honors `PermitDowngrade` (`Storage/Module.cls:1157`) and `LoadModuleReference` skips its developer-mode overwrite guard when it is set (`Utils/Module.cls:103`). `LoadNewModule` refuses every downgrade (`:1213-1217`), so a dependency downgrade the graph accepted fails later with "Cannot downgrade".
- Evidence: code reading.
- Impact: the flag cannot permit the downgrade it names, yet it silently disables the developer-mode guard.
- Decision: none, unless maintainers document the flag.
- Missing evidence: not run.
- Direction: remove the flag, or document it and make the graph, the overwrite guard and the downgrade refusal agree.
- Aliases: OBS-L14.



<a id="g-i8"></a>
### G:I8

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed the NoLock-to-compile-lock argument path. A setter/helper value does not establish a real competing compile-lock schedule; the command/caller/contention gate remains.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### I8 (new): `-DNoLock=1` turns compile locking on

- Expected: no source defines `NoLock` (wiki/02 L447 lists it without a meaning, A1 conflict 30).
- Observed: the code sets `tNoCompileLock = '$get(pParams("NoLock"))` and adds the `-l` (no lock) compile qualifier only when `tNoCompileLock` is true (`Lifecycle/Base.cls:1194-1198`). Without the flag compiles run unlocked and with it they lock.
- Evidence: code reading.
- Impact: a user who sets `NoLock` to avoid locks gets them.
- Decision: none.
- Missing evidence: not run.
- Direction: invert the test, or document the actual meaning.
- Aliases: CAT-28 note, A1 conflict 30.



<a id="g-obs-l3"></a>
### G:OBS-L3

Canonical finding/question: [C:D16](../BUGS.md#c-d16).

Disposition: alias · Reproduction: alias; see canonical owner

Only the first plain-load lockfile location is aliased to C:D16. The broader reading that Root remains absent after return is refuted by the fresh stored-Root reader; later phase/root writers are preserved. Repository absence on a direct directory load is not itself made a defect.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-L3: A plain load does not save `Root` and `Repository`, so the first lock file goes to the wrong folder

- Expected: `-create-lockfile` writes `module-lock.json` in the module root (`deps.lockfile`, `features/dependencies.g5.md:34`). `load.core` (`features/load.g5.md:56`) says install records where a module came from.
- Observed (code): the save at `Utils/Module.cls:1298-1304` runs only when `params("DeveloperMode")` is defined, which a plain load leaves undefined. The in-memory `Repository` (`:1296`) is discarded by `%Reload` (`:1332`). The lock file is written before that reload, from an empty `Root` (`General/LockFile.cls:82`), so on a first plain load it lands relative to the process's directory. A module first installed from a registry and then loaded from a folder keeps the old repository name.
- Impact: a misplaced lock file and a stale repository in `list-installed`.
- Decision: none.
- Missing evidence: not run (no case in scope covered lock files).
- Direction: save `Root` and `Repository` on every load path before the lock file is written.



<a id="g-obs-l4"></a>
### G:OBS-L4

Canonical finding/question: [P:F-INST-5](../../../../playbook-uplift/BUGS.md#p-f-inst-5).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-L4: `update -dev` from a registry ends in normal mode

- Expected: developer mode applies to `update` (`dev-mode`, `concepts/transactions-and-dev-mode.g5.md:70`). `update.dev-mode` (`features/update.g5.md:32`) excludes only dependencies.
- Observed (code): `Main:Install` sets `DeveloperMode` but no `Install` flag for an update (`Main.cls:2623-2630`) and `LoadModuleFromDirectory` then replaces it with `pOverrideDeveloperMode`, 0 by default (`Utils/Module.cls:258`, `:309-311`).
- Impact: the updated module leaves developer mode.
- Decision: none.
- Missing evidence: needs a registry, not run.
- Direction: pass the requested developer mode to the top-level module of an update.



<a id="g-obs-l5"></a>
### G:OBS-L5

Canonical finding/question: [P:F-INST-10](../../../../playbook-uplift/BUGS.md#p-f-inst-10).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-L5: A dependency first installed by `update` runs all its update steps

- Expected: `update` runs the steps needed to move from the current version to the new one (`update.steps`, `features/update.g5.md:42`). A module with no current version needs none and a normal install seeds them as done.
- Observed (code): `Update=1` propagates to dependency loads (`Utils/Module.cls:316-317`), which then run `ApplyUpdateSteps` (`:1344-1345`). `%Activate` skips seeding when `Update` is set (`Lifecycle/Module.cls:122`) and `HandleUpdateStepsFromList` runs every step from the first one not yet run (`Storage/Module.cls:186-195`).
- Impact: migration steps meant for upgrades run on a fresh install. Steps must be safe to rerun, which limits the harm.
- Decision: none.
- Missing evidence: needs a registry and update packages, not run.
- Direction: seed the steps of a dependency that was not installed before the update.



<a id="g-obs-l9"></a>
### G:OBS-L9

Canonical finding/question: [P:F-INST-1](../../../../playbook-uplift/BUGS.md#p-f-inst-1).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-L9: Update steps are skipped silently when the update lock times out

- Expected: `update.steps` requires the steps to run.
- Observed (code): the method takes `lock +^IPM.UpdateLock(name):3` (`src/inc/IPM/Common.inc:105`). On timeout it returns its OK status without running any step (`Storage/Module.cls:105-108`). No human source covers the timeout (A1 conflict 24).
- Impact: an update and its history record both report success while its steps did not run.
- Decision: none.
- Missing evidence: needs two concurrent processes, not run.
- Direction: return an error on timeout, or wait and report.



<a id="g-obs-l16"></a>
### G:OBS-L16

Disposition: inconclusive/execution-scope-blocked · Reproduction: none in this audit

Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-L16: A failed legacy XML install reads `Committed=0` although it kept changes

- Expected: `Committed` shows whether changes were kept, including writes outside a transaction (`history.committed`, `features/history.g5.md:125`; `General/AbstractHistory.cls:41-42`).
- Observed (code): the legacy XML payload path (`Utils/Module.cls:175-184`) has no `SetCommitted` and `Finalize` sets `Committed` only from a successful status. This is the one caller that relies on `Finalize`'s "or OK" (model control 05 notes, round 1 finding R1-MOD-07).
- Impact: a failed legacy install that wrote classes reads uncommitted.
- Decision: none.
- Missing evidence: needs a registry serving a legacy XML package, not run.
- Direction: mark `Committed` before the legacy path writes (S3).



<a id="g-obs-l17"></a>
### G:OBS-L17

Canonical finding/question: [P:F-UNIN-6](../../../../playbook-uplift/BUGS.md#p-f-unin-6).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-L17: Uninstalling a module others require reports success

- Expected: IPM prevents uninstalling a module that installed modules depend on, unless `-force` (`uninstall.dependents`, `features/uninstall.g5.md:20`). `-recurse` keeps dependencies that others need (`uninstall.recurse`, `:25`). For uninstall, `Committed` shows success (`history.committed.uninstall`, `features/history.g5.md:141`).
- Observed (code): uninstall runs Clean with level 1 (`Storage/Module.cls:742`). `CheckBeforeClean` marks the module skipped with an OK status (`Lifecycle/Base.cls:290-294`) and `%Clean` returns OK (`Lifecycle/Module.cls:12-15`). The module stays while the command succeeds and the record reads success with `Committed=1`. A recursive uninstall finalizes each skipped dependency as a successful uninstall (`Lifecycle/Base.cls:471-484`). The maintainer test for the refusal is skipped as "does not yet exist" (`tests/integration_tests/Test/PM/Integration/InstallModule.cls:135`).
- Impact: the user and history both report an uninstall that did not happen.
- Decision: none.
- Missing evidence: uninstall paths were not run natively.
- Direction: return an error when Clean is skipped for dependents and record the skip as a failure or a distinct outcome.
- Aliases: OBS-H8.



<a id="g-obs-h7"></a>
### G:OBS-H7

Disposition: policy-only/source-reviewed · Reproduction: source review; historical observations preserved

Dependency load labels and install-only source display are source/test-supported as built. Alternative labels/display require Q-15; do not retain behavior matching the current working assumption as a defect.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-H7 (Q-15): Dependency records are labeled `load`, keep a source and list it as `n/a`

- Expected: under the provisional answer of Q-15, dependencies of `load` and `update` keep the `load` label and their stored source. Listings show a source only for installs. The product does exactly this.
- Observed: only `cmd="install"` selects `InstallInit` (`Utils/Module.cls:63-68`), so dependencies of `load`, `update`, `test` and `verify` read `load`. They carry the repository's name and the listing shows `n/a` for every action other than install (`General/History.cls:461-463`). A top-level `update` from a registry stores a source that the listing also hides.
- Governing source: the open question `history.fields.dependency-source` (`features/history.g5.md:95`). The `history.fields` bullet (`:79`) says sources are empty for load, which the provisional answer of Q-15 overrides for dependency records.
- Evidence: A0 (`RN/20261113T182759Z-cases-a`: dependency record `Action=load`, `SourceName=ipm-temp-modules-1`).
- Impact: users cannot see in the listing where a dependency came from.
- Decision: Q-15. Option A (provisional): not a defect. Options B (label such records `install` with their source), C (show any stored source in listings) and D (clear the source of `load` records) each make the current behavior a defect in labeling, display or storage.
- Missing evidence: registry and ORAS sources were not run.
- Direction: none until Q-15 is decided.
- Aliases: none.



<a id="g-obs-h12"></a>
### G:OBS-H12

Disposition: inconclusive · Reproduction: none newly qualified

Early purge return is source-supported. Constructing a real namespace enumeration or purge-marker storage failure was not established with ordinary inputs; no monkeypatch was counted.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-H12 (Q-13): A purge failure stops the user's command

- Expected: under the provisional answer of `history.retention.effect` (`features/history.g5.md:307`), the user's command still runs.
- Observed (code): `Shell` returns the purge's error before running the command (`Main.cls:862-865`). `ShellInternal` and the API writers never purge. Because `DeleteHistoryGlobally` swallows per-namespace errors (D23), a blocking failure has to come from the namespace list query (`General/History.cls:206-207`) or the stamp.
- Impact: every IPM command fails while the purge fails.
- Decision: Q-13. Option A (provisional, the command still runs): a defect, as a proposed change. Option B (report the error and stop): intended.
- Missing evidence: a purge failure was not run.
- Direction: log the purge error and continue with the command.



<a id="g-obs-h13"></a>
### G:OBS-H13

Disposition: inconclusive · Reproduction: none newly qualified

Cross-namespace temporary history clearing is source-supported; complete self-uninstall/active-writer reader remains unqualified. No source determination chooses that policy.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-H13 (Q-16): IPM self-uninstall with full cleanup deletes other namespaces' in-progress records

- Expected: under the provisional answer of `history.keep-on-ipm-uninstall.other-namespaces` (`features/history.g5.md:321`), uninstalling IPM in one namespace does not touch other namespaces' records.
- Observed (code): full cleanup kills `^IRIS.Temp.HistoryTempD` (`Lifecycle/Module.cls:51`). That global is shared by every namespace (N-P06, N-P17 confirmed natively), so in-progress records of loads running elsewhere lose their IRISTEMP copy and the ID counter restarts. A live log in another process may then reload a different log's row that reuses its ID.
- Trigger class: a full-cleanup self-uninstall of IPM in one namespace while a load runs in another.
- Impact: lost or mixed-up history for loads running elsewhere.
- Decision: Q-16. Option A (provisional): a defect. Option B (accept it, because removing IPM is rare): intended.
- Missing evidence: concurrent loads were not run.
- Direction: delete only this namespace's IRISTEMP rows.



<a id="g-obs-h15"></a>
### G:OBS-H15

Disposition: inconclusive · Reproduction: none newly qualified

Self-uninstall ordering read; an interactive skipped removal and final shell/history reader was not qualified by the safe directory-only tests.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

### OBS-H15: Self-uninstall records success before it runs and exits the shell even when skipped

- Expected: `Success` is the action's status (`history.fields`, `features/history.g5.md:79`). For uninstall, `Committed` shows success (`:141`). History survives IPM's own uninstall (`history.keep-on-ipm-uninstall`, `:316`).
- Observed (code): `Finalize($$$OK)` and `PersistToTwin` run before `%Clean` (`Storage/Module.cls:630-643`). The self-uninstall flag is set before the uninstall call (`Main.cls:2746-2749`), so the shell prints that IPM was uninstalled and exits (`Main.cls:1152-1156`) even when Clean is skipped because another module requires IPM.
- Impact: history and the shell report an uninstall of IPM that did not happen.
- Decision: none.
- Missing evidence: self-uninstall was not run. Only its persist step was probed: a second `PersistToTwin` on a finalized row fails with SQLCODE -132 and changes nothing. `%OnClose` still deletes the IRISTEMP row without raising (N-P16).
- Direction: finalize after Clean, using an explicit persist that does not depend on the classes Clean removes. Set the exit flag only on success.



<a id="g-d7"></a>
### G:D7

Disposition: withdrawn/source-settled · Reproduction: historical

Preserve the documented withdrawal and its original attributed source. Raw status details and unconfirmed global deletion were explicitly documented; no new contrary intent inferred.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| D7 | withdrawn | `history details` prints the raw `%Status` |

<a id="g-obs-h9"></a>
### G:OBS-H9

Disposition: withdrawn/source-settled · Reproduction: historical

Preserve the documented withdrawal and its original attributed source. Raw status details and unconfirmed global deletion were explicitly documented; no new contrary intent inferred.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| OBS-H9 | withdrawn | An unfiltered global delete needs no confirmation |

<a id="g-obs-h10"></a>
### G:OBS-H10

Disposition: refuted · Reproduction: historical contrary evidence

Refuted historical empty-group prediction; DisplayQueryResult only prints a namespace after a row. No claim that source-only reachability proves unrelated behavior.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| OBS-H10 (remainder) | refuted | Namespaces without IPM were thought to appear as empty groups in `-globally` output |

<a id="g-obs-l15"></a>
### G:OBS-L15

Disposition: outside-functional-contract · Reproduction: historical

Existing API/reporting scope limitation preserved. No new product requirement was created to turn an observation into a defect.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| OBS-L15 | out of scope | `LoadedResource` rows survive a rolled-back load |

<a id="g-oos-1"></a>
### G:OOS-1

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| OOS-1 | out of scope | A git URL load passes the URL and `-branch` to a shell command unquoted |

<a id="g-oos-2"></a>
### G:OOS-2

Disposition: inconclusive/execution-scope-blocked · Reproduction: none in this audit

Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| OOS-2 | out of scope | The module that `verify` installs gets no history record |

<a id="g-oos-3"></a>
### G:OOS-3

Disposition: outside-functional-contract · Reproduction: historical

Existing API/reporting scope limitation preserved. No new product requirement was created to turn an observation into a defect.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| OOS-3 | out of scope | IPM's test summary counted a skipped method as passed |

<a id="p-f-dep-1"></a>
### P:F-DEP-1

Disposition: active · Reproduction: e2e reproduced

PermitDowngrade relaxes resolution then load refuses; normal load bound refusal neighbor

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps)

Starting evidence and claim:

| F-DEP-1 | Low | `PermitDowngrade` is half implemented: the resolver drops the "≥ installed" bound and picks an older version, then the load refuses any downgrade. | Proved (`dep_witness_permit_downgrade_refused`); IRIS (`nc-dep-12-permit-downgrade`) | `Storage/Module.cls#L1157`, `Utils/Module.cls#L1214`, `#L103` |

<a id="p-f-dep-2"></a>
### P:F-DEP-2

Disposition: active · Reproduction: e2e reproduced

ordinary create-lockfile emits name order while load output is dependency first; downstream ci is unimplemented

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps.lockfile)

Starting evidence and claim:

| F-DEP-2 | Low | Lock-file dependencies are written in name order, not dependency order, although `LockFile.cls#L24` promises least-dependent first. Latent (no `ci` command yet); fixtures happen to sort correctly. | Proved (`dep_witness_lockfile_name_order`); IRIS (`nc-dep-13-lockfile-order`) | `LockFile.cls#L25`, `#L69` (array keyed by name) |

<a id="p-f-dep-3"></a>
### P:F-DEP-3

Disposition: active · Reproduction: e2e reproduced

list-dependents lowercase displays parent; mixed case displays none

[Canonical ontology](../../../ontology/features/inspection.g5.md#inspect.dependents)

Starting evidence and claim:

| F-DEP-3 | Low | `list-dependents` with a capitalised name (as in the help examples) prints nothing. | Proved (`dep_witness_list_dependents_case`); IRIS (`nc-dep-16-list-dependents-case`) | `Main.cls#L3154`; `Utils/Module.cls#L482`, `#L529` |

<a id="p-f-dep-4"></a>
### P:F-DEP-4

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps)

Starting evidence and claim:

- **F-DEP-4.** `Storage/Module.cls#L892` says an empty phase list means all scoped dependencies count; `HasScope`

<a id="p-f-dep-5"></a>
### P:F-DEP-5

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps)

Starting evidence and claim:

- **F-DEP-5.** `BuildDependencyGraph.cls#L232` (test) says exact-versus-range has a known Intersect edge case; the same

<a id="p-f-dep-6"></a>
### P:F-DEP-6

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps)

Starting evidence and claim:

- **F-DEP-6.** `DependencyResolution.TestEverything` asserts nothing (annotation extraction commented out, L17/L21);

<a id="p-f-dep-7"></a>
### P:F-DEP-7

Disposition: active · Reproduction: e2e reproduced

ordinary solvable graph install refuses; pinned solution installs all modules

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps)

Starting evidence and claim:

| F-DEP-7 | Medium | The resolver is greedy (newest first, no backtracking past one level) and fails on solvable inputs, although its comment says "with backtracking". Known upstream: open issue #1299, fix PR #1303 open. | Proved (`dep_witness_greedy_rejects_solvable`); IRIS (`nc-dep-11-greedy-resolution`, `-11b-solvable-with-pin`); explorer (2,040 of 70,656 instances) | `Storage/Module.cls#L1248-L1291` |

<a id="p-f-dep-8"></a>
### P:F-DEP-8

Disposition: active · Reproduction: e2e reproduced

failed create-lockfile install leaves JSON while module/dependency rows disappear; successful lockfile neighbor

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps.lockfile)

Starting evidence and claim:

| F-DEP-8 | Low | A failed `install -create-lockfile` leaves a new lock file behind (written before the module's own Activate, outside the transaction). PR #963's author said a failed install leaves none. | IRIS (`nc-dep-15-lockfile-failed-install`) | `Utils/Module.cls#L1415-L1421` |

<a id="p-f-dep-9"></a>
### P:F-DEP-9

Disposition: active · Reproduction: e2e reproduced

ordinary local-loaded dependency produces lockfile warning and no changed file; registry supplier neighbor

[Canonical ontology](../../../ontology/features/dependencies.g5.md#deps.lockfile.local-supplier-policy)

Starting evidence and claim:

| F-DEP-9 † | Low | A needed module loaded from a folder (no registry) makes lock-file creation throw, so no file is written; the base module's empty registry is already handled. | IRIS (`nc-dep-14-lockfile-folder-dependency`) | `LockFile.cls#L72`, `#L115` |

<a id="p-f-hist-1"></a>
### P:F-HIST-1

Canonical finding/question: [G:D1](../../../Verus_verification/bugs_issues.md#g-d1).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| F-HIST-1 | Medium | Dependency history records vanish when the parent load rolls back, though `AbstractHistory.cls#L4` says history is kept apart from rollbacks. A failing dependency's own record is lost too. | Proved (`g_dependency_records_lost_unless_fixed`); IRIS (`ipm-dep-parent-fails`); model-checked (109 runs) | `Utils/Module.cls#L64-L68` (log created inside the parent's transaction, `#L1195`), rollback `#L1358-L1361` |

<a id="p-f-hist-10"></a>
### P:F-HIST-10

Canonical finding/question: [C:Q7](../QUESTIONS.md).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

Withdrawn as intended: **F-HIST-10** (`history delete` of a running record wins; DEC-HIST-6, assumption). A row

<a id="p-f-hist-2"></a>
### P:F-HIST-2

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed developer manifest commit, dependency entry and durable history persistence plus the original crash receipt. New directory tests return normally, so the crash-in-dependency/durable-row gate remains open.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| F-HIST-2 | Medium | A developer-mode load that crashes while loading dependencies keeps its module record, but history says Committed = 0 and names no package. | Proved (`g_dev_crash_dishonest_unless_fixed`); IRIS (`ipm-halt-dev-crash-in-dependency`) | `Utils/Module.cls#L1311-L1313` (SetCommitted in IRISTEMP only), next `TSTART` `#L1318` |

<a id="p-f-hist-3"></a>
### P:F-HIST-3

Canonical finding/question: [G:D25](../../../Verus_verification/bugs_issues.md#g-d25).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

- **F-HIST-3.** A command run inside a caller's transaction loses its history if the caller rolls back. Intended

<a id="p-f-hist-4"></a>
### P:F-HIST-4

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

- **F-HIST-4.** Answering "Commit" at the crash prompt keeps a half-done load marked Committed = 0. An operator error

<a id="p-f-hist-5"></a>
### P:F-HIST-5

Canonical finding/question: [G:D9](../../../Verus_verification/bugs_issues.md#g-d9).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| F-HIST-5 | Low | A refused or failed command that ended after reading the manifest leaves a record with no module name (wiki page 09 allows placeholders only for a process that dies mid-action). | IRIS (`ipm-dev-downgrade-refused`, `ipm-halt-txn`, `ipm-terminate-txn`); model-checked | `Main.cls#L2420` (`LoadInit("")`), `SetName` only at `Utils/Module.cls#L1293`, after the guards at `#L1213-L1222` |

<a id="p-f-hist-6"></a>
### P:F-HIST-6

Canonical finding/question: [G:D3](../../../Verus_verification/bugs_issues.md#g-d3).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| F-HIST-6 | Medium | History auto-purge never deletes anything: the cut-off date is quoted inside a bound parameter, so the filter matches no row. The command reports success. History grows without bound although `HistoryRetain` is set. | IRIS (`ipm-history-autopurge`, `ipm-history-purge-filter-cause`) | `History.cls#L173` (quotes), `#L107-L109` (already binds) |

<a id="p-f-hist-7"></a>
### P:F-HIST-7

Disposition: inconclusive · Reproduction: none newly qualified

The deterministic commit-window witness uses a patched stall and cannot be promoted to an ordinary-input observation on the unmodified pin. The older graceful-stop case has an unidentified hang cause. Both records are preserved without a fresh whole-command claim.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| F-HIST-7 | Low | A crash between the outer `TCOMMIT` and `SetCommitted` leaves Committed = 0 for a committed load. A graceful `iris stop` during a load did the same: IRIS waited for the commit, then forced the job down after about 5 minutes; the record stayed `load//`, Committed = 0, not finalized (cause not identified). | Proved (`g_window_crash_dishonest`); patched probe p8a (`nc-gap-P8-commit-window-after-commit`); IRIS graceful stop (`r4-hist-graceful-stop-txn-after`) | `Utils/Module.cls#L1348-L1350` |

<a id="p-f-hist-8"></a>
### P:F-HIST-8

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed restart/IRISTEMP loss code and existing before/after restart logs. The new tests use fresh instances without restarting an in-flight command; that exact restart/outcome reader was not rerun.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| F-HIST-8 | Medium | After an IRIS restart, a developer-mode or no-transaction load that kept its changes reads Committed = 0, with no name, no end time and an unlocked row that SQL can still edit. | Proved (`g_instance_crash_dishonest_unless_fixed`); IRIS (`nc-gap-P1-docker-dev-after`, `-notxn-after`) | RC-4; `AbstractHistory.cls#L120-L125`, `HistoryTemp.cls#L62-L72` |

<a id="p-f-hist-9"></a>
### P:F-HIST-9

Canonical finding/question: [G:D6](../../../Verus_verification/bugs_issues.md#g-d6).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/features/history.g5.md#history)

Starting evidence and claim:

| F-HIST-9 | Medium | A phase that fails through an `<Invoke>`'s returned status stays open in history with placeholder error `ERROR #00`. | IRIS (`nc-gap-P3-n2-ph-*`, all modes) | `Storage/Module.cls#L669`, `#L671` (only the catch calls `FinalizePhase`) |

<a id="p-f-inst-1"></a>
### P:F-INST-1

Disposition: active · Reproduction: e2e reproduced

real competing update lock; command OK with old step rows; unlocked retry runs due steps

[Canonical ontology](../../../ontology/features/update.g5.md#update.steps.busy-policy)

Starting evidence and claim:

| F-INST-1 † | **High** | `update` reports success without running update steps when the update lock is busy for 3 s. No message unless verbose. The steps stay due, and nothing prompts a rerun. | Proved (`inst_witness_busy_lock_reports_success`); IRIS (`nc-inst-3-update-lock-busy`, `-3b-...`) | `Storage/Module.cls#L105-L108` (`return sc` with `sc = $$$OK`); `inc/IPM/Common.inc#L105` |

<a id="p-f-inst-10"></a>
### P:F-INST-10

Disposition: active · Reproduction: e2e reproduced

new dependency installed during update runs V1; fresh normal install seeds without running

[Canonical ontology](../../../ontology/features/update.g5.md#update.steps.fresh-seeding-policy)

Starting evidence and claim:

| F-INST-10 † | Medium | An update that brings in a module for the first time runs all of that module's update steps instead of seeding them as a fresh install does. | Proved (`inst_fresh_update_runs_all`); IRIS (`nc-inst-6-dep-added-by-update` vs `-6b-fresh-install-control`) | `Utils/Module.cls#L308`, `#L316`; `Lifecycle/Module.cls#L122` |

<a id="p-f-inst-11"></a>
### P:F-INST-11

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/features/update.g5.md#update.steps)

Starting evidence and claim:

- **F-INST-11 (documentation).** The `-dev` bypass of the update-package guard is intended (DEC-INST-1: `TestDevMode`

<a id="p-f-inst-2"></a>
### P:F-INST-2

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/features/update.g5.md#update.steps)

Starting evidence and claim:

- **F-INST-2 (documentation + test gap).** `VersionBase.cls#L18` says primary-only steps run "when the instance becomes

<a id="p-f-inst-3"></a>
### P:F-INST-3

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/features/update.g5.md#update.steps)

Starting evidence and claim:

- **F-INST-3 (gap).** `TestUpdateAfterInterruptedUpdate` (`Update.cls#L130`) kills the job at an uncontrolled point and

<a id="p-f-inst-4"></a>
### P:F-INST-4

Disposition: active · Reproduction: e2e reproduced

two filesystem repositories with tied versions; creation-order selection and reordered neighbor

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo.search-order)

Starting evidence and claim:

| F-INST-4 | Medium | Between registries that offer the same version, the registry created first wins; `OverriddenSortOrder` has no effect. Deleting and re-adding a registry changes which copy installs. | IRIS (`nc-inst-10b-registry-tie-break`, `-10c-...`) | `Repo/Manager.cls#L148` (`versions(...,repoID,name)` read back in subscript order; SQL order at `#L70` lost) |

<a id="p-f-inst-5"></a>
### P:F-INST-5

Disposition: active · Reproduction: e2e reproduced

ordinary update -dev from filesystem registry ends normal; local -path developer neighbor

[Canonical ontology](../../../ontology/features/update.g5.md#update.dev-mode)

Starting evidence and claim:

| F-INST-5 | Medium | `update -dev` from a registry does not run in developer mode, and a plain `update` of a module stored in developer mode clears it. Only `update -dev -path` keeps it. | Proved (`inst_witness_dev_update_from_registry_rolls_back`); IRIS (`nc-inst-4e-...`, `nc-dep-19f-...`, `nc-inst-7-...`) | `Utils/Module.cls#L128-L136`, `#L307-L310`, `#L293-L296` |

<a id="p-f-inst-6"></a>
### P:F-INST-6

Disposition: active · Reproduction: e2e reproduced

real V3Helper fixture suppresses V3 execution; V2/V9/V10 ordinary order neighbor

[Canonical ontology](../../../ontology/features/update.g5.md#update.steps)

Starting evidence and claim:

| F-INST-6 | **High** | A class of the update package named `V<n><something>` (e.g. `V3Helper`) replaces step class `V<n>` in the step list; update then reports success without running `V<n>`'s steps. | Proved (`inst_witness_helper_class_hides_steps`); IRIS (`nc-inst-model-class-number-collision`) | `General/Update/VersionBase.cls#L34-L60` (`$zstrip(...,"*E'N")`, later row overwrites) |

<a id="p-f-inst-7"></a>
### P:F-INST-7

Disposition: active · Reproduction: e2e reproduced

real Validator fixture makes update fail; numbered-class neighbor

[Canonical ontology](../../../ontology/features/update.g5.md#update.steps)

Starting evidence and claim:

| F-INST-7 | Medium | A class of the update package whose name starts with `V` but has no digit (e.g. `Validator`) makes every update of the module fail with `<SUBSCRIPT>`, naming no class. | Proved (`inst_witness_class_without_number_fails`); IRIS (`nc-inst-model-class-without-number`) | `VersionBase.cls#L51-L52` |

<a id="p-f-inst-8"></a>
### P:F-INST-8

Disposition: active · Reproduction: e2e reproduced

developer fresh install leaves zero step records; later update executes old steps; normal fresh install seeds

[Canonical ontology](../../../ontology/features/update.g5.md#update.steps.dev-seeding-policy)

Starting evidence and claim:

| F-INST-8 † | Medium | A developer-mode install (or a plain `load` of a module stored in developer mode) seeds no update steps, so the next update reruns every step from V1, including steps of versions already in place. | Proved (`inst_witness_dev_install_leaves_steps_due`); IRIS (`nc-inst-2b-dev-fresh-then-update`, `nc-inst-2-dev-install-then-update`, `r4-inst-u22-dev-kill-before-record-save`) | `Lifecycle/Module.cls#L122` |

<a id="p-f-inst-9"></a>
### P:F-INST-9

Disposition: active · Reproduction: e2e reproduced

ordinary parent install replaces developer dependency; initial developer load verifies prestate

[Canonical ontology](../../../ontology/concepts/transactions-and-dev-mode.g5.md#dev-mode.overwrite-guard.dependency-policy)

Starting evidence and claim:

| F-INST-9 † | Medium | Install silently replaces a needed module that is in developer mode and clears its developer mode; the guard covers only the named module. Wiki Concepts: a dev-mode module "cannot be overwritten by a non-dev install". | IRIS (`nc-inst-8-dev-dependency-replaced`) | `Utils/Module.cls#L109`, `#L310`, `#L317-L319` |

<a id="p-f-load-1"></a>
### P:F-LOAD-1

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed unchecked clone status and the old benign failed-URL receipt. This URL-to-shell path overlaps G:OOS-1 security scope; no URL-load workflow or shell-boundary investigation was executed in this audit.

[Canonical ontology](../../../ontology/features/load.g5.md#load)

Starting evidence and claim:

| F-LOAD-1 | Low | A failed `git clone` in a URL load is not reported. The load then fails with "No module.xml" and writes a history record with no module name. | IRIS (`nc-gap-P5-url-missing`) | `Main.cls#L2358`, `Utils/Module.cls#L2409` (clone return code unchecked) |

<a id="p-f-load-2"></a>
### P:F-LOAD-2

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed clone lifetime/root storage and the old successful URL-load receipt. No URL-to-shell workflow was executed because that boundary overlaps the explicitly excluded security investigation; temporary-root behavior remains historically evidenced.

[Canonical ontology](../../../ontology/features/load.g5.md#load)

Starting evidence and claim:

| F-LOAD-2 | Medium | Every URL load leaves its clone in `mgr/Temp/<timestamp>/<repo>/`, and that temporary folder becomes the module's Root, so later `reload`/`sync` read from a temp folder. | IRIS (`nc-gap-P5-url-ok-txn`, `-ok-dev`) | `Main.cls#L2352-L2359` |

<a id="p-f-load-3"></a>
### P:F-LOAD-3

Disposition: inconclusive/execution-scope-blocked · Reproduction: none in this audit

Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased.

[Canonical ontology](../../../ontology/features/load.g5.md#load)

Starting evidence and claim:

| F-LOAD-3 | Medium | A failing archive load of an installed name and version replaces the installed module's folder before any check. The database rolls back, so the installed record's folder no longer matches its compiled code, and history says Committed = 0. | IRIS (`nc-gap-P4-txn-bad-archive-same-version`) | `Main.cls#L2507-L2513` |

<a id="p-f-load-4"></a>
### P:F-LOAD-4

Disposition: active · Reproduction: e2e reproduced

ordinary after-install macro failure with durable module/history committed; successful message neighbor

[Canonical ontology](../../../ontology/features/load.g5.md#load)

Starting evidence and claim:

| F-LOAD-4 | Low | A load can fail **after** its final commit: an after-install message with `{` and a `$$$` macro that does not compile throws in `%EvaluateMacro`. The command returns an error and history records failure, while the module and its compiled classes stay installed (committed = 1). Seen in transactional, no-transaction and developer modes. Also leaves `%IPM.Generated.MacroEval` defined. | IRIS (`r4-hist-post-commit-throw-r4h-msg2-txn`, `-notxn`, `-dev`) | `Utils/Module.cls#L1348-L1354`, `#L2781` |

<a id="p-f-nsp-1"></a>
### P:F-NSP-1

Disposition: inconclusive · Reproduction: none newly qualified

The failing enable-from-registry reader requires a real registry/installer-delivery failure. Directory mapping success is not that test; no excluded package/archive traversal was used as setup.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

| F-NSP-1 | Medium | `enable` from a registry ignores the install status and always prints "IPM enabled for namespace X" and returns OK, even when the installer failed to compile. | IRIS (`nc-nsp-N5-enable-remote-broken`) | `Main.cls#L3885-L3886` (the local-file path at `#L3904-L3917` does it right) |

<a id="p-f-nsp-10"></a>
### P:F-NSP-10

Disposition: inconclusive · Reproduction: none newly qualified

Existing simulation is explicitly a simulated legacy install, not real ZPM 0.7.4. A real install would replace the server-wide language extension; no baseline substitution is called end-to-end.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

| F-NSP-10 | Low | `enable -map` and `enable -community` never migrate a target namespace's legacy ZPM module records (a maintainer requirement in issue #470). Rare: the upgrade path migrates the namespaces it maps. | IRIS, simulated legacy records (`r4-nsp-legacy-setup`, `r4-nsp-legacy-enable-map`) | `Main.cls#L3553-L3708` |

<a id="p-f-nsp-11"></a>
### P:F-NSP-11

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed the three enable-community status calls and old step-failure log. The fresh mapping cohort checks successful first/second passes, not a failing component step plus final shell status; that exact gate remains.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

| F-NSP-11 † | Low | `enable -community` prints each failed step in red, runs the next, and returns OK. | IRIS (`nc-nsp-N4-community-step-fails`) | `Main.cls#L3564-L3575` |

<a id="p-f-nsp-12"></a>
### P:F-NSP-12

Canonical finding/question: [G:D4](../../../Verus_verification/bugs_issues.md#g-d4).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

| F-NSP-12 † | Low | In developer mode, a load refused by a system requirement (e.g. `SYSNamespace`) stays installed: the requirement check runs after the dev-mode manifest commit. | IRIS (`nc-nsp-N8-sysns-dev`) | `Utils/Module.cls#L1311` vs `#L1321` |

<a id="p-f-nsp-2"></a>
### P:F-NSP-2

Disposition: active · Reproduction: e2e reproduced

ordinary requirement load fails in mapped namespace, succeeds in source namespace

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace.version.mapped-reader)

Starting evidence and claim:

| F-NSP-2 | Medium | In a namespace that gets IPM by mapping, an `IPMVersion` requirement is checked against an empty version, so every module with such a requirement is refused ("Current IPM version is .0.0"). | Proved (`nsp_witness_version_empty_when_mapped`); IRIS (`nc-nsp-N9-ipmversion-mapped-snap`) | `Storage/SystemRequirements.cls#L91-L95` (current-namespace query; `GetIPMVersionInfo` at `Main.cls#L984-L1013` already handles mapping) |

<a id="p-f-nsp-3"></a>
### P:F-NSP-3

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

Documentation defect: **F-NSP-3** (Low). `version` prints build labels such as `+snapshot` although its help says it

<a id="p-f-nsp-4"></a>
### P:F-NSP-4

Disposition: active · Reproduction: e2e reproduced

enable -map -repos skips first repo mapping; same command second run maps it

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace.enable.mapping-results)

Starting evidence and claim:

| F-NSP-4 | Medium | `enable -map -repos` (and `enable -community`) on a namespace with no IPM maps the code but skips the registries; a second run is needed. The repo step reads the active mapping, but new mappings are activated only at the end. | Proved (`nsp_witness_repos_second_run`); IRIS (`nc-nsp-N3-*`, `nc-nsp-N4-community-step-fails`) | `Main.cls#L3661`, `#L1636`, `#L3698` |

<a id="p-f-nsp-5"></a>
### P:F-NSP-5

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed mapped installer behavior and the old namespace/compile-time logs. The fresh mapped-IPM tests do not run the full source installer into that mapped target; the installer/source-namespace/unmap reader remains unqualified.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

| F-NSP-5 | Medium | Running the IPM installer in a namespace that gets IPM by mapping keeps the mappings, so it recompiles the source namespace's IPM classes, and gives the namespace its own `zpm` record. Afterwards `unmap` says "No mapping found" and cannot remove the mappings. | Proved (`nsp_witness_installer_in_mapped`); IRIS (`nc-nsp-N7-*`, `nc-nsp-installer-repoint`) | `Installer.cls#L25-L29` (`%Installer` mappings are add-only). Intent (DEC-NSP-1/2, PR #485, README#L69): unmap first, then install; repo settings are mapped only with the code. After `unmap`, the `install zpm` guard still refuses (`r4-nsp-unmap-then-install-guard`) |

<a id="p-f-nsp-6"></a>
### P:F-NSP-6

Disposition: active · Reproduction: e2e reproduced

unmap removes package mapping but routine mapping remains and IPM still callable; source refusal neighbor

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace.unmap.routine-result)

Starting evidence and claim:

| F-NSP-6 | Medium | `unmap` leaves the `%IPM.*` routine mapping: the namespace still runs the source's IPM and is still reported as mapped. Argument-slot bug: the third argument of `RemoveRoutineMapping` is the routine type, not skip-activation. | Proved (`nsp_witness_unmap_leaves_routines`); IRIS (`nc-nsp-N6-*`, `nc-nsp-model-env-rtn-slot`) | `Main.cls#L3998`; `Utils/Module.cls#L1707-L1720` |

<a id="p-f-nsp-7"></a>
### P:F-NSP-7

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed last-source overwrite and source selection. The fresh namespaces have one actual zpm owner, so they do not create the two-record/shared-code precondition; the ambiguous-source reader remains open under its restored Q owner.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

| F-NSP-7 † | Medium | When two namespaces with a `zpm` record share one IPM database (e.g. IPM's own `USER-VERIFY` test namespace), the last by name is taken as the source of every namespace mapped to it: `enable -map -repos` from the real source maps no registries, and `version` names the wrong source. | Proved (`nsp_witness_repos_tie_other_source`); IRIS (`nc-nsp-model-mapinfo`, `-repos-premapped`) | `Main.cls#L1628-L1654`, `#L3661`. † Also loses the self-uninstall "Will affect the following namespaces" warning (U21) |

<a id="p-f-nsp-8"></a>
### P:F-NSP-8

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed AddGlobalMapping positional arguments and earlier SDK activation counters. Fresh enable commands establish final mapping state but do not count individual configuration activations or collation effects. The narrower provider witness is retained as historical evidence.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

| F-NSP-8 | Low | Argument-slot bug: `MapGlobalEquivalently` passes skip-activation into `AddGlobalMapping`'s set-collation slot, so each repo global mapping activates the configuration on its own (three extra activations). End state is the same; the collation side effect was not checked. | IRIS (`nc-nsp-model-env-repo-activation`); transcription check (0 end-state mismatches) | `Utils/Build.cls#L628`; `Utils/Module.cls#L1736-L1742` |

<a id="p-f-nsp-9"></a>
### P:F-NSP-9

Disposition: inconclusive · Reproduction: none newly qualified

Migration source and saved scratch logs checked. No genuine legacy upgrade producer was qualified in this audit; current installation success is not that neighbor.

[Canonical ontology](../../../ontology/concepts/namespaces.g5.md#namespace)

Starting evidence and claim:

| F-NSP-9 | Medium | On the upgrade path from ZPM 0.7, `${ipmdir}` in the FileCopy targets is not expanded: a literal `mgr/${ipmdir}bin/` folder is created, the `ipm`/`iriscli` scripts are not installed, and the installer says they were. | IRIS (`nc-mig-ipmdir-placeholder-unexpanded`, `nc-mig-ipm-cli-scripts`) | `validation/native-gaps-n1.md` G3 |

<a id="p-f-repo-1"></a>
### P:F-REPO-1

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

| F-REPO-1 | Medium | `-token-auth-method` is ignored for remote registries: the token is always sent as an `apikey` header, so a registry expecting `bearer` rejects it. | IRIS (`nc-repo-4-token-auth-method`) | `Repo/Remote/Definition.cls#L60-L115`; `Http/PackageService.cls#L59-L69` |

<a id="p-f-repo-10"></a>
### P:F-REPO-10

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

| F-REPO-10 | **High** (regression from `3174d72d`) | The publish flag on a registry is ignored if an older enabled registry of the same kind exists: each kind's query has no `ORDER BY` and only its first row is read, so `publish` goes to the oldest one. | Proved (`repo_witness_publish_flag_ignored`); IRIS (`nc-repo-model-publish-row-order`) | `Http/Definition.cls#L146-L162`; `Remote/Definition.cls#L117-L129`; `Oras/Definition.cls#L143-L155` |

<a id="p-f-repo-11"></a>
### P:F-REPO-11

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

| F-REPO-11 † | Medium | Changing an ORAS registry's URL keeps its stored credentials, which are then sent to the new host; a remote registry clears them. | IRIS (`nc-repo-3-url-change-credentials`) | `Oras/Definition.cls#L87-L141` |

<a id="p-f-repo-12"></a>
### P:F-REPO-12

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed version/platform tag construction and old unpublish logs. No fresh real deployed-tag producer/unpublish consumer was exercised; successful filesystem installs do not qualify this ORAS route.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

| F-REPO-12 † | Medium | A deployed module version in an ORAS registry cannot be unpublished by version: only its platform tag `V__<platform>` exists, `unpublish R/M V` says "not found", and only `all` removes it. | IRIS (`nc-repo-9-unpublish-deployed-version`) | `Oras/PublishService.cls#L72-L81` |

<a id="p-f-repo-2"></a>
### P:F-REPO-2

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

| F-REPO-2 | Medium | `-sslconfig` and `-publish-url` are documented, accepted and ignored. The SSL configuration is always one named after the host, created with defaults. | IRIS (`nc-repo-5-sslconfig-publish-url`) | `Http/Definition.cls#L38`, `#L46`; `Http/PackageService.cls#L18-L27` |

<a id="p-f-repo-3"></a>
### P:F-REPO-3

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

Documentation: **F-REPO-3.** The publish fallback (flag, then token, then user/password, then any enabled registry) is

<a id="p-f-repo-4"></a>
### P:F-REPO-4

Disposition: active · Reproduction: e2e reproduced

config delete restores first stored value; recorded changed-value prestate

[Canonical ontology](../../../ontology/concepts/settings.g5.md#settings)

Starting evidence and claim:

| F-REPO-4 | Low | `config delete` restores the first value ever set, not the factory default, for keys the installer does not seed (`TerminalPrompt`, `PublishTimeout`, `HistoryRetain`). | Proved (`repo_witness_delete_restores_first_value`); IRIS (`nc-repo-6-config-delete`) | `UniversalSettings.cls#L166-L177` |

<a id="p-f-repo-5"></a>
### P:F-REPO-5

Disposition: active · Reproduction: e2e reproduced

real namespaces read shared default modifiers; local namespace settings neighbor

[Canonical ontology](../../../ontology/concepts/settings.g5.md#settings.default-modifiers)

Starting evidence and claim:

| F-REPO-5 | Medium | `default-modifiers` are documented per namespace but stored in `^%IPM.DefaultModifiers`, which IRIS keeps in IRISSYS: setting `-DNoTransaction=1` in one namespace affects every namespace. | Proved (`repo_witness_dm_shared`); IRIS (`nc-repo-7-default-modifiers-namespaces`) | `inc/IPM/Common.inc#L31`; `Main.cls#L1682-L1696` |

<a id="p-f-repo-6"></a>
### P:F-REPO-6

Disposition: refuted/unreachable-CLI-trigger · Reproduction: historical contrary evidence

Refuted by moniker selection before Configure, as the original nc-repo-10-invalid-type evidence records; preserve F-REPO-9 as the separate prompting issue.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

Refuted: **F-REPO-6** (an invalid `-type` calls `Configure` on an unrelated class). On IRIS the value is matched

<a id="p-f-repo-7"></a>
### P:F-REPO-7

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

| F-REPO-7 | Medium | A disabled registry is still contacted (credentials sent) by `search` and by `publish -r`. Install and unpublish refuse it. | Proved (`repo_witness_disabled_registry_used`); IRIS (`nc-repo-2-disabled-registry`) | `Main.cls#L2047`; `Utils/Module.cls#L916-L930`; `Lifecycle/Base.cls#L1598-L1600`. DEC-REPO-2 |

<a id="p-f-repo-8"></a>
### P:F-REPO-8

Disposition: active · Reproduction: e2e reproduced

ordinary config invalid key/value returns OK; valid config neighbor

[Canonical ontology](../../../ontology/concepts/settings.g5.md#settings.print)

Starting evidence and claim:

| F-REPO-8 | Low | `config` with an unknown key or a bad value prints a message and returns OK, so scripts cannot detect typos. | Proved (`repo_witness_config_errors_report_ok`); IRIS (`nc-repo-8-config-errors`) | `Main.cls#L2063-L2093`. DEC-REPO-4 |

<a id="p-f-repo-9"></a>
### P:F-REPO-9

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed moniker filtering, the unreachable Invalid type branch and earlier interactive receipt. A missing-name setup incident also prompted, but it is a different trigger and is not counted as an invalid-type reproduction. A controlled invalid-type terminal/input reader remains.

[Canonical ontology](../../../ontology/concepts/repositories.g5.md#repo)

Starting evidence and claim:

| F-REPO-9 | Low | `repo -type <not a moniker>` silently drops the value and prompts "Which sort of repository...". In a script the next line is read as the answer. The intended "Invalid type" error branch is unreachable. | IRIS (`nc-repo-10-invalid-type`) | `Main.cls#L2099-L2112`, `#L2274-L2277` |

<a id="p-f-res-1"></a>
### P:F-RES-1

Canonical finding/question: [C:D19](../BUGS.md#c-d19).

Disposition: alias · Reproduction: alias; see canonical owner

Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Starting evidence and claim:

| F-RES-1 | Medium | If adding a deployed item to the package fails, the error is overwritten with OK and the item's **source** is shipped instead of object code, with no `Deployed.xml`. Old (from the original import). | Patched probe p8b (`nc-gap-P8-deployed-export-forced-error`, control `-control`) | `ResourceProcessor/Default/Document.cls#L256-L257` |

<a id="p-f-res-2"></a>
### P:F-RES-2

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed the documented register phase against the phase dispatcher and existing method-missing log. The new tests do not execute that phase and its multi-phase success neighbor; no current ordinary-command regression was claimed.

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Starting evidence and claim:

| F-RES-2 | Low | The documented `register` phase and its help example do not exist: `<module> register` fails with `<METHOD DOES NOT EXIST>`; `<module> clean register` returns OK and registers nothing. | IRIS (`nc-res-N13-register`) | `DataType/PhaseName.cls#L13`; `Lifecycle/Base.cls#L64-L76`, `#L142`; help `Main.cls#L53`, `#L64` |

<a id="p-f-res-3"></a>
### P:F-RES-3

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed makedeployed warning and deployment/source readers plus the old native case. No fresh developer-module command/source-readability neighbor was executed; a model or warning alone does not establish source loss.

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Starting evidence and claim:

| F-RES-3 | Medium | `makedeployed` on a developer-mode module says it "will only report what WOULD be deployed", then deploys the classes (source no longer readable) and marks the module deployed. | IRIS (`nc-res-N12a-makedeployed-dev`) | `Lifecycle/Base.cls#L1836-L1837` vs `#L1878-L1897` |

<a id="p-f-res-4"></a>
### P:F-RES-4

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Starting evidence and claim:

| F-RES-4 | Low | Without `-v`, `package` never says where it wrote the package file, under a comment `// Always show this message`. | IRIS (`nc-res-N12b-package-dev`, `-N12c-package-verbose`) | `Lifecycle/Module.cls#L183-L184` |

<a id="p-f-res-5"></a>
### P:F-RES-5

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Starting evidence and claim:

- **F-RES-5.** `run-from-file` help shows `MyCommands.text` and says the file holds "a single array"; the code accepts

<a id="p-f-res-6"></a>
### P:F-RES-6

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Starting evidence and claim:

- **F-RES-6.** The `exec` help example "set the namespace to MYAPP" has no lasting effect since PR #996 isolated

<a id="p-f-res-7"></a>
### P:F-RES-7

Disposition: inconclusive/execution-scope-blocked · Reproduction: none in this audit

Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased.

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Starting evidence and claim:

| F-RES-7 | Medium | IPM's tar writer cuts file names of 100 characters or more to 99 and has no long-name header, so such files are lost from packages. The ARM64 `markupsafe` wheel name is exactly 100 characters, so `ProcessPythonWheel#TestPackageWithPythonDeps` fails on ARM64 (IPM's CI is amd64). | IRIS (`nc-suite-tar-name-100-truncated`); test run (`Test.PM.Integration.ProcessPythonWheel`) | `Utils/FileBinaryTar.cls#L7`, `#L469` |

<a id="p-f-res-8"></a>
### P:F-RES-8

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed the shared null-device close path and old TestQuietOutput failure/pass logs. Nested report tests exercise run identity, not nested quiet device lifetime; they are not substituted for the required quiet command/neighbor.

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle)

Starting evidence and claim:

| F-RES-8 | Low | Nested `-quiet` runs fail with `<NOTOPEN>`: the inner output suppressor closes the null device the outer one still uses. | Test run (`TestOutputFormat#TestQuietOutput` under `-quiet`; passes with `-v`) | `Utils/OutputSuppressor.cls#L25-L28` |

<a id="p-f-sync-1"></a>
### P:F-SYNC-1

Disposition: active · Reproduction: e2e reproduced

ordinary failed compile then exact-byte revert makes sync skip stale server definition; successful initial load neighbor

[Canonical ontology](../../../ontology/features/developer-tools.g5.md#devtools.sync.failed-import-policy)

Starting evidence and claim:

| F-SYNC-1 † | Medium | A failed `compile`, `load -dev` or `reload` that puts edited source on the server records no baseline and no retry mark. If the file is then reverted to its exact earlier bytes, `sync` says "Nothing to sync" while the server keeps the broken source. | Proved (`sync_witness_failed_compile_then_revert`, `_reload_then_revert`, `_skip_sync_load_then_revert`); IRIS (`nc-sync-N18-compile-fail-revert`, `nc-nsp-model-sync-reload-revert`, `-skipsync-revert`) | `Lifecycle/Base.cls#L1235-L1244`; `Pipeline.cls#L149`, `#L210-L223`. † Intent extends the #1224 fix to these loaders (DEC-SYNC-1) |

<a id="p-f-sync-2"></a>
### P:F-SYNC-2

Disposition: documentation-or-test-gap · Reproduction: source review

Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.

[Canonical ontology](../../../ontology/features/developer-tools.g5.md#devtools.sync)

Starting evidence and claim:

Documentation: **F-SYNC-2.** After `load -dev -skip-sync`, the first `sync` only records a baseline, so edits made

<a id="p-f-sync-3"></a>
### P:F-SYNC-3

Disposition: active · Reproduction: e2e reproduced

ordinary reload of normal module succeeds without reloading edited file; root-bearing developer load neighbor

[Canonical ontology](../../../ontology/concepts/lifecycle.g5.md#lifecycle.empty-reload-policy)

Starting evidence and claim:

| F-SYNC-3 † | Low | `reload` of a module not in developer mode (no root) loads nothing and reports SUCCESS. | IRIS (`nc-sync-N21-reload-non-dev`) | `Lifecycle/Base.cls#L633-L634` |

<a id="p-f-unin-1"></a>
### P:F-UNIN-1

Disposition: active · Reproduction: e2e reproduced

blocked uninstall deletes actual test web application; module remains; ordinary global-resource uninstall neighbor

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall.dependents.as-built)

Starting evidence and claim:

| F-UNIN-1 | **High** | A blocked uninstall (a dependent exists, or global scope) still deletes the module's web and CSP applications and runs `Clean`/`Unconfigure` hooks, then prints `Clean SUCCESS`. `<module> clean` does the same to the module it runs on; so does `uninstall -r` for a need it keeps. | Proved (`unin_witness_blocked_deletes_webapp`, `_clean_own_module`, `_recurse_skipped_dep`); IRIS (`nc-unin-1-blocked-uninstall`, `nc-unin-2-clean-own-module`, `nc-repo-model-unin-recurse-skipped-dep`) | `Storage/Module.cls#L587-L600` (OnBeforePhase) runs before `Lifecycle/Base.cls#L258-L302` (CheckBeforeClean) |

<a id="p-f-unin-10"></a>
### P:F-UNIN-10

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed cleanup killing CacheD but not cache indices, and old cleanup/reinstall/add-repo evidence. A fresh full IPM self-removal, source reinstall and filesystem-repo reader was not executed; ordinary module uninstall is not that gate.

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall)

Starting evidence and claim:

| F-UNIN-10 | Medium | A full cleanup leaves the folder-registry cache index (`^IPM.Repo.Filesystem.CacheI`, `CacheS`). After IPM is reinstalled, adding a folder registry fails with `ERROR #5809` until the globals are killed by hand. | IRIS (`nc-gap-P7-fscache-stale` vs `-repaired`, `nc-gap-P7-*-post-probe`) | `Lifecycle/Module.cls#L41-L43` (kills only `CacheD`) |

<a id="p-f-unin-11"></a>
### P:F-UNIN-11

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed self-Clean branching and old warning output. A fresh level-0 self-Clean and after-hook observer is not covered by blocked ordinary-module Clean; no root-cause equivalence is assumed.

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall)

Starting evidence and claim:

| F-UNIN-11 | Low | `zpm clean` prints "WARNING: N module(s) orphaned" while removing nothing, and goes down the self-uninstall path (by reading, its after-phase hooks do not run). Intended: an ordinary level-0 Clean (DEC-UNIN-2). | IRIS (`nc-unin-9-clean-on-ipm`); inspection | `Lifecycle/Module.cls#L27-L81`; `Storage/Module.cls#L630-L655` |

<a id="p-f-unin-2"></a>
### P:F-UNIN-2

Disposition: active · Reproduction: e2e reproduced

ordinary persistent sentinel survives -purge and schema removal/reload; real extent-delete and installer callback neighbors

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall.purge.as-built)

Starting evidence and claim:

| F-UNIN-2 | Medium | `uninstall -purge` deletes no table data. Nothing reads the `Purge` parameter; `%Clean` deletes classes without the extent flag `e`. (An installer can read `pVars("Purge")`.) | IRIS (`nc-unin-3-uninstall-plain`, `-purge`; IRIS fact `nc-unin-env-obj-delete-e-flag`) | `Main.cls#L505`; `Lifecycle/Base.cls#L387`, `#L554-L561` |

<a id="p-f-unin-3"></a>
### P:F-UNIN-3

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall)

Starting evidence and claim:

| F-UNIN-3 | Medium | The self-uninstall guard (mapping check, module list, full-cleanup question, shell exit) runs only when the typed name is exactly `zpm`. `uninstall IPM`/`ipm`/`ZPM` removes IPM's classes with no question and leaves the shell running. | Proved (`unin_witness_external_name_bypass`); IRIS (`nc-unin-self-external-name-IPM`; mapped namespaces refuse, `nc-unin-4-*`) | `Main.cls#L2712` |

<a id="p-f-unin-4"></a>
### P:F-UNIN-4

Disposition: active · Reproduction: e2e reproduced

dependent graph error lets uninstall remove required class; resolvable dependent neighbor blocks

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall.dependents)

Starting evidence and claim:

| F-UNIN-4 | **High** | Uninstall without `-force` removes a module that a dependent still needs if that dependent's dependency graph fails to build (e.g. a registry is missing): graph errors are ignored. | Proved (`unin_witness_unresolvable_dependent`); IRIS (`nc-unin-5-unresolvable-dependent`) | `Utils/Module.cls#L460-L499`, `#L615-L622` (`pErrorList` unread) |

<a id="p-f-unin-5"></a>
### P:F-UNIN-5

Disposition: active · Reproduction: e2e reproduced

ordinary name-only global resource retains data; explicit Global attribute deletes it

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall)

Starting evidence and claim:

| F-UNIN-5 | Medium | A global resource declared only by name (`<Resource Name="X.GBL"/>`, the usual form) keeps its data after uninstall; only resources with the `Global` attribute are killed. | Proved (`unin_witness_global_kept`); IRIS (`nc-unin-6-global-resources`) | `ResourceProcessor/Default/Global.cls#L76-L80` |

<a id="p-f-unin-6"></a>
### P:F-UNIN-6

Disposition: active · Reproduction: e2e reproduced

blocked uninstall returns OK and logs success with module retained; successful ordinary uninstall neighbor

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall.dependents)

Starting evidence and claim:

| F-UNIN-6 | Medium | A blocked uninstall returns OK, prints `Clean SUCCESS` and logs `uninstall/success` while the module stays. The code comment at `Lifecycle/Base.cls#L276` says it returns an error; IPM's own test asserts success. | Proved (in `unin_witness_blocked_deletes_webapp`, `_recurse_skipped_dep`); IRIS (`nc-unin-1-blocked-uninstall`) | `Lifecycle/Base.cls#L290-L301`; `Storage/Module.cls#L746-L755`. Intent (DEC-UNIN-1, issue #58): block with an error naming the dependents; IPM's own skipped test `TestUninstallDependencyDoesError` asserts that |

<a id="p-f-unin-7"></a>
### P:F-UNIN-7

Disposition: inconclusive · Reproduction: none newly qualified

Reviewed typed/untyped orphan enumeration and inherited interoperability noise logs. The current CLS ownership evidence does not establish the full untyped interoperability/system inventory, so no whole-reader defect claim is retained.

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall)

Starting evidence and claim:

| F-UNIN-7 | Low | `orphans` with no `-type` lists IRIS-shipped and mapped interoperability items (BPL/DTL views of `Ens*`, X12 schemas, CSP defaults) that no module could own. `-type CLS` is correct. | IRIS (`nc-unin-10-orphans`, `-detail`) | `Utils/Module.cls#L2152-L2153` |

<a id="p-f-unin-8"></a>
### P:F-UNIN-8

Disposition: security-outside-reproduction · Reproduction: defensive source/existing evidence review only

Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall)

Starting evidence and claim:

| F-UNIN-8 | **High** | `uninstall <module> -r` on a module that lists `zpm` among its dependencies (and is the only one) removes IPM's classes, then fails with `<CLASS DOES NOT EXIST>`; both records stay and IPM must be reinstalled. | Proved (`unin_witness_recurse_removes_ipm`); IRIS (`nc-repo-model-unin-recurse-ipm-dependency`) | `Lifecycle/Base.cls#L431-L490`, `#L472` |

<a id="p-f-unin-9"></a>
### P:F-UNIN-9

Disposition: active · Reproduction: e2e reproduced

ordinary recursive uninstall removes dependency code/module but retains its manifest; top manifest is removed

[Canonical ontology](../../../ontology/features/uninstall.g5.md#uninstall.recurse)

Starting evidence and claim:

| F-UNIN-9 | Low | A dependency removed by `uninstall -r` keeps its stored manifest in `^IPM.StudioDoc.ModuleStreamD`; only the top module's manifest is deleted. | Proved (`unin_witness_recurse_leaves_manifest`); IRIS (`nc-repo-model-unin-recurse-removed-dep`) | `Lifecycle/Base.cls#L472` vs `StudioDocument/Module.cls#L205-L227` |

## Referenced-only inherited aliases

<a id="c-d1"></a>
### C:D1 precedence claim

Normative defect credit was already withdrawn. The same-version temporary-repository precedence is owned by C:Q1 / load.dependencies.local-repo.precedence. Its observed creation-order tie is preserved; no new defect is counted.

[Inherited meaning-based aliases](../extension/reconciled-findings.json) also retain G:V6 / P:V4 empty-manager links to G:D13. The post-commit macro failure P:F-LOAD-4 remains a different trigger.

[Exact constructor and active-history withdrawals](../iter2/boundaries/withdrawals.json) preserve OW-CONSTRUCTOR and L12: immediate default-lock release does not withdraw all BuildCache properties, and the locked close schedule does not withdraw the accessible-delete absence schedule.
