"""Shared helpers for the iteration-75 runners (mutate.py, run_verus.py)."""
import fcntl, hashlib, json, os, re, subprocess, time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
VERUS = os.environ.get('VERUS', str(Path.home() / '.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus'))
VERUS_NAME = 'verus-0.2026.09.27.3cf1832/verus-x86-linux'
PATHREF = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def path_closure(model):
    """The model and every file it reaches through #[path = ...] (the inputs Verus reads)."""
    files, todo = set(), [Path(model)]
    while todo:
        p = todo.pop().resolve()
        if p in files or not p.is_file():
            continue
        files.add(p)
        for ref in PATHREF.findall(p.read_text(errors='replace')):
            todo.append(p.parent / ref)
    return files


def rel(p):
    return str(Path(p).resolve().relative_to(ROOT))


def run(path, args=(), cwd=None):
    t = time.time()
    r = subprocess.run([VERUS, str(path), '--triggers-mode', 'silent', *args], capture_output=True, text=True, cwd=cwd)
    text = r.stdout + r.stderr
    m = re.search(r'verification results:: (\d+) verified, (\d+) errors', text)
    return r.returncode, text, (int(m.group(1)), int(m.group(2))) if m else None, round(time.time() - t, 2)


def append_jsonl(path, rec):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, 'a') as fh:
        fcntl.flock(fh, fcntl.LOCK_EX)
        fh.write(json.dumps(rec) + '\n')
        fcntl.flock(fh, fcntl.LOCK_UN)
