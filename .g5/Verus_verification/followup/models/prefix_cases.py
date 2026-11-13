#!/usr/bin/env python3
"""R06d: a permitted partial prefix and rejected impossible states at process ends, judged by the
follow-up PREFIX clause and by the original contract's process-end alternatives.

  python3 -I -B prefix_cases.py OUT_DIR      (writes OUT_DIR/prefix_cases.json; exit 1 on a mismatch)

The original contract (crates/ipm-load-history/src/contract.rs:184-195, l1_alts_kill) gives a
process end without a transaction `any_alt` (every module kind 3, "any"), and a developer-mode
process end kind 5 for the top module ("unchanged or present") and kind 6 for dependencies
("unchanged or installed"). l1sem.mexp_ok is the transcription of those kinds used by the
corrected replay evaluator.
"""
import json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "replay"))
import loadtx as M  # noqa: E402
import l1sem as L  # noqa: E402


def end_at(cfg, label, end="sigkill"):
    steps, acts, _, _ = M.program(cfg)
    k = [i for i, (l, _) in enumerate(steps) if l == label][0]
    return M.run(cfg, {}, k, end), acts


def prefix_ok(cfg, obs, acts):
    return not any(c.startswith("PREFIX") for c in M.check(cfg, "A", "B", obs, acts))


def with_mods(obs, mods):
    o = json.loads(json.dumps({"x": 0}))  # fresh dict
    o = dict(obs)
    st = dict(obs["settled"])
    v = {k: x for k, x in st["v"].items() if k[0] != "mod"}
    for a, m in mods.items():
        if m:
            v[("mod", a)] = {"present": m[0], "res": m[1], "dev": 0, "wr": 1}
    st["v"] = v
    o["settled"] = st
    return o


def mod_view(present, res, ver=2):
    """A module in the replay evaluator's projection (absent record: ver/dev not meaningful)."""
    if present:
        return {"present": True, "ver": ver, "dev": False, "res": bool(res), "doc": ver, "wr_proxy": "t1"}
    return {"present": False, "ver": L.ABSENT, "dev": L.ABSENT, "res": bool(res), "doc": 0, "wr_proxy": L.ABSENT}


ABSENT0 = {"present": False, "ver": L.ABSENT, "dev": L.ABSENT, "res": False, "doc": 0, "wr_proxy": L.ABSENT}


def main(argv):
    out = argv[0]
    rows = []
    # 1. NoTransaction load ended after its manifest write, before compile: a permitted partial prefix.
    cfg = M.Cfg("NoTransaction", None, (), False, None, False, "as-built")
    obs, acts = end_at(cfg, "top:write-res")
    settled = {a: (obs["settled"]["v"].get(("mod", a), {}).get("present", 0), obs["settled"]["v"].get(("mod", a), {}).get("res", 0)) for a in acts}
    orig_any = L.mexp_ok((3, 0, 2), ABSENT0, mod_view(*settled["top"]))
    rows.append({"case": "NoTransaction, SIGKILL before compile: record kept, no resources", "as_built_settled": settled,
                 "prefix_permits": prefix_ok(cfg, obs, acts), "original_permits": orig_any, "expect": [True, True]})
    # 2. The same end with resources and no module record: impossible (resources are compiled after the manifest).
    bad = with_mods(obs, {"top": (0, 1)})
    rows.append({"case": "NoTransaction, process end: resources without a module record (impossible)", "prefix_permits": prefix_ok(cfg, bad, acts),
                 "original_permits": L.mexp_ok((3, 0, 2), ABSENT0, mod_view(0, 1)), "expect": [False, True]})
    # 3. -dev load with a dependency ended after C1, before the dependency: manifest kept, dependency untouched.
    cfg = M.Cfg(None, None, (None,), True, None, False, "as-built")
    obs, acts = end_at(cfg, "dep1:log-init")
    settled = {a: (obs["settled"]["v"].get(("mod", a), {}).get("present", 0), obs["settled"]["v"].get(("mod", a), {}).get("res", 0)) for a in acts}
    o5 = L.mexp_ok((5, 2, 1), ABSENT0, {**mod_view(*settled["top"]), "dev": True})
    o6 = L.mexp_ok((6, 1, 2), ABSENT0, mod_view(*settled["dep1"], ver=1))
    rows.append({"case": "-dev, SIGKILL after the first commit, before the dependency: manifest kept", "as_built_settled": settled,
                 "prefix_permits": prefix_ok(cfg, obs, acts), "original_permits": bool(o5 and o6), "expect": [True, True]})
    # 4. -dev end: dependency installed while the top manifest is absent. Impossible: the dependency is
    #    committed by C2, after C1 committed the top manifest.
    bad = with_mods(obs, {"top": None, "dep1": (1, 1)})
    o5 = L.mexp_ok((5, 2, 1), ABSENT0, ABSENT0)
    o6 = L.mexp_ok((6, 1, 2), ABSENT0, mod_view(1, 1, ver=1))
    rows.append({"case": "-dev, process end: dependency installed, top manifest absent (impossible)", "prefix_permits": prefix_ok(cfg, bad, acts),
                 "original_permits": bool(o5 and o6), "expect": [False, True]})
    # 5. -dev end after C2 with the top's compile done: phase work kept (permitted, and required).
    obs, acts = end_at(cfg, "top:activate")
    settled = {a: (obs["settled"]["v"].get(("mod", a), {}).get("present", 0), obs["settled"]["v"].get(("mod", a), {}).get("res", 0)) for a in acts}
    rows.append({"case": "-dev, SIGKILL inside Activate: manifest, resources and dependency kept", "as_built_settled": settled,
                 "prefix_permits": prefix_ok(cfg, obs, acts), "original_permits": True, "expect": [True, True]})
    # 6. The same end but the resources rolled back: not what IRIS does (compile ran outside a transaction).
    bad = with_mods(obs, {"top": (1, 0), "dep1": (1, 1)})
    o5 = L.mexp_ok((5, 2, 1), ABSENT0, {**mod_view(1, 0), "dev": True})
    rows.append({"case": "-dev, process end inside Activate: resources gone though compiled outside the transaction", "prefix_permits": prefix_ok(cfg, bad, acts),
                 "original_permits": bool(o5), "expect": [False, True]})
    ok = all([r["prefix_permits"], r["original_permits"]] == r["expect"] for r in rows)
    os.makedirs(out, exist_ok=True)
    json.dump({"rows": rows, "all_as_expected": ok}, open(os.path.join(out, "prefix_cases.json"), "w"), indent=1, default=str)
    for r in rows:
        print(("OK  " if [r["prefix_permits"], r["original_permits"]] == r["expect"] else "BAD ") + r["case"],
              "| PREFIX permits:", r["prefix_permits"], "| original permits:", r["original_permits"], "| settled:", r.get("as_built_settled"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
