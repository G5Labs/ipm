"""Semantic member coverage for the consolidated uplift.

B (baseline) = members graded `qualifies=yes` in baseline/G-baseline.tsv or baseline/P-baseline.tsv.
New = members credited by a consolidated family (checks/framework.py: as-built twin matches every claimed native field,
a wrong variant attributed to that member is rejected by a native field, and the member's model function exists),
plus rows in register-extra.tsv whose named check passed, minus members already in B.
Lost = members of B whose baseline evidence a consolidated finding invalidated (lost.tsv). Net = |New| - |Lost|.
Citations alone never count. Run: python3 count.py [--selftest]
"""
import csv, json, sys, shutil, subprocess, tempfile, re
from pathlib import Path
HERE = Path(__file__).resolve().parent
CONS = HERE.parent
VV = CONS.parents[1] / 'Verus_verification' / 'consolidated'
sys.path.insert(0, str(VV / 'checks'))
import framework  # noqa: E402

def members():
    return {r['member']: r for r in csv.DictReader(open(HERE / 'members.tsv'), delimiter='\t')}

def baseline():
    out = {}
    for arm in ('G', 'P'):
        f = CONS / 'baseline' / f'{arm}-baseline.tsv'
        if f.exists():
            for r in csv.DictReader(open(f), delimiter='\t'):
                if r.get('qualifies', '').strip().lower() == 'yes':
                    out.setdefault(r['member'].strip(), set()).add(arm)
    return out

def family_credits(model_dir=VV / 'models'):
    rows = []
    for f in sorted(model_dir.glob('*.py')):
        if f.name.startswith('_'): continue
        mod = framework.load_family(f)
        r = framework.check_family(mod)
        for mid, m in r['members'].items():
            rows.append({'member': mid, 'family': r['family'], 'round': getattr(mod, 'ROUND', '?'), 'credited': m['credited'],
                         'slice': m['slice'], 'fn': f.name + '::' + m['fn'], 'variants_rejected': m['variants_rejected'],
                         'behavior': m.get('behavior', '')})
    return rows

def extra_credits():
    f = HERE / 'register-extra.tsv'
    if not f.exists(): return []
    rows = []
    for r in csv.DictReader(open(f), delimiter='\t'):
        ok = (VV / r['check_receipt']).exists() and r.get('verdict') == 'pass'
        rows.append({'member': r['member'], 'family': r['family'], 'round': r['round'], 'credited': ok and r.get('kind') != 'citation',
                     'slice': r['slice'], 'fn': r['model_ref'], 'variants_rejected': [r.get('control', '')], 'behavior': r['behavior']})
    return rows

def slice_inside(mem, sl):
    m = re.match(r'(.+\.cls):(\d+)-(\d+)$', sl)
    if not m: return False
    f, a, b = m.group(1), int(m.group(2)), int(m.group(3))
    return mem['member'].split('::')[0] == f and int(mem['start']) <= a <= b <= int(mem['end'])

def compute(model_dir=VV / 'models', extra=True):
    mem, base = members(), baseline()
    rows = family_credits(model_dir) + (extra_credits() if extra else [])
    problems = [r for r in rows if r['member'] not in mem or not slice_inside(mem[r['member']], r['slice'])]
    credited = {}
    for r in rows:
        if r['credited'] and r not in problems:
            credited.setdefault(r['member'], r)
    lost = set()
    lf = HERE / 'lost.tsv'
    if lf.exists():
        lost = {r['member'] for r in csv.DictReader(open(lf), delimiter='\t')}
    new = {m: r for m, r in credited.items() if m not in base}
    reconfirmed = {m: r for m, r in credited.items() if m in base}
    return {'denominator': len(mem), 'B': sorted(base), 'B_size': len(base), 'B_by_arm': {a: sum(a in v for v in base.values()) for a in 'GP'},
            'new': new, 'reconfirmed': reconfirmed, 'lost': sorted(lost), 'net': len(new) - len(lost),
            'problems': [{'member': p['member'], 'slice': p['slice']} for p in problems], 'rows': rows}

def selftest():
    """Seeded controls (plan audit acceptance 4)."""
    before = compute()
    with tempfile.TemporaryDirectory() as d:
        d = Path(d)
        # Control 1: a citation-only row for an unmodeled member must not count.
        src = HERE / 'register-extra.tsv'; saved = src.read_text() if src.exists() else None
        hdr = 'member\tfamily\tround\tkind\tslice\tmodel_ref\tbehavior\tcheck_receipt\tverdict\tcontrol\n'
        src.write_text((saved or hdr) + 'Utils/LockManager.cls::Lock\tseeded\tR0\tcitation\tUtils/LockManager.cls:29-47\tnone\tcitation only\tREADME.md\tpass\t\n')
        try:
            c1 = compute()
        finally:
            src.write_text(saved) if saved is not None else src.unlink()
        ok1 = 'Utils/LockManager.cls::Lock' not in c1['new'] and len(c1['new']) == len(before['new'])
        # Control 2: delete a model function (keep its citation) in a copy; that member must lose its credit.
        md = d / 'models'; shutil.copytree(VV / 'models', md); (d / 'native').symlink_to(VV / 'native')
        f = md / 'temp_repo.py'; t = f.read_text()
        t = t.replace('def skip_create(s, location):', 'def skip_create_REMOVED(s, location):').replace('if skip_create(s, location)', 'if skip_create_REMOVED(s, location)')
        f.write_text(t)
        c2 = compute(md)
        ok2 = 'General/TempLocalRepoManager.cls::SkipCreate' not in c2['new'] and 'General/TempLocalRepoManager.cls::SkipCreate' in before['new']
        # Control 3: break one prediction of a credited family; every member of that family must lose credit.
        f = md / 'attr_evaluator.py'; t = f.read_text().replace("('E05', '{$namespaceLower}')", "('E05', '{$namespace}')")
        f.write_text(t)
        c3 = compute(md)
        ok3 = not any(r['family'] == 'attribute-evaluator' for r in c3['new'].values())
    print(f'selftest: citation-only row not counted={ok1}; removed model function drops credit={ok2}; wrong prediction drops family={ok3}')
    return ok1 and ok2 and ok3

def main():
    if '--selftest' in sys.argv:
        sys.exit(0 if selftest() else 1)
    c = compute()
    by_round, by_area = {}, {}
    for m, r in c['new'].items():
        by_round.setdefault(r['round'], []).append(m)
        area = m.split('/')[0] if '/' in m.split('::')[0] else '(root)'
        by_area[area] = by_area.get(area, 0) + 1
    out = {k: v for k, v in c.items() if k not in ('rows', 'new', 'reconfirmed')}
    out['new_members'] = {m: {k: r[k] for k in ('family', 'round', 'slice', 'fn', 'variants_rejected')} for m, r in sorted(c['new'].items())}
    out['reconfirmed_members'] = sorted(c['reconfirmed'])
    out['new_by_round'] = {k: len(v) for k, v in sorted(by_round.items())}
    out['new_by_area'] = by_area
    (HERE / 'counts.json').write_text(json.dumps(out, indent=1, sort_keys=True) + '\n')
    print(f"denominator {c['denominator']}; B={c['B_size']} (G {c['B_by_arm']['G']}, P {c['B_by_arm']['P']}); new={len(c['new'])} "
          f"by round {out['new_by_round']}; reconfirmed in B={len(c['reconfirmed'])}; lost={len(c['lost'])}; net={c['net']}; slice problems={c['problems']}")

if __name__ == '__main__':
    main()
