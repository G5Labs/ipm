#!/usr/bin/env python3
"""Run one model mutation on a scratch copy and record it (iteration 75; iteration-72 mutate.py style).

  python3 -I .g5/uplift/coverage-20261117/iter75/mutate.py --id R1-m01 \
      --model .g5/Verus_verification/consolidated/iter2/registry.rs \
      --target .g5/Verus_verification/coverage-20261117/iter75/oras_integrity.rs \
      --old 'a && b' --new 'a' --obligations repo.oras.integrity#3 --change 'digest comparison dropped'

--model is the file holding the deciding computation (often an inherited model). --target is the file
Verus runs (the iteration-75 neighbour file that imports the model through #[path]); it defaults to
--model. Both #[path] closures are copied to /tmp/ipmcov75-mut/<id>/ with the repository layout kept,
the copy of --model is mutated, and the copy of --target is verified. The repository is never modified.
--old must occur exactly once (or pass --nth N). One JSON line per run is appended to
iter75/mutations.jsonl.

Verdicts: killed (an assertion, postcondition, precondition or proof failed that is not only a resource
limit), survived (0 errors), inconclusive (only rlimit errors), invalid (no verification results line and
no proof failure, e.g. a type error)."""
import argparse, difflib, re, shutil, sys, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import HERE, ROOT, sha, path_closure, run, append_jsonl


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--id', required=True)
    ap.add_argument('--model', required=True)
    ap.add_argument('--target')
    ap.add_argument('--old'); ap.add_argument('--new')
    ap.add_argument('--old-file'); ap.add_argument('--new-file')
    ap.add_argument('--nth', type=int, default=0)
    ap.add_argument('--obligations', default='')
    ap.add_argument('--change', required=True)
    ap.add_argument('--args', default='')
    a = ap.parse_args()
    old = Path(a.old_file).read_text() if a.old_file else a.old
    new = Path(a.new_file).read_text() if a.new_file else a.new
    if old is None or new is None:
        sys.exit('need --old/--new or --old-file/--new-file')
    model = (ROOT / a.model).resolve()
    target = (ROOT / (a.target or a.model)).resolve()
    src = model.read_text()
    n = src.count(old)
    if n == 0 or (n > 1 and not a.nth):
        sys.exit(f'--old occurs {n} times in {a.model}; make it unique or pass --nth')
    idx = -1
    for _ in range(a.nth or 1):
        idx = src.index(old, idx + 1)
    mutated = src[:idx] + new + src[idx + len(old):]
    scratch = Path('/tmp/ipmcov75-mut') / a.id
    if scratch.exists():
        shutil.rmtree(scratch)
    for f in path_closure(target) | path_closure(model):
        try:
            r = f.relative_to(ROOT)
        except ValueError:
            continue
        dst = scratch / r
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(f, dst)
    (scratch / model.relative_to(ROOT)).write_text(mutated)
    extra = a.args.split()
    code, text, res, secs = run(scratch / target.relative_to(ROOT), extra)
    (scratch / 'verus.log').write_text(text)
    errs = [l for l in text.split('\n') if l.startswith('error')]
    only_rlimit = bool(errs) and all('rlimit' in l or 'resource limit' in l for l in errs)
    proof_failure = any(s in text for s in ('simplifies to false', 'evaluates to false', 'assertion failed',
                                             'postcondition not satisfied', 'precondition not satisfied'))
    if res is None and proof_failure:
        verdict = 'killed'
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
        failing.append(m.group(0).split('\n')[0][:120] + ' @ ' + m.group(1).split('ipmcov75-mut/')[-1][:160])
    rec = {'id': a.id, 'model': a.model, 'target': a.target or a.model,
           'model_sha256': sha(model), 'target_sha256': sha(target), 'change': a.change,
           'obligations': [x for x in a.obligations.split(',') if x], 'args': extra,
           'verified': res[0] if res else None, 'errors': res[1] if res else None, 'exit': code,
           'verdict': verdict, 'failing': failing[:8], 'wall_seconds': secs,
           'utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'diff': diff[:4000]}
    append_jsonl(HERE / 'mutations.jsonl', rec)
    print({k: rec[k] for k in ('id', 'verdict', 'verified', 'errors', 'wall_seconds', 'failing')})
    if verdict == 'invalid':
        print('\n'.join(text.split('\n')[:30]))


if __name__ == '__main__':
    main()
