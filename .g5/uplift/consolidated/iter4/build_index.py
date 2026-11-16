#!/usr/bin/env python3
"""Generate the iteration 4 evidence index from explicit binding rows.

Each row names a canonical owner block, whether the work is a new discovery, a repair of a now-public
case, or stronger proof of an inherited finding, and binds it to pinned product source split into the
member that makes the decision ("direct") and the members that call or read the result ("reader").
Source rows are checked against the pinned revision when validate.py runs.
"""
from pathlib import Path
import json

HERE = Path(__file__).resolve().parent
V = '.g5/Verus_verification/consolidated/iter4/'
BP = '.g5/ontology/features/build-and-publish.g5.md'
DT = '.g5/ontology/features/developer-tools.g5.md'
RP = '.g5/ontology/concepts/repositories.g5.md'
MD = '.g5/ontology/concepts/modules.g5.md'
T, M = 'src/cls/IPM/ResourceProcessor/Test.cls', 'src/cls/IPM/Test/Manager.cls'
P = 'src/cls/IPM/General/Sync/Pipeline.cls'
R, H = 'src/cls/IPM/Repo/Remote/PackageService.cls', 'src/cls/IPM/Repo/Http/PackageService.cls'

ROWS = [
    {'group': 'W1 CI test-run outcome', 'kind': 'discovery', 'owner': BP + '#build.test.outcome',
     'public_overlap': ['I3F05 (two-processor aggregation)'],
     'direct': [[T, 'OnPhase'], [T, 'ReportTestResults'], [M, 'GetAllTestsStatus']],
     'reader': [['src/cls/IPM/Lifecycle/Base.cls', '%Test'], ['src/cls/IPM/Storage/Module.cls', 'ExecutePhases'], [M, 'OutputFailures'], ['src/cls/IPM/Test/Abstract.cls', 'BuildResultTree']],
     'verus': [V + 'test_outcome.rs::failures', V + 'test_outcome.rs::resource_ok', V + 'test_outcome.rs::all_ok', V + 'test_outcome.rs::lifecycle_invoked', V + 'test_outcome.rs::report_written', V + 'test_outcome.rs::outcome_neighbors'],
     'controls': ['test_outcome/method-error-not-counted', 'test_outcome/case-error-not-counted', 'test_outcome/extension-not-checked', 'test_outcome/lifecycle-stops-at-first-failure'],
     'native': ['T1-default', 'T5-bad-extension', 'T6-json-file']},
    {'group': 'W1 CI test-run outcome', 'kind': 'discovery', 'owner': BP + '#build.test.outcome.not-fatal',
     'direct': [[T, 'ReportTestResults']], 'reader': [['src/cls/IPM/Main.cls', 'Shell']],
     'verus': [V + 'test_outcome.rs::resource_ok'], 'controls': ['test_outcome/not-fatal-still-fails'], 'native': ['T2-not-fatal', 'exit:E4-not-fatal-terminate']},
    {'group': 'W1 CI test-run outcome', 'kind': 'discovery', 'owner': BP + '#build.test.outcome.empty',
     'direct': [[T, 'OnPhase']], 'reader': [[M, 'LoadTestDirectory']],
     'verus': [V + 'test_outcome.rs::load_fails', V + 'test_outcome.rs::ran'], 'controls': ['test_outcome/empty-package-fails', 'test_outcome/missing-suite-dir-skipped'],
     'native': ['T1-default', 'T4-empty-only'], 'question': 'C:Q13'},
    {'group': 'W1 CI test-run outcome', 'kind': 'discovery', 'owner': BP + '#build.test.outcome.exit',
     'direct': [['src/cls/IPM/Main.cls', 'Shell']], 'reader': [['src/cls/IPM/Main.cls', 'LanguageExtensions']],
     'verus': [V + 'test_outcome.rs::exit_code'], 'controls': ['test_outcome/exit-ignores-terminate-flag'],
     'native': ['exit:E1-fail-terminate', 'exit:E2-pass-terminate', 'exit:E3-fail-no-terminate']},
    {'group': 'W1 CI test-run outcome', 'kind': 'repair', 'owner': BP + '#build.test.outcome.nested',
     'public_overlap': ['I2E09', 'I3F02'],
     'direct': [[M, 'GetAllTestsStatus'], [M, 'OutputFailures'], [M, 'SaveResult']], 'reader': [[T, 'OnPhase'], [T, 'OnSyncRunTests']],
     'verus': [V + 'test_outcome.rs::get_all_status', V + 'test_outcome.rs::displayed_lines', V + 'test_outcome.rs::lemma_nested_keeps_parent', V + 'test_outcome.rs::lemma_suffix_ignores_prefix', V + 'test_outcome.rs::accumulator_neighbors'],
     'controls': ['test_outcome/nested-clears-parent', 'test_outcome/nested-counts-parent-slots', 'test_outcome/display-includes-start-slot'],
     'native': ['A1-nested-fail', 'A2-parent-after', 'A3-top-both', 'A4-nested-pass', 'D1-display-from-1', 'D2-display-gap', 'D3-display-one-slot']},
    {'group': 'W2 remote registry install', 'kind': 'discovery', 'owner': RP + '#repo.remote.download',
     'direct': [[R, 'GetModule'], [H, 'GetHttpRequest']], 'reader': [['src/cls/IPM/Utils/Module.cls', 'LoadModuleReference']],
     'verus': [V + 'registry.rs::metadata_request', V + 'registry.rs::bytes_request', V + 'registry.rs::repaired_bytes_request', V + 'registry.rs::lemma_cross_origin_carries_credentials'],
     'controls': ['registry/bytes-sent-to-configured-origin', 'registry/bytes-drop-credentials-as-built', 'registry/repair-still-forwards'],
     'native': ['R-cross', 'R-same'], 'defect': 'C:D20', 'question': 'C:Q14', 'proposal': 'C:S14'},
    {'group': 'W2 remote registry install', 'kind': 'discovery', 'owner': RP + '#repo.remote.download.integrity',
     'direct': [[R, 'GetModule']], 'reader': [['src/cls/IPM/Utils/Module.cls', 'LoadModuleReference'], ['src/cls/IPM/General/Archive.cls', 'Extract']],
     'verus': [V + 'registry.rs::integrity_ok', V + 'registry.rs::get_module', V + 'registry.rs::install', V + 'registry.rs::registry_neighbors'],
     'controls': ['registry/hash-compared-case-insensitively', 'registry/empty-hash-rejected', 'registry/mismatch-sends-analytics', 'registry/junk-refused-at-download', 'registry/bytes-failure-reports-status'],
     'native': ['R-mismatch', 'R-upper', 'R-nohash', 'R-dl404', 'R-meta404', 'I1-install-good', 'I2-install-bad', 'I3-install-junk'], 'defect': 'C:D22', 'proposal': 'C:S16'},
    {'group': 'W2 remote registry install', 'kind': 'discovery', 'owner': RP + '#repo.remote.download.token',
     'public_overlap': ['I3F11 (token after SHA1)'],
     'direct': [[R, 'GetModule']], 'reader': [[R, 'SendAnalyticsToRegistry']],
     'verus': [V + 'registry.rs::token_stored'], 'controls': ['registry/token-stored-before-integrity'], 'native': ['R-same', 'R-mismatch', 'R-analytics']},
    {'group': 'W2 remote registry install', 'kind': 'stronger-proof', 'owner': '.g5/ontology/features/artifact-download.g5.md#artifact.archive-admission',
     'inherited': 'C:D18', 'direct': [['src/cls/IPM/General/Archive.cls', 'Extract']], 'reader': [['src/cls/IPM/Utils/Module.cls', 'LoadModuleReference']],
     'verus': [V + 'registry.rs::install'], 'controls': ['registry/junk-refused-at-download'], 'native': ['I3-install-junk']},
    {'group': 'W3 developer sync loop', 'kind': 'repair', 'owner': DT + '#devtools.sync.compile-after-resolve', 'public_overlap': ['I3F20'],
     'direct': [[P, 'SyncCompile']], 'reader': [[P, 'Run'], [P, 'SyncDispatchProcessors'], ['src/cls/IPM/General/Sync/Summary.cls', 'AddWarning']],
     'verus': [V + 'sync_compile.rs::sync_compile', V + 'sync_compile.rs::sync_compile_neighbors'],
     'controls': ['sync_compile/resolution-failure-fails-step', 'sync_compile/compile-called-without-survivors', 'sync_compile/unresolved-resource-still-compiled', 'sync_compile/warning-always-retained', 'sync_compile/undefined-class-compiled', 'sync_compile/project-resource-warned'],
     'native': ['Y1-survivor', 'Y2-none-left']},
    {'group': 'W3 developer sync loop', 'kind': 'discovery', 'owner': DT + '#devtools.sync.test-stop',
     'direct': [[P, 'SyncRunTests']], 'reader': [['src/cls/IPM/Lifecycle/Base.cls', '%Test']],
     'verus': [V + 'test_outcome.rs::sync_invoked', V + 'test_outcome.rs::lifecycle_invoked'], 'controls': ['test_outcome/sync-runs-all-resources'], 'native': [],
     'limit': 'Sync side source-only.'},
    {'group': 'W3 developer sync loop', 'kind': 'repair', 'owner': DT + '#devtools.sync.test-paths', 'public_overlap': ['I2C04'],
     'direct': [[T, 'RelPathToClassName']], 'reader': [[T, 'OnSync']],
     'verus': [V + 'repairs4.rs::package_rel_path', V + 'repairs4.rs::rel_path_neighbors'], 'controls': ['repairs4/outside-path-returns-empty', 'repairs4/prefix-requires-boundary'],
     'native': ['C-tests/unit/SyncTest/Tests/Trivial.cls', 'C-other/SyncTest/Tests/Trivial.cls', 'C-tests/unitX/A/B.cls'], 'defect': 'C:D21', 'proposal': 'C:S15'},
    {'group': 'R public repairs', 'kind': 'repair', 'owner': MD + '#module.deployed', 'public_overlap': ['I2E14'],
     'direct': [['src/cls/IPM/Lifecycle/Base.cls', 'DEPLOYBYDEFAULT'], ['src/cls/IPM/Lifecycle/DeployedModule.cls', 'DEPLOYBYDEFAULT']], 'reader': [['src/cls/IPM/Lifecycle/Base.cls', 'Deploy","Parameter']],
     'verus': [V + 'repairs4.rs::deploy_by_default', V + 'repairs4.rs::deploy', V + 'repairs4.rs::deploy_neighbors'],
     'controls': ['repairs4/lifecycle-defaults-swapped', 'repairs4/explicit-false-ignored', 'repairs4/explicit-overrides-class-parameter'], 'native': ['P-Base', 'P-Module', 'P-DeployedModule']},
    {'group': 'R public repairs', 'kind': 'repair', 'owner': BP + '#build.test.selection', 'public_overlap': ['I2E07 (domain qualification)'],
     'direct': [[T, 'TestsShouldRun'], [T, 'Property Phase']], 'reader': [['src/cls/IPM/DataType/ListOfOptions.cls', 'IsValid']],
     'verus': [V + 'repairs4.rs::admitted', V + 'repairs4.rs::phase_enabled', V + 'repairs4.rs::lemma_admitted_agree', V + 'repairs4.rs::phase_neighbors'],
     'controls': ['repairs4/stored-phase-folded', 'repairs4/request-not-lowercased'], 'native': ['P-stored-test', 'P-stored-TEST']},
    {'group': 'R public repairs', 'kind': 'repair', 'owner': RP + '#repo.search-order', 'public_overlap': ['I2E21'],
     'direct': [['src/cls/IPM/Repo/SearchCriteria.cls', 'KeywordsSet']], 'reader': [['src/cls/IPM/Repo/Manager.cls', 'SearchRepositoriesForModule']],
     'verus': [V + 'repairs4.rs::keywords', V + 'repairs4.rs::visits_filesystem', V + 'repairs4.rs::criteria_name', V + 'repairs4.rs::keyword_neighbors'],
     'controls': ['repairs4/empty-tokens-kept', 'repairs4/dollar-list-not-filtered', 'repairs4/object-filtered', 'repairs4/keywords-ignored-at-gate'], 'native': [],
     'limit': 'Token rule source-only; no new native case.'},
]

if __name__ == '__main__':
    out = {'product_pin': '571e04a19aba9f9b4bc0ea23fbe09182038f4e55',
           'counting': 'Rows are canonical owner blocks. Groups W1-W3 are work packages chosen from product evidence; R rows repair now-public cases. kind=discovery means the distinction was absent from the canonical ontology, consolidated registers and both historical foundations (alias check in BUGS.md); public_overlap names published cases the row also answers. Implementation proofs: 0.',
           'rows': ROWS}
    (HERE / 'INDEX.json').write_text(json.dumps(out, indent=2) + '\n')
    print(len(ROWS), 'rows')
