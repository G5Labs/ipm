#!/usr/bin/env python3
"""Reviewer scratch: decompose the loadtx claims. Run with python3 -I -B.

  python3 -I -B loadtx_claims.py VV_DIR

1. Which end states make M-BEFORE-IMAGE differ from as-built (REC-EXISTS 9,942 vs 9,558)?
2. as-built COMMITTED 30 under Q-22 A/Q-23 B: configurations and end kinds.
3. REPAIR-103 COMMITTED 962: split by slot, by dependency own NoJournal, by end kind, and the
   Terminate-between-mark-and-commit window (cut at C*-late or C*-commit after a C*-mark).
"""
import importlib.util, json, os, sys
from collections import Counter

VV = os.path.abspath(sys.argv[1])
spec = importlib.util.spec_from_file_location("loadtx", os.path.join(VV, "followup", "models", "loadtx.py"))
LT = importlib.util.module_from_spec(spec)
spec.loader.exec_module(LT)

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))


def violations(variant, q=("A", "B"), runner=None):
    out = []
    for cfg in LT.domain([variant]):
        steps, acts, _, _ = LT.program(cfg)
        ends = [("return", None)] + [(e, k) for k in range(1, len(steps)) for e in ("terminate", "sigkill")]
        for end, cut in ends:
            if end != "return" and cfg.caller_tx:
                continue
            obs = (runner or LT.run)(cfg, {}, cut, end)
            cl = LT.check(cfg, q[0], q[1], obs, acts)
            if end == "return" and cfg.caller_tx:
                cl = sorted(set(cl) | {c + "@caller-rollback" for c in LT.check(cfg, q[0], q[1], obs, acts, "rollback")})
            for c in cl:
                out.append({"cfg": cfg, "end": end, "cut": obs.get("cut_label"), "clause": c, "trace": obs["trace"]})
    return out


def key(v):
    return (v["cfg"].name().replace("variant=" + v["cfg"].variant, ""), v["end"], v["cut"], v["clause"])


# 1
ab = violations("as-built")
bi = violations("M-BEFORE-IMAGE")
extra = sorted(set(map(key, bi)) - set(map(key, ab)))
missing = sorted(set(map(key, ab)) - set(map(key, bi)))
print("1. M-BEFORE-IMAGE extra violations:", len(extra), "missing:", len(missing))
print("   end kinds:", Counter(e[1] for e in extra), "clauses:", Counter(e[3].split("[")[0] for e in extra))
own_nj = Counter("dep own NoJournal" if "'NoJournal'" in e[0].split("dep_defs=")[1].split("]")[0] else "no dep own NoJournal" for e in extra)
print("   ", own_nj)
# compare with the P-06c extras computed by loadtx_jm_order.py, if present
p = os.path.join(os.path.dirname(os.path.abspath(__file__)), "loadtx_jm_order.py")
spec2 = importlib.util.spec_from_file_location("jm", p)
import types
src = open(p).read().split("def tally(")[0]
jm = types.ModuleType("jm")
jm.__dict__["sys"] = sys
sys_argv = sys.argv
sys.argv = [p, VV, "/dev/null"]
exec(compile(src, p, "exec"), jm.__dict__)
sys.argv = sys_argv
p6 = violations("as-built", runner=jm.run_p06c)
extra6 = sorted(set(map(key, p6)) - set(map(key, ab)))
print("   P-06c extras:", len(extra6), "same set as M-BEFORE-IMAGE extras:", set(extra6) == set(extra))

# 2
com = [v for v in ab if v["clause"].startswith("COMMITTED")]
print("2. as-built COMMITTED:", len(com), Counter((v["end"], v["clause"]) for v in com))
print("   all with a dependency's own NoJournal:", all("NoJournal" in list(v["cfg"].dep_defs) for v in com),
      "top transactional:", all(v["cfg"].caller is None and v["cfg"].top_def is None for v in com),
      "faults:", Counter(v["cfg"].fault for v in com))

# 3
r = violations("REPAIR-103")
rc = [v for v in r if v["clause"].startswith("COMMITTED")]
print("3. REPAIR-103 COMMITTED:", len(rc))
def own_nj_of(v):
    s = v["clause"].split("[")[1].rstrip("]")
    return s != "top" and v["cfg"].dep_defs[int(s[3:]) - 1] == "NoJournal"
win = [v for v in rc if v["end"] == "terminate" and v["cut"] and v["cut"].split(":")[1] in ("C1-late", "C1-commit", "C2-late", "C2-commit", "C3-late", "C3-commit")]
print("   by slot kind:", Counter("top" if v["clause"] == "COMMITTED[top]" else "dep" for v in rc))
print("   dep with own NoJournal:", sum(1 for v in rc if own_nj_of(v)), "of which returned:", sum(1 for v in rc if own_nj_of(v) and v["end"] == "return"))
print("   top with some dep own NoJournal:", sum(1 for v in rc if v["clause"] == "COMMITTED[top]" and "NoJournal" in list(v["cfg"].dep_defs)),
      "top total:", sum(1 for v in rc if v["clause"] == "COMMITTED[top]"))
print("   window (terminate cut at C*-late/C*-commit):", len(win), "violations;",
      len({(v['cfg'].name(), v['cut']) for v in win}), "end states;", "cut labels:", Counter(v["cut"] for v in win))
print("   window violations whose slot has own NoJournal:", sum(1 for v in win if own_nj_of(v)))
rest = [v for v in rc if v not in win and not own_nj_of(v) and v["clause"] != "COMMITTED[top]"]
print("   dep violations neither window nor own NoJournal:", len(rest), Counter((v["end"], v["cut"]) for v in rest).most_common(5))
g = violations("REPAIR-103-GUARDED")
gc = [v for v in g if v["clause"].startswith("COMMITTED")]
print("   GUARDED COMMITTED:", len(gc), "all with some dep own NoJournal:", all("NoJournal" in list(v["cfg"].dep_defs) for v in gc),
      "window:", sum(1 for v in gc if v["end"] == "terminate" and v["cut"] and v["cut"].split(":")[1] in ("C1-late", "C1-commit", "C2-late", "C2-commit", "C3-late", "C3-commit")))
