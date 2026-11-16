#!/usr/bin/env python3
"""Generate the iteration 5 evidence index. kind: discovery, promotion (inherited finding moved into a
canonical owner with new evidence), challenge (an iteration 4 claim retested at its input boundary).
direct = member that decides; reader = caller or consumer of that decision."""
from pathlib import Path
import json
HERE = Path(__file__).resolve().parent
V = '.g5/Verus_verification/consolidated/iter5/'
MD, UN, RT, RP, BP = ('.g5/ontology/concepts/modules.g5.md', '.g5/ontology/features/uninstall.g5.md', '.g5/ontology/root.g5.md',
                      '.g5/ontology/concepts/repositories.g5.md', '.g5/ontology/features/build-and-publish.g5.md')
W = 'src/cls/IPM/ResourceProcessor/WebApplication.cls'
ROWS = [
 {'group': 'W4 web application lifecycle', 'kind': 'discovery', 'owner': MD + '#module.webapp',
  'direct': [[W, 'CreateOrUpdateWebApp'], [W, 'DeleteWebApp'], [W, 'OnBeforePhase']],
  'reader': [['src/cls/IPM/Storage/Module.cls', 'ExecutePhases'], ['src/cls/IPM/Storage/Module.cls', 'cannot also be listed in module'], [W, 'OnGetUniqueName']],
  'verus': [V + 'webapp.rs::merged', V + 'webapp.rs::install_app', V + 'webapp.rs::modify_called', V + 'webapp.rs::refused_at_import', V + 'webapp.rs::lemma_adoption_keeps_undeclared', V + 'webapp.rs::webapp_neighbors'],
  'controls': ['webapp/adoption-overwrites-undeclared', 'webapp/existing-app-not-adopted', 'webapp/modify-always-called'],
  'native': ['W1-install-a', 'W2-before', 'W2-install-b', 'W3-uninstall-b', 'W4-install-c', 'W5-reload-g'], 'question': 'C:Q15'},
 {'group': 'W4 web application lifecycle', 'kind': 'discovery', 'owner': MD + '#module.webapp.failure',
  'direct': [[W, 'CreateOrUpdateWebApp']], 'reader': [['src/cls/IPM/Utils/Module.cls', 'NoTransaction']],
  'verus': [V + 'webapp.rs::failed_after_activate'], 'controls': ['webapp/transaction-keeps-app'], 'native': ['W8-install-e', 'W8b-install-e-notxn']},
 {'group': 'W4 web application lifecycle', 'kind': 'discovery', 'owner': MD + '#module.webapp.upgrade',
  'direct': [['src/cls/IPM/Lifecycle/Base.cls', '%Clean']], 'reader': [['src/cls/IPM/Storage/Module.cls', 'ExecutePhases']],
  'verus': [V + 'webapp.rs::upgrade_drops_declaration'], 'controls': ['webapp/upgrade-removes-app'], 'native': ['W10-upgrade-g', 'W10-uninstall-g'], 'defect': 'C:D26'},
 {'group': 'W4 web application lifecycle', 'kind': 'discovery', 'owner': MD + '#module.webapp.validation',
  'direct': [['src/cls/IPM/Lifecycle/Base.cls', 'Method %Validate'], [W, 'DoPropertiesExist']], 'reader': [['src/cls/IPM/ResourceProcessor/Abstract.cls', 'OnBeforePhase']],
  'verus': [V + 'webapp.rs::validate_dispatches', V + 'webapp.rs::refused_at_validate'], 'controls': ['webapp/validate-dispatches'], 'native': ['W9-install-f'], 'defect': 'C:D23', 'proposal': 'C:S19'},
 {'group': 'W4 web application lifecycle', 'kind': 'promotion', 'owner': UN + '#uninstall.dependents.as-built',
  'inherited': ['P:F-UNIN-1', 'P:F-UNIN-6', 'G:OBS-L17'],
  'direct': [['src/cls/IPM/Storage/Module.cls', 'ExecutePhases'], ['src/cls/IPM/Lifecycle/Base.cls', 'CheckBeforeClean']], 'reader': [[W, 'DeleteWebApp'], ['src/cls/IPM/Lifecycle/Base.cls', '%Clean']],
  'verus': [V + 'webapp.rs::uninstall', V + 'webapp.rs::repaired_uninstall', V + 'webapp.rs::lemma_blocked_uninstall_deletes'],
  'controls': ['webapp/blocked-uninstall-keeps-app', 'webapp/blocked-uninstall-fails', 'webapp/repair-still-deletes'], 'native': ['W6-install-d', 'W6-uninstall-a', 'W7-force-a'], 'defect': 'C:D25', 'proposal': 'C:S17'},
 {'group': 'X challenges to iteration 4', 'kind': 'challenge', 'owner': RT + '#ipm.shell.forms', 'challenged': BP + '#build.test.outcome.exit',
  'direct': [['src/cls/IPM/Main.cls', 'UpdateLanguageExtensions'], ['src/cls/IPM/Main.cls', 'LanguageExtensions']], 'reader': [['src/cls/IPM/Main.cls', 'ClassMethod Shell(']],
  'verus': [V + 'entry_forms.rs::function_returns_value', V + 'entry_forms.rs::pinned_body', V + 'entry_forms.rs::outcome', V + 'entry_forms.rs::entry_neighbors'],
  'controls': ['entry_forms/matcher-case-insensitive', 'entry_forms/pinned-body-capitalized', 'entry_forms/command-flag-terminates', 'entry_forms/plain-shell-terminates'],
  'native': ['form:Z1-zpm-command-flag', 'form:Z2-zpm-command', 'form:Z3-shell-flag-stdin', 'form:Z4-zpm-function-fail', 'form:Z5-zpm-function-pass'], 'defect': 'C:D24', 'proposal': 'C:S18'},
 {'group': 'X challenges to iteration 4', 'kind': 'challenge', 'owner': RP + '#repo.remote.download.redirect', 'challenged': 'C:S14',
  'direct': [['src/cls/IPM/Repo/Http/PackageService.cls', 'FollowRedirect']], 'reader': [['src/cls/IPM/Repo/Remote/PackageService.cls', 'GetModule']],
  'verus': [V + 'redirect.rs::as_built_sends', V + 'redirect.rs::s14_sends', V + 'redirect.rs::s14b_sends', V + 'redirect.rs::lemma_s14_insufficient', V + 'redirect.rs::lemma_s14b_never_leaks'],
  'controls': ['redirect/redirect-drops-credentials-as-built', 'redirect/s14-checks-every-hop', 'redirect/s14b-checks-first-hop'], 'native': ['X1-redirect'], 'defect': 'C:D20', 'proposal': 'C:S14b'},
]
if __name__ == '__main__':
    (HERE / 'INDEX.json').write_text(json.dumps({'product_pin': '571e04a19aba9f9b4bc0ea23fbe09182038f4e55', 'counting': 'Rows are canonical owner blocks; groups are work packages, not independent case wins. Implementation proofs: 0.', 'rows': ROWS}, indent=2) + '\n')
    print(len(ROWS), 'rows')
