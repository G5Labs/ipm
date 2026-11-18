#!/usr/bin/env python3
"""Print the per-repair tables of the iteration-75 handoff from the ledgers, mutations.jsonl, delta.json and the
receipts index. Run from the repository root after apply.py."""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ORDER = ['R12', 'R345', 'R67', 'R8A', 'R8B']


def main():
    muts = {}
    for line in (HERE / 'mutations.jsonl').read_text().splitlines():
        r = json.loads(line)
        muts[r['id']] = r          # the last run of an id wins
    delta = json.loads((HERE / 'delta.json').read_text())
    ledgers = {json.loads(p.read_text())['group']: json.loads(p.read_text()) for p in (HERE / 'ledger').glob('*.json')}
    for g in ORDER + sorted(set(ledgers) - set(ORDER)):
        L = ledgers.get(g)
        if not L:
            continue
        print(f'\n### {g}: {L.get("items", "")}\n')
        for e in L['entries']:
            d = delta.get(e['id'], {})
            old = d.get('old', {})
            ms = ', '.join(f"{m} {muts[m]['verdict'] if m in muts else 'NOT LOGGED'}" for m in e.get('mutations', []))
            print(f"- **{e['id']}**: {old.get('grade', e['old_grade'])}/{old.get('check', '?')} -> "
                  f"{e['new_grade']}/{e['check']}{' (instance-scoped)' if e.get('instance_scoped') else ''}.")
            print(f"  - Product: {e['product']}")
            print(f"  - Neighbour: {e['neighbour']}")
            print(f"  - Mutations: {ms or 'none'}")
            for k in ('refutation', 'finding', 'ontology_repair', 'gap'):
                if e.get(k):
                    print(f"  - {k.replace('_', ' ').capitalize()}: {e[k]}")
        for n in L.get('not_done', []):
            print(f"- Not done: {n['id']}: {n['why']}")
    tot = {}
    for r in muts.values():
        tot[r['verdict']] = tot.get(r['verdict'], 0) + 1
    print('\nMutation verdicts (last run per id):', json.dumps(tot))
    moves = {}
    for d in delta.values():
        k = f"{d['old']['grade']} -> {d['new']['grade']}"
        moves[k] = moves.get(k, 0) + 1
    print('Census moves:', json.dumps(moves))


if __name__ == '__main__':
    main()
