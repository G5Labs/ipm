#!/usr/bin/env python3
"""Run one follow-up check and keep its receipt. Run with python3 -I -B from .g5/Verus_verification.

  python3 -I -B followup/bin/receipt.py LABEL "EXPECTED" --inputs PATH... -- COMMAND ARG...

Creates followup/results/<UTC>-<LABEL>/ with command.txt, raw.log (stdout and stderr) and
receipt.json. The token {OUT} in COMMAND is replaced by that directory, so a check can write its
own outputs beside the receipt. Each input path is hashed before the command runs; a directory is
hashed file by file. The receipt records the process outcome only. The substantive verdict is in
the check's own output.
"""
import hashlib, json, os, platform, subprocess, sys
from datetime import datetime, timezone

VV = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def hash_path(p):
    if os.path.isdir(p):
        h = hashlib.sha256()
        for root, dirs, files in os.walk(p):
            dirs.sort()
            for f in sorted(files):
                if f.endswith(".pyc") or "__pycache__" in root or "/target/" in root + "/":
                    continue
                fp = os.path.join(root, f)
                h.update(os.path.relpath(fp, p).encode() + b"\0" + hashlib.sha256(open(fp, "rb").read()).digest())
        return "dir:" + h.hexdigest()
    return hashlib.sha256(open(p, "rb").read()).hexdigest()


def main(argv):
    label, expected = argv[0], argv[1]
    sep = argv.index("--")
    inputs = argv[argv.index("--inputs") + 1:sep] if "--inputs" in argv[:sep] else []
    cmd = argv[sep + 1:]
    start = datetime.now(timezone.utc)
    run_id = start.strftime("%Y%m%dT%H%M%SZ") + "-" + label
    out = os.path.join(VV, "followup", "results", run_id)
    os.makedirs(out)
    cmd = [c.replace("{OUT}", out) for c in cmd]
    hashes = {os.path.relpath(p, VV): hash_path(p) for p in inputs}
    open(os.path.join(out, "command.txt"), "w").write(" ".join(cmd) + "\n")
    with open(os.path.join(out, "raw.log"), "w") as log:
        p = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, cwd=VV)
    end = datetime.now(timezone.utc)
    rec = {"run_id": run_id, "label": label, "cwd": ".g5/Verus_verification", "command": cmd, "exit_code": p.returncode,
           "process_outcome": "COMPLETED" if p.returncode == 0 else "NONZERO-EXIT", "expected": expected,
           "inputs_sha256": hashes, "started_utc": start.strftime("%Y-%m-%dT%H:%M:%SZ"), "ended_utc": end.strftime("%Y-%m-%dT%H:%M:%SZ"),
           "python": platform.python_version(),
           "git_head": subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True, cwd=VV).stdout.strip(),
           "git_dirty_paths": subprocess.run(["git", "status", "--porcelain", "--", "."], capture_output=True, text=True, cwd=VV).stdout.splitlines(),
           "substantive_verdict": "see the check's own output in this directory; exit code alone is not a verdict"}
    json.dump(rec, open(os.path.join(out, "receipt.json"), "w"), indent=1)
    print(run_id, rec["process_outcome"], "rc=" + str(p.returncode))
    return p.returncode


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
