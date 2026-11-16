#!/usr/bin/env python3
"""Freshness gate for iterations sealed in the 2026-11-16 continuation (iteration30 onward).

Usage: freshness.py [--root REPO] [--map MAP.json] [--json OUT]

For each iteration with a run.json and bindings.json it decides two things separately:
- theorem: does each model file still hash to the value its last complete run recorded? If not, the
  proof, controls and replay must be rerun (`run.py --no-native` reuses the native receipt only when
  the native fixture bytes are unchanged, which the harness itself enforces).
- correspondence: does each bound production source file still hash to the value in the project map
  that the iteration sealed with? If not, the binding must be re-established at the new revision
  (reread the deciding source, rerun native).
It also flags production files added or removed since the census, which may add writers or readers
that no binding knows about, so they reopen the affected subsystem rather than being ignored.
Exit status 0 = everything fresh, 1 = something stale (details in the report)."""
from pathlib import Path
import argparse, csv, hashlib, json, sys

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--root', default=str(Path(__file__).resolve().parents[4]))
    ap.add_argument('--map')
    ap.add_argument('--json')
    a = ap.parse_args()
    root = Path(a.root)
    cons = root / '.g5/uplift/consolidated'
    report = {'root': str(root), 'iterations': {}, 'census': {}}
    stale = False
    for it in sorted(cons.glob('iter[0-9]*'), key=lambda p: int(p.name[4:])):
        n = int(it.name[4:])
        if n < 30 or not (it / 'run.json').exists() or not (it / 'bindings.json').exists():
            continue
        run = json.loads((it / 'run.json').read_text())
        verus_dir = root / '.g5/Verus_verification/consolidated' / it.name
        theorem = {}
        for f, h in run.get('model_source_sha256', {}).items():
            p = verus_dir / f
            theorem[f] = 'fresh' if p.exists() and sha(p) == h else 'STALE: formal input changed; rerun proofs, controls and replay'
        mp = Path(a.map) if a.map else (it / 'project-map.json')
        corr = {}
        if mp.exists():
            m = json.loads(mp.read_text())
            for e in m['entries']:
                if not any(o['origin'] == it.name for o in e['origins']):
                    continue
                src = root / e['source']
                now = sha(src) if src.exists() else None
                key = e['member']
                if now is None:
                    corr[key] = 'STALE: bound source file removed'
                elif now != e['source_sha256']:
                    corr[key] = 'STALE: bound source changed; re-establish correspondence and rerun native'
                else:
                    corr.setdefault(key, 'fresh')
        else:
            corr['(map)'] = 'STALE: no sealed project map'
        report['iterations'][it.name] = {'theorem': theorem, 'correspondence': corr}
        stale |= any(v != 'fresh' for v in list(theorem.values()) + list(corr.values()))
    # Every production class file at the pin, not only files with counted members.
    files = set(json.loads((cons / 'common30/pin-source-files.json').read_text()))
    now_files = {str(p.relative_to(root)) for p in (root / 'src/cls/IPM').rglob('*.cls')}
    report['census'] = {'added_files': sorted(now_files - files), 'removed_files': sorted(files - now_files)}
    if report['census']['added_files'] or report['census']['removed_files']:
        stale = True
    out = json.dumps(report, indent=1)
    if a.json:
        Path(a.json).write_text(out + '\n')
    summary = {k: {'theorem_stale': sum(v != 'fresh' for v in r['theorem'].values()),
                   'correspondence_stale': sum(v != 'fresh' for v in r['correspondence'].values())}
               for k, r in report['iterations'].items()}
    print(json.dumps({'stale': stale, 'iterations': summary, 'census': {k: len(v) for k, v in report['census'].items()}}))
    sys.exit(1 if stale else 0)

if __name__ == '__main__':
    main()
