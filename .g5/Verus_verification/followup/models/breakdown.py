#!/usr/bin/env python3
"""Breakdown of the loadtx Committed violations by row, end kind and cause, under the product's
policy (Q-22 A, Q-23 B). Run with python3 -I -B:  python3 -I -B breakdown.py OUT_DIR

Causes: "dep-own-NoJournal" (a dependency declares NoJournal under a transactional parent),
"load-without-transaction" (the caller or the top module's defaults turn transactions off),
"other". Writes OUT_DIR/breakdown.json. The totals equal loadtx.json's per-clause counts.
"""
import collections, json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import loadtx as M  # noqa: E402

VARIANTS = ["as-built", "REPAIR-103", "REPAIR-103-GUARDED", "REPAIR-103-GUARDED-GLOBAL"]


def main(argv):
    out = {}
    for variant in VARIANTS:
        c = collections.Counter()
        for cfg in M.domain([variant]):
            steps, acts, _, _ = M.program(cfg)
            ends = [("return", None)] + [(e, k) for k in range(1, len(steps)) for e in ("terminate", "sigkill")]
            for end, cut in ends:
                if end != "return" and cfg.caller_tx:
                    continue
                o = M.run(cfg, {}, cut, end)
                parent_tx = cfg.caller is None and cfg.top_def is None
                nj = parent_tx and any(d == "NoJournal" for d in cfg.dep_defs)
                cause = "dep-own-NoJournal" if nj else ("load-without-transaction" if not parent_tx else "other")
                for cl in M.check(cfg, "A", "B", o, acts):
                    if cl.startswith("COMMITTED"):
                        row = "top" if "[top]" in cl else "dependency"
                        c[f"{cl.split('[')[0]} | {row} | {end} | {cause}"] += 1
        out[variant] = dict(sorted(c.items()))
        print("==", variant)
        for k, n in sorted(c.items()):
            print(f"  {n:6d}  {k}")
    os.makedirs(argv[0], exist_ok=True)
    json.dump(out, open(os.path.join(argv[0], "breakdown.json"), "w"), indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
