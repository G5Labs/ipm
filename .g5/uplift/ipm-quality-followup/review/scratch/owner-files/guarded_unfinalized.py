"""Count, for REPAIR-103 and REPAIR-103-GUARDED, end states where a dependency row exists,
is NOT finalized (so the model's COMMITTED clause skips it) and its Committed disagrees with
whether the dependency's module writes were kept. Read-only use of loadtx.py."""
import sys, importlib.util, collections
path = sys.argv[1]
spec = importlib.util.spec_from_file_location("loadtx", path)
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
for variant in ("REPAIR-103", "REPAIR-103-GUARDED"):
    c = collections.Counter()
    for cfg in m.domain([variant]):
        if cfg.caller_tx:
            continue
        steps, acts, _, _ = m.program(cfg)
        ends = [("return", None)] + [(e, k) for k in range(1, len(steps)) for e in ("terminate", "sigkill")]
        for end, cut in ends:
            obs = m.run(cfg, {}, cut, end)
            snap = obs["settled"] if end != "return" else obs["returned"]
            for a in acts[1:]:
                row = snap["v"].get(("hist", a))
                if not row:
                    continue
                kept = snap["v"].get(("mod", a), {"present": 0, "res": 0})
                kept = kept.get("present", 0) or kept.get("res", 0)
                wrong = bool(row["com"]) != bool(kept)
                fin = bool(row.get("fin") and row.get("tend"))
                c[(end if end == "return" else "process-end", "checked" if fin else "unchecked", "wrong" if wrong else "ok")] += 1
    print(variant)
    for k in sorted(c): print("  ", k, c[k])
