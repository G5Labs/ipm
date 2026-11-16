#!/usr/bin/env python3
"""Broken-implementation and vacuity controls for every Verus model (playbook 3.3 gate, section 5 mutants).

Controls live in models/verus/controls/<stem>.json as a list of
  {"name", "description", "old", "new", "expect": [function names the error must be located in]}
Each control edits a copy of models/verus/<stem>.rs and must make Verus fail, with an error located in one of the
expected functions. The unmutated baseline must verify. Results: models/verus/results/<stem>.json.

  python3 -I models/verus/controls.py            # all models with a controls file
  python3 -I models/verus/controls.py <stem>     # one model
"""
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
VERUS = "/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus"


def run(text: str):
    with tempfile.TemporaryDirectory() as d:
        f = Path(d) / "m.rs"
        f.write_text(text)
        p = subprocess.run([VERUS, str(f), "--rlimit", "30"], capture_output=True, text=True, timeout=1800)
        out = p.stdout + p.stderr
        m = re.search(r"verification results:: (\d+) verified, (\d+) errors", out)
        return p.returncode, (int(m.group(1)), int(m.group(2))) if m else None, out


def enclosing_fns(text: str, out: str):
    """Functions that contain the PRIMARY location of each error (the first `--> m.rs:N` after `error:`)."""
    lines = text.splitlines()
    names = set()
    primary = re.findall(r"^error[^\n]*\n\s*--> [^\n]*?m\.rs:(\d+):\d+", out, re.M)
    for n in primary:
        i = int(n) - 1
        while 0 <= i < len(lines):
            mm = re.match(r"\s*(?:pub )?(?:open |closed )?(?:proof |spec )?fn (\w+)", lines[i])
            if mm:
                names.add(mm.group(1))
                break
            i -= 1
    return sorted(names)


def run_model(stem: str) -> bool:
    src = HERE / f"{stem}.rs"
    controls = json.loads((HERE / "controls" / f"{stem}.json").read_text())
    base = src.read_text()
    rc, counts, out = run(base)
    ok = rc == 0 and counts is not None and counts[1] == 0
    print(f"[{stem}] baseline: {counts}")
    rows = []
    for c in controls:
        if c["old"] not in base:
            rows.append({"name": c["name"], "status": "harness-error", "detail": "pattern not found"})
            print(f"[{stem}] {c['name']}: HARNESS ERROR (pattern not found)")
            ok = False
            continue
        text = base.replace(c["old"], c["new"], 1)
        rc, cnt, out = run(text)
        where = enclosing_fns(text, out)
        killed = rc != 0 and (cnt is None or cnt[1] > 0)
        located = any(e in where for e in c["expect"])
        status = "killed" if killed and located else ("killed-elsewhere" if killed else "survived")
        rows.append({"name": c["name"], "description": c["description"], "status": status, "errors_in": where,
                     "expected_in": c["expect"], "counts": cnt})
        print(f"[{stem}] {c['name']}: {status} (errors in {where}; expected {c['expect']})")
        ok = ok and status == "killed"
    (HERE / "results").mkdir(exist_ok=True)
    (HERE / "results" / f"{stem}.json").write_text(json.dumps(
        {"baseline": counts, "controls": rows, "all_killed_in_expected_place": ok and len(rows) > 0}, indent=2) + "\n")
    return ok


def main(argv):
    stems = argv or sorted(p.stem for p in (HERE / "controls").glob("*.json"))
    return 0 if all([run_model(s) for s in stems]) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
