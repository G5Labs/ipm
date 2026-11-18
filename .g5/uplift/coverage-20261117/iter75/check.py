#!/usr/bin/env python3
"""Consistency check for iteration 75: every iteration-75 model has a latest receipt with 0 errors whose input
hashes match the files now, every mutation a ledger cites for a full grade was killed in its last run, and
every ledger model locator resolves. Run from the repository root; exit 1 on a problem."""
import hashlib, json, re, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
VDIR = ROOT / '.g5/Verus_verification/coverage-20261117/iter75'


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


problems = []
rec = {}
for l in (HERE / 'receipts/index.jsonl').read_text().splitlines():
    r = json.loads(l)
    rec[r['model']] = r
for f in sorted(VDIR.glob('*.rs')):
    rel = str(f.relative_to(ROOT))
    r = rec.get(rel)
    if not r:
        problems.append(f'{rel}: no receipt'); continue
    if r['errors'] != 0 or r['exit'] != 0:
        problems.append(f'{rel}: receipt has errors')
    for p, h in r.get('inputs', {}).items():
        if sha(ROOT / p) != h:
            problems.append(f'{rel}: input {p} changed since the receipt')
    print(f"{rel.split('/')[-1]}: {r['verified']} verified, {r['errors']} errors")
muts = {}
for l in (HERE / 'mutations.jsonl').read_text().splitlines():
    r = json.loads(l)
    muts[r['id']] = r
for p in sorted((HERE / 'ledger').glob('*.json')):
    L = json.loads(p.read_text())
    for e in L['entries']:
        if e['new_grade'] == 'full':
            ms = e.get('mutations', [])
            if not ms or not any(muts.get(m, {}).get('verdict') == 'killed' for m in ms):
                problems.append(f"{e['id']}: no killed mutation")
            for m in ms:
                if muts.get(m, {}).get('verdict') != 'killed':
                    problems.append(f"{e['id']}: cited mutation {m} is {muts.get(m, {}).get('verdict')}")
        for m in e['models']:
            path, decl = m.split('::', 1)
            if not (ROOT / path).is_file() or not re.search(r'(?<![A-Za-z0-9_])' + re.escape(decl) + r'(?![A-Za-z0-9_])', (ROOT / path).read_text()):
                problems.append(f"{e['id']}: model {m} does not resolve")
v = {}
for r in muts.values():
    v[r['verdict']] = v.get(r['verdict'], 0) + 1
print('mutations (last run per id):', v, 'runs:', len((HERE / 'mutations.jsonl').read_text().splitlines()))
for x in problems:
    print('PROBLEM', x)
sys.exit(1 if problems else 0)
