"""Break down unfinalized dependency rows whose Committed disagrees with kept module writes,
for REPAIR-103-GUARDED, by direction and by the dependency's own defaults and the load mode."""
import sys, importlib.util, collections
spec = importlib.util.spec_from_file_location("loadtx", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
for variant in sys.argv[2:]:
    c = collections.Counter()
    for cfg in m.domain([variant]):
        if cfg.caller_tx:
            continue
        steps, acts, _, _ = m.program(cfg)
        ends = [("return", None)] + [(e, k) for k in range(1, len(steps)) for e in ("terminate", "sigkill")]
        for end, cut in ends:
            obs = m.run(cfg, {}, cut, end)
            snap = obs["settled"] if end != "return" else obs["returned"]
            for i, a in enumerate(acts[1:]):
                row = snap["v"].get(("hist", a))
                if not row or (row.get("fin") and row.get("tend")):
                    continue
                mod = snap["v"].get(("mod", a), {"present": 0, "res": 0})
                kept = bool(mod.get("present", 0) or mod.get("res", 0))
                if bool(row["com"]) == kept:
                    continue
                own = cfg.dep_defs[i]
                loadmode = cfg.caller or cfg.top_def or "tx"
                c[(end, "com=1,kept=0" if row["com"] else "com=0,kept=1", "own=" + str(own), "load=" + str(loadmode), "dev" if cfg.dev else "nodev")] += 1
    print(variant)
    for k, v in sorted(c.items(), key=lambda kv: -kv[1])[:25]: print("  ", v, k)
