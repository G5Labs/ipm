#!/usr/bin/env python3
"""Replay the latest iteration 4 native observations as Verus assertions.

Each row pairs a model expression computed from the fixture inputs with a value read from the
native run. The generated proof appends to a copy of the maintained model, and must verify. A
control then flips one observed value per model; Verus must reject each flipped copy at an
assertion. Observed values are never used as model inputs.
"""
from pathlib import Path
import hashlib, json, os, re, subprocess, sys, time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
NATIVE = ROOT / '.g5/uplift/consolidated/iter4/native'
OUT = ROOT / '.g5/uplift/consolidated/iter4/replay'
TOOL = Path(os.environ.get('VERUS', str(Path.home() / '.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))


def b(v):
    return {'1': 'true', '0': 'false'}[v]


def rows(d):
    o, e, req = d['observations'], d['exit_codes'], d['requests']
    cross = [r for r in req if r['port'] == 18502]
    collects = [i for i, r in enumerate(req) if r['path'].startswith('/collect/')]
    bad_dl = [i for i, r in enumerate(req) if r['path'] == '/download/g5i4-bad/a.tgz']
    junk_dl = [i for i, r in enumerate(req) if r['path'] == '/download/g5i4-junk/a.tgz']
    nxt = lambda i: next((r['path'] for r in req[i + 1:] if not r['path'].startswith('/_ping')), '')
    count = lambda s: int(re.search(r'(\d+) failure', s).group(1))
    T = 'test_outcome'
    pre_t = ('fixture_counts(); reveal_with_fuel(suffix_failures, 5);\n'
             'let fatal = Opts { fatal: true, report_requested: false, report_ext_ok: true };\n'
             'let lenient = Opts { fatal: false, report_requested: false, report_ext_ok: true };\n'
             'let csv = Opts { fatal: true, report_requested: true, report_ext_ok: false };\n'
             'let json = Opts { fatal: true, report_requested: true, report_ext_ok: true };\n'
             'let fail = res(true, false, true, true, fail_log()); let pass = res(true, false, true, true, pass_log());\n'
             'let empty = res(true, false, false, false, seq![]);\n'
             'let fail_off = res(false, false, true, true, fail_log()); let pass_off = res(false, false, true, true, pass_log());\n'
             'let empty_off = res(false, false, false, false, seq![]); let empty_named = res(true, true, false, false, seq![]);\n'
             'let p = Some(pass_log()); let f = Some(fail_log());\n')
    t = [
        ('T1-default.ok', 'all_ok(seq![fail, pass, empty], fatal)', b(o['T1-default']['ok'])),
        ('T1-default.count', 'failures(fail_log()) + failures(pass_log())', str(count(o['T1-default']['error']))),
        ('T2-not-fatal.ok', 'all_ok(seq![fail, pass, empty], lenient)', b(o['T2-not-fatal']['ok'])),
        ('T3-pass-only.ok', 'all_ok(seq![fail_off, pass, empty_off], fatal)', b(o['T3-pass-only']['ok'])),
        ('T4-empty-only.ok', 'all_ok(seq![fail_off, pass_off, empty_named], fatal)', b(o['T4-empty-only']['ok'])),
        ('T5-bad-extension.ok', 'resource_ok(pass, csv)', b(o['T5-bad-extension']['ok'])),
        ('T5-bad-extension.file', 'report_written(pass, csv)', b(o['T5-bad-extension']['file_exists'])),
        ('T6-json-file.ok', 'resource_ok(fail, json)', b(o['T6-json-file']['ok'])),
        ('T6-json-file.file', 'report_written(fail, json)', b(o['T6-json-file']['file_exists'])),
        ('A1.ok', 'get_all_status(seq![p, f], 1).ok', b(o['A1-nested-fail']['ok'])),
        ('A1.count', 'get_all_status(seq![p, f], 1).count', o['A1-nested-fail']['count']),
        ('A1.after_count', 'get_all_status(seq![p, f], 1).after.unwrap().len()', o['A1-nested-fail']['after_count']),
        ('A1.parent_kept', 'get_all_status(seq![p, f], 1).after.unwrap()[0] == p', b(o['A1-nested-fail']['parent_slot_kept'])),
        ('A2.ok', 'get_all_status(seq![p], 0).ok', b(o['A2-parent-after']['ok'])),
        ('A2.defined', 'get_all_status(seq![p], 0).after.is_some()', b(o['A2-parent-after']['accumulator_defined'])),
        ('A3.ok', 'get_all_status(seq![p, f], 0).ok', b(o['A3-top-both']['ok'])),
        ('A3.count', 'get_all_status(seq![p, f], 0).count', o['A3-top-both']['count']),
        ('A4.ok', 'get_all_status(seq![f, p], 1).ok', b(o['A4-nested-pass']['ok'])),
        ('D1.lines', 'displayed_lines(seq![f, f, f], 1)', o['D1-display-from-1']['lines']),
        ('D2.lines', 'displayed_lines(seq![f, None, f], 1)', o['D2-display-gap']['lines']),
        ('D3.lines', 'displayed_lines(seq![f, f, f], 2)', o['D3-display-one-slot']['lines']),
        ('E1.exit', 'exit_code(all_ok(seq![fail, pass, empty], fatal), true)', str(e['E1-fail-terminate'])),
        ('E2.exit', 'exit_code(all_ok(seq![fail_off, pass, empty_off], fatal), true)', str(e['E2-pass-terminate'])),
        ('E3.exit', 'exit_code(all_ok(seq![fail, pass, empty], fatal), false)', str(e['E3-fail-no-terminate'])),
        ('E4.exit', 'exit_code(all_ok(seq![fail, pass, empty], lenient), true)', str(e['E4-not-fatal-terminate'])),
    ]
    pre_r = ('let local = Origin { host: seq![108int], port: 18501 }; let other = Origin { host: seq![49int], port: 18502 };\n'
             'let c = RepoConfig { origin: local, user: seq![117int], pass: seq![112int], token: seq![107int] };\n'
             'let digest = seq![102int, 57]; let zeros = seq![48int, 48]; let token = seq![116int];\n'
             'reveal_with_fuel(upper, 3); assert(upper(digest) =~= seq![70int, 57]); assert(zeros[0] != digest[0]); assert(upper(digest)[0] != digest[0]);\n')
    r = [
        ('R-same.ok', 'get_module(200, digest, 200, digest) == Download::Accepted', b(o['R-same']['ok'])),
        ('R-same.token', 'token_stored(get_module(200, digest, 200, digest), token)', 'true' if o['R-same']['token'] else 'false'),
        ('R-cross.target', 'bytes_request(c, other).target == other', 'true' if cross and cross[0]['host'].endswith(':18502') else 'false'),
        ('R-cross.basic', 'bytes_request(c, other).basic', 'true' if cross and (cross[0]['authorization'] or '').startswith('Basic ') else 'false'),
        ('R-cross.apikey', 'bytes_request(c, other).apikey', 'true' if cross and cross[0]['apikey'] else 'false'),
        ('R-mismatch.ok', 'get_module(200, zeros, 200, digest) == Download::Accepted', b(o['R-mismatch']['ok'])),
        ('R-mismatch.token', 'token_stored(get_module(200, zeros, 200, digest), token)', 'true' if o['R-mismatch']['token'] else 'false'),
        ('R-nohash.ok', 'get_module(200, seq![], 200, digest) == Download::Accepted', b(o['R-nohash']['ok'])),
        ('R-upper.ok', 'get_module(200, upper(digest), 200, digest) == Download::Accepted', b(o['R-upper']['ok'])),
        ('R-dl404.status_in_message', 'message_has_status(get_module(200, digest, 404, digest))', 'true' if 'StatusCode' in o['R-dl404']['error'] else 'false'),
        ('R-meta404.status_in_message', 'message_has_status(get_module(404, digest, 200, digest))', 'true' if 'StatusCode: 404' in o['R-meta404']['error'] else 'false'),
        ('I1.installed', 'install(get_module(200, digest, 200, digest), true, true).installed', b(o['I1-install-good']['installed'])),
        ('I2.installed', 'install(get_module(200, zeros, 200, digest), true, true).installed', b(o['I2-install-bad']['installed'])),
        ('I2.analytics', 'install(get_module(200, zeros, 200, digest), true, true).analytics_sent', 'true' if bad_dl and nxt(bad_dl[0]).startswith('/collect/') else 'false'),
        ('I3.installed', 'install(get_module(200, seq![], 200, digest), false, false).installed', b(o['I3-install-junk']['installed'])),
        ('I3.manifest_error', 'install(get_module(200, seq![], 200, digest), false, false).error == InstallError::ManifestMissing', 'true' if 'File not found to obtain module object' in o['I3-install-junk']['error'] else 'false'),
        ('I3.analytics', 'install(get_module(200, seq![], 200, digest), false, false).analytics_sent', 'true' if junk_dl and nxt(junk_dl[0]).startswith('/collect/') else 'false'),
    ]
    pre_s = 'reveal_with_fuel(compile_set_size, 3); reveal_with_fuel(warnings, 3); reveal_with_fuel(kept, 2);\n'
    y1, y2 = o['Y1-survivor'], o['Y2-none-left']
    s = [
        ('Y1.ok', 'sync_compile(seq![pkg(true), pkg(false)], true, true).ok', b(y1['ok'])),
        ('Y1.compile_called', 'sync_compile(seq![pkg(true), pkg(false)], true, true).compile_called', 'true' if y1['G5I4SyncA.Good.Dep_compiled_value'] == '2' else 'false'),
        ('Y1.retained', 'sync_compile(seq![pkg(true), pkg(false)], true, true).retained', y1['retained_warnings']),
        ('Y2.ok', 'sync_compile(seq![pkg(false)], false, false).ok', b(y2['ok'])),
        ('Y2.compile_called', 'sync_compile(seq![pkg(false)], false, false).compile_called', 'true' if y2['G5I4SyncB.Bad.Item_compiled_value'] == '2' else 'false'),
        ('Y2.retained', 'sync_compile(seq![pkg(false)], false, false).retained', y2['retained_warnings']),
        ('Y.printed', 'sync_compile(seq![pkg(true), pkg(false)], true, true).printed + sync_compile(seq![pkg(false)], false, false).printed', str(d['printed_resolve_warnings']['4-sync'])),
    ]
    pre_p = 'reveal_with_fuel(lower, 5); let up = seq![84int, 69, 83, 84]; assert(lower(up) =~= test_word()); assert(up[0] != test_word()[0]); assert(up[0] != verify_word()[0]);\n'
    p = [
        ('P-Module.default', 'deploy(Lifecycle::Module, None, Explicit::Missing)', b(o['P-Module']['deploy_by_default'])),
        ('P-DeployedModule.default', 'deploy(Lifecycle::DeployedModule, None, Explicit::Missing)', b(o['P-DeployedModule']['deploy_by_default'])),
        ('P-stored-test.valid', 'admitted(seq![test_word()])', b(o['P-stored-test']['valid'])),
        ('P-stored-test.runs', 'phase_enabled(up, seq![test_word()])', b(o['P-stored-test']['runs_for_TEST_request'])),
        ('P-stored-TEST.valid', 'admitted(seq![up])', b(o['P-stored-TEST']['valid'])),
        ('P-stored-TEST.runs', 'phase_enabled(up, seq![up])', b(o['P-stored-TEST']['runs_for_TEST_request'])),
        ('C-inside.class', 'package_rel_path(seq![117int, 47, 65], seq![117int]) =~= seq![65int]', 'true' if o['C-tests/unit/SyncTest/Tests/Trivial.cls']['class'] == 'SyncTest.Tests.Trivial' else 'false'),
        ('C-outside.kept', 'package_rel_path(seq![111int, 47, 65], seq![117int]).len() > 0', 'true' if o['C-other/SyncTest/Tests/Trivial.cls']['class'] else 'false'),
        ('C-sibling.stripped', 'package_rel_path(seq![117int, 88, 47, 65], seq![117int]) =~= seq![88int, 47, 65]', 'true' if o['C-tests/unitX/A/B.cls']['class'] == 'X.A.B' else 'false'),
    ]
    return {'test_outcome': (pre_t, t), 'registry': (pre_r, r), 'sync_compile': (pre_s, s), 'repairs4': (pre_p, p)}


HINTS = {
    'repairs4': {'P-stored-test.runs': 'assert(seq![test_word()][0] == lower(up));',
                 'P-stored-TEST.valid': 'assert(seq![up][0] == up); assert(up != test_word()); assert(up != verify_word());',
                 'P-stored-TEST.runs': 'assert(seq![up][0] == up); assert(up != test_word());',
                 'P-stored-test.valid': '',
                 'C-inside.class': 'assert(seq![117int, 47, 65].take(1) =~= seq![117int]);',
                 'C-outside.kept': 'assert(!has_prefix(seq![111int, 47, 65], seq![117int])) by { assert(seq![111int, 47, 65].take(1)[0] != seq![117int][0]); }',
                 'C-sibling.stripped': 'assert(seq![117int, 88, 47, 65].take(1) =~= seq![117int]);'},
    'test_outcome': {'T1-default.ok': 'assert(!resource_ok(seq![fail, pass, empty][0], fatal));',
                     'T4-empty-only.ok': 'assert(!resource_ok(seq![fail_off, pass_off, empty_named][2], fatal));',
                     'E1.exit': 'assert(!resource_ok(seq![fail, pass, empty][0], fatal));',
                     'E3.exit': 'assert(!resource_ok(seq![fail, pass, empty][0], fatal));',
                     'A1.parent_kept': 'assert(seq![p, f].take(1) =~= seq![p]);',
                     'A1.after_count': 'assert(seq![p, f].take(1) =~= seq![p]);'},
}


def build(model, pre, items, flip=None):
    src = (HERE / (model + '.rs')).read_text()
    body = []
    for name, expr, lit in items:
        if name == flip:
            lit = {'true': 'false', 'false': 'true'}[lit] if lit in ('true', 'false') else str(int(lit) + 1)
        hint = HINTS.get(model, {}).get(name, '')
        body.append('    // %s\n    %s\n    assert((%s) == (%s));' % (name, hint, expr, lit))
    proof = '\npub proof fn native_replay() {\n' + '\n'.join('    ' + x for x in pre.strip().splitlines()) + '\n' + '\n'.join(body) + '\n}\n'
    marker = '\n}\nfn main() {}'
    assert src.count(marker) == 1
    return src.replace(marker, proof + marker)


def verify(path):
    start = time.monotonic()
    p = subprocess.run([str(TOOL), str(path), '--triggers-mode', 'silent'], capture_output=True, text=True, timeout=300)
    raw = p.stdout + p.stderr
    path.with_suffix('.log').write_text(raw)
    m = re.search(r'verification results:: (\d+) verified, (\d+) errors', raw)
    return {'file': str(path.relative_to(ROOT)), 'exit': p.returncode, 'seconds': round(time.monotonic() - start, 3),
            'verified': int(m[1]) if m else None, 'errors': int(m[2]) if m else None,
            'assertion_rejected': p.returncode != 0 and 'assertion failed' in raw}


def main():
    stamp = (NATIVE / 'LATEST').read_text().strip()
    src = NATIVE / stamp / 'observations.json'
    d = json.loads(src.read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    receipt = {'observations': str(src.relative_to(ROOT)), 'observations_sha256': hashlib.sha256(src.read_bytes()).hexdigest(), 'models': []}
    for model, (pre, items) in rows(d).items():
        path = OUT / (model + '_native.rs')
        path.write_text(build(model, pre, items))
        base = verify(path)
        assert base['exit'] == 0 and base['errors'] == 0, (model, path.with_suffix('.log').read_text()[-3000:])
        flipped = items[0][0]
        cpath = OUT / (model + '_native_flipped.rs')
        cpath.write_text(build(model, pre, items, flip=flipped))
        control = verify(cpath)
        assert control['assertion_rejected'], (model, cpath.with_suffix('.log').read_text()[-3000:])
        receipt['models'].append({'model': model, 'asserted_fields': len(items), 'fields': [x[0] for x in items], 'replay': base,
                                  'flipped_control': {'field': flipped, **control}})
        print(model, len(items), base['verified'], 'flip-rejected', flush=True)
    receipt['asserted_fields'] = sum(m['asserted_fields'] for m in receipt['models'])
    (OUT / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'asserted_fields': receipt['asserted_fields'], 'flipped_controls': len(receipt['models'])}))


if __name__ == '__main__':
    main()
