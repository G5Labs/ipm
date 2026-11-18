#!/usr/bin/env python3
"""Run pinned Verus on model files and keep receipts (iteration 75).

  python3 -I .g5/uplift/coverage-20261117/iter75/run_verus.py <repo-relative .rs> ...

Writes receipts/<slug>.log and appends one JSON line per file to receipts/index.jsonl: the command, the
Verus binary sha256, the sha256 of every input file (the model and its #[path] closure), exit status and
the verified/error counts."""
import sys, time
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import HERE, ROOT, VERUS, VERUS_NAME, sha, path_closure, rel, run, append_jsonl


def main(files):
    out = HERE / 'receipts'
    out.mkdir(exist_ok=True)
    vsha = sha(VERUS)
    ok = True
    for f in files:
        p = ROOT / f
        code, text, res, secs = run(p)
        slug = f.replace('.g5/Verus_verification/', '').replace('/', '__')
        (out / (slug + '.log')).write_text(text)
        rec = {'model': f, 'command': f'$VERUS {f} --triggers-mode silent', 'verus': VERUS_NAME, 'verus_sha256': vsha,
               'inputs': {rel(x): sha(x) for x in sorted(path_closure(p))}, 'exit': code,
               'verified': res[0] if res else None, 'errors': res[1] if res else None, 'wall_seconds': secs,
               'utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'log': 'receipts/' + slug + '.log'}
        ok &= code == 0
        append_jsonl(out / 'index.jsonl', rec)
        print({k: rec[k] for k in ('model', 'exit', 'verified', 'errors', 'wall_seconds')})
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
