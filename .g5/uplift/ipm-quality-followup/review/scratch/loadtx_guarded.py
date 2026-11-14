#!/usr/bin/env python3
"""Reviewer scratch: COMMITTED on ended rows regardless of Finalized; REC-EXISTS split. python3 -I -B loadtx_guarded.py VV_DIR"""
import importlib.util, os, sys
from collections import Counter
VV = os.path.abspath(sys.argv[1])
spec = importlib.util.spec_from_file_location("loadtx", os.path.join(VV, "followup", "models", "loadtx.py"))
LT = importlib.util.module_from_spec(spec); spec.loader.exec_module(LT)

def committed_ended(cfg, obs, acts):
    """COMMITTED as in loadtx.check, but for every row with TimeEnd set (finalized or not)."""
    if cfg.caller_tx:
        return []
    snap = obs.get("settled") or obs.get("returned")
    got = LT.mods_of(snap, acts)
    out = []
    for a in acts:
        row = snap["v"].get(("hist", a))
        if row and row.get("tend") and not row.get("fin"):
            kept = any(got[b] != {"present": 0, "res": 0} for b in acts) if a == "top" else got[a] != {"present": 0, "res": 0}
            if bool(row["com"]) != kept:
                out.append(f"COMMITTED-UNFIN[{a}]")
    return out

for variant in ("REPAIR-103", "REPAIR-103-GUARDED"):
    c, rec = Counter(), Counter()
    for cfg in LT.domain([variant]):
        steps, acts, _, _ = LT.program(cfg)
        ends = [("return", None)] + [(e, k) for k in range(1, len(steps)) for e in ("terminate", "sigkill")]
        for end, cut in ends:
            if end != "return" and cfg.caller_tx:
                continue
            obs = LT.run(cfg, {}, cut, end)
            for x in committed_ended(cfg, obs, acts):
                ownnj = x != "COMMITTED-UNFIN[top]" and cfg.dep_defs[int(x.split("[dep")[1][0]) - 1] == "NoJournal"
                c[(x.split("[")[0] + ("[dep]" if "[dep" in x else "[top]"), end, "own NoJournal" if ownnj else "no own NoJournal")] += 1
            if end == "return" and cfg.caller_tx:
                for x in LT.check(cfg, "A", "B", obs, acts, "rollback"):
                    if x.startswith("REC-EXISTS"):
                        rec[(x, "caller-rollback")] += 1
            for x in LT.check(cfg, "A", "B", obs, acts):
                if x.startswith("REC-EXISTS"):
                    rec[(x, end)] += 1
    print(variant, "COMMITTED on ended, unfinalized rows:", dict(c))
    print(variant, "REC-EXISTS split:", dict(rec))
