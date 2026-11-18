#!/usr/bin/env python3
"""Obligation census for the IPM ontology (coverage uplift 2026-11-17).

Replayable from a clean checkout, standard library only. Run from the repository root.

  python3 -I .g5/uplift/coverage-20261117/census/census.py inventory   # rewrite inventory.json
  python3 -I .g5/uplift/coverage-20261117/census/census.py units ID     # print a block's units
  python3 -I .g5/uplift/coverage-20261117/census/census.py count        # validate records, write counts
  python3 -I .g5/uplift/coverage-20261117/census/census.py check        # validate only; exit 1 on error
  python3 -I .g5/uplift/coverage-20261117/census/census.py selftest     # seeded controls

Inventory: every `[I|D|Q · id]` block in .g5/ontology/**/*.g5.md, its body (marker line to the next
block anchor, heading or the Metadata section), a body hash and its units. A unit is a paragraph (P<n>),
a list item (B<n>) or a table body row (T<n>), numbered within the block.

Records: census/blocks/<page-slug>.json, one per ontology page:

  {"page": ".g5/ontology/...g5.md",
   "blocks": {"<id>": {
      "body_sha": "<inventory hash at assessment>",
      "status": "enumerated" | "structural",
      "reason": "why structural / why no additional obligation (required when structural)",
      "units": {"P1": ["<obligation id>", ...] | "excluded: <reason>", ...},   # every unit accounted for
      "parent": {"children": [...], "entailment": "..."},                      # optional
      "obligations": [{
         "id": "<id>#<n>", "units": ["P1"], "text": "...", "cases": "...", "observation": "...",
         "grade": "full|partial|absent|unassessed",
         "check": "proved|bounded|refuted|unchecked|none",
         "models": ["<repo path>::<declaration>"], "evidence": ["<repo path>"],
         "basis": "reused:<where> | new:<iteration> | composed:<obligation ids>",
         "gap": "missing distinction or premise (required unless full)",
         "alias_of": "<obligation id>"     # optional: restatement counted once at its owner
         "assessed_in": "iteration-NN"}]}}}

A full grade needs at least one resolvable model declaration, existing evidence and a design check
(proved, bounded or refuted). An alias is not counted. A block whose hash changed is STALE and is not
counted as enumerated until reconciled.
"""
import hashlib
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
ONTOLOGY = ROOT / '.g5' / 'ontology'
BLOCKS = HERE / 'blocks'
MARKER = re.compile(r'^\[([IDQ]) · ([^\s\[\]]+)\]\s*$')
ANCHOR = re.compile(r'^\s*<a\s+id="([^"]+)"\s*>\s*</a>\s*$')
HEADING = re.compile(r'^#{1,6}\s')
LIST = re.compile(r'^(\s*)(?:[-+*]|\d+[.)])\s+')
GRADES = ('full', 'partial', 'absent', 'unassessed')
CHECKS = ('proved', 'bounded', 'refuted', 'unchecked', 'none')


def page_slug(rel):
    return rel.replace('.g5/ontology/', '').replace('/', '__').replace('.g5.md', '')


def parse_page(path):
    lines = path.read_text().split('\n')
    meta = next((i for i, l in enumerate(lines) if l.strip() == '## Metadata'), len(lines))
    starts = [(i, MARKER.match(l)) for i, l in enumerate(lines[:meta]) if MARKER.match(l)]
    out = []
    for n, (i, m) in enumerate(starts):
        end = starts[n + 1][0] if n + 1 < len(starts) else meta
        j = i + 1
        while j < end:
            if HEADING.match(lines[j]):
                break
            if ANCHOR.match(lines[j]) and any(MARKER.match(x) or ANCHOR.match(x) for x in lines[j + 1:j + 3]):
                break
            j += 1
        body = lines[i + 1:j]
        while body and not body[-1].strip():
            body.pop()
        out.append({'id': m.group(2), 'kind': m.group(1), 'line': i + 1, 'end': i + 1 + len(body),
                    'body': body})
    return out


def units(body, first_line):
    result, current, counters = [], None, {'P': 0, 'B': 0, 'T': 0}

    def close():
        if current:
            current['text'] = ' '.join(x.strip() for x in current['lines']).strip()
            current['sha'] = hashlib.sha256(current['text'].encode()).hexdigest()[:12]
            del current['lines']
            result.append(current)
    for k, line in enumerate(body):
        s = line.strip()
        if not s:
            close(); current = None
            continue
        if s.startswith('|'):
            if re.match(r'^\|[\s:|-]+\|?$', s):
                continue
            close()
            nxt = body[k + 1].strip() if k + 1 < len(body) else ''
            if re.match(r'^\|[\s:|-]+\|?$', nxt):   # header row
                current = None
                continue
            counters['T'] += 1
            current = {'unit': f"T{counters['T']}", 'line': first_line + 1 + k, 'lines': [s]}
            continue
        if LIST.match(line) and not line.startswith('    '):
            close()
            counters['B'] += 1
            current = {'unit': f"B{counters['B']}", 'line': first_line + 1 + k, 'lines': [s]}
            continue
        if current is None:
            counters['P'] += 1
            current = {'unit': f"P{counters['P']}", 'line': first_line + 1 + k, 'lines': [s]}
        else:
            current['lines'].append(s)
    close()
    return result


def inventory():
    blocks = []
    for path in sorted(ONTOLOGY.rglob('*.g5.md')):
        rel = str(path.relative_to(ROOT))
        for b in parse_page(path):
            text = '\n'.join(b['body']).strip()
            blocks.append({'id': b['id'], 'kind': b['kind'], 'page': rel, 'line': b['line'],
                           'end': b['end'], 'body_sha': hashlib.sha256(text.encode()).hexdigest()[:16],
                           'units': [{k: u[k] for k in ('unit', 'line', 'sha')} for u in units(b['body'], b['line'])]})
    return blocks


def load_records():
    records, errors = {}, []
    for path in sorted(BLOCKS.glob('*.json')):
        try:
            data = json.loads(path.read_text())
        except ValueError as exc:
            errors.append(f'{path.name}: invalid JSON: {exc}')
            continue
        for bid, rec in data.get('blocks', {}).items():
            if bid in records:
                errors.append(f'{bid}: recorded twice')
            rec['_file'] = path.name
            rec['_page'] = data.get('page')
            records[bid] = rec
    return records, errors


_decl_cache = {}


def resolve_model(ref):
    if '::' not in ref:
        return f'model locator without declaration: {ref}'
    path, decl = ref.split('::', 1)
    file = ROOT / path
    if not file.is_file():
        return f'model file missing: {path}'
    names = [d for d in re.split(r'[,{}\s]+', decl) if d]
    text = _decl_cache.setdefault(path, file.read_text(errors='replace'))
    for name in names:
        if not re.search(r'(?<![A-Za-z0-9_])' + re.escape(name.split('(')[0]) + r'(?![A-Za-z0-9_])', text):
            return f'declaration {name} not found in {path}'
    return None


def validate(inv, records):
    errors, warnings = [], []
    by_id = {b['id']: b for b in inv}
    all_obl = {}
    for bid, rec in records.items():
        for o in rec.get('obligations', []):
            if o.get('id') in all_obl:
                errors.append(f"{o.get('id')}: duplicate obligation id")
            all_obl[o.get('id')] = (bid, o)
    for bid, rec in records.items():
        b = by_id.get(bid)
        if b is None:
            errors.append(f'{bid}: record for a block not in the inventory ({rec["_file"]})')
            continue
        if rec.get('status') not in ('enumerated', 'structural'):
            errors.append(f'{bid}: status must be enumerated or structural')
        if rec.get('status') == 'structural' and not rec.get('reason'):
            errors.append(f'{bid}: structural block needs a reason')
        unit_ids = {u['unit'] for u in b['units']}
        umap = rec.get('units', {})
        for u in sorted(unit_ids - set(umap)):
            errors.append(f'{bid}: unit {u} not accounted for')
        for u in sorted(set(umap) - unit_ids):
            errors.append(f'{bid}: unknown unit {u}')
        for u, v in umap.items():
            if isinstance(v, str):
                if not v.startswith('excluded:') or len(v) < 14:
                    errors.append(f'{bid}: unit {u} must list obligation ids or "excluded: <reason>"')
            else:
                for oid in v:
                    if oid not in all_obl:
                        errors.append(f'{bid}: unit {u} names unknown obligation {oid}')
        if rec.get('status') == 'enumerated' and not rec.get('obligations'):
            errors.append(f'{bid}: enumerated block without obligations (use structural with a reason)')
        for o in rec.get('obligations', []):
            oid = o.get('id', '?')
            if not str(oid).startswith(bid + '#'):
                errors.append(f'{oid}: obligation id must start with {bid}#')
            for u in o.get('units', []):
                if u not in unit_ids:
                    errors.append(f'{oid}: unknown unit {u}')
            if 'alias_of' in o:
                if o['alias_of'] not in all_obl:
                    errors.append(f'{oid}: alias_of unknown obligation {o["alias_of"]}')
                continue
            g, c = o.get('grade'), o.get('check')
            if g not in GRADES:
                errors.append(f'{oid}: grade {g!r}')
            if c not in CHECKS:
                errors.append(f'{oid}: check {c!r}')
            for f in ('text', 'cases', 'observation', 'basis', 'assessed_in'):
                if not o.get(f):
                    errors.append(f'{oid}: missing {f}')
            for m in o.get('models', []):
                r = resolve_model(m)
                if r:
                    errors.append(f'{oid}: {r}')
            for e in o.get('evidence', []):
                if not (ROOT / e.split('#')[0]).exists():
                    errors.append(f'{oid}: evidence missing: {e}')
            if g == 'full':
                if not o.get('models') or not o.get('evidence'):
                    errors.append(f'{oid}: full needs models and evidence')
                if c not in ('proved', 'bounded', 'refuted'):
                    errors.append(f'{oid}: full needs a design check (proved, bounded or refuted)')
            elif g in ('partial', 'absent') and not o.get('gap'):
                errors.append(f'{oid}: {g} needs a gap')
            if g == 'partial' and not o.get('models'):
                errors.append(f'{oid}: partial needs at least one model locator')
            if g == 'absent' and c in ('proved', 'bounded'):
                errors.append(f'{oid}: absent cannot carry a passing check')
            for comp in o.get('composed', []):
                if comp not in all_obl:
                    errors.append(f'{oid}: composed from unknown {comp}')
        if rec.get('body_sha') != b['body_sha']:
            warnings.append(f'{bid}: STALE (recorded {rec.get("body_sha")}, current {b["body_sha"]})')
    return errors, warnings


def tally(obls):
    t = {g: 0 for g in GRADES}
    c = {k: 0 for k in CHECKS}
    for o in obls:
        if 'alias_of' in o:
            continue
        t[o['grade']] += 1
        c[o['check']] += 1
    return t, c


def counts(inv, records, warnings):
    stale = {w.split(':')[0] for w in warnings if 'STALE' in w}
    per_block, pool = [], {g: 0 for g in GRADES}
    checks = {k: 0 for k in CHECKS}
    full_passing = 0
    status_count = {'enumerated': 0, 'structural': 0, 'unenumerated': 0, 'stale': 0}
    by_page = {}
    for b in inv:
        rec = records.get(b['id'])
        if rec is None:
            st = 'unenumerated'
        elif b['id'] in stale:
            st = 'stale'
        else:
            st = rec['status']
        status_count[st] += 1
        row = {'id': b['id'], 'page': b['page'], 'status': st}
        if st in ('enumerated', 'structural'):
            t, c = tally(rec.get('obligations', []))
            for g in GRADES:
                pool[g] += t[g]
            for k in CHECKS:
                checks[k] += c[k]
            full_passing += sum(1 for o in rec.get('obligations', []) if 'alias_of' not in o
                                and o['grade'] == 'full' and o['check'] in ('proved', 'bounded'))
            total = sum(t.values())
            row.update(t)
            row['checks'] = c
            row['coverage'] = (round(t['full'] / total, 4) if total else 'N/A')
        per_block.append(row)
        p = by_page.setdefault(b['page'], {'blocks': 0, **{g: 0 for g in GRADES}, 'unenumerated': 0})
        p['blocks'] += 1
        if st in ('enumerated', 'structural'):
            for g in GRADES:
                p[g] += row[g]
        else:
            p['unenumerated'] += 1
    total = sum(pool.values())
    complete = status_count['unenumerated'] == 0 and status_count['stale'] == 0
    summary = {
        'inventory_blocks': len(inv), 'block_status': status_count, 'enumeration_complete': complete,
        'assessed_obligations': total, **pool, 'checks': checks,
        'full_with_passing_check': full_passing,
        'assessed_scope_coverage': round(pool['full'] / total, 4) if total else None,
        'assessed_scope_upper_bound': round((pool['full'] + pool['unassessed']) / total, 4) if total else None,
        'whole_foundation_coverage': (round(pool['full'] / total, 4) if complete and total else 'unmeasured'),
    }
    return {'summary': summary, 'pages': by_page, 'blocks': per_block}


def write_counts(result):
    (HERE / 'counts.json').write_text(json.dumps(result, indent=1) + '\n')
    s = result['summary']
    lines = ['# Obligation census counts (generated by census.py count; do not edit)', '',
             f"Inventory blocks: {s['inventory_blocks']}. Status: " +
             ', '.join(f'{k} {v}' for k, v in s['block_status'].items()) + '.', '',
             f"Assessed obligations: {s['assessed_obligations']} (full {s['full']}, partial {s['partial']}, "
             f"absent {s['absent']}, unassessed {s['unassessed']}).",
             f"Design checks on assessed obligations: " + ', '.join(f'{k} {v}' for k, v in s['checks'].items()) +
             f". Full with a passing (proved or bounded) check: {s['full_with_passing_check']}.",
             f"Assessed-scope coverage: {s['assessed_scope_coverage']} (upper bound with unassessed "
             f"{s['assessed_scope_upper_bound']}). Whole-foundation coverage: {s['whole_foundation_coverage']}.",
             '', '| Page | Blocks | Unenumerated or stale | Full | Partial | Absent | Unassessed |',
             '|---|---|---|---|---|---|---|']
    for page, p in sorted(result['pages'].items()):
        lines.append(f"| {page.replace('.g5/ontology/', '')} | {p['blocks']} | {p['unenumerated']} | {p['full']} | "
                     f"{p['partial']} | {p['absent']} | {p['unassessed']} |")
    lines += ['', '| Block | Status | Full | Partial | Absent | Unassessed | Coverage |', '|---|---|---|---|---|---|---|']
    for r in result['blocks']:
        if r['status'] in ('enumerated', 'structural'):
            lines.append(f"| {r['id']} | {r['status']} | {r['full']} | {r['partial']} | {r['absent']} | "
                         f"{r['unassessed']} | {r['coverage']} |")
        else:
            lines.append(f"| {r['id']} | {r['status']} | | | | | |")
    (HERE / 'COUNTS.md').write_text('\n'.join(lines) + '\n')


def selftest():
    inv = [{'id': 'x.a', 'kind': 'I', 'page': 'p', 'line': 1, 'end': 3, 'body_sha': 'h',
            'units': [{'unit': 'P1', 'line': 2, 'sha': 's'}, {'unit': 'B1', 'line': 3, 'sha': 't'}]}]
    good = {'x.a': {'_file': 'f', 'body_sha': 'h', 'status': 'enumerated',
                    'units': {'P1': ['x.a#1'], 'B1': 'excluded: rationale only'},
                    'obligations': [{'id': 'x.a#1', 'units': ['P1'], 'text': 't', 'cases': 'c',
                                     'observation': 'o', 'grade': 'absent', 'check': 'none', 'models': [],
                                     'evidence': [], 'basis': 'new', 'gap': 'g', 'assessed_in': 'i'}]}}
    e, w = validate(inv, good)
    assert not e and not w, (e, w)
    import copy
    controls = {
        'unit not accounted': lambda r: r['x.a']['units'].pop('B1'),
        'full without check': lambda r: r['x.a']['obligations'][0].update(grade='full', check='unchecked'),
        'full without models': lambda r: r['x.a']['obligations'][0].update(grade='full', check='proved',
                                                                           evidence=['README.md']),
        'unresolvable model': lambda r: r['x.a']['obligations'][0].update(
            grade='partial', models=['.g5/Verus_verification/README.md::' + 'zz' + 'NoSuchDeclaration']),
        'partial without gap': lambda r: r['x.a']['obligations'][0].update(
            grade='partial', gap='', models=['.g5/uplift/coverage-20261117/census/census.py::selftest']),
        'enumerated without obligations': lambda r: r['x.a'].update(obligations=[], units={'P1': 'excluded: x y z w',
                                                                                         'B1': 'excluded: x y z w'}),
        'absent with passing check': lambda r: r['x.a']['obligations'][0].update(check='proved'),
    }
    for name, mutate in controls.items():
        r = copy.deepcopy(good)
        mutate(r)
        e, _ = validate(inv, r)
        assert e, f'control not detected: {name}'
    r = copy.deepcopy(good)
    r['x.a']['body_sha'] = 'old'
    _, w = validate(inv, r)
    assert any('STALE' in x for x in w), 'stale not detected'
    res = counts(inv, r, w)
    assert res['summary']['whole_foundation_coverage'] == 'unmeasured'
    res = counts(inv, good, [])
    assert res['summary']['whole_foundation_coverage'] == 0.0
    print(f'selftest: {len(controls) + 2} controls detected; valid record accepted')


def main(argv):
    cmd = argv[1] if len(argv) > 1 else 'count'
    if cmd == 'selftest':
        return selftest()
    inv = inventory()
    if cmd == 'inventory':
        (HERE / 'inventory.json').write_text(json.dumps(inv, indent=1) + '\n')
        start = Path(sys.argv[2]) if len(argv) > 2 else None
        print(f'{len(inv)} blocks in {len({b["page"] for b in inv})} pages')
        if start and start.exists():
            owners = {o['owner'] for o in json.loads(start.read_text())}
            ids = {b['id'] for b in inv}
            print('missing from inventory:', sorted(owners - ids))
            print('new since starting owners:', sorted(ids - owners))
        return 0
    if cmd == 'units':
        for b in inv:
            if b['id'] == argv[2]:
                text = (ROOT / b['page']).read_text().split('\n')[b['line']:b['end']]
                for u in units(text, b['line']):
                    print(f"{u['unit']} L{u['line']}: {u['text']}")
        return 0
    records, errors = load_records()
    e, warnings = validate(inv, records)
    errors += e
    for w in warnings:
        print('WARN', w)
    for x in errors:
        print('ERROR', x)
    if cmd == 'count':
        res = counts(inv, records, warnings)
        write_counts(res)
        print(json.dumps(res['summary'], indent=1))
    return 1 if errors else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv) or 0)
