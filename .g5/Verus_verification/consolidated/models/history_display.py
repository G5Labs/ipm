"""Family R3: what the history log records for install, uninstall and refused or unfinished actions, and what an
operator reads back through `history details` and `history find` (the final reader). Also the once-a-day purge marker
carried between commands. Twin of IPM 571e04a1 `General/History.cls` and `General/AbstractHistory.cls`.

Native runs: `20261114T032054Z-r3-history.partial-purge-failed` (H1-H6 valid; its purge part failed in direct mode
and is replaced by the next run), `20261114T032524Z-r3-purge` (P1-P4), `20261114T032620Z-r3-history-b` (phases).
Inputs taken from the runs, not predicted: record IDs (15, 16, 18, 19; 5), the user name, the filesystem repository's
Details text and the timestamps (normalized to <T>). The auto-purge deletion itself is P:F-HIST-6 (a baseline member);
the twin predicts only the marker.
"""
import re
ROUND = 'R3'
FAMILY = 'history-records-and-display'
RUNS = ['20261114T032054Z-r3-history.partial-purge-failed', '20261114T032524Z-r3-purge', '20261114T032620Z-r3-history-b', '20261114T032747Z-r3-delete']
def case_run(case): return RUNS[1] if re.fullmatch(r'P\d', case) else RUNS[2] if case.startswith('HB') else RUNS[3] if re.fullmatch(r'D\d', case) else RUNS[0]
H, A = 'General/History.cls', 'General/AbstractHistory.cls'
MEMBERS = {
    A + '::InstallInit': {'slice': A + ':65-68', 'fn': 'install_init', 'behavior': 'install records action install (H1)'},
    A + '::UninstallInit': {'slice': A + ':76-79', 'fn': 'uninstall_init', 'behavior': 'uninstall records action uninstall (H2)'},
    A + '::SetSource': {'slice': A + ':86-97', 'fn': 'set_source', 'behavior': 'records repository name, moniker and details (H1)'},
    H + '::VersionToString': {'slice': H + ':425-437', 'fn': 'version_to_string', 'behavior': 'placeholder 0.0.0-0 shows <unset>; otherwise v<version> (H3, H4)'},
    H + '::TimeToString': {'slice': H + ':439-455', 'fn': 'time_to_string', 'behavior': 'no end time shows the start only; details show both (H4)'},
    H + '::SourceToString': {'slice': H + ':457-472', 'fn': 'source_to_string', 'behavior': 'n/a unless install; details add moniker and details (H1, H2)'},
    H + '::SuccessToString': {'slice': H + ':474-483', 'fn': 'success_to_string', 'behavior': 'details print the raw status; one line prints Success or the error text, ERROR #00 for an unfinished record (H3, H4)'},
    H + '::CommittedToString': {'slice': H + ':485-490', 'fn': 'committed_to_string', 'behavior': 'Committed or Uncommitted (H3, H4)'},
    H + '::ShowColumns': {'slice': H + ':334-382', 'fn': 'show_columns', 'behavior': 'ten columns in fixed order, through the *ToString formatters (H1d)'},
    H + '::ShowPhases': {'slice': H + ':384-423', 'fn': 'show_phases', 'behavior': 'phase name, time and result per phase (HB1d)'},
    H + '::DisplayOneRecord': {'slice': H + ':319-329', 'fn': 'display_one_record', 'behavior': 'details of one ID; a missing ID is an error (H5)'},
    H + '::DisplayQueryResult': {'slice': H + ':492-516', 'fn': 'display_query_result', 'behavior': 'namespace and header once, then one line per record (H4f, H6)'},
    H + '::SetHistoryLastPurge': {'slice': H + ':183-189', 'fn': 'set_last_purge', 'behavior': 'marks today after a purge, so later commands skip it (P1, P2, P4)'},
}
V = {'v': 'as-built'}
def var(n): return V['v'] == n
VARIANTS = {
    'install-action-load': {'member': A + '::InstallInit', 'wrong': 'install recorded as load'},
    'uninstall-action-install': {'member': A + '::UninstallInit', 'wrong': 'uninstall recorded as install'},
    'source-not-recorded': {'member': A + '::SetSource', 'wrong': 'the repository is not recorded'},
    'source-no-details': {'member': A + '::SetSource', 'wrong': 'only the repository name is recorded'},
    'version-placeholder-shown': {'member': H + '::VersionToString', 'wrong': 'the placeholder prints as v0.0.0-0'},
    'time-open-zero': {'member': H + '::TimeToString', 'wrong': 'an unfinished record shows (0 sec)'},
    'source-all-actions': {'member': H + '::SourceToString', 'wrong': 'non-install records show their (empty) source'},
    'success-detail-text': {'member': H + '::SuccessToString', 'wrong': 'details print the error text (intended reading)'},
    'success-unfinished-blank': {'member': H + '::SuccessToString', 'wrong': 'an unfinished record shows an empty result (intended reading)'},
    'committed-yes-no': {'member': H + '::CommittedToString', 'wrong': 'prints yes/no'},
    'columns-raw-values': {'member': H + '::ShowColumns', 'wrong': 'columns print stored values without the formatters'},
    'phases-hidden': {'member': H + '::ShowPhases', 'wrong': 'phase rows are not printed'},
    'details-missing-silent': {'member': H + '::DisplayOneRecord', 'wrong': 'a missing ID prints nothing'},
    'find-no-header': {'member': H + '::DisplayQueryResult', 'wrong': 'no namespace or header line'},
    'marker-never-set': {'member': H + '::SetHistoryLastPurge', 'wrong': 'the marker is not written, so every command purges'},
}
STATUS = '<status>'   # the raw %Status bytes as printed; normalized by log_fields

def install_init(pkg): return {'Action': 'load' if var('install-action-load') else 'install', 'Package': pkg}
def uninstall_init(pkg): return {'Action': 'install' if var('uninstall-action-install') else 'uninstall', 'Package': pkg}
def set_source(rec, name, moniker, details):
    if var('source-not-recorded'): return
    rec['SourceName'] = name
    rec['SourceMoniker'] = '' if var('source-no-details') else moniker
    rec['SourceDetails'] = '' if var('source-no-details') else details
def version_to_string(rec):
    if rec['Version'] == '0.0.0-0' and not var('version-placeholder-shown'): return '<unset>'
    return 'v' + rec['Version']
def time_to_string(rec, detail):
    if detail: return ('Started At <T>; Ended At ' + ('<T>' if rec['Ended'] else '')).rstrip()  # the reader strips trailing blanks
    if not rec['Ended'] and not var('time-open-zero'): return '<T>'
    return '<T> (<n> sec)'
def source_to_string(rec, detail):
    if rec['Action'] != 'install' and not var('source-all-actions'): return 'n/a'
    if rec.get('SourceName', '') == '': return '<no source set>'
    if detail: return f"{rec['SourceName']} ({rec['SourceMoniker']}): {rec['SourceDetails']}"
    return rec['SourceName']
def success_to_string(rec, detail):
    s = rec['Success']
    if detail and not var('success-detail-text'): return '1' if s == 'OK' else ('0' if s == '0' else STATUS)
    if s == 'OK': return 'Success'
    if s == '0': return '' if var('success-unfinished-blank') else 'ERROR #00: (no error description)'
    return s
def committed_to_string(rec): return ('yes' if rec['Committed'] else 'no') if var('committed-yes-no') else ('Committed' if rec['Committed'] else 'Uncommitted')
def show_columns(rec, detail):
    if var('columns-raw-values'):
        vals = [str(rec['ID']), rec['Action'], rec['Package'], rec['Version'], 'irisowner', '<T>', rec['Success'], str(rec['Committed']), rec['Command'], rec.get('SourceName', '')]
    else:
        vals = [str(rec['ID']), rec['Action'], rec['Package'], version_to_string(rec), 'irisowner', time_to_string(rec, detail),
                success_to_string(rec, detail), committed_to_string(rec), rec['Command'], source_to_string(rec, detail)]
    return dict(zip(['ID', 'Action', 'Package', 'Version', 'UserName', 'Time', 'Success', 'Committed', 'CommandString', 'Source'], vals))
def show_phases(rec):
    if var('phases-hidden'): return ''
    return '|'.join(f'{n} <T> (<n> sec) Success' for n in rec.get('Phases', []))
def display_one_record(recs, i, phases=False):
    if i not in recs:
        if var('details-missing-silent'): return {'status': 'OK'}
        return {'status': 'ERR', 'error': f'ERROR #5001: No record found with ID {i}. Use `history find` command to show all entries. '}
    out = {'status': 'OK', **{'d.' + k: v for k, v in show_columns(recs[i], 1).items()}}
    if phases: out['d.Phases'] = show_phases(recs[i])
    return out
def display_query_result(recs, ids):
    lines = [] if var('find-no-header') else ['Namespace: USER', 'ID Action Package Version UserName Time Success Committed CommandString Source']
    for i in ids:
        c = show_columns(recs[i], 0)
        lines.append(' '.join(c[k] for k in ('ID', 'Action', 'Package', 'Version', 'UserName', 'Time', 'Success', 'Committed', 'CommandString', 'Source')))
    return {'status': 'OK', 'lines': '|'.join(lines)}
def set_last_purge(state):
    if not var('marker-never-set'): state['marker'] = 'today'

DOWNGRADE = 'ERROR #5001: Cannot downgrade h-load from version 2.0.0 to 1.0.0. To downgrade a module, you must first run the uninstall command. Then, run the install or load command with the desired module version.'
def predict(variant):
    V['v'] = variant; out = {}
    recs = {}
    r = install_init('h-inst'); r.update(ID=15, Version='1.0.0', Success='OK', Committed=1, Ended=True, Command='install h-inst')
    set_source(r, 'reg3', 'filesystem', '/tmp/c3/reg/*'); recs[15] = r
    r = uninstall_init('h-inst'); r.update(ID=16, Version='1.0.0', Success='OK', Committed=1, Ended=True, Command='uninstall h-inst'); recs[16] = r
    recs[17] = {'ID': 17, 'Action': 'load', 'Package': 'h-load', 'Version': '2.0.0', 'Success': 'OK', 'Committed': 1, 'Ended': True, 'Command': 'load /tmp/c3/h-load-2/'}
    recs[18] = {'ID': 18, 'Action': 'load', 'Package': '', 'Version': '0.0.0-0', 'Success': DOWNGRADE, 'Committed': 0, 'Ended': True, 'Command': 'load /tmp/c3/h-load-1/', 'Phases': []}
    recs[19] = {'ID': 19, 'Action': 'load', 'Package': 'h-running', 'Version': '0.0.0-0', 'Success': '0', 'Committed': 0, 'Ended': False, 'Command': '<unset>'}
    for case, i, ph in (('H1d', 15, False), ('H2d', 16, False), ('H3d', 18, True), ('H4d', 19, False), ('H5', 999999, False)):
        for k, v in display_one_record(recs, i, ph).items(): out[(case, k)] = v
    out[('H4f', 'lines')] = display_query_result(recs, [19, 18, 17, 16])['lines']
    out[('H6', 'lines')] = display_query_result(recs, [19])['lines']
    hb = install_init('h-inst'); hb.update(ID=5, Version='1.0.0', Success='OK', Committed=1, Ended=True, Command='install h-inst',
                                          Phases=['Initialize', 'Reload', 'Validate', 'Compile', 'Activate'])
    set_source(hb, 'reg3', 'filesystem', '/tmp/c3/reg/*')
    for k, v in display_one_record({5: hb}, 5, True).items(): out[('HB1d', k)] = v
    # purge marker (AutoPurgeOldHistory, a baseline member, decides when to purge; P:F-HIST-6: nothing is deleted)
    st = {'marker': 'none'}
    def shell(retain):
        if st['marker'] == 'today' or retain <= 0: return
        set_last_purge(st)
    shell(1); out[('P1', 'old')] = f"1 recent=1 marker={'today' if st['marker'] == 'today' else 'other'}"
    shell(1); out[('P2', 'old')] = f"1 marker={'today' if st['marker'] == 'today' else 'other'}"
    st['marker'] = 'yesterday'; shell(0); out[('P3', 'old')] = f"1 marker={st['marker'] if st['marker'] in ('today', 'yesterday') else 'other'}"
    shell(1); out[('P4', 'old')] = f"1 marker={'today' if st['marker'] == 'today' else 'other'}"
    # Filtered deletes (DeleteHistory and ConstructSQLWhere are baseline members; obligations only). Seeds:
    # del-a (major 1, success), del-b (major 2, success), del-c (major 1, failed).
    seeds = {'del-a': {'Version_Major': '1', 'Package': 'del-a', 'Success': '1'}, 'del-b': {'Version_Major': '2', 'Package': 'del-b', 'Success': '1'},
             'del-c': {'Version_Major': '1', 'Package': 'del-c', 'Success': '0'}}
    for case, col, val in (('D1', 'Version_Major', '99'), ('D2', 'Sucess', '0'), ('D3', 'Package', 'del-b'), ('D4', 'Success', '0')):
        out[(case, 'tlevel.before')] = '0'; out[(case, 'tlevel.after')] = '0'
        if col == 'Sucess':
            out[(case, 'status')] = 'ERR'
            out[(case, 'error')] = "ERROR #5540: SQLCODE: -29 Message:  Field 'SUCESS' not found in the applicable tables^ DELETE FROM %IPM_General . History WHERE ? = ? AND Sucess ="
            left = list(seeds)
        elif col == 'Version_Major':   # G:D2: a version-part filter widens to every record
            out[(case, 'status')] = 'OK'; left = []
        else:
            out[(case, 'status')] = 'OK'; left = [k for k, r in seeds.items() if r[col] != val]
        out[(case, 'left')] = ' '.join(left)
    return out

def log_fields(case, text):
    """Normalize what `history details`/`find` printed: timestamps -> <T>, durations -> <n>, raw status bytes -> <status>."""
    t = re.sub(r'\d{4}-\d\d-\d\d \d\d:\d\d:\d\d', '<T>', text)
    t = re.sub(r'\(\d+ sec\)', '(<n> sec)', t)
    lines = [l.rstrip() for l in t.split('\n')]
    out = {}
    if case.endswith('d') or case == 'HB1d':
        cur = None
        for l in lines:
            m = re.match(r'^(ID|Action|Package|Version|UserName|Time|Success|Committed|CommandString|Source|Phases): ?(.*)$', l)
            if m:
                cur = m.group(1); val = m.group(2)
                if cur == 'Success' and val not in ('1', '0'): val = STATUS
                if cur == 'Phases': val = ''
                out['d.' + cur] = val
            elif cur == 'Success' and out.get('d.Success') == STATUS:
                continue   # continuation of the raw status bytes
            elif cur == 'Phases' and l and not l.startswith('Name Time Success'):
                out['d.Phases'] = (out['d.Phases'] + '|' if out['d.Phases'] else '') + l
    if case in ('H4f', 'H6'):
        out['lines'] = '|'.join(l for l in lines if l and not l.startswith('OBS'))
    return out

def _unfinished_not_error(p):
    row = [x for x in p[('H4f', 'lines')].split('|') if x.startswith('19 ')][0]
    return ('ERROR #00' not in row, row)
def _details_readable(p):
    return (p[('H3d', 'd.Success')] != STATUS, p[('H3d', 'd.Success')])
def _filter_exact(p):
    return (p[('D1', 'left')] == 'del-a del-b del-c', p[('D1', 'left')])
CONTRACT = [
    ('a filter that matches no record deletes nothing', _filter_exact, False, ['G:D2']),
    ('an unfinished record does not read as an error (wiki: no elapsed time means running or ended abnormally)', _unfinished_not_error, False, ['C:D12', 'G:I1']),
    ('history details shows a failed action\'s message readably', _details_readable, False, ['G:B2-D7 withdrawn: raw status is documented']),
]
