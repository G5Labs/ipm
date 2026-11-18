#!/usr/bin/env python3
"""Re-run pinned Verus on credited model files and record receipts (iteration 72 census re-audit).

  python3 -I .g5/uplift/coverage-20261117/iter72/run_verus.py <repo-relative .rs> ...

Writes receipts/<slug>.log and appends one JSON line per file to receipts/index.jsonl with the model
sha256, the Verus binary sha256, exit status and the verified/error counts. VERUS defaults to the
pinned 0.2026.09.27.3cf1832 x86 build."""
import hashlib, json, os, re, subprocess, sys, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
VERUS = os.environ.get('VERUS', str(Path.home() / '.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus'))


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def main(files):
    out = HERE / 'receipts'
    out.mkdir(exist_ok=True)
    vsha = sha(VERUS)
    ok = True
    for f in files:
        p = ROOT / f
        t = time.time()
        r = subprocess.run([VERUS, str(p), '--triggers-mode', 'silent'], capture_output=True, text=True)
        text = r.stdout + r.stderr
        m = re.search(r'verification results:: (\d+) verified, (\d+) errors', text)
        slug = f.replace('.g5/Verus_verification/', '').replace('/', '__')
        (out / (slug + '.log')).write_text(text)
        rec = {'model': f, 'model_sha256': sha(p), 'verus': 'verus-0.2026.09.27.3cf1832/verus-x86-linux',
               'verus_sha256': vsha, 'exit': r.returncode,
               'verified': int(m.group(1)) if m else None, 'errors': int(m.group(2)) if m else None,
               'wall_seconds': round(time.time() - t, 3),
               'utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'log': 'receipts/' + slug + '.log'}
        ok &= r.returncode == 0
        with open(out / 'index.jsonl', 'a') as fh:
            fh.write(json.dumps(rec) + '\n')
        print(json.dumps(rec))
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
