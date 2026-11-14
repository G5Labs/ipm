# 06 — Registries, publishing and settings

Module root: REPO-0

Scope: `repo` (remote, ORAS and folder registries; enable and disable; read-only; snapshots and prereleases; ORAS
registry namespace; credentials; delete; list), `publish` and `unpublish`, `search`, and the read-back commands
`list-installed`, `show` and `module-version`, plus `config` and `default-modifiers`. Installing from a registry
(dependency resolution, install versus update) belongs to modules 03 and 04; this module only states what the
registry flags change. Baseline: ipm 571e04a1. Paths are relative to the IPM checkout (`ipm/`). `X` tags name tests that assert the claim but were
not run here. `O` and `T` tags come from native runs on IRIS 2026.1 in container `ipm-uplift-iris-repo` (logs
`validation/logs/7*-repo-*.log`; check table in `intents/05-06-seed-challenge.md`).
`P` tags name lemmas of `models/verus/settings.rs` (verified, controls all killed); `M` tags name checks of
`models/explorer/repo/check.py` in `evidence/model-checks-repo.json`. Premises are named in each Note and in
`models/verus/settings.TRUST.md`. `O!` marks an observation that refutes its statement (always with F or OPEN). A
statement with F carries no P: a proof under a premise that excludes the statement's case is not evidence for it, and
a defect witness is cited in the finding.

## Glossary
| Term | Defined by |
|---|---|
| remote registry | REPO-D1 |
| ORAS registry | REPO-D2 |
| folder registry | REPO-D3 |
| registry namespace | REPO-D4 |
| publish | REPO-D5 |
| unpublish | REPO-D6 |
| enabled | REPO-D7 |
| publish flag | REPO-D8 |
| developer mode | REPO-D9 |
| read-only | REPO-D10 |
| credentials | REPO-D11 |
| prerelease | REPO-D12 |
| snapshot | REPO-D13 |
| setting | REPO-D14 |
| default modifiers | REPO-D15 |

## Statements
| ID | Parent | Kind | Statement | Evidence | Source | Notes |
|---|---|---|---|---|---|---|
| REPO-0 | SYS-0 | R | Each namespace keeps its own list of registries. IPM can send modules to them and look for modules in them. | I | ipm/src/cls/IPM/Repo/Definition.cls#L3-L29; ipm/src/cls/IPM/Main.cls#L222-L266 | Stored in `^IPM.Repo.DefinitionD` of the namespace, unless repo settings are mapped (`enable -map -repos`, out of scope). |
| REPO-D1 | REPO-0 | D | A **remote registry** is a registry server that IPM talks to over the web. | I | ipm/src/cls/IPM/Repo/Remote/Definition.cls#L1-L33 | Moniker `registry`, alias `remote`, flag `-r`. The community registry is one. |
| REPO-D2 | REPO-0 | D | An **ORAS registry** is a kind of registry server. It keeps each module as a container image. | I | ipm/src/cls/IPM/Repo/Oras/Definition.cls#L1-L33 | OCI registry via the ORAS client; flag `-o`. Changelog #541. |
| REPO-D3 | REPO-0 | D | A **folder registry** is a folder on the server that holds module files. | I | ipm/src/cls/IPM/Repo/Filesystem/Definition.cls#L1-L40 | Moniker `filesystem`, flags `-f` and `-fs`. |
| REPO-D4 | REPO-0 | D | A **registry namespace** is a path inside an ORAS registry. It is not an IRIS namespace. | I | ipm/src/cls/IPM/Repo/Oras/Definition.cls#L28-L29 | `-namespace` (`-ns`). Module path is `<url>/<namespace>/<module>`. |
| REPO-D5 | REPO-0 | D | To **publish** a module is to pack it and send it to a registry. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1580-L1663 | Publish phase. |
| REPO-D6 | REPO-0 | D | To **unpublish** is to delete a module, or one version of it, from a registry. | I | ipm/src/cls/IPM/Main.cls#L197-L209 | |
| REPO-D7 | REPO-0 | D | A registry is **enabled** when IPM may use it. The -disable option turns that off. | I | ipm/src/cls/IPM/Repo/Definition.cls#L29; ipm/src/cls/IPM/Main.cls#L258-L259 | Help: "-disable: specifies that it can not be used". |
| REPO-D8 | REPO-0 | D | The **publish flag** marks the registry to use when publish names no registry. | I | ipm/src/cls/IPM/Repo/Http/Definition.cls#L25-L27 | `-publish 1` sets `DeploymentEnabled`; shown as "Default Deployment Registry?". |
| REPO-D9 | REPO-0 | D | **Developer mode** installs a module so that its source can still be edited. | I | ipm/src/cls/IPM/Main.cls#L306 | `load -dev`; details belong to module 03. |
| REPO-D10 | REPO-0 | D | A registry can be marked **read-only**. Then IPM will not use developer mode for its modules. | I | ipm/src/cls/IPM/Main.cls#L262; ipm/src/cls/IPM/Repo/Definition.cls#L71-L73 | `-read-only` (`-ro`). |
| REPO-D11 | REPO-0 | D | **Credentials** are the user name, password or token that a registry may ask for. | I | ipm/src/cls/IPM/Repo/Http/Definition.cls#L17-L23 | Remote and ORAS registries only. |
| REPO-D12 | REPO-0 | D | A **prerelease** is a test version, such as 1.0.0-beta. | I | ipm/src/cls/IPM/Main.cls#L261 | |
| REPO-D13 | REPO-0 | D | A **snapshot** is a build that stands for the latest work on a version. | I | ipm/src/cls/IPM/Main.cls#L260 | Version ending in `+snapshot`. |
| REPO-D14 | REPO-0 | D | A **setting** is a named value that changes how IPM works. | I | ipm/src/cls/IPM/Repo/UniversalSettings.cls#L1-L53 | `config` command. |
| REPO-D15 | REPO-0 | D | **Default modifiers** are options that IPM adds to each command. | I | ipm/src/cls/IPM/Main.cls#L617-L637 | `default-modifiers` command; values are `-D` data options. |
| REPO-B1 | REPO-0 | B | The repo command adds a registry to the namespace, or changes one. | I | ipm/src/cls/IPM/Main.cls#L2233-L2283; ipm/src/cls/IPM/Repo/Definition.cls#L138-L251 | `Configure`. |
| REPO-B2 | REPO-B1 | B | Each registry has a name. No two registries in a namespace share a name. | I | ipm/src/cls/IPM/Repo/Definition.cls#L22 | Unique index `ServerDefinitionKey`. |
| REPO-B3 | REPO-B1 | B | If the name is new, IPM makes a new registry. | I | ipm/src/cls/IPM/Repo/Definition.cls#L168-L172 | |
| REPO-B4 | REPO-B1 | B | If the name exists, IPM changes only the options that are given. | I | ipm/src/cls/IPM/Repo/Definition.cls#L174-L193; ipm/src/cls/IPM/Repo/Remote/Definition.cls#L88-L103 | The kind may be left out for an existing name (Main.cls#L2256-L2261). |
| REPO-B5 | REPO-B1 | B | IPM will not turn a registry into another kind. | I | ipm/src/cls/IPM/Repo/Definition.cls#L161-L165 | Error suggests `repo -n <name> -delete` first. |
| REPO-B6 | REPO-B1 | B | A new registry is enabled. | I | ipm/src/cls/IPM/Repo/Definition.cls#L174-L178 | If both -enable and -disable are given, -disable wins. |
| REPO-B7 | REPO-B1 | B | A new registry looks for snapshots and prereleases unless told not to. | I | ipm/src/cls/IPM/Repo/Definition.cls#L180-L189 | The property defaults are 0 (#L33-L35) but `repo` sets 1 for a new registry. |
| REPO-B8 | REPO-B1 | B | A new registry is not read-only. | I | ipm/src/cls/IPM/Repo/Definition.cls#L190-L194 | |
| REPO-B9 | REPO-B1 | B | When a needed value is missing, IPM asks the operator for it. | I | ipm/src/cls/IPM/Repo/Definition.cls#L148-L153; ipm/src/cls/IPM/Main.cls#L2263-L2272 | Name, kind, address or folder path. |
| REPO-B10 | REPO-B1 | B | IPM refuses a kind name that is not a kind of registry. | F:F-REPO-9; O!:nc-repo-10-invalid-type | ipm/src/cls/IPM/Main.cls#L2274-L2277 | `-type` is matched against monikers, not class names; an unknown value is dropped and IPM asks for the kind (`nc-repo-10-invalid-type`). The "Invalid type" status at #L2274-L2277 is never reached. F-REPO-6 refuted. |
| REPO-B11 | REPO-B1 | B | The -enable option lets IPM use the registry again. | I | ipm/src/cls/IPM/Repo/Definition.cls#L176-L177 | |
| REPO-B12 | REPO-B1 | B | When IPM looks for a module to install, it skips registries that are not enabled. | I; O:nc-repo-2-disabled-registry; P:repo_install_enabled_only; M:repo-explore-set-install-enabled | ipm/src/cls/IPM/Repo/Manager.cls#L66-L71 | `where Enabled = 1`. Observed: install did not find a module that only the disabled registry holds. Proof (P, M): no premise. Premise of the model: the registry query keeps rows with Enabled = 1 (transcribed). |
| REPO-B13 | REPO-B1 | B | Take a module from a read-only registry. IPM will not use developer mode for it. | I | ipm/src/cls/IPM/Utils/Module.cls#L1276-L1286 | Overrides `-dev`. |
| REPO-B14 | REPO-B13 | B | The read-only mark does not change how publish or unpublish work. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1580-L1663; ipm/src/cls/IPM/Main.cls#L2768-L2828 | No reader of `ReadOnly` on those paths. |
| REPO-B15 | REPO-B1 | B | A registry set to skip prereleases hides them when IPM looks for modules. | I | ipm/src/cls/IPM/Repo/Manager.cls#L99-L108; ipm/src/cls/IPM/Utils/Module.cls#L926-L927 | Same rule for snapshots. |
| REPO-B16 | REPO-0 | B | A remote or ORAS registry needs a web address. | I | ipm/src/cls/IPM/Repo/Http/Definition.cls#L13 | `-url`. |
| REPO-B17 | REPO-B16 | B | IPM refuses a second registry with the same address. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L566; O:nc-repo-3-url-change-credentials | ipm/src/cls/IPM/Repo/Http/Definition.cls#L66-L89 | Error names the registry that holds the address. Changelog #1281. Observed across kinds: a remote registry may not take an ORAS registry's URL. |
| REPO-B18 | REPO-B17 | B | Addresses that differ only in form count as the same. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L532 | ipm/src/cls/IPM/Repo/Http/Definition.cls#L53-L58 | Case of the host, default port, trailing slash, missing scheme. |
| REPO-B19 | REPO-B17 | B | Two ORAS registries can share an address. Each must then have its own registry namespace. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L511 | ipm/src/cls/IPM/Repo/Oras/Definition.cls#L10-L14 | |
| REPO-B20 | REPO-B16 | B | IPM refuses an ORAS address with a path after the host. It suggests a registry namespace. | X:tests/unit_tests/Test/PM/Unit/Oras.cls#L93; T:Test.PM.Unit.Oras | ipm/src/cls/IPM/Repo/Oras/PackageService.cls#L35-L52 | Changelog #1283. An old registry with a path still shows the hint on `repo -list`. |
| REPO-B21 | REPO-B16 | B | IPM stores a registry namespace in lower case, with no slash at either end. | X:tests/unit_tests/Test/PM/Unit/Oras.cls#L64; T:Test.PM.Unit.Oras | ipm/src/cls/IPM/DataType/OrasNamespace.cls | Also #L67, #L69. |
| REPO-B22 | REPO-B16 | B | ORAS search results leave the registry namespace out of the names. | I; X:tests/unit_tests/Test/PM/Unit/Oras.cls#L135; O:nc-repo-12-nested-namespace-name; T:Test.PM.Unit.Oras | ipm/src/cls/IPM/Repo/Oras/PackageService.cls#L14-L23; ipm/src/cls/IPM/Repo/Oras/PackageService.cls#L456 | Seed "Registry namespace". Observed: published under `nc-repo-ns12/sub`, then listed with namespace `nc-repo-ns12` as `sub/nc-repo-pub` (namespace left out, deeper part kept; seed refined). |
| REPO-B23 | REPO-B16 | B | The needs of a module from a registry namespace are looked for in that same namespace. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L153 | ipm/src/cls/IPM/Repo/Oras/PackageService.cls#L14-L23 | Also #L154-L155. Changelog #1267, commit 8fd97c0b. |
| REPO-B24 | REPO-B16 | B | A search in a registry namespace also finds modules in paths below it. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L612 | ipm/src/cls/IPM/Repo/Oras/PackageService.cls#L420-L456 | Changelog #1269. |
| REPO-B25 | REPO-0 | B | A folder registry needs the path of a folder. Two folder registries can not share one. | I | ipm/src/cls/IPM/Repo/Filesystem/Definition.cls#L10-L15 | Unique index `RootIndex`. |
| REPO-B26 | REPO-B25 | B | When it is set up, IPM scans the folder and keeps a list of the modules it finds. | I; O:nc-repo-11-folder-cache | ipm/src/cls/IPM/Repo/Filesystem/Definition.cls#L85; ipm/src/cls/IPM/Repo/Filesystem/Definition.cls#L117-L182 | `BuildCache`; also fixes the scan depth to the deepest level found. |
| REPO-B27 | REPO-B26 | B | IPM does not see module files added later until the registry is set up again. | I; O:nc-repo-11-folder-cache | ipm/src/cls/IPM/Repo/Filesystem/PackageService.cls#L12-L23 | Lookups read the stored list only. The files themselves are read when installing. Observed: a module folder added later is not found by `search` or `install` until `repo -n <name> -path` runs again. |
| REPO-B28 | REPO-B25 | B | A folder registry can not take published modules. | I | ipm/src/cls/IPM/Repo/Filesystem/Definition.cls#L93-L97; ipm/src/cls/IPM/Lifecycle/Base.cls#L1619-L1623 | "Repository '%1' does not support publishing modules." |
| REPO-B29 | REPO-0 | B | The -delete option removes the registry it names. | I | ipm/src/cls/IPM/Main.cls#L2206-L2214 | tests/integration_tests/Test/PM/Integration/PublishModuleWithDeps.cls#L14 asserts only command success. |
| REPO-B30 | REPO-B29 | B | Delete fails when the name is missing or no such registry exists. | I | ipm/src/cls/IPM/Main.cls#L2208-L2213 | "Missing -name modifier"; "Repository ... does not exist so cannot be deleted." |
| REPO-B31 | REPO-B29 | B | The -delete-all option removes all registries. Kind flags can limit it to some kinds. | I | ipm/src/cls/IPM/Main.cls#L2197-L2205 | No question is asked. |
| REPO-B32 | REPO-B29 | B | Deleting a registry leaves the modules installed from it in place. | I | ipm/src/cls/IPM/Main.cls#L2206-L2214 | Only the registry row is deleted. |
| REPO-B33 | REPO-0 | B | The -list option shows each registry with its kind, address and flags. | I | ipm/src/cls/IPM/Repo/Definition.cls#L112-L130; ipm/src/cls/IPM/Main.cls#L2113-L2135 | Prints "No repositories found." when the list is empty. Also printed after every change (Main.cls#L2282). |
| REPO-B34 | REPO-B33 | B | It shows whether each registry can be reached right now. | I | ipm/src/cls/IPM/Repo/Definition.cls#L117-L126 | A setup error is shown, not thrown. |
| REPO-B35 | REPO-B33 | B | It never shows a password or token. It only says that one is set. | I; O:nc-repo-13-secret-at-rest | ipm/src/cls/IPM/Repo/Http/Definition.cls#L124-L133 |  |
| REPO-B36 | REPO-B33 | B | For an ORAS registry with credentials, it shows if the registry takes them. | I | ipm/src/cls/IPM/Repo/Oras/Definition.cls#L71-L83 | Changelog #1130. |
| REPO-B37 | REPO-B33 | B | IPM looks in folder registries first, then in the others by age. | I | ipm/src/cls/IPM/Repo/Manager.cls#L69-L70; ipm/src/cls/IPM/Repo/Http/Definition.cls#L136-L140 | Sort order 2 for folders, 2+ID for the rest. Changelog HSIEO-9269. |
| REPO-B39 | REPO-0 | B | The password can be typed in the command or at a hidden prompt. It can also come from an environment variable. | I; X:tests/unit_tests/Test/PM/Unit/CLI.cls#L242; O:nc-repo-13-secret-at-rest; T:Test.PM.Unit.CLI | ipm/src/cls/IPM/Repo/Definition.cls#L196-L226 | `-password`, `-password-stdin`, `-password-env`. Changelog #462, #1178. |
| REPO-B40 | REPO-B39 | B | A token can come from the command line or from an environment variable. | X:tests/unit_tests/Test/PM/Unit/CLI.cls#L267; O:nc-repo-13-secret-at-rest; T:Test.PM.Unit.CLI | ipm/src/cls/IPM/Repo/Definition.cls#L228-L241 | `-token`, `-token-env`. |
| REPO-B42 | REPO-B39 | B | IPM refuses two sources for the password, or two for the token. | X:tests/unit_tests/Test/PM/Unit/CLI.cls#L258; T:Test.PM.Unit.CLI | ipm/src/cls/IPM/Repo/Definition.cls#L199-L201; ipm/src/cls/IPM/Repo/Definition.cls#L229-L231 | Token case asserted at #L280. |
| REPO-B43 | REPO-B39 | B | IPM fails if the named environment variable is empty or not set. | X:tests/unit_tests/Test/PM/Unit/CLI.cls#L250; T:Test.PM.Unit.CLI | ipm/src/cls/IPM/Repo/Definition.cls#L217-L223 | Token case asserted at #L275. |
| REPO-B44 | REPO-0 | B | IPM keeps the password and token as plain text in the namespace. | I; O:nc-repo-13-secret-at-rest | ipm/src/cls/IPM/Repo/Http/Definition.cls#L17-L21 | Plain `%String` properties in `^IPM.Repo.DefinitionD`. An environment variable is read once and its value is stored. Observed: values from `-password-env` and `-token-env` stored in plain text. Accepted interim per issue #628 (security wallet planned, label upstream). Intent: DEC-REPO-6 resolved (evidence), decisions/research/repo.md. |
| REPO-B110 | REPO-B44 | B | Where IRIS has a secure wallet, IPM keeps these secrets in it. | N | ipm/src/cls/IPM/Repo/Http/Definition.cls#L17-L21 | Planned direction, issue #628 (open, enhancement, upstream). IRIS 2026.1 has the wallet API (%Wallet.KeyValue, %Wallet.Collection; r4-repo-env-secure-wallet). IPM still supports IRIS 2022.1, so older versions need a fallback. Intent: DEC-REPO-6 resolved (evidence), decisions/research/repo.md. |
| REPO-B45 | REPO-0 | B | When a remote registry gets a new address, IPM clears its stored credentials. | I; O:nc-repo-3-url-change-credentials | ipm/src/cls/IPM/Repo/Remote/Definition.cls#L81-L86 |  |
| REPO-B46 | REPO-0 | B | When an ORAS registry gets a new address, IPM clears its stored credentials. | F:F-REPO-11; O!:nc-repo-3-url-change-credentials | ipm/src/cls/IPM/Repo/Oras/Definition.cls#L87-L141 | Code keeps them and may send them to the new host. Observed: ORAS kept user and password after a URL change (`nc-repo-3-url-change-credentials`). Intent: issue #649 and its fix for remote registries (Remote/Definition.cls#L81-L86, CHANGELOG #649); #1115 treats credentials sent to another host as a bug (F-REPO-11). Intent: DEC-REPO-3 resolved (assumption), decisions/research/repo.md. |
| REPO-B47 | REPO-0 | B | The -token-auth-method option sets how IPM sends the token. | F:F-REPO-1; O!:nc-repo-4-token-auth-method | ipm/src/cls/IPM/Repo/Http/Definition.cls#L45; ipm/src/cls/IPM/Repo/Oras/Definition.cls#L118-L120 | Read for ORAS only; a remote registry keeps `apiKey`. Observed: stored `apiKey`, header `apikey` (`nc-repo-4-token-auth-method`). |
| REPO-B48 | REPO-0 | B | The -sslconfig option picks the secure link setup that IPM uses. | F:F-REPO-2; O!:nc-repo-5-sslconfig-publish-url | ipm/src/cls/IPM/Repo/Http/Definition.cls#L46; ipm/src/cls/IPM/Repo/Http/PackageService.cls#L18-L27 | Never read; IPM makes an SSL config named after the host. Observed: request used the host-named config (`nc-repo-5-sslconfig-publish-url`). |
| REPO-B49 | REPO-0 | B | The -publish-url option sets a second address just for publish. | F:F-REPO-2; O!:nc-repo-5-sslconfig-publish-url | ipm/src/cls/IPM/Repo/Http/Definition.cls#L38 | Never read. Observed: not stored (`nc-repo-5-sslconfig-publish-url`). |
| REPO-B50 | REPO-0 | B | If an ORAS registry rejects the credentials, IPM warns once. It then tries the other registries. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L428 | ipm/src/cls/IPM/Repo/Manager.cls#L117-L134 | Changelog #1115. |
| REPO-B51 | REPO-0 | B | The publish command builds the module, packs it and sends it to a registry. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1580-L1663; ipm/src/cls/IPM/Lifecycle/Base.cls#L182 | Runs Initialize to Activate first; `-only` skips them (changelog #769). |
| REPO-B52 | REPO-B51 | B | The -repo option names the registry to publish to. | I; X:tests/integration_tests/Test/PM/Integration/Oras.cls#L294; O:nc-repo-2-disabled-registry; O:nc-repo-9-unpublish-deployed-version | ipm/src/cls/IPM/Lifecycle/Base.cls#L1597-L1600 | The test publishes with `-r`, installs from that registry, then calls the module. |
| REPO-B53 | REPO-B52 | B | Publish fails when the named registry does not exist. | I; P:repo_publish_named_missing | ipm/src/cls/IPM/Lifecycle/Base.cls#L1598-L1611 | "No server found to publish to." Proof (P): no premise. |
| REPO-B54 | REPO-B52 | B | Publish to a named registry that is not enabled does not go ahead. | F:F-REPO-7; O!:nc-repo-2-disabled-registry | ipm/src/cls/IPM/Lifecycle/Base.cls#L1598-L1600 | Code has no enabled check here and publishes. Observed: `publish -r` to a disabled ORAS registry returned OK (`nc-repo-2-disabled-registry`, refuting). Witness `repo_witness_disabled_registry_used` is the evidence for F-REPO-7; `repo_refines_publish_named` needs premise PE (the named registry is enabled), which excludes this case, so it is not cited here. Intent: help 'specifies that it can not be used' (Main.cls#L259); install, the default publish pick and unpublish already refuse it; `repo -list` still shows it. Intent: DEC-REPO-2 resolved (evidence), decisions/research/repo.md. |
| REPO-B55 | REPO-B51 | B | With no -repo, publish uses the registry that has the publish flag. | I; F:F-REPO-10; O!:nc-repo-model-publish-row-order | ipm/src/cls/IPM/Repo/Http/Definition.cls#L142-L182; ipm/src/cls/IPM/Repo/Remote/Definition.cls#L117-L129 | Each kind's query is read for its first row only and has no ORDER BY, so a flag on a later registry of the same kind is never seen. Observed: remote A (no flag) made before remote B (flag): the pick is A (`nc-repo-model-publish-row-order`, cases 1, 2 and 4; refuting). `repo_refines_publish_default` holds only under PH, PU and PK (the flag holder is the first enabled registry of its kind); PK excludes exactly the case this finding is about, so it is not cited here. Witness `repo_witness_publish_flag_ignored` is the evidence for F-REPO-10. Positive parts without premises: REPO-B57, REPO-B58. |
| REPO-B56 | REPO-B55 | B | With no -repo and no publish flag set, publish still picks a registry. | I; O:nc-repo-1-publish-no-flag | ipm/src/cls/IPM/Repo/Remote/Definition.cls#L117-L129; ipm/src/cls/IPM/Repo/Oras/Definition.cls#L143-L155 | Intent: PR #332 (author: not 'to force users to use publish flag'; order flag, token, user and password, any other; maintainer agreed); PR #569 renamed the column to Default Deployment Registry. The help Main.cls#L56-L57 predates this and is stale (F-REPO-3, documentation). Observed: a token-only remote registry was chosen. A configured community registry with credentials can receive a private module; maintainers accepted that in #332. Intent: DEC-REPO-1 resolved (evidence), decisions/research/repo.md. |
| REPO-B109 | REPO-B56 | B | It tries one with a token first. Next it tries one with a user and password, then any other. | I; F:F-REPO-10; O:nc-repo-1-publish-no-flag | ipm/src/cls/IPM/Repo/Remote/Definition.cls#L117-L129; ipm/src/cls/IPM/Repo/Oras/Definition.cls#L143-L155 | Within a kind the first row wins: the query lost its ORDER BY in 3174d72d (PR #541), so an older registry can win (F-REPO-10, nc-repo-model-publish-row-order); across kinds, order then name. Intent: DEC-REPO-1 resolved (evidence), decisions/research/repo.md. |
| REPO-B57 | REPO-B55 | B | With no -repo, publish never picks a registry that is not enabled. | I; P:repo_publish_default_enabled; M:repo-explore-set-publish-default-enabled | ipm/src/cls/IPM/Repo/Remote/Definition.cls#L128; ipm/src/cls/IPM/Repo/Oras/Definition.cls#L154 | Proof (P, M): no premise. |
| REPO-B58 | REPO-B55 | B | With no -repo, publish never picks a folder registry. | I; P:repo_publish_default_enabled; M:repo-explore-set-publish-default-enabled | ipm/src/cls/IPM/Repo/Http/Definition.cls#L142-L164 | Only remote and ORAS kinds are asked. Proof (P, M): no premise. |
| REPO-B59 | REPO-B55 | B | Only one registry in the namespace can hold the publish flag at a time. | I; F:F-REPO-3 | ipm/src/cls/IPM/Repo/Remote/Definition.cls#L104-L110; ipm/src/cls/IPM/Repo/Oras/Definition.cls#L127-L133 | Restated by the review fix (finding 9) from the help, Main.cls#L56-L57: "there may only be one of these per namespace". Code clears the flag only on other registries of the same kind (Remote/Definition.cls#L104-L110, Oras/Definition.cls#L127-L133), so a remote and an ORAS registry can both hold it; the default pick then orders by flag, then by registry name. Premise PU of settings.rs is this statement. |
| REPO-B60 | REPO-B59 | B | Giving the flag to one registry takes it from the one that held it. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L526; F:F-REPO-3 | ipm/src/cls/IPM/Repo/Oras/Definition.cls#L127-L133 | Restated with REPO-B59. The test moves the flag between two ORAS registries, the case the code handles; a holder of the other kind keeps it (F-REPO-3). |
| REPO-B61 | REPO-B51 | B | Publish has a -use-external-name option. It sends the module under its other name. | I | ipm/src/cls/IPM/Lifecycle/Base.cls#L1625-L1632 | Fails when that name is empty or not valid. |
| REPO-B62 | REPO-B51 | B | If an ORAS registry has that version, publish puts the new one in its place. | I | ipm/src/cls/IPM/Repo/Oras/PublishService.cls#L93-L97 | "Existing tags will be overwritten". Remote registries decide for themselves. |
| REPO-B64 | REPO-B51 | B | When an ORAS registry refuses a publish, IPM says why. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L326 | ipm/src/cls/IPM/Repo/Oras/HttpException.cls | Bad credentials, no access (#L337), no credentials (#L347). |
| REPO-B65 | REPO-0 | B | The unpublish command deletes one version, or all versions, of a module. | I; O:nc-repo-9-unpublish-deployed-version | ipm/src/cls/IPM/Main.cls#L2768-L2828 | Version `all` deletes every version. |
| REPO-B66 | REPO-B65 | B | The registry name goes in front, as in MyRepo/my-module. | I | ipm/src/cls/IPM/Main.cls#L2777-L2781 | The whole argument is lower-cased first. |
| REPO-B67 | REPO-B65 | B | With no prefix, unpublish uses the registry that publish would pick. | I | ipm/src/cls/IPM/Main.cls#L2782-L2786 | Same query as REPO-B55, so the fallback of REPO-B56 applies too. |
| REPO-B68 | REPO-B65 | B | Unpublish fails when the named registry does not exist. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L367 | ipm/src/cls/IPM/Main.cls#L2790-L2792 | "Registry not found: <name>". |
| REPO-B69 | REPO-B65 | B | Unpublish asks the operator to confirm. It warns that the delete can not be undone. | I | ipm/src/cls/IPM/Main.cls#L2799-L2818 | |
| REPO-B70 | REPO-B69 | B | If the operator says no, nothing is deleted. | I | ipm/src/cls/IPM/Main.cls#L2820-L2824 | The command still reports success and prints nothing. |
| REPO-B71 | REPO-B69 | B | The -force option skips the question. | I; X:tests/integration_tests/Test/PM/Integration/Oras.cls#L158; O:nc-repo-9-unpublish-deployed-version | ipm/src/cls/IPM/Main.cls#L2801 | Changelog #951. The registry must still allow it. |
| REPO-B72 | REPO-B65 | B | Unpublish fails when the module or version is not in the registry. | X:tests/integration_tests/Test/PM/Integration/Oras.cls#L354; O:nc-repo-9-unpublish-deployed-version | ipm/src/cls/IPM/Repo/Oras/PublishService.cls#L82-L88 | Module case at #L361. Changelog #1115. |
| REPO-B73 | REPO-B65 | B | Unpublish fails when the registry turns the request down. | I | ipm/src/cls/IPM/Main.cls#L2825-L2827; ipm/src/cls/IPM/Repo/Remote/PublishService.cls#L56-L74 | |
| REPO-B74 | REPO-B65 | B | For a remote registry, unpublish needs credentials. Without them it stops at once. | I | ipm/src/cls/IPM/Repo/Remote/PublishService.cls#L42-L44 | "Unpublishing module, authorization required." |
| REPO-B75 | REPO-B65 | B | Unpublish will not work on a registry that is not enabled. | I; O:nc-repo-2-disabled-registry | ipm/src/cls/IPM/Repo/Oras/PublishManager.cls#L15-L19; ipm/src/cls/IPM/Repo/Remote/PublishManager.cls#L26-L37 | "Repository '%1' is unavailable." |
| REPO-B76 | REPO-B65 | B | Unpublish will not work on a folder registry. | I | ipm/src/cls/IPM/Main.cls#L2796 | The folder kind has no publishing manager, so the call fails with a method error, not a clear message. |
| REPO-B77 | REPO-B65 | B | In ORAS, unpublish of a version also drops its copy for each platform. | F:F-REPO-12; O!:nc-repo-9-unpublish-deployed-version | ipm/src/cls/IPM/Repo/Oras/PublishService.cls#L72-L81; ipm/src/cls/IPM/Repo/Oras/PublishService.cls#L47 | Code deletes only the plain version tag; deployed copies use a tag with a platform suffix. Observed: only tag `1.0.0__2026.1` exists; unpublish of `1.0.0` fails as not found and that tag stays (`nc-repo-9-unpublish-deployed-version`). So a deployed version cannot be unpublished by version at all, only with `all` (F-REPO-12). Intent: a platform build is a variant of the version (wiki 05 Deployed Packages); the remote registry's DeletePackage hides all platform builds of a version. Intent: DEC-REPO-5 resolved (assumption), decisions/research/repo.md. |
| REPO-B78 | REPO-0 | B | The search command lists the modules in each registry of the namespace. | I; O:nc-repo-11-folder-cache | ipm/src/cls/IPM/Main.cls#L2037-L2061; ipm/src/cls/IPM/Main.cls#L2286-L2345 | `repo -list-modules -n <name>` searches one registry. |
| REPO-B80 | REPO-B78 | B | Search hides deployed code built for a different IRIS version. | I | ipm/src/cls/IPM/Utils/Module.cls#L936-L940 | Changelog #1158. Not with `-versions`. |
| REPO-B82 | REPO-B78 | B | Search leaves out a registry that is not enabled. | F:F-REPO-7; O!:nc-repo-2-disabled-registry | ipm/src/cls/IPM/Main.cls#L2047; ipm/src/cls/IPM/Utils/Module.cls#L916-L930 | Code lists every registry, enabled or not. Observed: the disabled registry and its module were listed (`nc-repo-2-disabled-registry`, refuting). Witness `repo_witness_disabled_registry_used` is the evidence for F-REPO-7; `repo_refines_search` needs premise PE (every registry enabled), which excludes this case. Intent: help 'specifies that it can not be used' (Main.cls#L259); install, the default publish pick and unpublish already refuse it; `repo -list` still shows it. Intent: DEC-REPO-2 resolved (evidence), decisions/research/repo.md. |
| REPO-B83 | REPO-0 | B | The list-installed command shows each module in the namespace. | I | ipm/src/cls/IPM/Main.cls#L2939-L3013; ipm/src/cls/IPM/Main.cls#L1304-L1418 | Read-back for uninstall (module 05) and module-version. |
| REPO-B84 | REPO-B83 | B | IPM is always first in the list. | X:tests/unit_tests/Test/PM/Unit/CLI.cls#L548; O:nc-unin-self-zpm-after-reinstall; T:Test.PM.Unit.CLI | ipm/src/cls/IPM/Main.cls#L3015 |  |
| REPO-B88 | REPO-0 | B | The show command prints one installed module, its parts and its needs. | X:tests/integration_tests/Test/PM/Integration/Show.cls#L19; T:Test.PM.Integration.Show | ipm/src/cls/IPM/General/ModuleSummary.cls#L9-L31 | Also #L20-L22. Changelog #1019. |
| REPO-B89 | REPO-B88 | B | Show finds a module by its other public name too. | X:tests/integration_tests/Test/PM/Integration/Show.cls#L29; T:Test.PM.Integration.Show | ipm/src/cls/IPM/General/ModuleSummary.cls#L11-L14 |  |
| REPO-B90 | REPO-B88 | B | Show lists the installed modules that name it as a direct need. | X:tests/integration_tests/Test/PM/Integration/Show.cls#L36; T:Test.PM.Integration.Show | ipm/src/cls/IPM/General/ModuleSummary.cls#L33-L46 | Scoped needs left out (#L23); compare UNIN-B34. |
| REPO-B91 | REPO-B88 | B | Show fails for a module that is not installed. The error names the namespace. | X:tests/integration_tests/Test/PM/Integration/Show.cls#L50; T:Test.PM.Integration.Show | ipm/src/cls/IPM/General/ModuleSummary.cls#L15-L17 |  |
| REPO-B92 | REPO-0 | B | The module-version command sets a new version for a module. | X:tests/integration_tests/Test/PM/Integration/ModuleVersion.cls#L81; T:Test.PM.Integration.ModuleVersion | ipm/src/cls/IPM/Main.cls#L4011-L4104 | Changelog #710. |
| REPO-B93 | REPO-B92 | B | It can raise the major, minor or patch part by one. | X:tests/integration_tests/Test/PM/Integration/ModuleVersion.cls#L81; T:Test.PM.Integration.ModuleVersion | ipm/src/cls/IPM/Main.cls#L4033-L4043 | Cases at #L14-L19 of the XData. |
| REPO-B94 | REPO-B92 | B | It refuses a version that is not higher than the current one, unless -force is given. | X:tests/integration_tests/Test/PM/Integration/ModuleVersion.cls#L73; T:Test.PM.Integration.ModuleVersion | ipm/src/cls/IPM/Main.cls#L4055-L4061 | Cases at #L25-L44. Without a new version and without -force, it always refuses. |
| REPO-B96 | REPO-B92 | B | It also writes the new version into the stored manifest, in one transaction. | X:tests/integration_tests/Test/PM/Integration/ModuleVersion.cls#L88; T:Test.PM.Integration.ModuleVersion | ipm/src/cls/IPM/Main.cls#L4077-L4098 |  |
| REPO-B97 | REPO-0 | B | Settings are shared by every namespace on the server. | I; P:repo_settings_shared; M:repo-explore-set-config-shared | ipm/src/cls/IPM/Repo/UniversalSettings.cls#L1; ipm/src/cls/IPM/Repo/UniversalSettings.cls#L150-L177 | `^IPM.settings` in %SYS. Proof (P, M): the settings are one store in %SYS (the namespace switch at UniversalSettings.cls#L150-L177 is transcribed as that). |
| REPO-B98 | REPO-B97 | B | The command config set changes one setting. Config get reads it back. | X:tests/unit_tests/Test/PM/Unit/CLI.cls#L444; O:nc-repo-6-config-delete; T:Test.PM.Unit.CLI; P:repo_refines_config_set; M:repo-explore-set-config-get-after-set; O:nc-repo-model-config-delete | ipm/src/cls/IPM/Main.cls#L2063-L2093 | `config list` shows all. Test reads back through `GetTestReportFormat`. Proof (P, M): for a known key and a valid value the setting changes, config get reads it back, and the command reports success (`l1_config_status`; control `impl-valid-set-reports-err`). No premise; ModuleRoot taken as already normalized. That a bad key or value reports failure is REPO-B101 (F-REPO-8). |
| REPO-B99 | REPO-B97 | B | Config delete puts a setting back to its default. | F:F-REPO-4; O!:nc-repo-6-config-delete; O:nc-repo-model-config-delete | ipm/src/cls/IPM/Repo/UniversalSettings.cls#L87-L107; ipm/src/cls/IPM/Repo/UniversalSettings.cls#L166-L177 | Observed: delete brought back 30, the first value set (`nc-repo-6-config-delete`, refuting). Also observed: delete of a never-set key stores an empty value and an empty default, so later deletes restore empty, its default (`nc-repo-model-config-delete`). `repo_refines_config_delete` holds only under premise PS (the default node holds the installer's seeded value), which excludes exactly this statement's case, so it is not cited here. Witness `repo_witness_delete_restores_first_value` is the evidence for F-REPO-4. |
| REPO-B101 | REPO-B97 | B | An unknown name or a bad value makes the command fail. | F:F-REPO-8; O!:nc-repo-8-config-errors | ipm/src/cls/IPM/Repo/UniversalSettings.cls#L117-L121; ipm/src/cls/IPM/Main.cls#L2074-L2079 | Code prints a message and reports success. Observed: all three cases return OK (`nc-repo-8-config-errors`). Proof: witness `repo_witness_config_errors_report_ok`; `repo_config_errors_unchanged` proves the settings stay unchanged. Also 'Key and Value required' returns OK (Main.cls#L2076-L2078). Intent: Shell() returns a %Status that CI checks (wiki 01); #1115 judged 'printing the error and reporting success' a bug. Intent: DEC-REPO-4 resolved (evidence), decisions/research/repo.md. |
| REPO-B103 | REPO-0 | B | The -set option adds options to the default list. It does not drop any. | I; O:nc-repo-7-default-modifiers-namespaces; P:repo_dm_laws; M:repo-explore-set-dm-laws; O:nc-repo-model-default-modifiers-merge | ipm/src/cls/IPM/Main.cls#L1747-L1750; ipm/src/cls/IPM/Main.cls#L621 | Proof (P, M): no premise. |
| REPO-B104 | REPO-B103 | B | The -get option prints the list, and -delete clears it. | I; O:nc-repo-7-default-modifiers-namespaces; P:repo_dm_laws; M:repo-explore-set-dm-laws; O:nc-repo-model-default-modifiers-merge | ipm/src/cls/IPM/Main.cls#L1733-L1754 | Exactly one of -set, -get, -delete is required (#L1730-L1732). Proof (P, M): no premise. |
| REPO-B105 | REPO-B103 | B | An option typed in a command wins over the default one. | I; P:repo_dm_laws; M:repo-explore-set-dm-laws | ipm/src/cls/IPM/Main.cls#L1066-L1070 | Defaults merged first, then the parsed command. Proof (P, M): no premise. |
| REPO-B106 | REPO-B103 | B | Default modifiers count only in the namespace where they were set. | F:F-REPO-5; O!:nc-repo-7-default-modifiers-namespaces | ipm/src/inc/IPM/Common.inc#L31; ipm/src/cls/IPM/Main.cls#L619 | Stored in `^%IPM.DefaultModifiers`, which IRIS maps to IRISSYS for every namespace. Observed: set in USER, listed by `-get` in NCMAP (`nc-repo-7-default-modifiers-namespaces`, refuting; `nc-repo-env-percent-global-shared`). `repo_refines_dm_set` and the check `repo-explore-set-dm-per-namespace` hold only under premise PNS (one namespace uses IPM), where the statement is vacuous, so neither is cited. Witness `repo_witness_dm_shared` is the evidence for F-REPO-5. |
| REPO-B107 | REPO-B1 | B | The -info option shows how this namespace is set up as a registry for others. | N | ipm/src/cls/IPM/Main.cls#L251 | Listed in help; `Repository` has no branch for it. |
| REPO-B108 | REPO-B1 | B | With -enable and -app, IPM turns on its web services in this namespace. | N | ipm/src/cls/IPM/Main.cls#L242-L244 | Help example only; `-app` is not a modifier, and no code serves it. |

## States
| ID | State |
|---|---|
| S1 | No registry named R exists in the namespace. |
| S2 | R is enabled and holds the publish flag. |
| S3 | R is enabled and does not hold the publish flag. |
| S4 | R exists but is not enabled. |
| S5 | R is a folder registry. |
| S6 | R is enabled and read-only. |

## Commands
| ID | Command |
|---|---|
| K1 | `repo -<kind> -n R <options>` (make or change R) |
| K2 | `repo -n R -delete` |
| K3 | `repo -delete-all` |
| K4 | `repo -list` |
| K5 | `search` |
| K6 | `publish M -r R` |
| K7 | `unpublish R/M <version>` |
| K8 | `publish M` (no registry named) |

## Cells
| Cell | State | Command | Outcome | Decision |
|---|---|---|---|---|
| REPO-C1 | S1 | K1 | REPO-B3, REPO-B6, REPO-B7, REPO-B8, REPO-B9 | |
| REPO-C2 | S1 | K2 | REPO-B30 | |
| REPO-C3 | S1 | K3 | REPO-B31 | |
| REPO-C4 | S1 | K4 | REPO-B33 | |
| REPO-C5 | S1 | K5 | REPO-B78 | |
| REPO-C6 | S1 | K6 | REPO-B53 | |
| REPO-C7 | S1 | K7 | REPO-B68 | |
| REPO-C8 | S1 | K8 | REPO-B55, REPO-B56, REPO-B109 |  |
| REPO-C9 | S2 | K1 | REPO-B4, REPO-B45, REPO-B46, REPO-B60 |  |
| REPO-C10 | S2 | K2 | REPO-B29, REPO-B32, REPO-B56 |  |
| REPO-C11 | S2 | K3 | REPO-B31, REPO-B32 | |
| REPO-C12 | S2 | K4 | REPO-B33, REPO-B34, REPO-B35 | |
| REPO-C13 | S2 | K5 | REPO-B78 | |
| REPO-C14 | S2 | K6 | REPO-B51, REPO-B52, REPO-B62 | |
| REPO-C15 | S2 | K7 | REPO-B65, REPO-B69, REPO-B72 | |
| REPO-C16 | S2 | K8 | REPO-B55 | |
| REPO-C17 | S3 | K1 | REPO-B4, REPO-B59, REPO-B60 | |
| REPO-C18 | S3 | K2 | REPO-B29, REPO-B32 | |
| REPO-C19 | S3 | K3 | REPO-B31 | |
| REPO-C20 | S3 | K4 | REPO-B33 | |
| REPO-C21 | S3 | K5 | REPO-B78 | |
| REPO-C22 | S3 | K6 | REPO-B52 | |
| REPO-C23 | S3 | K7 | REPO-B65, REPO-B69 | |
| REPO-C24 | S3 | K8 | REPO-B56, REPO-B109 |  |
| REPO-C25 | S4 | K1 | REPO-B11, REPO-B4 | |
| REPO-C26 | S4 | K2 | REPO-B29 | |
| REPO-C27 | S4 | K3 | REPO-B31 | |
| REPO-C28 | S4 | K4 | REPO-B33 | |
| REPO-C29 | S4 | K5 | REPO-B82 |  |
| REPO-C30 | S4 | K6 | REPO-B54 |  |
| REPO-C31 | S4 | K7 | REPO-B75 | |
| REPO-C32 | S4 | K8 | REPO-B57 | |
| REPO-C33 | S5 | K1 | REPO-B26, REPO-B27 | |
| REPO-C34 | S5 | K2 | REPO-B29 | |
| REPO-C35 | S5 | K3 | REPO-B31 | |
| REPO-C36 | S5 | K4 | REPO-B33, REPO-B37 | |
| REPO-C37 | S5 | K5 | REPO-B78, REPO-B27 | |
| REPO-C38 | S5 | K6 | REPO-B28 | |
| REPO-C39 | S5 | K7 | REPO-B76 | |
| REPO-C40 | S5 | K8 | REPO-B58 | |
| REPO-C41 | S6 | K1 | REPO-B4 | |
| REPO-C42 | S6 | K2 | REPO-B29, REPO-B32 | |
| REPO-C43 | S6 | K3 | REPO-B31 | |
| REPO-C44 | S6 | K4 | REPO-B33 | |
| REPO-C45 | S6 | K5 | REPO-B78 | |
| REPO-C46 | S6 | K6 | REPO-B14, REPO-B52 | |
| REPO-C47 | S6 | K7 | REPO-B14, REPO-B65 | |
| REPO-C48 | S6 | K8 | REPO-B14, REPO-B56, REPO-B109 |  |

## Write and read-back pairs
| Command that writes | State it changes | Command that reads it back | Statements |
|---|---|---|---|
| `repo -<kind> -n R ...` | registry row in `^IPM.Repo.DefinitionD` | `repo -list` | REPO-B1, REPO-B33 |
| `repo -n R -delete`, `repo -delete-all` | registry rows | `repo -list` | REPO-B29, REPO-B31, REPO-B33 |
| `publish M -r R` | artifact in the registry | `search`, `repo -list-modules -n R` | REPO-B51, REPO-B78 |
| `unpublish R/M V` | artifact in the registry | `search -versions` | REPO-B65, REPO-B78 |
| `module-version M V` | module record and stored manifest | `list-installed`, `show M` | REPO-B92, REPO-B83, REPO-B88 |
| `uninstall M` (module 05) | module record | `list-installed`, `show M` | REPO-B83, REPO-B91 |
| `config set K V`, `config delete K` | `^IPM.settings` in %SYS | `config get K`, `config list` | REPO-B98, REPO-B99 |
| `default-modifiers -set` or `-delete` | `^%IPM.DefaultModifiers` | `default-modifiers -get` | REPO-B103, REPO-B104 |
