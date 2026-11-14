#!/usr/bin/env python3
"""Reviewer scratch: the REPAIR-103 Terminate-between-mark-and-commit bucket. python3 -I -B loadtx_window.py VV_DIR"""
import importlib.util, os, sys
from collections import Counter
VV = os.path.abspath(sys.argv[1])
spec = importlib.util.spec_from_file_location("loadtx", os.path.join(VV, "followup", "models", "loadtx.py"))
LT = importlib.util.module_from_spec(spec); spec.loader.exec_module(LT)
WIN = ("C1-late", "C1-commit", "C2-late", "C2-commit", "C3-late", "C3-commit")
rows = []
for cfg in LT.domain(["REPAIR-103"]):
    steps, acts, _, _ = LT.program(cfg)
    for k in range(1, len(steps)):
        lab = steps[k][0]
        if lab.split(":")[1] not in WIN:
            continue
        obs = LT.run(cfg, {}, k, "terminate")
        if obs["cut_label"] != lab:
            continue
        for c in LT.check(cfg, "A", "B", obs, acts):
            if not c.startswith("COMMITTED"):
                continue
            s = c.split("[")[1].rstrip("]")
            own = s != "top" and cfg.dep_defs[int(s[3:]) - 1] == "NoJournal"
            marked = any(t.endswith("-mark") and t.startswith("top:") for t in obs["trace"])
            last_mark = [t for t in obs["trace"] if t.endswith("-mark")][-1:] 
            sig = repr(sorted(obs["settled"]["v"].items()))
            rows.append((cfg.name(), lab, c, own, last_mark[0] if last_mark else None, sig))
notown = [r for r in rows if not r[3] and r[2] != "COMMITTED[top]"]
print("window COMMITTED violations:", len(rows), "non-own-NoJournal dep:", len(notown))
print("  end states (config, cut):", len({(r[0], r[1]) for r in notown}), "distinct (config, settled):", len({(r[0], r[5]) for r in notown}))
print("  by cut:", Counter(r[1] for r in notown))
print("  by last mark step before the cut:", Counter(r[4] for r in notown))
d2 = [r for r in rows if r[1].startswith("dep2:")]
for r in d2[:3]:
    print("  dep2 example:", r[0], r[1], r[2], "own NoJournal:", r[3], "last mark:", r[4])
