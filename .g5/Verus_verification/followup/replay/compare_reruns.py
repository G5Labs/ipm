#!/usr/bin/env python3
"""R03f: compare a fresh re-execution of replay cases (committed driver, bound at start) with the
archived run, field by field. Run with python3 -I -B from .g5/Verus_verification.

  python3 -I -B followup/replay/compare_reruns.py OUT_DIR OLD_RUN NEW_RUN [OLD_RUN NEW_RUN ...]

Writes OUT_DIR/compare.json and prints one line per case. Timestamps (`x.last_updated` values) and
the free-text error are compared by shape only: whether a value is present, and whether the
before/after LastUpdated values differ (the write proxy).
"""
import json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluate_replay2 as E2  # noqa: E402


def observations(run):
    obs, marks, *_ = E2.parse_log(os.path.join(run, "raw.log"))
    return {cid: E2.collapse(m)[0] for cid, m in obs.items()}


def normalize(o):
    out = {}
    for k, v in o.items():
        if k.endswith("x.last_updated"):
            out[k] = "<set>" if v else v
        elif k == "err":
            out[k] = "<set>" if v else v
        else:
            out[k] = v
    for k in [k for k in o if k.startswith("modules.") and k.endswith("x.last_updated")]:
        b = "before." + k
        if b in o:
            out[k + ".changed"] = str(o[k] != o[b])
    return out


def main(argv):
    out = argv[0]
    pairs = list(zip(argv[1::2], argv[2::2]))
    rows = {}
    for old, new in pairs:
        a, b = observations(old), observations(new)
        for cid in sorted(set(a) | set(b)):
            x, y = normalize(a.get(cid, {})), normalize(b.get(cid, {}))
            diff = sorted(k for k in set(x) | set(y) if x.get(k) != y.get(k))
            rows[cid] = {"old_run": os.path.basename(old), "new_run": os.path.basename(new), "fields_compared": len(set(x) | set(y)),
                         "differing": {k: [x.get(k), y.get(k)] for k in diff}}
            print(cid, "identical" if not diff else f"{len(diff)} differ: " + ", ".join(diff[:6]))
    os.makedirs(out, exist_ok=True)
    same = sum(1 for r in rows.values() if not r["differing"])
    json.dump({"cases": rows, "identical_cases": same, "total_cases": len(rows)}, open(os.path.join(out, "compare.json"), "w"), indent=1, sort_keys=True)
    print(f"identical {same}/{len(rows)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
