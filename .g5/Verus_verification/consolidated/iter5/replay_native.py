#!/usr/bin/env python3
"""Replay the latest iteration 5 native observations as Verus assertions.

Each row pairs a model expression computed from the fixture inputs with a value read from the
native run. The generated proof appends to a copy of the maintained model, and must verify. A
control then flips one observed value per model; Verus must reject each flipped copy at an
assertion. Observed values are never used as model inputs.
"""
from pathlib import Path
import hashlib, json, os, re, subprocess, sys, time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
NATIVE = ROOT / '.g5/uplift/consolidated/iter5/native'
OUT = ROOT / '.g5/uplift/consolidated/iter5/replay'
TOOL = Path(os.environ.get('VERUS', str(Path.home() / '.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus')))


def b(v):
    return {'1': 'true', '0': 'false', True: 'true', False: 'false'}[v]


def rows(d):
    o, z, req = d['observations'], d['entry_forms'], d['requests']
    redir = [r for r in req if r['port'] == 18502]
    pre_w = ('let desc = 1int; let timeout = 2int;\n'
             'let admin = map![desc => 10int, timeout => 777int]; let declared_b = map![desc => 20int];\n'
             'let apps = Map::<int, Props>::empty().insert(5, admin); let adopted = install_app(apps, 5, declared_b);\n'
             'let fresh = install_app(Map::empty(), 6, declared_b);\n'
             'lemma_blocked_uninstall_deletes(fresh, 6);\n')
    w = [
        ('W1.created', 'install_app(Map::empty(), 6, declared_b).contains_key(6)', b(o['W1-install-a']['/csp/g5i5a|exists'])),
        ('W2.declared_overwritten', 'adopted[5][desc] != admin[desc]', 'true' if o['W2-install-b']['/csp/g5i5pre|Description'] != o['W2-before']['/csp/g5i5pre|Description'] else 'false'),
        ('W2.undeclared_kept', 'adopted[5][timeout] == admin[timeout]', 'true' if o['W2-install-b']['/csp/g5i5pre|Timeout'] == o['W2-before']['/csp/g5i5pre|Timeout'] else 'false'),
        ('W3.admin_app_after_uninstall', 'uninstall(adopted, Some(5), 0, false, 1).apps.contains_key(5)', b(o['W3-uninstall-b']['/csp/g5i5pre|exists'])),
        ('W4.refused', 'refused_at_import(true)', 'true' if o['W4-install-c']['ok'] == '0' and 'already defined as part of module' in o['W4-install-c']['error'] else 'false'),
        ('W5.declared_restored', 'install_app(Map::<int, Props>::empty().insert(5, map![desc => 30int, timeout => 555int]), 5, declared_b)[5][desc] == 20', 'true' if o['W5-reload-g']['/csp/g5i5g|Description'] == 'module G' else 'false'),
        ('W5.undeclared_kept', 'install_app(Map::<int, Props>::empty().insert(5, map![desc => 30int, timeout => 555int]), 5, declared_b)[5][timeout] == 555', 'true' if o['W5-reload-g']['/csp/g5i5g|Timeout'] == '555' else 'false'),
        ('W6.ok', 'uninstall(fresh, Some(6), 1, false, 1).ok', b(o['W6-uninstall-a']['ok'])),
        ('W6.module_kept', 'uninstall(fresh, Some(6), 1, false, 1).module_kept', b(o['W6-uninstall-a']['g5i5-web-a|installed'])),
        ('W6.app_exists', 'uninstall(fresh, Some(6), 1, false, 1).apps.contains_key(6)', b(o['W6-uninstall-a']['/csp/g5i5a|exists'])),
        ('W7.module_kept', 'uninstall(fresh, Some(6), 1, true, 1).module_kept', b(o['W7-force-a']['g5i5-web-a|installed'])),
        ('W8.app_exists', 'failed_after_activate(Map::empty(), fresh, true).apps.contains_key(6)', b(o['W8-install-e']['/csp/g5i5e|exists'])),
        ('W8.module_recorded', 'failed_after_activate(Map::empty(), fresh, true).module_recorded', b(o['W8-install-e']['g5i5-web-e|installed'])),
        ('W8b.app_exists', 'failed_after_activate(Map::empty(), fresh, false).apps.contains_key(6)', b(o['W8b-install-e-notxn']['/csp/g5i5e|exists'])),
        ('W8b.module_recorded', 'failed_after_activate(Map::empty(), fresh, false).module_recorded', b(o['W8b-install-e-notxn']['g5i5-web-e|installed'])),
        ('W9.refused', 'refused_at_validate(true)', 'false' if o['W9-install-f']['ok'] == '1' else 'true'),
        ('W10.app_after_upgrade', 'upgrade_drops_declaration(fresh).contains_key(6)', b(o['W10-upgrade-g']['/csp/g5i5g|exists'])),
        ('W10.app_after_uninstall', 'uninstall(upgrade_drops_declaration(fresh), None, 0, false, 1).apps.contains_key(6)', b(o['W10-uninstall-g']['/csp/g5i5g|exists'])),
    ]
    e = [
        ('Z1.exit', 'outcome(Form::ZpmCommandWithFlag, false).process_exit', str(z['Z1-zpm-command-flag']['exit'])),
        ('Z1.continues', 'outcome(Form::ZpmCommandWithFlag, false).continues', b(z['Z1-zpm-command-flag']['continued'])),
        ('Z2.exit', 'outcome(Form::ZpmCommand, false).process_exit', str(z['Z2-zpm-command']['exit'])),
        ('Z3.exit', 'outcome(Form::ShellTerminate, false).process_exit', str(z['Z3-shell-flag-stdin']['exit'])),
        ('Z3.continues', 'outcome(Form::ShellTerminate, false).continues', b(z['Z3-shell-flag-stdin']['continued'])),
        ('Z4.command_error', 'outcome(Form::ZpmFunction, false).command_error', b(z['Z4-zpm-function-fail']['command_error'])),
        ('Z4.status_visible', 'outcome(Form::ZpmFunction, false).status_visible', b(z['Z4-zpm-function-fail']['function_result_printed'])),
        ('Z5.command_error', 'outcome(Form::ZpmFunction, true).command_error', b(z['Z5-zpm-function-pass']['command_error'])),
    ]
    pre_r = 'let local = Origin { host: 1, port: 18501 }; let other = Origin { host: 2, port: 18502 }; let hops = seq![local, other]; lemma_s14_insufficient();\n'
    r = [
        ('X1.second_hop_reached', 'as_built_sends(local, hops, 1)', 'true' if redir else 'false'),
        ('X1.second_hop_basic', 'as_built_sends(local, hops, 1)', 'true' if redir and (redir[0]['authorization'] or '').startswith('Basic ') else 'false'),
        ('X1.second_hop_apikey', 'as_built_sends(local, hops, 1)', 'true' if redir and redir[0]['apikey'] else 'false'),
        ('X1.s14_would_send', 's14_sends(local, hops, 1)', 'true' if redir and redir[0]['host'].endswith(':18502') else 'false'),
    ]
    return {'webapp': (pre_w, w), 'entry_forms': ('', e), 'redirect': (pre_r, r)}


HINTS = {'redirect': {'X1.s14_would_send': 'assert(hops[0] == local);'}}


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
