#!/usr/bin/env python3
"""Compare OBS lines of the 2026.1 run (logs 2x/3x) with the second-version run (logs 9[0-4]-v2-*, names ending @<ver>).
Usage: python3 -I validation/iris/n1/compare_versions.py [suffix]   (prints one line per observation)"""
import re, sys
from pathlib import Path
LOGS = Path(__file__).resolve().parents[2] / "logs"
OBS = re.compile(r"OBS\|([^|]+)\|(.*)$")
suf = sys.argv[1] if len(sys.argv) > 1 else "@2025.3"
def load(globs):
    out = {}
    for g in globs:
        for p in sorted(LOGS.glob(g)):
            for line in p.read_text(errors="replace").splitlines():
                m = OBS.search(line)
                if m and not m.group(1).startswith("error-"):
                    out[m.group(1)] = (dict(kv.split("=", 1) for kv in m.group(2).split(";") if "=" in kv), p.name)
    return out
base = load(["20-*.log", "3[0-5]-*.log"])
v2 = {k[: -len(suf)]: v for k, v in load(["9[0-4]-v2-*.log"]).items() if k.endswith(suf)}
VOLATILE = {"commandAfter"}
same = diff = 0
for name in sorted(set(base) | set(v2)):
    if name not in v2: print(f"MISSING-IN-V2 {name}"); continue
    if name not in base: print(f"ONLY-IN-V2 {name}"); continue
    a, b = base[name][0], v2[name][0]
    d = {k: (a[k], b[k]) for k in set(a) & set(b) if a[k] != b[k] and k not in VOLATILE}
    extra = sorted(set(a) ^ set(b))  # keys only one probe version reports (probe added keys after the 2026.1 run)
    if extra: print(f"NOTE {name}: keys reported on one side only (probe version, not compared): {extra}")
    if d: diff += 1; print(f"DIFF {name}: " + "; ".join(f"{k}: 2026.1={x!r} {suf[1:]}={y!r}" for k, (x, y) in sorted(d.items())))
    else: same += 1; print(f"SAME {name}")
print(f"TOTAL same={same} diff={diff}")
