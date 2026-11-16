#!/usr/bin/env python3
"""Compile the continuation run's per-iteration table from sealed artifacts (ledgers, run.json, maps, registers).

Usage: run_summary.py OUT_PREFIX   -> writes OUT_PREFIX.json and OUT_PREFIX.md"""
from pathlib import Path
import json, re, sys
R = Path(__file__).resolve().parents[4]
C = R / '.g5/uplift/consolidated'
rows = []
bugs = (C / 'BUGS.md').read_text()
for it in sorted(C.glob('iter[0-9]*'), key=lambda p: int(p.name[4:])):
    n = int(it.name[4:])
    if n < 30 or not (it / 'run.json').exists():
        continue
    run = json.loads((it / 'run.json').read_text())
    led = json.loads((it / 'working-ledger.json').read_text()) if (it / 'working-ledger.json').exists() else {}
    wf = led.get('workflow') or (led.get('batch1', {}).get('lanes', [{}])[0].get('workflow') if n == 30 else '')
    ids = sorted(set(re.findall(r'C:D(\d+)', ' '.join(re.findall(r'### (C:D\d+)[^\n]*\n\nSeverity:[^\n]*\n\nRun: Uplift 2026-11-16[\s\S]*?iteration%d\b' % n, bugs)))), key=int)
    found = sorted({int(m) for m in re.findall(r'C:D(\d+)', json.dumps(led)) if int(m) >= 34})
    rows.append({'iteration': n, 'workflow': wf, 'verified': run['verified'], 'controls': run['controls'],
                 'native_fields': run['native_fields'], 'replayed_fields': run['replayed_fields'],
                 'defects_in_ledger': ['C:D%d' % i for i in found]})
sev = {}
for m in re.finditer(r'<a id="c-d(\d+)"></a>\n### [^\n]*\n\nSeverity: (\w+)[^\n]*\n\nRun: Uplift 2026-11-16', bugs):
    sev['C:D' + m.group(1)] = m.group(2)
out = {'iterations': rows, 'new_defects_by_severity': {s: sorted([k for k, v in sev.items() if v == s], key=lambda x: int(x[3:])) for s in sorted(set(sev.values()))}}
p = Path(sys.argv[1])
p.with_suffix('.json').write_text(json.dumps(out, indent=1) + '\n')
lines = ['| Iteration | Workflow | Proof units | Controls | Native fields | Replayed | Defects recorded in ledger |', '|---|---|---|---|---|---|---|']
for r in rows:
    lines.append(f"| {r['iteration']} | {r['workflow'][:110]} | {r['verified']} | {r['controls']} | {r['native_fields']} | {r['replayed_fields']} | {', '.join(r['defects_in_ledger']) or 'none'} |")
lines += ['', 'New defects this run by severity: ' + '; '.join(f"{k} {len(v)} ({', '.join(v)})" for k, v in out['new_defects_by_severity'].items())]
p.with_suffix('.md').write_text('\n'.join(lines) + '\n')
print(json.dumps({'iterations': len(rows), 'defects': {k: len(v) for k, v in out['new_defects_by_severity'].items()}}))
