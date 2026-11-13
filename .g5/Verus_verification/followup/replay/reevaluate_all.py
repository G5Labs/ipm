#!/usr/bin/env python3
"""Re-evaluate every saved native replay run with the corrected evaluator and tabulate the delta.

  python3 -I -B followup/replay/reevaluate_all.py OUT_DIR

Writes OUT_DIR/<run_id>.verdict.json per run (via evaluate_replay2.py), OUT_DIR/summary.json and
OUT_DIR/delta.md. The original verdict files stay where they are; this script only reads them.
"""
import json, os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
VV = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
import l1sem as L  # noqa: E402

NATIVE = os.path.join(VV, "results", "native")
CASES = os.path.join(VV, "cases", "cases.json")
# (run, original verdict file evaluated against the final cases.json or, failing that, the latest one, status)
RUNS = [
    ("20261113T193745Z-replay-return", "verdict-cases-05842975.json", "superseded (binding errors, attempt 1)"),
    ("20261113T193952Z-replay-return-b", "verdict-cases-10410864.json", "final"),
    ("20261113T194122Z-replay-terminate-b", "verdict-cases-10410864.json", "final"),
    ("20261113T194155Z-replay-sigkill-b", "verdict-cases-10410864.json", "final"),
    ("20261113T204430Z-replay-approx-b", "verdict-cases-05842975.json", "superseded approximation (stall line after SetCommitted)"),
    ("20261113T205137Z-replay-c51-c52", "verdict-cases-10410864.json", "final"),
    ("20261113T205218Z-replay-approx-c", "verdict.json", "final approximation"),
]


def kept_any_vs_slot(case, o, slots, snap):
    """Where the original any-slot `kept` and the per-slot `kept` differ for a dependency row that
    requires Committed to equal "changes kept" (R02a)."""
    e = L.decode_expect(case["l1_permitted"])
    before = {s: L.project_mod(o, "before.", s, s in snap) for s in slots}
    after = {s: L.project_mod(o, "", s, s in snap) for s in slots}
    out = []
    for s in slots[1:]:
        if e["recs"].get(s, {}).get("com") == 3 and o.get(f"history.{s}.row") == "1":
            ka, ks = L.kept(before, after, slots, "top"), L.kept(before, after, slots, s)
            if ka != ks:
                out.append({"slot": s, "kept_any_slot": ka, "kept_own_slot": ks, "committed": o.get(f"history.{s}.committed")})
    return out


def observability(case, vcase):
    """Which corrected checks the original evaluator did not make for this case."""
    e = L.decode_expect(case["l1_permitted"])
    kinds = sorted({m[0] for a in e["alts"] for m in a.values()})
    notes = []
    if 1 in kinds or 6 in kinds:
        notes.append("resources now required for 'installed' (R01a)")
    if 2 in kinds:
        notes.append("document version now required for 'manifest only' (R01b)")
    if 0 in kinds or 5 in kinds or 6 in kinds:
        notes.append("'unchanged' now three-valued and includes the LastUpdated write proxy (R01c)")
    if len(e["alts"]) > 1:
        notes.append(f"{len(e['alts'])} alternatives, each reported per slot (R01f)")
    return notes


def main(argv):
    out = argv[0]
    os.makedirs(out, exist_ok=True)
    cases = {c["id"]: c for c in json.load(open(CASES))["cases"]}
    rows, summary = [], {"runs": [], "counts": {}}
    for run, orig_name, status in RUNS:
        rd = os.path.join(NATIVE, run)
        p = subprocess.run([sys.executable, "-I", "-B", os.path.join(HERE, "evaluate_replay2.py"), rd, "--cases", CASES, "--out", out],
                           capture_output=True, text=True)
        if p.returncode != 0:
            print(p.stdout, p.stderr)
            return 1
        v = json.load(open(os.path.join(out, run + ".verdict.json")))
        orig = json.load(open(os.path.join(rd, orig_name)))
        summary["runs"].append({"run": run, "status": status, "original_verdict_file": orig_name,
                                "original_evaluated_against": orig.get("cases_json_sha256"), "corrected_counts": v["counts"],
                                "original_counts": orig.get("counts"), "run_flags": v["run_flags"],
                                "observed_but_not_scheduled": v["observed_but_not_scheduled"]})
        rec = json.load(open(os.path.join(rd, "receipt.json")))
        for cid, r in v["cases"].items():
            oc = orig["cases"].get(cid, {})
            c = cases[cid]
            # Re-derive the observation the corrected evaluator used, for the R02a column.
            import evaluate_replay2 as E2
            obs, marks, *_ = E2.parse_log(os.path.join(rd, "raw.log"))
            o, _, _ = E2.collapse(obs.get(cid, {}))
            slots = ["top"] + ["dep1", "dep2"][: len(c["inputs"]["dependencies"])]
            snap = {s for s, d in zip(["dep1", "dep2"], c["inputs"]["dependencies"]) if d.get("snapshot")}
            row = {"case": cid, "run": run, "run_status": status,
                   "original_l2": oc.get("l2_verdict"), "corrected_l2": r["l2_verdict"],
                   "original_l1": oc.get("l1_verdict"), "corrected_l1": r["l1_verdict"],
                   "original_l1_fields": oc.get("l1_violations"), "corrected_failed_clauses": r.get("l1_failed_clauses"),
                   "corrected_unknown_clauses": r.get("l1_unknown_clauses"), "model_violated_clauses": r.get("model_violated_clauses"),
                   "clause_relation_to_model": r.get("clause_relation_to_model"),
                   "original_boolean_match": oc.get("native_matches_b1_l1_judgement"),
                   "evidence_problems": r["evidence"].get("problems"),
                   "l1_permitted_revised_after_run": r["evidence"].get("l1_permitted_revised_after_run"),
                   "aux_refusal_kind_ok": (r.get("aux_refusal_kind") or {}).get("ok"),
                   "r02a_kept_difference": kept_any_vs_slot(c, o, slots, snap),
                   "observability_changes": observability(c, r),
                   "helpers_sha256": rec.get("helpers_sha256")}
            row["verdict_changed"] = (row["original_l1"] != row["corrected_l1"]) or (row["original_l2"] != row["corrected_l2"])
            rows.append(row)
    json.dump({"summary": summary, "cases": rows}, open(os.path.join(out, "summary.json"), "w"), indent=1, sort_keys=True)
    final = [r for r in rows if r["run_status"].startswith("final")]
    lines = ["# Replay re-evaluation: original versus corrected evaluator", "",
             f"Target `cases.json`: `{json.load(open(os.path.join(out, RUNS[1][0] + '.verdict.json')))['target_cases_sha256']}`. "
             "Original verdicts are read from each run's own verdict file and left in place.", "",
             "## Counts", "", "| Run | Status | Original | Corrected |", "|---|---|---|---|"]
    for s in summary["runs"]:
        fmt = lambda d: ", ".join(f"{k}={v}" for k, v in sorted((d or {}).items()))
        lines.append(f"| `{s['run']}` | {s['status']} | {fmt(s['original_counts'])} | {fmt(s['corrected_counts'])} |")
    changed = [r for r in rows if r["verdict_changed"]]
    lines += ["", f"Final-run cases: {len(final)}. Verdict changed in final runs: {sum(1 for r in final if r['verdict_changed'])}. "
              f"Clause sets equal to the model's in final runs: {sum(1 for r in final if r['clause_relation_to_model'] == 'same clauses')}. "
              f"Changed anywhere (including superseded runs): {len(changed)}.", "",
              "## Case by case", "",
              "| Case | Run | Original l2 / l1 | Corrected l2 / l1 | Corrected failed clauses | Model | Relation | l1 expectation revised after run | R02a kept difference | Checks the original did not make |",
              "|---|---|---|---|---|---|---|---|---|---|"]
    for r in rows:
        lines.append(f"| {r['case']} | `{r['run'][:16]}…{r['run'][-10:]}` | {r['original_l2']} / {r['original_l1']} | {r['corrected_l2']} / {r['corrected_l1']} | "
                     f"{', '.join(r['corrected_failed_clauses'] or []) or '-'}{' (unknown: ' + ', '.join(r['corrected_unknown_clauses']) + ')' if r['corrected_unknown_clauses'] else ''} | "
                     f"{', '.join(r['model_violated_clauses'] or []) or '-'} | {r['clause_relation_to_model'] or '-'} | {'yes' if r['l1_permitted_revised_after_run'] else 'no'} | "
                     f"{'; '.join(str(x) for x in r['r02a_kept_difference']) or '-'} | {'; '.join(r['observability_changes']) or '-'} |")
    open(os.path.join(out, "delta.md"), "w").write("\n".join(lines) + "\n")
    print("rows", len(rows), "final", len(final), "changed", len(changed), "changed-final", sum(1 for r in final if r["verdict_changed"]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
