#!/usr/bin/env python3
"""Apply the iteration72 rule-4a re-audit and the review-71 repairs to the census records.

  git show fca3ed0f:<blocks> is the starting point; run once from the repository root:
  python3 -I .g5/uplift/coverage-20261117/iter72/apply.py

Phase A applies iter72/audit/*.json: every obligation graded full at the starting commit has one
ledger entry. A partial verdict regrades it with the ledger gap; a full verdict records its support
kind. Phase B applies REPAIRS (review-71 A2, A3 and part B) on top, so a repair decides when both
touch one obligation. Phase C writes iter72/changes.json (old vs new for every obligation).
"""
import json, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BLOCKS = HERE.parent / 'census' / 'blocks'
START = 'fca3ed0f'
REL = '.g5/uplift/coverage-20261117/'
E68 = REL + 'iter68/receipts/index.jsonl'
E70 = REL + 'iter70/receipts/index.jsonl'
E72R = REL + 'iter72/receipts/index.jsonl'
E72M = REL + 'iter72/mutations/M.jsonl'
REV = REL + 'reviews/review-71.md'
IT = 'iteration-72'
V = '.g5/Verus_verification/'
CR = V + 'crates/ipm-load-history/src/'
PB = 'playbook-uplift/models/verus/'
R72 = 'iteration-72 (review-71 '


def start_pages():
    pages = {}
    for p in sorted(BLOCKS.glob('*.json')):
        rel = str(p.relative_to(ROOT))
        pages[p.name] = json.loads(subprocess.check_output(['git', 'show', f'{START}:{rel}'], cwd=ROOT))
    return pages


def index(pages):
    idx = {}
    for name, d in pages.items():
        for bid, r in d['blocks'].items():
            for o in r.get('obligations', []):
                idx[o['id']] = (name, bid, o)
    return idx


def add_unique(lst, items):
    for x in items:
        if x not in lst:
            lst.append(x)


def phase_a(pages, idx):
    ledgers = {}
    for p in sorted((HERE / 'audit').glob('*.json')):
        d = json.loads(p.read_text())
        for e in d['entries']:
            if e['id'] in ledgers:
                sys.exit(f"{e['id']}: two ledger entries")
            e['_auditor'] = d.get('auditor', '?')
            e['_ledger'] = REL + 'iter72/audit/' + p.name
            ledgers[e['id']] = e
    fulls = {oid for oid, (_, _, o) in idx.items() if 'alias_of' not in o and o.get('grade') == 'full'}
    missing, extra = fulls - set(ledgers), set(ledgers) - fulls
    if missing or extra:
        sys.exit(f'ledger coverage: missing {sorted(missing)} extra {sorted(extra)}')
    for oid, e in ledgers.items():
        o = idx[oid][2]
        ev = o.setdefault('evidence', [])
        add_unique(ev, [e['_ledger']])
        groups = sorted({m.split('-')[0] for m in e.get('mutations', []) if '-m' in m})
        add_unique(ev, [REL + f'iter72/mutations/{g}.jsonl' for g in groups])
        if any(m.startswith(V + 'consolidated/iter41/settings.rs') or m.startswith(V + 'consolidated/iter50/defmods.rs')
               for m in o.get('models', [])):
            add_unique(ev, [E72R])
        tag = []
        if e.get('mutations'):
            tag.append('mutations ' + ', '.join(e['mutations']))
        if e.get('control'):
            tag.append('control ' + e['control'])
        if e.get('instance_scoped'):
            tag.append('instance-scoped')
        tail = f"; {IT} rule-4a re-audit ({e['_auditor']}): {e['verdict']}, support {e.get('support')}" + \
               (' (' + '; '.join(tag) + ')' if tag else '')
        o['basis'] = o.get('basis', '') + tail
        o['assessed_in'] = IT
        if e['verdict'] == 'partial':
            o['grade'] = 'partial'
            o['gap'] = 'Rule 4a (iteration-72): ' + e['gap']
            o.pop('support', None)
        elif e['verdict'] == 'full':
            o['support'] = e['support']
            if e.get('instance_scoped'):
                o['instance_scoped'] = True
        else:
            sys.exit(f'{oid}: verdict {e["verdict"]!r}')
    return ledgers


# ---------------------------------------------------------------- phase B: review-71 repairs
def S(oid, **kw):
    return ('set', oid, kw)


def A(bid, units, ob):
    return ('add', bid, units, ob)


def U(bid, unit, value):
    return ('unit', bid, unit, value)


def B(bid, key, value):
    return ('block', bid, key, value)


def new(oid, units, text, cases, observation, grade, check, models, evidence, basis, gap=None, support=None, **kw):
    o = {'id': oid, 'units': units, 'text': text, 'cases': cases, 'observation': observation, 'grade': grade,
         'check': check, 'models': models, 'evidence': evidence, 'basis': basis, 'assessed_in': IT}
    if gap:
        o['gap'] = gap
    if support:
        o['support'] = support
    o.update(kw)
    return o


REPAIRS = [
    # ---- A2: grades too strict
    S('build.publish#2', grade='full', check='proved', support='mutation-checked',
      models=[PB + 'settings.rs::publish_target', PB + 'settings.rs::find_name',
              PB + 'settings.rs::repo_refines_publish_named', PB + 'settings.rs::repo_publish_named_missing'],
      evidence=[E70, E72M, PB + 'controls/settings.json', PB + 'results/settings.json'],
      basis=f'new:{R72}A2): publish_target picks find_name(rs, n) for -repo n (Main.cls modifier repo -> PublishTo) and '
            'PublishingServer otherwise; repo_refines_publish_named proves -r R publishes to R under premise PE (the named '
            'registry is enabled; the product opens a named registry with no Enabled check, so a disabled named registry is '
            'outside the proved domain) and unique names. Mutation M-m05 (ignore -repo) killed; controls impl-publish-to-folder '
            'and impl-missing-name-falls-back rejected.', gap=None),
    S('update#2', grade='partial', check='refuted', models=[V + 'consolidated/iter34/versions.rs::installed',
      V + 'consolidated/iter34/versions.rs::fixture_neighbors', V + 'consolidated/iter34/versions.rs::releases_agree'],
      evidence=[E72R], basis=f'new:{R72}A2)',
      gap='Update without a version calls Install (Main.cls:4445-4449), whose choice versions.rs::installed computes from '
          'the offered versions (Main.cls:2603 takes GetAt(1) of the SQL-ordered results). With releases only the order is '
          'semantic (releases_agree). With prereleases enabled fixture_neighbors shows the install picks 2.0.0-beta.1 while '
          'the semantic maximum is 2.0.0, which refutes "latest". The Update -> Install routing is source reading only, and '
          'no theorem states the installed version after update.'),
    S('history.actions#2', text='Every install and update attempt that reaches its History init is recorded, including '
      'failed ones and refusals after the init (for example install with no module name).',
      cases='reinstall success vs failure (each one row); install with no name or an unknown name; update reaching Install',
      grade='partial', check='unchecked',
      models=[V + 'consolidated/iter22/reinstall.rs::history_neighbor', V + 'consolidated/iter22/reinstall.rs::reinstall',
              V + 'consolidated/iter22/reinstall.rs::begin_install'],
      evidence=[E70], basis=f'new:{R72}A2); text narrowed: refusals that throw before the init are history.actions#6 and #7',
      gap='reinstall.rs computes the install-record delta: a successful and a failed reinstall each add one record '
          '(history_neighbor), but the failed rows are FailedInstallFieldsUnknown. Plain install (no name, unknown name, not '
          'found; InstallInit at Main.cls:2565-2566 precedes the name check) and the update path (UpdateInit, Main.cls:4432) '
          'are not modeled.'),
    A('history.actions', ['P1', 'B1'], new(
        'history.actions#6', ['P1', 'B1'],
        'Intended: a refused reinstall is recorded too. As built, reinstall of a module that is not installed throws before '
        'any History row (Main.cls:2685-2687).',
        'reinstall of a name that is not installed vs reinstalls that reach Install (success, failure)',
        'install record count', 'full', 'refuted',
        [V + 'consolidated/iter22/reinstall.rs::reinstall', V + 'consolidated/iter22/reinstall.rs::history_neighbor',
         V + 'consolidated/iter22/reinstall.rs::begin_install'],
        [E70, E72M],
        f'new:{R72}A2): reinstall() checks installed_name before begin_install; history_neighbor asserts record_delta 0 for '
        'missing() against 1 for the failed and successful reinstalls. Mutation M-m06 (enter install before the check) killed.',
        support='mutation-checked')),
    A('history.actions', ['P1', 'B1'], new(
        'history.actions#7', ['P1', 'B1'],
        'Intended: a refused update is recorded too. As built, update with -PermitDowngrade or of a module that is not '
        'installed throws before HistoryTemp.UpdateInit writes any row (Main.cls:4415-4423).',
        'update of a not-installed module vs an update reaching Install', 'History row count', 'absent', 'none', [], [REV],
        f'new:{R72}A2)',
        gap='No model of Main.cls::Update. Source reading (review-71 A2) shows both refusals throw before UpdateInit at '
            'Main.cls:4432. Contrast: update of a not-installed module leaves the History row count unchanged, against an '
            'update that reaches Install and adds one row.')),
    S('lifecycle.custom#2', grade='full', check='proved', support='mutation-checked',
      models=[V + 'consolidated/iter48/cpfmerge.rs::merges_in', V + 'consolidated/iter48/cpfmerge.rs::merged_by',
              V + 'consolidated/iter48/cpfmerge.rs::neighbors', V + 'consolidated/iter30/phases.rs::custom_and_unknown_neighbors'],
      evidence=[E72R, E72M, '.g5/uplift/consolidated/iter48/run.json'], composed=['lifecycle.custom#4'],
      basis=f'new:{R72}A2): merges_in merges a CPF that names a CustomPhase only when that custom phase runs; neighbors '
            'contrasts merged_by(B, [48]) with !merged_by(B, load()). The phase list of `<m> mycustom` comes from '
            'phases.rs (a custom name alone runs CUSTOM, custom_and_unknown_neighbors without -only). Mutations M-m03 (ignore '
            'CustomPhase) and M-m04 (custom CPF merges in any phase) killed.', gap=None),
    # ---- A3: ontology contradicted
    S('history.committed.marking#1', text='For load, IPM sets Committed at each commit, or just before the first write '
      'when there is no transaction, and does not derive it afterwards from developer mode and the final status.',
      grade='full', check='proved', support='control',
      evidence_add=[V + 'results/verus/20261113T223457Z-control-01-no-setcommitted-c1/receipt.json',
                V + 'results/verus/20261113T223457Z-control-02-no-setcommitted-c3/receipt.json',
                V + 'results/verus/20261113T223457Z-control-03-no-setcommitted-notx/receipt.json',
                V + 'results/verus/20261113T223113Z-asbuilt-invariants-nojournal/receipt.json',
                '.g5/Verus_verification/coverage-map.md'],
      basis='reused:.g5/Verus_verification/coverage-map.md (marking: SetCommitted sites bound); '
            f'{R72}A3): narrowed to load, the only caller the crate models. Controls 01-03 remove each SetCommitted site '
            'and COMMIT-SOUND reports a violation; the explorer check ISOK-REDUNDANT shows an OK Finalize finds Committed '
            'already set.', gap=None),
    A('history.committed.marking', ['P1'], new(
        'history.committed.marking#3', ['P1'],
        'For install and update, IPM sets Committed at each commit or before the first write without a transaction (the '
        'shared LoadNewModule sites), not from the final status.',
        'install / update success, failure after a commit, refusal', 'Committed vs kept writes', 'partial', 'unchecked',
        [CR + 'model.rs::set_com', CR + 'proofs.rs::theorem_commit_sound'], [REV],
        f'new:{R72}A3)',
        gap='The SetCommitted sites (Utils/Module.cls:1227,1313,1350) are inside LoadNewModule, which the crate models only '
            'under the Main:Load caller. Main:Install and Main:Update are not modeled.')),
    A('history.committed.marking', ['P1'], new(
        'history.committed.marking#4', ['P1'],
        'P1 applied to uninstall: Committed is set when changes are kept, not derived from the final status. As built, '
        'uninstall never calls SetCommitted and Finalize sets Committed = Committed || ISOK(status) '
        '(General/AbstractHistory.cls:131).',
        'uninstall that fails after deleting resources vs one that succeeds', 'Committed on the uninstall row',
        'partial', 'unchecked', [V + 'consolidated/verus/history.rs::finalize'], [E70, REV],
        f'new:{R72}A3)',
        gap='history.rs::finalize computes Committed || status OK as AbstractHistory.cls:131 does, but no theorem exercises '
            'a row that never had SetCommitted (uninstall), and the uninstall.rs rows carry no Committed field. The '
            'contradiction is source-confirmed, not modeled. Contrast: an uninstall that fails after deleting a resource '
            'reads Committed=0 although it kept changes, against a successful one that reads 1.')),
    S('lifecycle.module-action#2', text='compile, reload, test, package, verify, publish and makedeployed are shortcuts '
      'for module-action.',
      gap='Only the compile shortcut (RunOnePhase, native case c9) is exercised. The reload, test, package, verify, publish '
          'and makedeployed shortcuts are not modeled.', basis='new:iteration-70; iteration-72: update split to #3'),
    A('lifecycle.module-action', ['P1'], new(
        'lifecycle.module-action#3', ['P1'], 'update is a shortcut for module-action.',
        'update <m> vs <m> update', 'phases run and update steps', 'absent', 'none', [], [REV],
        f'new:{R72}A3)',
        gap='Contradicted by source: Main.cls:1118 dispatches update to Main::Update (Install or Load with update steps) '
            'before the standard-phase check at Main.cls:1120, so it is not a phase shortcut. No model of the shell '
            'dispatch.')),
    S('namespace.enable#11', gap='ns_mapping.rs has no success/failure result for any command (header). As built (review-71 '
      'A3, source only), enable -community catches each step\'s failure, keeps going and reports no error '
      '(Main.cls:3564-3574), which contradicts this assumption. Contrast: a failing `repo -reset-defaults` step inside '
      '`enable -community` returns its error and no namespace is mapped, vs success. Open question '
      'namespace.enable.community-failure-policy.'),
    S('settings#7', gap='No model represents the installer seeding, and iter41 c0() and playbook settings.rs Cfg.seed take '
      'seeded values as fixture input. As built (review-71 A3, source only) HistoryRetain is never seeded '
      '(preload/cls/IPM/Installer.cls:110-124), so `config get HistoryRetain` on a fresh install shows empty, not 0. '
      'Retention treats empty as 0, so the meaning holds and the displayed default does not.'),
    S('inspect#3', gap='grouped is exercised only with all_versions=true (no neighbour where -versions is off and only the '
      'latest shows), and repo_visited decides one registry at a time with no aggregation across registries into the '
      'printed list (Main.cls::Search / Repo/Manager.cls::SearchRepositoriesForModule). As built -versions also lifts the '
      'platform filter (Utils/Module.cls:938, review-71 A3), which the block does not say.'),
    # ---- B: lifecycle.custom
    A('lifecycle.custom', ['B1'], new(
        'lifecycle.custom#9', ['B1'], 'Two custom phase names in one command run neither custom phase.',
        'mycustom othercustom vs mycustom', 'custom hooks run; status', 'partial', 'unchecked',
        [V + 'consolidated/iter30/phases.rs::complete_plan', V + 'consolidated/iter30/phases.rs::ordered_from'], [E72R],
        f'new:{R72}B)',
        gap='complete_plan of two non-standard names is the empty plan (ok, nothing runs), but no theorem or native case '
            'exercises two custom names, and phases.rs has a single CUSTOM token, so two distinct custom phases are not '
            'represented. The block does not say that the command succeeds.')),
    # ---- B: repo.oras.integrity
    S('repo.oras.integrity#1', cases='no tag: provider order [2,1] vs [1,2], empty tag list; explicit tag vs no tag',
      grade='partial', check='unchecked',
      gap='last_tag has no tag input. A pull with an explicit tag (the tag used as given) is not modeled, so the "without a '
          'tag" condition is not distinguished (review-71 B).'),
    U('repo.oras.integrity', 'B4', ['repo.oras.integrity#7']),
    A('repo.oras.integrity', [], new(
        'repo.oras.integrity#7', ['B4'],
        'Working assumption: expose the tag-order and file/layer mismatches and retain size and digest checking.',
        'first tar layer: size mismatch only vs digest mismatch only vs both match', 'validation result', 'partial',
        'unchecked', [V + 'consolidated/iter2/registry.rs::verify_layers', V + 'consolidated/iter2/registry.rs::first_tar_controls_integrity',
                      V + 'consolidated/iter2/registry.rs::last_tag'], [E70],
        f'new:{R72}B): B4 was excluded as undecided alternatives; its first sentence is a working assumption (rule 3)',
        gap='Retaining both checks is the as-built verify_layers, but its mismatching neighbour differs in size and digest '
            'together, so dropping either comparison still verifies (repo.oras.integrity#3). "Expose the mismatch" is a '
            'documentation policy with no formal observation. The alternatives in the bullet stay undecided (open question '
            'repo.oras.integrity.policy).')),
    # ---- B: transaction.flag-sources
    S('transaction.flag-sources#1', text='NoTransaction/NoJournal can come from the command line, the namespace default '
      'modifiers or the module <Defaults>; a command-line value wins over a stored default modifier.',
      cases='same key on the command line vs stored default modifier', observation='effective flag value in the command data',
      models=[V + 'consolidated/iter50/defmods.rs::merged', V + 'consolidated/iter50/defmods.rs::command_line_wins'],
      evidence_add=[E72R], basis_append=f'; {R72}B): split; the shell site (Main.cls:1067-1069) stays here and the '
      'LoadNewModule site is #10'),
    A('transaction.flag-sources', ['P1'], new(
        'transaction.flag-sources#10', ['P1'],
        'A command-line NoTransaction/NoJournal value wins over the module\'s own <Defaults> (LoadNewModule merges the '
        '<Defaults> first and the command parameters over them, Utils/Module.cls:1166-1167).',
        'command parameter Some(false) vs <Defaults> Some(true); command absent', 'effective flag of the activation',
        'full', 'proved',
        [V + 'consolidated/verus/loadtx.rs::merge_key', V + 'consolidated/verus/loadtx.rs::effective',
         V + 'consolidated/verus/loadtx.rs::explicit_false_parent_overrides_child'], [E70, E72M],
        f'new:{R72}B): split from #1. Mutation M-m07 (the <Defaults> value wins) killed.', support='mutation-checked')),
    A('transaction.flag-sources', ['B3'], new(
        'transaction.flag-sources#11', ['B3'],
        'Process journaling is restored when the NoJournal child activation ends ("during that activation").',
        'journaled parent write after a NoJournal dependency activation, parent rolled back', 'that write after the rollback',
        'absent', 'none', [], [REV], f'new:{R72}B)',
        gap='loadtx takes Write.journal per write as an input. The JournalManager restore (Utils/Module.cls:1191-1193, '
            'journaling switched back when the manager object closes) is not modeled.')),
    A('transaction.flag-sources', ['B5'], new(
        'transaction.flag-sources#12', ['B5'],
        'Working assumption: a module\'s <Defaults> apply whenever that module is installed, including as a dependency.',
        'dependency <Defaults> NoJournal=1 under a parent with NoJournal absent vs given as 0', 'effective flag of the dependency',
        'full', 'refuted',
        [V + 'consolidated/verus/loadtx.rs::merge_key', V + 'consolidated/verus/loadtx.rs::explicit_false_parent_overrides_child'],
        [E70, E72M],
        f'new:{R72}B): refuted as built, because a parent parameter that exists, even 0, overrides the dependency\'s own '
        '<Defaults> (merge_key(Some(false), Some(true)) == Some(false)). Mutation M-m07 (child wins) killed.',
        support='mutation-checked')),
    S('transaction.flag-sources#8', text='Working assumption (intended): a module that declares NoTransaction must not run '
      'inside a transaction opened for its parent, so the change runs without one and a failure may leave a partial install.',
      cases='child NoTransaction inside an open parent transaction; child failure after a write',
      basis_append=f'; {R72}B): "partial install" restored to the text'),
    # ---- B: uninstall.cleanup
    S('uninstall.cleanup#1', text="Uninstall deletes the module's classes, packages, includes and routines with their "
      'compiled forms.',
      cases='code kinds; mapped code without -global (as built skipped, Lifecycle/Base.cls:375-378) vs with -global',
      observation='code present after uninstall', grade='partial', check='unchecked',
      models=[PB + 'uninstall.rs::clean_code', PB + 'uninstall.rs::code_gone', PB + 'uninstall.rs::unin_refines_complete',
              V + 'consolidated/iter63/cleanup.rs::code_removed'],
      evidence_add=[E70, E72M], basis_append=f'; {R72}B): split into code (#1), record (#7), stored manifest (#8, #10) and '
      'sync baseline (#9)',
      gap='clean_code removes non-mapped code and exempts mapped code without -global (code_gone), but no theorem pins the '
          'mapped-code condition. M-m08 (delete mapped code without -global) survives and M-m09 (never delete mapped code) '
          'is killed. The block omits the as-built condition.'),
    A('uninstall.cleanup', ['P1'], new(
        'uninstall.cleanup#7', ['P1'], "Uninstall deletes the module's record.", 'top-level uninstall vs blocked uninstall',
        'module record present after uninstall', 'full', 'proved',
        [PB + 'uninstall.rs::record_delete', PB + 'uninstall.rs::exec_clean', PB + 'uninstall.rs::unin_refines_honest'],
        [E70, PB + 'controls/uninstall.json', PB + 'results/uninstall.json'],
        f'new:{R72}B): split from #1. Control impl-top-record-kept (the top-level Clean never deletes the record) is '
        'killed in unin_refines_honest.', support='control')),
    A('uninstall.cleanup', ['P1'], new(
        'uninstall.cleanup#8', ['P1'], "Uninstall deletes the module's stored manifest together with its record.",
        'top-level target without -r', 'stored manifest after uninstall', 'full', 'proved',
        [PB + 'uninstall.rs::record_delete', PB + 'uninstall.rs::unin_refines_manifest_with_record'], [E70, E72M],
        f'new:{R72}B): split from #1; premise PR (no -r), -r dependencies are #10. Mutation M-m10 (record_delete keeps the '
        'stream) killed.', support='mutation-checked')),
    A('uninstall.cleanup', ['P1'], new(
        'uninstall.cleanup#9', ['P1'], "Uninstall deletes the module's sync baseline.", 'baseline rows before vs after',
        'FileHash rows after uninstall', 'partial', 'unchecked', [V + 'consolidated/iter63/cleanup.rs::remains'], [E68],
        f'new:{R72}B): split from #1',
        gap='cleanup.rs::remains is a per-kind constant table (Baseline removed, by the FileHash foreign-key cascade its '
            'header cites). No model derives the cascade from the record delete (rule 4a).')),
    A('uninstall.cleanup', ['P1'], new(
        'uninstall.cleanup#10', ['P1'],
        'Intended (P1 for each removed module): uninstall -recurse deletes each removed dependency\'s stored manifest with '
        'its record. As built the nested NameDelete deletes the record only.',
        '-r uninstall removing a dependency vs the top-level target', 'dependency stored manifest after uninstall',
        'full', 'refuted',
        [PB + 'uninstall.rs::unin_witness_recurse_leaves_manifest', PB + 'uninstall.rs::dep_step'], [E70, E72M],
        f'new:{R72}B): unin_witness_recurse_leaves_manifest shows the dependency record gone and its stream kept '
        '(l1_manifest_with_record fails). Mutation M-m11 (nested removal also deletes the stream) killed.',
        support='mutation-checked')),
    S('uninstall.cleanup#2', cases='Global attribute set vs name-only global resource; Global set with Preserve on '
      '(product keeps it, Default/Global.cls:76-80)', grade='partial', check='unchecked',
      evidence_add=[E72M],
      gap='The Global-attribute contrast is pinned (M-m13 killed; unin_witness_global_kept), but the Preserve condition is '
          'not: M-m12 (delete Global data even with Preserve) survives. The block does not state Preserve.'),
    # ---- B: devtools.authoring.module-version
    S('devtools.authoring.module-version#1', text='On success, module-version sets the stored manifest and the installed '
      'record to the same new version.', cases='successful bump', observation='record and stored manifest versions',
      grade='full', check='proved', support='mutation-checked',
      models=[V + 'consolidated/verus/owners.rs::module_version', V + 'consolidated/verus/owners.rs::update_version',
              V + 'consolidated/verus/owners.rs::witnessed_owners_distinctions'], evidence=[E68, E72M],
      basis=f'reused:.g5/ontology/features/developer-tools.g5.md metadata; {R72}B): split, failure half is #7. Mutation '
            'M-m14 (success sets the record only) killed.', gap=None),
    A('devtools.authoring.module-version', ['P1'], new(
        'devtools.authoring.module-version#7', ['P1'],
        'When a step after the record save fails, neither the record nor the stored manifest changes.',
        'checkout, save or manifest rewrite failure after the record is set vs success', 'record and stored manifest versions',
        'partial', 'unchecked', [V + 'consolidated/verus/owners.rs::module_version'], [E68, E72M],
        f'new:{R72}B): split from #1',
        gap='module_version returns state s on every error, so the transaction rollback (Main.cls:4076-4096) is written '
            'down, not modeled. M-m16 (a rewrite failure keeps the new record) survives (rule 4a). Rollback not run '
            'natively.')),
    S('devtools.authoring.module-version#6', gap='The disk-unchanged half is proved (see #5). The source-control handoff '
      '(StudioDocument/Module.cls::UpdateVersion through the extension) is only an error input (checkout), so no rule shows '
      'the hook receiving the new version. As built only CheckOut is called (Main.cls:4080-4084), so review-71 B notes the '
      'hand-off may be false as built.'),
    # ---- B: load.options
    U('load.options', 'B6', ['load.options#11']),
    A('load.options', [], {'id': 'load.options#11', 'units': ['B6'], 'text': "-create-lockfile writes the module's lock file.",
                           'alias_of': 'deps.lockfile#1', 'assessed_in': IT, 'basis': f'{R72}B): was excluded'}),
    A('load.options', ['B8'], {'id': 'load.options#12', 'units': ['B8'],
                               'text': '-D<name>=<value> also passes the transaction flags.',
                               'alias_of': 'transaction.opt-out#4', 'assessed_in': IT, 'basis': f'{R72}B)'}),
    S('load.options#9', basis=f'{R72}B): alias kept. module.parameter#2 (override_reaches_both) covers a -D value '
      'reaching the module and its phases for a declared name. No owner states that an undeclared -D name is passed, '
      'which is an ontology repair.'),
    # ---- B: module.env
    S('module.env#2', text='${VAR} in the files is replaced with environment values.', cases='VAR set; nested and array values',
      basis_append=f'; {R72}B): "an unset variable gives empty" removed; the block does not say it'),
    S('module.env#8', text='Intended: an environment value is inserted literally.', cases='value a$1b, a\\b vs plain',
      basis_append=f'; {R72}B): split, the as-built defect is #12'),
    A('module.env', ['B5'], new(
        'module.env#12', ['B5'], 'As built (C:D14), a $ or \\ in an environment value changes or breaks the result.',
        'value a$1b, a\\b, a$9b vs plain', 'configuration value or error', 'partial', 'unchecked',
        [V + 'consolidated/verus/env_config.rs::replacement', V + 'consolidated/verus/env_config.rs::dollar_is_not_literal'],
        [E68], f'new:{R72}B): split from #8',
        gap='dollar_is_not_literal proves the ICU replacement semantics, but evaluate_json takes the substitution as an '
            'input, so no rule connects an environment value to that replacement text. Contrast: ${CONS_VAR}=a$9b makes the '
            'load fail while =ab succeeds (native U7, U8).')),
    B('module.env', 'unit_notes', {'B7': 'first sentence ("Both direct merge and the actual file loader have receipts") is '
                                         'an evidence statement and excluded; the rest is module.env#10'}),
    # ---- B: module.processor.file-copy
    U('module.processor.file-copy', 'B2', ['module.processor.file-copy#14']),
    A('module.processor.file-copy', [], {'id': 'module.processor.file-copy#14', 'units': ['B2'],
                                         'text': 'FileCopy refuses to copy when the source and target are the same folder '
                                                 'or one contains the other.',
                                         'alias_of': 'build.copy.overlap#1', 'assessed_in': IT,
                                         'basis': f'{R72}B): the owner is now enumerated'}),
    S('module.processor.file-copy#12', text='A successful copy remains after database rollback while a journaled sentinel '
      'disappears.', basis=f'{R72}B): the failed-copy sentence is #15'),
    A('module.processor.file-copy', ['B8'], new(
        'module.processor.file-copy#15', ['B8'], 'Rollback does not turn a failed copy into a successful one.',
        'failed copy (missing source) followed by a database rollback vs no rollback', 'copy status and target existence',
        'partial', 'unchecked',
        [V + 'consolidated/verus/thin_surfaces.rs::rollback', V + 'consolidated/verus/thin_surfaces.rs::file_copy_observation'],
        [E68], f'new:{R72}B)',
        gap='rollback() replaces only the database by definition, so a failed copy\'s status is untouched by construction, '
            'and no theorem composes a failed copy with a rollback (rule 4a).')),
    A('module.processor.file-copy', ['B8'], new(
        'module.processor.file-copy#16', ['B8'],
        'Creating the destination directory and partial operating-system copy errors are effects separate from the copy '
        'outcome.', 'mkdir succeeds then the copy fails part-way vs clean failure', 'directory and partial files on disk',
        'partial', 'unchecked', [V + 'consolidated/verus/thin_surfaces.rs::copy_step'], [E68], f'new:{R72}B): B8 third sentence',
        gap='copy_step takes directory_created, mkdir_ok and the OS result as relation inputs. No rule derives the '
            'directory creation or a partial copy from FileCopy.cls / %File.CopyDir.')),
    S('module.processor.file-copy#13', alias_of='module.processor.file-copy.protection#9',
      basis=f'{R72}B): re-aliased to the same working assumption on the protection block'),
    # ---- B: settings.default-modifiers
    S('settings.default-modifiers#10', text='-set of a name already stored replaces its value.',
      cases='existing key in the -set data', basis_append=f'; {R72}B): split, the empty -set is #12'),
    A('settings.default-modifiers', ['B6'], new(
        'settings.default-modifiers#12', ['B6'], '-set without any -D value stores nothing.',
        '-set with empty data vs -set with a value', 'stored list', 'full', 'proved',
        [V + 'consolidated/iter50/defmods.rs::default_modifiers', V + 'consolidated/iter50/defmods.rs::set_appends_and_overwrites'],
        [E72R, E72M], f'new:{R72}B): split from #10. Mutation M-m17 (an empty -set clears the store) killed.',
        support='mutation-checked')),
    # ---- B: history.committed
    S('history.committed#8', text='A failure with -DNoTransaction=1 is committed once the action has started writing.',
      cases='NoTransaction Activate failure after writes vs refusal before writes',
      basis_append=f'; {R72}B): split, NoJournal is #16'),
    A('history.committed', ['B4'], new(
        'history.committed#16', ['B4'], 'A failure with -DNoJournal=1 is committed once the action has started writing.',
        'NoJournal failure after writes vs refusal before writes', 'Committed vs kept writes', 'full', 'bounded',
        [CR + 'model.rs::set_com', CR + 'proofs.rs::theorem_commit_sound'],
        [V + 'results/verus/20261113T223113Z-asbuilt-invariants-nojournal/receipt.json',
         V + 'results/verus/20261113T223457Z-control-03-no-setcommitted-notx/receipt.json'],
        f'new:{R72}B): split from #8. The NoJournal explorer run (1,950 configurations, not truncated) checks COMMIT-SOUND '
        '(Committed=0 implies nothing kept), whose contrapositive is this obligation. Control 03 (no SetCommitted before '
        'writes, txms 1 and 2) is reported as a violation.', support='control')),
    A('history.committed', ['B1'], new(
        'history.committed#15', ['B1'], 'A successful dependency under a successful parent reads Committed=1.',
        'dependency and parent both succeed vs parent rolls back', 'dependency row Committed', 'partial', 'unchecked',
        [CR + 'contract.rs::dep_rec', CR + 'model.rs::set_com'], [REV], f'new:{R72}B)',
        gap='contract.rs::dep_rec expects committed-iff-kept for dependency rows and model.rs sets Committed at the '
            'dependency commit, but no named case or reported explorer configuration isolates a successful dependency '
            'under a successful parent.')),
]


def apply_repairs(pages, idx):
    by_block = {bid: (name, r) for name, d in pages.items() for bid, r in d['blocks'].items()}
    touched = []
    for op in REPAIRS:
        if op[0] == 'set':
            _, oid, kw = op
            o = idx[oid][2]
            kw = dict(kw)
            if 'basis_append' in kw:
                o['basis'] = o.get('basis', '') + kw.pop('basis_append')
            if 'evidence_add' in kw:
                add_unique(o.setdefault('evidence', []), kw.pop('evidence_add'))
            for k, v in kw.items():
                if v is None:
                    o.pop(k, None)
                else:
                    o[k] = v
            if o.get('grade') != 'full':
                o.pop('support', None)
                o.pop('instance_scoped', None)
            if 'alias_of' in o:
                for k in ('grade', 'check', 'models', 'evidence', 'gap', 'support', 'cases', 'observation'):
                    o.pop(k, None)
            o['assessed_in'] = IT
            touched.append(oid)
        elif op[0] == 'add':
            _, bid, units, ob = op
            name, r = by_block[bid]
            if ob['id'] in idx:
                sys.exit(f"{ob['id']} exists")
            r['obligations'].append(ob)
            idx[ob['id']] = (name, bid, ob)
            for u in units:
                cur = r['units'][u]
                if isinstance(cur, str):
                    r['units'][u] = [ob['id']]
                elif ob['id'] not in cur:
                    cur.append(ob['id'])
            touched.append(ob['id'])
        elif op[0] == 'unit':
            _, bid, unit, value = op
            by_block[bid][1]['units'][unit] = value
        elif op[0] == 'block':
            _, bid, key, value = op
            by_block[bid][1][key] = value
    return touched


def main():
    pages = start_pages()
    before = {oid: json.loads(json.dumps(o)) for oid, (_, _, o) in index(pages).items()}
    idx = index(pages)
    ledgers = phase_a(pages, idx)
    touched = apply_repairs(pages, idx)
    for name, d in pages.items():
        (BLOCKS / name).write_text(json.dumps(d, indent=1, ensure_ascii=False) + '\n')
    after = {oid: o for oid, (_, _, o) in index(pages).items()}
    rows = []
    for oid in sorted(set(before) | set(after)):
        b, a = before.get(oid), after.get(oid)
        g = lambda o: (o or {}).get('grade', 'alias' if o and 'alias_of' in o else None)
        if b is None:
            kind = 'new'
        elif a is None:
            kind = 'removed'
        elif ('alias_of' in b) != ('alias_of' in a) or b.get('alias_of') != a.get('alias_of'):
            kind = 'alias-change'
        elif b.get('text') != a.get('text') or b.get('cases') != a.get('cases'):
            kind = 'scope'
        else:
            kind = 'retained'
        rows.append({'id': oid, 'kind': kind, 'before': g(b), 'after': g(a),
                     'check_before': (b or {}).get('check'), 'check_after': (a or {}).get('check'),
                     'support': (a or {}).get('support'), 'instance_scoped': bool((a or {}).get('instance_scoped')),
                     'by': 'repair' if oid in touched else ('audit' if oid in ledgers else None)})
    (HERE / 'changes.json').write_text(json.dumps(rows, indent=1) + '\n')
    print(f'{len(ledgers)} ledger entries, {len(REPAIRS)} repairs, {len(rows)} obligations')


if __name__ == '__main__':
    main()
