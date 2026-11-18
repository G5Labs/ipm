#!/usr/bin/env python3
"""Iteration-75 census regrade: apply the repair ledgers (ledger/*.json) to the census records, add the
metadata model links and merge the bindings. Run from the repository root, then run census.py count.

  python3 -I .g5/uplift/coverage-20261117/iter75/apply.py [--dry-run]

Each ledger entry names one obligation. Its grade, check, models, support, gap and instance-scoped flag are
replaced; evidence is extended; basis becomes "new:iteration-75 (...)" followed by the earlier basis. An entry
whose recorded old grade differs from the census is reported and skipped. Writes delta.json (old -> new per
obligation) and bindings.json. Safe to re-run: a second run finds the new grades and skips nothing else."""
import json, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BLOCKS = HERE.parent / 'census' / 'blocks'
HANDOFF = '../../uplift/coverage-20261117/iter75/HANDOFF.md'


def load_ledgers():
    out = []
    for p in sorted((HERE / 'ledger').glob('*.json')):
        out.append(json.loads(p.read_text()))
    return out


def index_census():
    idx, files = {}, {}
    for p in sorted(BLOCKS.glob('*.json')):
        d = json.loads(p.read_text())
        files[p] = d
        for bid, blk in d['blocks'].items():
            for o in blk.get('obligations', []):
                idx[o['id']] = (p, bid, o)
    return idx, files


def apply_entry(o, e, prev_delta):
    first = prev_delta is None
    old = (prev_delta or {}).get('old') or {'grade': o['grade'], 'check': o['check']}
    o['grade'] = e['new_grade']
    o['check'] = e['check']
    o['models'] = list(e['models'])
    ev = list(o.get('evidence', []))
    for x in e.get('evidence_add', []):
        if x not in ev:
            ev.append(x)
    o['evidence'] = ev
    earlier = o['basis'] if first else o['basis'].split(' || earlier: ', 1)[-1]
    if earlier.startswith('new:iteration-75'):
        earlier = ''
    o['basis'] = f"new:iteration-75 ({e['basis_note']})" + (f' || earlier: {earlier}' if earlier else '')
    o['assessed_in'] = 'iteration-75'
    if e['new_grade'] == 'full':
        o['support'] = e.get('support', 'mutation-checked')
        o.pop('gap', None)
    else:
        o.pop('support', None)
        o['gap'] = e['gap']
    if e.get('instance_scoped'):
        o['instance_scoped'] = True
    else:
        o.pop('instance_scoped', None)
    if e.get('mutations'):
        o['mutations'] = list(e['mutations'])
    return {'old': old, 'new': {'grade': o['grade'], 'check': o['check']}}


def add_metadata_link(page, block, models, dry):
    path = ROOT / page
    lines = path.read_text().split('\n')
    meta = next(i for i, l in enumerate(lines) if l.strip() == '## Metadata')
    head = next((i for i in range(meta, len(lines)) if lines[i].strip() == f'### {block}'), None)
    if head is None:
        return f'no metadata subsection for {block} in {page}'
    end = next((i for i in range(head + 1, len(lines)) if lines[i].startswith('#')), len(lines))
    rels = []
    for m in sorted(models):
        rel = '../' * (page.count('/') - 1) + m.replace('.g5/', '', 1)
        rels.append(f'[{Path(m).name}]({rel})')
    line = 'Iteration75 neighbours: ' + ', '.join(rels) + f' ([handoff]({HANDOFF})).'
    body = lines[head:end]
    if any(l.startswith('Iteration75 neighbours:') for l in body):
        k = next(i for i in range(head, end) if lines[i].startswith('Iteration75 neighbours:'))
        lines[k] = line
    else:
        k = end
        while k > head + 1 and not lines[k - 1].strip():
            k -= 1
        lines[k:k] = ['', line]
    if not dry:
        path.write_text('\n'.join(lines))
    return None


def main(dry):
    ledgers = load_ledgers()
    idx, files = index_census()
    delta_path = HERE / 'delta.json'
    prev = json.loads(delta_path.read_text()) if delta_path.exists() else {}
    delta, problems, links, bindings = {}, [], {}, []
    for L in ledgers:
        for e in L['entries']:
            oid = e['id']
            if oid not in idx:
                problems.append(f'{oid}: not in census')
                continue
            p, bid, o = idx[oid]
            if 'alias_of' in o:
                problems.append(f'{oid}: alias, skipped')
                continue
            if oid not in prev and o['grade'] != e['old_grade']:
                problems.append(f'{oid}: census grade {o["grade"]} != ledger old_grade {e["old_grade"]}, skipped')
                continue
            d = apply_entry(o, e, prev.get(oid))
            d['group'] = L['group']
            d['mutations'] = e.get('mutations', [])
            delta[oid] = d
            ml = e.get('metadata_link')
            if ml:
                links.setdefault((ml['page'], ml['block']), set()).add(ml['model'])
        for b in L.get('bindings', []):
            bindings.append(dict(b, group=L['group']))
    for (page, block), models in sorted(links.items()):
        r = add_metadata_link(page, block, models, dry)
        if r:
            problems.append(r)
    if not dry:
        for p, d in files.items():
            p.write_text(json.dumps(d, indent=1, ensure_ascii=False) + '\n')
        delta_path.write_text(json.dumps(delta, indent=1) + '\n')
        (HERE / 'bindings.json').write_text(json.dumps({
            'verus_dir': '.g5/Verus_verification/coverage-20261117/iter75',
            'pin': '571e04a19aba9f9b4bc0ea23fbe09182038f4e55',
            'rows': bindings}, indent=1) + '\n')
    moves = {}
    for oid, d in delta.items():
        k = f"{d['old']['grade']} -> {d['new']['grade']}"
        moves[k] = moves.get(k, 0) + 1
    print(json.dumps({'applied': len(delta), 'moves': moves, 'links': len(links), 'bindings': len(bindings)}, indent=1))
    for x in problems:
        print('PROBLEM', x)
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main('--dry-run' in sys.argv))
