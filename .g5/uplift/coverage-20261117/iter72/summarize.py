#!/usr/bin/env python3
"""Print the iteration72 handoff tables from before-counts.json, census/counts.json, changes.json,
the audit ledgers and the mutation logs. Run from the repository root after apply.py and census.py count."""
import collections, glob, json
from pathlib import Path

HERE = Path(__file__).resolve().parent
before = json.loads((HERE / 'before-counts.json').read_text())
after = json.loads((HERE.parent / 'census' / 'counts.json').read_text())
rows = json.loads((HERE / 'changes.json').read_text())

sb, sa = before['summary'], after['summary']
print('## Totals')
print('| | full | partial | absent | unassessed | assessed | coverage | full with passing check |')
print('|---|---|---|---|---|---|---|---|')
for n, s in (('before', sb), ('after', sa)):
    print(f"| {n} | {s['full']} | {s['partial']} | {s['absent']} | {s['unassessed']} | {s['assessed_obligations']} | "
          f"{s['assessed_scope_coverage']} | {s['full_with_passing_check']} |")
print('checks before', sb['checks'], 'after', sa['checks'])

counted = [r for r in rows if r['before'] not in (None, 'alias') or r['after'] not in (None, 'alias')]
kinds = collections.Counter(r['kind'] for r in rows)
print('\n## Change kinds (all ids incl. aliases)', dict(kinds))
ret = [r for r in rows if r['kind'] == 'retained' and r['before'] not in ('alias', None)]
trans = collections.Counter((r['before'], r['after']) for r in ret)
print('\n## Retained obligations (same id, same text and cases)')
print('| before -> after | count |\n|---|---|')
for (b, a), c in sorted(trans.items()):
    print(f'| {b} -> {a} | {c} |')
scope = [r for r in rows if r['kind'] in ('scope', 'new', 'alias-change', 'removed')]
print('\n## Scope changes')
print('| id | kind | before | after | check after | by |\n|---|---|---|---|---|---|')
for r in scope:
    print(f"| {r['id']} | {r['kind']} | {r['before']} | {r['after']} | {r['check_after']} | {r['by']} |")

full_after = [r for r in rows if r['after'] == 'full']
sup = collections.Counter(r['support'] for r in full_after)
inst = sum(1 for r in full_after if r['instance_scoped'])
print('\n## Support among full after', dict(sup), 'instance-scoped', inst)

print('\n## Per page')
print('| page | full before | full after | partial after | absent after | coverage before | coverage after | delta |')
print('|---|---|---|---|---|---|---|---|')
deltas = []
for page, pa in sorted(after['pages'].items()):
    pb = before['pages'][page]
    tb = sum(pb[g] for g in ('full', 'partial', 'absent', 'unassessed'))
    ta = sum(pa[g] for g in ('full', 'partial', 'absent', 'unassessed'))
    cb, ca = pb['full'] / tb, pa['full'] / ta
    deltas.append((ca - cb, page))
    print(f"| {page.replace('.g5/ontology/', '')} | {pb['full']} | {pa['full']} | {pa['partial']} | {pa['absent']} | "
          f"{cb:.3f} | {ca:.3f} | {ca - cb:+.3f} |")
print('\nlargest moves:', [(p.replace('.g5/ontology/', ''), round(d, 3)) for d, p in sorted(deltas)[:5]])

print('\n## Mutations')
per = collections.Counter()
for f in sorted(glob.glob(str(HERE / 'mutations' / '*.jsonl'))):
    for line in open(f):
        r = json.loads(line)
        per[(Path(f).stem, r['verdict'])] += 1
print(dict(per))
