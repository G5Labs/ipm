#!/usr/bin/env python3
"""Iteration74 census regrade of the history obligations about which commands write records, when the record is
created, the phase lists and Committed for install, update and uninstall (rule 4a). Run from the repository root
after run.py and census.py inventory; then run census.py count."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[4]
path = root / '.g5/uplift/coverage-20261117/census/blocks/features__history.json'
inv = {b['id']: b for b in json.loads((root / '.g5/uplift/coverage-20261117/census/inventory.json').read_text())}
data = json.loads(path.read_text())
A = '.g5/Verus_verification/coverage-20261117/iter74/actions.rs::'
U = '.g5/uplift/coverage-20261117/iter74/'
stamp = (root / U / 'native/LATEST').read_text().strip()
EV = [U + 'run.json', U + 'proofs/models.json', U + 'replay/receipt.json', U + f'native/{stamp}/observations.json',
      U + 'mutations/S.jsonl', U + 'predictions.md', U + 'bindings.json']
B = 'iteration-74: '
blocks = data['blocks']


def get(block, oid):
    for o in blocks[block]['obligations']:
        if o['id'] == oid:
            return o
    return None


def put(block, o):
    obls = blocks[block]['obligations']
    for i, x in enumerate(obls):
        if x['id'] == o['id']:
            obls[i] = o
            return
    obls.append(o)


def ob(oid, units, text, cases, observation, grade, check, models, basis, support=None, gap=None, note=None,
       instance_scoped=None, keep_evidence=()):
    o = {'id': oid, 'units': units, 'text': text, 'cases': cases, 'observation': observation, 'grade': grade,
         'check': check, 'models': [A + m if '::' not in m else m for m in models], 'evidence': list(keep_evidence) + EV,
         'basis': basis, 'assessed_in': 'iteration-74'}
    if gap:
        o['gap'] = gap
    if support:
        o['support'] = support
    if note:
        o['note'] = note
    if instance_scoped:
        o['instance_scoped'] = True
    return o


# ---------------------------------------------------------------- history.actions
blk = 'history.actions'
blocks[blk]['body_sha'] = inv[blk]['body_sha']
blocks[blk]['units'] = {
    'P1': ['history.actions#1', 'history.actions#2', 'history.actions#3', 'history.actions#6', 'history.actions#7'],
    'B1': ['history.actions#1', 'history.actions#2', 'history.actions#3'],
    'B2': ['history.actions#6', 'history.actions#7', 'history.actions#8', 'history.actions#9'],
    'B3': ['history.actions#4'],
    'B4': ['history.actions#5', 'history.actions#10', 'history.actions#11'],
}
o1 = get(blk, 'history.actions#1'); o1['units'] = ['P1', 'B1']
o4 = get(blk, 'history.actions#4'); o4['units'] = ['B3']
put(blk, ob('history.actions#2', ['P1', 'B1'],
    'Every install and update attempt that reaches its History init is recorded, including failed ones and refusals after the init (no enabled repository, a name no repository has, a downgrade or update-required refusal, a Compile or ApplyUpdateSteps failure, and a program calling Install with no name).',
    'install success, reinstall over the installed version, name not found, API call with no name, Compile failure, downgrade refusal with and without transactions; update success, ApplyUpdateSteps failure with and without transactions',
    'History row with Action install or update',
    'full', 'proved',
    ['main_install', 'api_install', 'install_body', 'cli_update', 'install_always_recorded', 'update_recorded_iff', 'install_neighbours', 'update_neighbours'],
    B + 'InstallInit (Main.cls:2565-2566) precedes every check and Finalize runs in both exits (2666-2670); UpdateInit (4438) follows the two refusals and Install writes the update row (4445). install_always_recorded: one finalized install record for every input; update_recorded_iff: one update record exactly when the update passes its checks. Controls install-init-after-name-check and update-init-before-installed-check rejected; native i1-i7, i4api, u1-u3 replayed (flips i3.rows, i4api.rows, u2.r1.committed rejected).',
    support='mutation-checked'))
put(blk, ob('history.actions#3', ['P1', 'B1'],
    'Every uninstall attempt, including a failed one or one naming a module that is not installed, is recorded.',
    'uninstall of an installed module (success n1, failure n4 and n5) vs uninstall of a name that is not installed (n2)',
    'uninstall History rows',
    'full', 'proved',
    ['cli_uninstall', 'top_rec', 'top_unin', 'uninstall_committed_is_success', 'uninstall_one_record', 'n2_facts', 'n5_facts',
     'playbook-uplift/models/verus/uninstall.rs::storage_uninstall'],
    B + 'UninstallInit (Storage/Module.cls:734) precedes NameOpen, so every name gets a record: uninstall_committed_is_success (rows >= 1, the first is the uninstall record of the typed name, for every uninstall.rs world) and uninstall_one_record (exactly one without -r). The not-found record has no phase and the placeholder version (n2_facts). Controls uninstall-init-after-lookup, S-m06 and S-m07 rejected; native n1, n2, n4, n5 replayed (flip n2.rows rejected).',
    support='mutation-checked',
    note='Domain: every uninstall.rs world whose target is not IPM. Self-uninstall (finalized before Clean, Storage/Module.cls:630-643) and uninstall -all are outside it; uninstall with no name is history.actions#8.'))
put(blk, ob('history.actions#5', ['B4'],
    'List, sync and lifecycle phase commands such as reload or compile write no record of their own.',
    'list / sync / `<module> reload` / `<module> compile` / `reload <module>` / activate chain, and `<module> clean` (no record for the module itself) vs load',
    'History row count before and after the command',
    'full', 'proved',
    ['cmd_rows', 'run_list', 'cli_clean', 'other_commands_no_record', 'clean_without_deps_writes_nothing', 'c1_facts'],
    B + 'Settles iteration70 finding 17: `<module> clean` writes no record for the module itself but does write dependency records (history.actions#10). Phase commands run ExecutePhases with no log (Main.cls:2753-2766, 2830-2856); the only records a phase command can reach are Clean dependency records and %Test/%Verify dependency loads, decided by the computed chain (run_list). list (ListInstalled, Main.cls:2939) and sync (ExecutePhases of the Sync chain, Main.cls:2375-2410) reach no init site; their empty rows transcribe that. Controls phase-command-records, clean-writes-own-record, S-m04, S-m05 rejected; native c2 (list, h74a reload, h74a compile, reload h74a, sync: 0 rows; flip c2.rows rejected) and c1.',
    support='mutation-checked'))
o6 = get(blk, 'history.actions#6')
o6['units'] = ['P1', 'B2']
o6['models'] = o6['models'] + [A + 'cli_reinstall', A + 'reinstall_not_installed']
o6['evidence'] = o6.get('evidence', []) + EV
o6['basis'] = o6['basis'] + ' ' + B + 'also actions.rs cli_reinstall / reinstall_not_installed (Main.cls:2685-2688), control reinstall-enters-install rejected, native r1 (0 rows, flip rejected).'
put(blk, ob('history.actions#7', ['P1', 'B2'],
    'Intended: a refused update is recorded too. As built, update with -PermitDowngrade or of a module that is not installed throws before HistoryTemp.UpdateInit writes any row (Main.cls:4415-4424, init at 4438).',
    'update of a not-installed module (u4) and with -DPermitDowngrade=1 (u5) vs an update reaching Install (u1, u2)',
    'History row count',
    'full', 'refuted',
    ['cli_update', 'update_recorded_iff', 'update_neighbours'],
    B + 'update_recorded_iff: a record exactly when the name is non-empty, -PermitDowngrade is absent and the module is installed, so the intended promise is refuted for the two refusals. Controls update-init-before-permit-check and update-init-before-installed-check rejected; native u4, u5 (0 rows) against u1-u3 (1 row); flips u4.rows and u5.rows rejected.',
    support='mutation-checked'))
put(blk, ob('history.actions#8', ['B2'],
    'As built, uninstall with no name throws before Storage.Module:Uninstall (Main.cls:2710-2712), so it leaves no record.',
    'uninstall with no name (n3) vs uninstall of a name that is not installed (n2, one record)',
    'History row count',
    'full', 'proved',
    ['cli_uninstall', 'uninstall_no_name', 'n2_facts'],
    B + 'new obligation for the as-built bullet. Control uninstall-no-name-recorded rejected; native n3 (0 rows) against n2 (1); flip n3.rows rejected. Whether it should be recorded is the open question history.record.unlogged-refusals.',
    support='mutation-checked'))
put(blk, ob('history.actions#9', ['B2'],
    'As built, install, update and reinstall with no name are refused by the command parser before dispatch, so they leave no record; a program calling ##class(%IPM.Main).Install with no name gets a failed install record.',
    'CLI install with no name (i4, 0 rows) vs API Install with no name (i4api, 1 row)',
    'History row count',
    'full', 'proved',
    ['cli_install', 'api_install', 'cli_update', 'cli_reinstall', 'install_always_recorded', 'update_recorded_iff'],
    B + 'the parser rule is the required="true" module parameter (Main.cls:177, 427, 469). Mutants S-m01 (install parser check removed) and S-m02 (update) and control install-init-after-name-check rejected; native i4 and i4api; flips i4.rows and i4api.rows rejected. Reinstall with no name is decided by the same parser rule but was not run natively.',
    support='mutation-checked'))
put(blk, ob('history.actions#10', ['B4'],
    '`<module> clean` writes an uninstall record for each installed dependency its Clean recursion visits, and none for the module itself.',
    '`h74m clean` with one installed dependency (c1: one record, for h74d) vs a module with no dependencies (no record)',
    'History rows, their Package and the dependency record',
    'full', 'proved',
    ['cli_clean', 'dep_rec', 'c1_facts', 'clean_without_deps_writes_nothing', 'playbook-uplift/models/verus/uninstall.rs::dep_step', 'playbook-uplift/models/verus/uninstall.rs::clean_cmd'],
    B + 'new obligation. The rows come from uninstall.rs dep_step (Lifecycle/Base.cls:438-485) under Clean.Recurse defaulting to 1 (:313). clean_without_deps_writes_nothing holds for every world; c1_facts is the one-dependency instance (uninstall.rs w_recremove). Control clean-writes-own-record and dependency control clean-recursion-no-dependency-record rejected; native c1 (flip c1.rows rejected). Instance-scoped for the count with dependencies.',
    support='mutation-checked', instance_scoped=True,
    note='Row order is exact for one nesting level: uninstall.rs appends a nested dependency record after its own dependencies, while the product creates it before them.'))
put(blk, ob('history.actions#11', ['B4'],
    '`<module> test` and `<module> verify` write a load record for each dependency scoped to that phase that they install.',
    '`h74t test` installing its test-scoped dependency h74b (t1, one load record) vs reload/compile (no record)',
    'History rows and their Action',
    'partial', 'proved',
    ['cmd_rows', 'scoped_rows', 'dep_load_row', 'dep_load_label', 't1_facts', 'other_commands_no_record'],
    B + 'new obligation. Controls test-scoped-deps-unrecorded and scoped-dep-recorded-as-install (and mutant S-m12) rejected; native t1 replayed (flip t1.r1.action rejected).',
    support='control',
    gap='The list of phase-scoped dependencies LoadDependencies installs is an input (BuildDependencyGraph is not modelled), and verify (Lifecycle/Base.cls:1557, plus its LoadModuleFromDirectory at 1551) was not run natively.'))

# ---------------------------------------------------------------- history.record
blk = 'history.record'
put(blk, ob('history.record#2', ['P1'],
    'For install, update and uninstall, the record is created at start and filled in at the end.',
    'install/update/uninstall while running (mid.*) vs after the end; refusals after the init (i3, n2) still leave a record; uninstall record before its dependency records (n5)',
    'History row at start and at end',
    'full', 'proved',
    ['init', 'twin_while_running', 'running_ok', 'main_install', 'cli_update', 'top_rec', 'cli_uninstall', 'fin', 'persist', 'n5_facts'],
    B + 'init (AbstractHistory.cls:51-63, HistoryTemp.cls:9-26) is the first step of main_install, the update branch of cli_update and top_rec; fin and persist fill it at the end. Controls install-init-after-name-check, uninstall-init-after-lookup and uninstall-top-created-last rejected; native mid.install / mid.update / mid.uninstall (permanent row in progress while the action runs), n5 ID order; flips n5.r1.pkg and mid.install.permph rejected.',
    support='mutation-checked',
    note='Normal ends only. Abnormal ends (process death) are modelled for load in the ipm-load-history crate (history.record#3). While running, the permanent row lists no phases (history.record#6, refuted).'))
put(blk, ob('history.record#8', ['B3'],
    'Working assumption: IPM records refused update and reinstall of a module that is not installed and uninstall with no name.',
    'update/reinstall of a non-installed module, uninstall with no name vs a refused load',
    'History row count',
    'full', 'refuted',
    ['update_recorded_iff', 'reinstall_not_installed', 'uninstall_no_name', 'cli_update', 'cli_reinstall', 'cli_uninstall'],
    B + 'the as-built model refutes the working assumption for all three: no record (u4, r1, n3), while a refused load (crate) and a refused install (i3) are recorded. Controls update-init-before-installed-check, reinstall-enters-install and uninstall-no-name-recorded rejected. The policy stays open (history.record.unlogged-refusals).',
    support='mutation-checked'))

# ---------------------------------------------------------------- history.phases
blk = 'history.phases'
put(blk, ob('history.phases#3', ['B1'],
    'Install records the same five phases, update also records ApplyUpdateSteps, and uninstall records Clean.',
    'install (i1) vs update (u1) vs uninstall (n1) and dependency uninstall (c1)',
    'phase names on the row',
    'full', 'proved',
    ['phase_order', 'chain', 'order_by', 'logged', 'run_list', 'logged_lists', 'install_phases', 'uninstall_committed_is_success', 'dep_rec_committed'],
    B + 'the logged list is computed from PhaseName VALUELIST, GetCompletePhasesForOne and the "*" skip in ExecutePhases. Controls update-without-apply-update-steps, chain-drops-apply-update-steps, star-logged and S-m06 rejected; native i1 (five), u1 (six, flip u1.r1.ph6 rejected), n1 and c1 (Clean).',
    support='mutation-checked'))
put(blk, ob('history.phases#4', ['B2'],
    'A failed action lists the phases up to and including the one that failed.',
    'failure in Compile (i5), ApplyUpdateSteps (u2, u3), Clean (n4, n5); any failing index of any phase list (run_shape)',
    'listed phases and last status',
    'full', 'proved',
    ['run_from', 'run_shape', 'install_phases', 'uninstall_committed_is_success'],
    B + 'run_shape: for every phase list and failing index i, exactly i+1 entries, the last is the failing phase with an error, ended exactly when the error was thrown. install_phases applies it to install and update; load reaches the same loop through the same LoadNewModule call (Utils/Module.cls:1346; the crate covers its Activate failure). Controls failure-does-not-stop and thrown-failure-left-open rejected; native i5 (four entries, Compile ended with an error; flips i5.r1.nph and i5.r1.ph4.ended rejected).',
    support='mutation-checked'))

# ---------------------------------------------------------------- history.committed
blk = 'history.committed'
put(blk, ob('history.committed#3', ['P1'],
    'For install and update, Committed shows whether the action kept changes.',
    'install / update success (i1, u1), failure rolled back (i5, u2), failure without a transaction (u3), refusal (i3, i6, i7), developer-mode failure after its commit (dev_failure_committed)',
    'Committed vs namespace (installed version after the command)',
    'full', 'proved',
    ['lnm', 'install_body', 'install_committed_is_kept', 'install_refusal_uncommitted', 'dev_failure_committed', 'after_version'],
    B + 'install_committed_is_kept: for every input, Committed == kept || early_marker, where kept follows the transaction semantics (rollback at Utils/Module.cls:1358-1361, commits at 1311 and 1348) and early_marker is the no-transaction mark before a module.xml that fails to load (history.committed#9, refuted). Controls no-commit-mark-without-transaction, no-commit-mark-dev, guard-after-commit-mark, dependency control finalize-status-only and S-m09 rejected; native u2 (0, version stays 1.0.0), u3 (1, 1.1.0 kept), i5, i6, i7 replayed; flips u2.r1.committed, u3.r1.committed, i7.r1.committed rejected.',
    support='mutation-checked',
    note='Mutant S-m08 (update keeps -dev for a non-snapshot) survives: no case distinguishes a developer-mode update, whose mode LoadModuleReference overrides (F-INST-5). Read-only repositories forcing developer mode off (Utils/Module.cls:1277-1287) are not modelled.'))
put(blk, ob('history.committed#4', ['B1'],
    'A successful top-level action is committed.',
    'returned OK vs error, for load, install, update and uninstall',
    'Committed on the row',
    'full', 'proved',
    ['install_committed_is_kept', 'uninstall_committed_is_success', 'dep_rec_committed',
     'playbook-uplift/models/verus/load_general.rs::honest', '.g5/Verus_verification/crates/ipm-load-history/src/proofs.rs::theorem_top_record_complete'],
    B + 'install and update: ok implies kept implies Committed (install_committed_is_kept); uninstall: Committed == ok (uninstall_committed_is_success), where only the ISOK term of Finalize sets it, so dependency control finalize-without-isok is rejected (n1 and c1 read uncommitted); load from the crate and load_general.rs as before. Native i1, u1, n1, c1, t1 successes read Committed.',
    support='mutation-checked',
    note='For install and update the ISOK term is redundant with the mark at Utils/Module.cls:1350 (ISOK-REDUNDANT, as for load).'))

# ---------------------------------------------------------------- history.committed.marking
blk = 'history.committed.marking'
blocks[blk]['body_sha'] = inv[blk]['body_sha']
blocks[blk]['units'] = {'P1': ['history.committed.marking#1', 'history.committed.marking#3'],
                       'B1': ['history.committed.marking#4'], 'B2': ['history.committed.marking#2']}
get(blk, 'history.committed.marking#2')['units'] = ['B2']
put(blk, ob('history.committed.marking#3', ['P1'],
    'For install and update, IPM sets Committed at each commit or before the first write without a transaction (the shared LoadNewModule sites), not from the final status.',
    'install / update success, failure after a commit (u3 without a transaction, developer mode), refusal before the mark (i6, i7)',
    'Committed vs kept writes',
    'full', 'proved',
    ['lnm', 'mode_of', 'guard_refuses', 'install_committed_is_kept', 'install_refusal_uncommitted', 'dev_failure_committed'],
    B + 'the three marks (Utils/Module.cls:1227, 1313, 1350) are modelled under the Main.Install and Main.Update callers, after the guards (1213-1222). Controls no-commit-mark-without-transaction, no-commit-mark-dev, guard-after-commit-mark and dependency control finalize-status-only rejected; native u3 (failed, committed), i7 (refused without a transaction, uncommitted).',
    support='mutation-checked'))
put(blk, ob('history.committed.marking#4', ['B1'],
    'For uninstall, which runs no transaction, nothing sets Committed early: Finalize sets it at the end from the final status (Committed || ISOK, General/AbstractHistory.cls:131).',
    'uninstall that fails after deleting resources (n4, n5) vs one that succeeds (n1); dependency records (c1, n5)',
    'Committed on the uninstall row',
    'full', 'proved',
    ['top_rec', 'dep_rec', 'uninstall_committed_is_success', 'dep_rec_committed', 'n5_facts'],
    B + 'text changed with the ontology (the statement now scopes the early marks to load, install and update; source PR #1322 "Uninstall is unchanged"). Controls uninstall-sets-committed, S-m03 and dependency control finalize-without-isok rejected; native n1 (1), n4 and n5 (0 with kept removals); flips n1.r1.committed, n4.r1.committed and n5.r1.committed rejected.',
    support='mutation-checked'))

# ---------------------------------------------------------------- history.committed.uninstall
blk = 'history.committed.uninstall'
blocks[blk]['body_sha'] = inv[blk]['body_sha']
blocks[blk]['units'] = {'P1': ['history.committed.uninstall#1', 'history.committed.uninstall#2']}
put(blk, ob('history.committed.uninstall#1', ['P1'],
    'Uninstall runs without a transaction, so its Committed equals its success: a failed uninstall that removed some resources reads uncommitted.',
    'uninstall success (n1) vs failure after removing some resources (n4: FileCopy target deleted; n5: the module\'s class deleted)',
    'Committed on the uninstall row',
    'full', 'proved',
    ['top_rec', 'uninstall_committed_is_success', 'n5_facts', 'n1_facts'],
    B + 'uninstall_committed_is_success holds for every uninstall.rs world; n5_facts exhibits the kept removal (class 12 gone) with Committed 0. Controls uninstall-sets-committed and finalize-without-isok rejected; native n1, n4 (replayed from its observed Clean status: uninstall.rs has no resource-processor failure), n5 (fully computed).',
    support='mutation-checked'))
put(blk, ob('history.committed.uninstall#2', ['P1'],
    'The uninstall records of the dependencies that uninstall -r or `<module> clean` removes read Committed exactly when that dependency\'s Clean succeeded.',
    'dependency removed by clean (c1, committed) vs dependency whose Clean failed under uninstall -r (n5, uncommitted)',
    'Committed on the dependency row',
    'full', 'proved',
    ['dep_rec', 'dep_rec_committed', 'c1_facts', 'n5_facts'],
    B + 'new obligation for the added sentence. dep_rec_committed holds for every row uninstall.rs dep_step appends. Mutant S-m03 and dependency control finalize-without-isok rejected; native c1 and n5 (r2).',
    support='mutation-checked',
    note='A dependency skipped because another module still needs it is recorded as a successful, committed uninstall (uninstall.rs unin_witness_recurse_skipped_dep, P:F-UNIN-6 inside -r).'))

# ---------------------------------------------------------------- history.fields / history.phases#1 (re-judged)
blk = 'history.fields'
f1 = get(blk, 'history.fields#1')
f1.update({'grade': 'full', 'check': 'proved', 'assessed_in': 'iteration-74', 'support': 'mutation-checked',
           'models': f1['models'] + [A + m for m in ['main_install', 'cli_update', 'top_rec', 'dep_rec', 'dep_load_row',
                                                   'dep_load_label', 'install_always_recorded', 'update_recorded_iff',
                                                   'uninstall_committed_is_success']],
           'evidence': f1.get('evidence', []) + EV,
           'basis': f1['basis'] + ' ' + B + 'update records are labelled update (cli_update, update_recorded_iff), uninstall and dependency uninstall records uninstall (top_rec, dep_rec), phase-scoped dependency records load (dep_load_label); reinstall is labelled install by composition (cli_reinstall delegates to cli_install, install_always_recorded). Mutants S-m10, S-m11, S-m12 rejected; native action fields of i1-i7, u1-u3, n1-n5, c1, t1 replayed.'})
f1.pop('gap', None)
f3 = get(blk, 'history.fields#3')
f3['gap'] = ('The writer half is now modelled for install, update and uninstall (iteration74 init stores the placeholder; '
             'set_version from the search result, LoadNewModule or NameOpen; mutants install-version-not-from-search and S-m07 '
             'rejected; native verplaceholder fields). The load writer half (crate Twin version slot) still has no theorem, and '
             'a module whose real version is 0.0.0-0 is C:D43.')
f3['assessed_in'] = 'iteration-74'
blk = 'history.phases'
p1 = get(blk, 'history.phases#1')
p1['gap'] = ('iteration74 run_from represents every phase entry of install, update and uninstall with its status and whether it '
             'ended; the start and end times themselves are not represented, and load phases stay summarised in the crate.')
p1['assessed_in'] = 'iteration-74'

path.write_text(json.dumps(data, indent=1) + '\n')
print('updated', path)
