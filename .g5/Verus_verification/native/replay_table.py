#!/usr/bin/env python3
"""Print the per-case markdown table and the l2 disagreement list from replay verdict.json files.
Usage: python3 -I replay_table.py RUN_DIR [RUN_DIR ...]  (later runs override earlier ones for the same case)"""
import json, os, sys


FINAL = None  # e.g. "verdict-cases-05842975.json": l1 against the final cases.json, shown in its own column


def main(runs):
    global FINAL
    if runs and runs[0].startswith("--final="):
        FINAL = runs[0].split("=", 1)[1]
        runs = runs[1:]
    rows, dis, fin = {}, [], {}
    for run in runs:
        if FINAL and os.path.isfile(os.path.join(run, FINAL)):
            for cid, r in json.load(open(os.path.join(run, FINAL)))["cases"].items():
                fin[cid] = r
        v = json.load(open(os.path.join(run, "verdict.json")))
        rid = os.path.basename(os.path.normpath(run))
        for cid, r in v["cases"].items():
            rows[cid] = (rid, r)
        for cid, notes in v.get("not_realized", {}).items():
            rows.setdefault(cid, (rid, {"title": "", "l2_verdict": "NOT REALIZED", "l1_verdict": "NOT REALIZED",
                                        "l2_disagreements": [], "l1_violations": [], "b1_l1_vs_l2": {}, "binding_notes": notes}))
    key = lambda c: (int(c[4:6]), c[6:])
    print("| Case | Title | l2 (model vs product) | l1 vs replayed cases.json | l1 vs final cases.json | B1 l1_vs_l2 (final) | Native l1 matches B1 | Run |")
    print("|---|---|---|---|---|---|---|---|")
    for cid in sorted(rows, key=key):
        rid, r = rows[cid]
        f = fin.get(cid)
        b1 = (f or r).get("b1_l1_vs_l2", {})
        b1s = ("conforms" if b1.get("conforms") else "violates " + ", ".join(b1.get("violated_clauses", []))) if b1 else ""
        l2 = r["l2_verdict"] + (" (" + ", ".join(r["l2_disagreements"]) + ")" if r["l2_disagreements"] else "")
        l1 = r["l1_verdict"] + (" (" + ", ".join(r["l1_violations"]) + ")" if r["l1_violations"] else "")
        l1f = (f["l1_verdict"] + (" (" + ", ".join(f["l1_violations"]) + ")" if f["l1_violations"] else "")) if f else "same file"
        m = (f or r).get("native_matches_b1_l1_judgement")
        print(f"| {cid} | {r.get('title','')} | {l2} | {l1} | {l1f} | {b1s} | {'' if m is None else ('yes' if m else 'NO')} | `{rid}` |")
        for f in r["l2_disagreements"]:
            e = r["l2_fields"][f]
            dis.append((cid, f, e.get("predicted"), e.get("observed"), rid))
    print()
    print("| Case | Field | l2_predicted | Observed | Run |")
    print("|---|---|---|---|---|")
    for d in dis:
        print(f"| {d[0]} | `{d[1]}` | {d[2]} | {d[3]} | `{d[4]}` |")


if __name__ == "__main__":
    main(sys.argv[1:])
