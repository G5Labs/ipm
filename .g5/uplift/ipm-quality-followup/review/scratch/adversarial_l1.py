#!/usr/bin/env python3
"""Reviewer scratch: adversarial observations against followup/replay/l1sem.py. Run with python3 -I -B.

  python3 -I -B adversarial_l1.py VV_DIR
"""
import json, os, sys

VV = os.path.abspath(sys.argv[1])
sys.path.insert(0, os.path.join(VV, "followup", "replay"))
import l1sem as L  # noqa: E402
import evaluate_replay2 as E2  # noqa: E402

cases = {c["id"]: c for c in json.load(open(os.path.join(VV, "cases", "cases.json")))["cases"]}
RUN_B = os.path.join(VV, "results", "native", "20261113T193952Z-replay-return-b")
obs, marks, env, ne, stray = E2.parse_log(os.path.join(RUN_B, "raw.log"))


def saved(cid):
    v, c, _ = E2.collapse(obs[cid])
    return v


def slots_of(case):
    return ["top"] + ["dep1", "dep2"][: len(case["inputs"]["dependencies"])]


def run(cid, mut, label):
    c = cases[cid]
    o = dict(saved(cid))
    for k, v in mut.items():
        if v is None:
            o.pop(k, None)
        else:
            o[k] = v
    cl = L.l1_clauses(L.decode_expect(c["l1_permitted"]), o, slots_of(c))
    res = {k: E2.tri(v["ok"]) for k, v in cl.items()}
    fails = sorted(k for k, v in res.items() if v == "FAIL")
    unk = sorted(k for k, v in res.items() if v == "UNKNOWN")
    print(f"{label:70s} {cid}: FAIL={fails} UNKNOWN={unk}")
    return res


# 1. malformed status where the contract requires an error
c = cases["B1-C05"]
print("B1-C05 status required:", c["l1_permitted"]["status"], "observed:", saved("B1-C05").get("status"))
run("B1-C05", {}, "saved observation")
run("B1-C05", {"status": "OKAY"}, "malformed status 'OKAY' (driver never writes it)")
run("B1-C05", {"status": "ok"}, "malformed status 'ok'")
# 2. same for a success-required case
print("B1-C01 status required:", cases["B1-C01"]["l1_permitted"]["status"])
run("B1-C01", {"status": "OKAY"}, "malformed status 'OKAY' where OK required")

# 3. kind 0 unchanged, record absent before/after, resources appear (res changes)
print("B1-C05 alts:", cases["B1-C05"]["l1_permitted"]["modules_one_of"])
print("B1-C05 before top:", {k: v for k, v in saved("B1-C05").items() if "modules.top" in k})
run("B1-C05", {"modules.top.resources_loaded": "1"}, "absent record, resources now loaded (kind 0)")
run("B1-C05", {"modules.top.zpm_document_version": "2.0.0"}, "absent record, stored document now present (kind 0)")

# 4. history row present for a slot where only an absent row was printed (row key missing)
run("B1-C01", {"history.top.row": None}, "top row key missing")
# 5. phases count as non-digit
run("B1-C01", {"history.top.phases_started": "five"}, "phases_started malformed")
# 6. present module with blank LastUpdated on both sides (equal blank)
c11 = saved("B1-C11")
print("B1-C11 last_updated before/after:", c11.get("before.modules.top.x.last_updated"), c11.get("modules.top.x.last_updated"))
