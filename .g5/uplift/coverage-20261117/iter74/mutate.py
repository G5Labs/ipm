#!/usr/bin/env python3
"""Run one model mutation on a scratch copy and record it (iteration 74 supplementary mutants; copy of iter72/mutate.py).

  python3 -I .g5/uplift/coverage-20261117/iter74/mutate.py --group G1 --id G1-m01 \
      --model .g5/Verus_verification/consolidated/iter6/workflows.rs \
      --old 'if source_exists { 1 } else { 0 }' --new '1' \
      --obligations devtools.x#1,devtools.x#2 --change 'template_status ignores source_exists'

The model's directory (*.rs, recursively) and every file it reaches through #[path=...] are copied to
/tmp/ipmcov74-mut-<group>/<id>/ with the repository layout kept. --old must occur exactly once in the model
(use --old-file/--new-file for multi-line text, or --nth N to pick the Nth occurrence). The repository is
never modified. One JSON line per run is appended to iter74/mutations/<group>.jsonl.

Verdicts: killed (an assertion, postcondition, precondition or proof failed that is not only a resource
limit), survived (0 errors), inconclusive (only rlimit errors), invalid (no verification results line,
e.g. a type error: the mutation was not meaningful). Pass --args to repeat the receipt's flags
(e.g. --args='--rlimit 30' for playbook load_general.rs).
"""
import argparse, difflib, hashlib, json, os, re, shutil, subprocess, sys, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
VERUS = os.environ.get('VERUS72', str(Path.home() / '.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus'))
PATHREF = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')


def reach(model):
    files, todo = set(), [model]
    d = model.parent
    for p in d.rglob('*.rs'):
        if 'target' not in p.parts:
            todo.append(p)
    while todo:
        p = todo.pop().resolve()
        if p in files or not p.is_file():
            continue
        files.add(p)
        for ref in PATHREF.findall(p.read_text(errors='replace')):
            todo.append((p.parent / ref))
    return files


def run(path, args):
    t = time.time()
    r = subprocess.run([VERUS, str(path), '--triggers-mode', 'silent', *args], capture_output=True, text=True)
    text = r.stdout + r.stderr
    m = re.search(r'verification results:: (\d+) verified, (\d+) errors', text)
    return r.returncode, text, (int(m.group(1)), int(m.group(2))) if m else None, round(time.time() - t, 2)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--group', required=True)
    ap.add_argument('--id', required=True)
    ap.add_argument('--model', required=True)
    ap.add_argument('--old'); ap.add_argument('--new')
    ap.add_argument('--old-file'); ap.add_argument('--new-file')
    ap.add_argument('--nth', type=int, default=0)
    ap.add_argument('--obligations', default='')
    ap.add_argument('--change', required=True, help='what the mutation removes or alters in the deciding computation')
    ap.add_argument('--args', default='')
    a = ap.parse_args()
    old = Path(a.old_file).read_text() if a.old_file else a.old
    new = Path(a.new_file).read_text() if a.new_file else a.new
    if old is None or new is None:
        sys.exit('need --old/--new or --old-file/--new-file')
    model = (ROOT / a.model).resolve()
    src = model.read_text()
    n = src.count(old)
    if n == 0 or (n > 1 and not a.nth):
        sys.exit(f'--old occurs {n} times in {a.model}; make it unique or pass --nth')
    idx = -1
    for _ in range(a.nth or 1):
        idx = src.index(old, idx + 1)
    mutated = src[:idx] + new + src[idx + len(old):]
    scratch = Path(f'/tmp/ipmcov74-mut-{a.group}') / a.id
    if scratch.exists():
        shutil.rmtree(scratch)
    for f in reach(model):
        try:
            rel = f.relative_to(ROOT)
        except ValueError:
            continue
        dst = scratch / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(f, dst)
    target = scratch / model.relative_to(ROOT)
    target.write_text(mutated)
    extra = a.args.split()
    code, text, res, secs = run(target, extra)
    (scratch / 'verus.log').write_text(text)
    errs = [l for l in text.split('\n') if l.startswith('error')]
    only_rlimit = bool(errs) and all('rlimit' in l or 'resource limit' in l for l in errs)
    proof_failure = any(s in text for s in ('simplifies to false', 'evaluates to false', 'assertion failed', 'postcondition not satisfied',
                                             'precondition not satisfied'))
    if res is None and proof_failure:
        verdict = 'killed'   # a `by (compute)` failure stops Verus before the results line
    elif res is None:
        verdict = 'invalid'
    elif res[1] == 0:
        verdict = 'survived'
    elif only_rlimit:
        verdict = 'inconclusive'
    else:
        verdict = 'killed'
    diff = ''.join(difflib.unified_diff(src.splitlines(True), mutated.splitlines(True), a.model, a.model + ' (mutant)', n=1))
    failing = []
    for m in re.finditer(r'^error[^\n]*\n\s*--> ([^\n]+)', text, re.M):
        failing.append(m.group(0).split('\n')[0][:120] + ' @ ' + m.group(1).split('ipmcov74-mut-')[-1][:160])
    rec = {'id': a.id, 'group': a.group, 'model': a.model,
           'model_sha256': hashlib.sha256(src.encode()).hexdigest(), 'change': a.change,
           'obligations': [x for x in a.obligations.split(',') if x], 'args': extra,
           'verified': res[0] if res else None, 'errors': res[1] if res else None, 'exit': code,
           'verdict': verdict, 'failing': failing[:8], 'wall_seconds': secs,
           'utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'diff': diff[:4000]}
    out = HERE / 'mutations'
    out.mkdir(exist_ok=True)
    with open(out / f'{a.group}.jsonl', 'a') as fh:
        fh.write(json.dumps(rec) + '\n')
    print(json.dumps({k: rec[k] for k in ('id', 'verdict', 'verified', 'errors', 'wall_seconds', 'failing')}, indent=1))
    if verdict == 'invalid':
        print('\n'.join(text.split('\n')[:30]))


if __name__ == '__main__':
    main()
