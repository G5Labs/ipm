# Uninstall and settings models: twin checks

## A. Transcription

uninstall.rs: 84 spec fns, missing in twin: none.
settings.rs: 51 spec fns, missing in twin: none.
Witness facts reproduced: 16/16.

## B. Conformance with IRIS observations

| Observation | Result | Keys | Mismatches |
|---|---|---|---|
| nc-inst-10b-registry-tie-break | match | 1 | [] |
| nc-inst-10c-registry-tie-break-high | match | 1 | [] |
| nc-repo-1-publish-no-flag | match | 3 | [] |
| nc-repo-2-disabled-registry | match | 4 | [] |
| nc-repo-6-config-delete | match | 10 | [] |
| nc-repo-7-default-modifiers-namespaces | match | 3 | [] |
| nc-repo-8-config-errors | match | 5 | [] |
| nc-repo-env-percent-global-shared | match | 2 | [] |
| nc-repo-model-config-delete | match | 7 | [] |
| nc-repo-model-default-modifiers-merge | match | 5 | [] |
| nc-repo-model-publish-row-order | match | 9 | [] |
| nc-repo-model-unin-recurse-ipm-dependency | match | 8 | [] |
| nc-repo-model-unin-recurse-removed-dep | match | 11 | [] |
| nc-repo-model-unin-recurse-skipped-dep | match | 10 | [] |
| nc-unin-1-blocked-uninstall | match | 7 | [] |
| nc-unin-2-clean-own-module | match | 7 | [] |
| nc-unin-3-uninstall-plain | match | 4 | [] |
| nc-unin-3-uninstall-purge | match | 4 | [] |
| nc-unin-4-mapped-uninstall-IPM | match | 5 | [] |
| nc-unin-4-mapped-uninstall-ZPM | match | 5 | [] |
| nc-unin-4-mapped-uninstall-ipm | match | 5 | [] |
| nc-unin-4-mapped-uninstall-zpm | match | 5 | [] |
| nc-unin-5-unresolvable-dependent | match | 10 | [] |
| nc-unin-6-global-resources | match | 8 | [] |
| nc-unin-7-control-keep-module | match | 6 | [] |
| nc-unin-7-shared-target-folder | match | 7 | [] |
| nc-unin-8-dependency-hook-fails | match | 8 | [] |
| nc-unin-9-clean-on-ipm | match | 7 | [] |
| nc-unin-env-name-case | match | 4 | [] |
| nc-unin-self-external-name-IPM | match | 8 | [] |
| nc-unin-self-zpm-answer-no | match | 6 | [] |

nc-unin-*/nc-repo-* observations: 50; replayed: 29; not expressible: 21 (listed in the record repo-conf-coverage); unaccounted: none.

## C. Exhaustive exploration

Uninstall: 2951728 runs over 56718 worlds {'two-modules': 56448, 'three-modules': 256, 'mapped-ipm': 14}. Bounds: IPM plus two modules with every combination of dependency (on each other and on IPM), graph error, global scope, installer hook (none, ok, failing) and one extra resource (none, web app, unnamed global, kept folder copy, folder copy, file copy, mapped class), all copies into one folder; IPM plus three modules with every acyclic graph and web apps; a namespace that borrows IPM. Commands: uninstall of each module, of `zpm` (exact), of `IPM` and of an unknown name with every -force/-r/-global and full-cleanup answer; clean of each module.

| Clause | Meaning | Violations under its premises | Violations without them | Example (without) |
|---|---|---|---|---|
| blocked-unchanged | a blocked uninstall changes nothing (premises PG, PW) | 0 | 271984 | ('uninstall', {'key': 3, 'exact_zpm': False}, False, False, False) |
| honest | success exactly when the module is gone, both directions (premises PB, PG, PI, PNF) | 0 | 588272 | ('uninstall', {'key': 3, 'exact_zpm': False}, False, False, False) |
| dependents-protect | a needed module stays without -force (premise PG) | 0 | 84736 | ('uninstall', {'key': 0, 'exact_zpm': True}, False, False, False) |
| only-own | only the target's resources and records go, and its needs' with -r (premises PF, PNF) | 0 | 142992 | ('uninstall', {'key': 0, 'exact_zpm': True}, False, False, False) |
| kept-files | files copied with the keep flag stay (premises PF, PNF) | 0 | 9000 | ('uninstall', {'key': 3, 'exact_zpm': False}, False, False, False) |
| complete | once the record is gone, every non-exempt resource is gone (premises PGA, PNF) | 0 | 44228 | ('uninstall', {'key': 3, 'exact_zpm': False}, False, False, False) |
| self-guard | uninstalling IPM runs the guard (premise PN) | 0 | 453632 | ('uninstall', {'key': 1, 'exact_zpm': False}, False, False, False) |
| ipm-kept | IPM stays when another module is uninstalled (premise PI) | 0 | 105824 | ('uninstall', {'key': 3, 'exact_zpm': False}, False, True, False) |
| clean-keeps-target | clean leaves its module unchanged (premises PCW, PF) | 0 | 97648 | ('clean', 3, False, False, False) |
| rows-honest | a row says success exactly when its module is gone, both directions (premises PR, PB, PG, PNF, target not IPM) | 0 | 282896 | ('uninstall', {'key': 3, 'exact_zpm': False}, False, False, False) |
| manifest-with-record | a removed record takes its stored manifest (premises PR, PNF) | 0 | 8416 | ('uninstall', {'key': 3, 'exact_zpm': False}, False, True, False) |
| not-found | an unknown name fails and changes nothing (no premise) | 0 | 0 |  |

Settings and registries: 264314 checks. Bounds: config op sequences of length 0-2 over five keys (special, seeded, unseeded, unknown) and five values from two start states; default-modifier op sequences of length 0-3 over two namespaces; every list of up to three registries of each kind, enabled, flag and token; every list of up to three registries with offers and sort orders.

| Law | Meaning | Violations under its premises | Violations without them | Example (without) |
|---|---|---|---|---|
| config-get-after-set | config get returns the value config set stored (no premise) | 0 | 0 |  |
| config-delete-factory | config delete restores the factory default (premise PS) | 0 | 1323 | ((), 5) |
| config-shared | a setting set in one namespace is read in every namespace (no premise) | 0 | 0 |  |
| config-errors-unchanged | an unknown key or a bad value changes nothing (no premise) | 0 | 0 |  |
| dm-per-namespace | default modifiers set in one namespace change only that namespace (premise PNS) | 0 | 980 | ((), {1, 2}, {'A': 1}) |
| dm-laws | -set adds without dropping, -delete clears, a typed option wins (no premise) | 0 | 0 |  |
| publish-default-enabled | publish with no -repo picks an enabled, non-folder registry (no premise) | 0 | 0 |  |
| publish-default-holder | publish with no -repo picks the publish-flag holder (premises PH, PU, PK) | 0 | 4540 | (('Remote', True, False, False),) |
| publish-named | publish -r uses the named registry only if enabled (premise PE) | 0 | 14216 | ((('Remote', False, False, False),), 1) |
| search-enabled | search lists only enabled registries (premise PE) | 0 | 12540 | (('Remote', False, False, False),) |
| install-enabled | install takes only from enabled registries (no premise) | 0 | 0 |  |
| install-tie-sort-order | equal versions come from the lowest sort order (premise PT) | 0 | 2130 | (('Remote', True, 1, None), ('Remote', True, 1, 1)) |

## D. Twin mutants (each must be contradicted by IRIS)

- `no-before-delete` (web apps are deleted after the dependents check, not in OnBeforePhase): REJECTED by nc-unin-1-blocked-uninstall, nc-unin-2-clean-own-module, nc-repo-model-unin-recurse-skipped-dep, nc-repo-model-unin-recurse-removed-dep
- `top-record-kept` (the top-level Clean never deletes the record): REJECTED by nc-unin-3-uninstall-plain, nc-unin-3-uninstall-purge, nc-unin-5-unresolvable-dependent, nc-unin-6-global-resources, nc-repo-model-unin-recurse-skipped-dep, nc-repo-model-unin-recurse-removed-dep
- `no-name-guard` (the self-uninstall steps never run): REJECTED by nc-unin-4-mapped-uninstall-zpm
- `no-class-gone-error` (the nested Clean goes on after IPM's classes are gone): REJECTED by nc-repo-model-unin-recurse-ipm-dependency
- `default-overwritten` (SetValue overwrites the default node): REJECTED by nc-repo-6-config-delete, nc-repo-model-config-delete
- `reset-to-value` (config delete restores the current value): REJECTED by nc-repo-6-config-delete, nc-repo-model-config-delete
- `percent-per-namespace` (^% globals are kept per namespace): REJECTED by nc-repo-7-default-modifiers-namespaces, nc-repo-env-percent-global-shared
- `first-row-best-order` (each kind's publishing row is its best PublishOrder, not its first enabled row): REJECTED by nc-repo-model-publish-row-order
- `publish-checks-enabled` (publish -r refuses a disabled registry): REJECTED by nc-repo-2-disabled-registry
- `search-enabled-only` (search lists only enabled registries): REJECTED by nc-repo-2-disabled-registry
- `tie-by-sort-order` (equal versions go to the lowest sort order): REJECTED by nc-inst-10b-registry-tie-break
