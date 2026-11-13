#!/usr/bin/env python3
"""Evaluate one native run against expectations written before the run.

Usage (always run with python3 -I):
  python3 -I evaluate.py RUN_DIR EXPECTED.json [EXPECTED2.json ...]
        [--mutate CASE:FIELD=VALUE ...] [--index CONCERN] [--expect-summary TEXT]

Reads RUN_DIR/raw.log (OBS, CASE-BEGIN, CASE-END lines) and RUN_DIR/receipt.json, writes RUN_DIR/verdict.json.
Process outcome (receipt) and substantive verdict are kept separate. Each --mutate flips one observed
field after parsing and checks that the evaluation of that case changes; the result is recorded under
"mutation_controls" and the unmutated verdict is what the run reports.

Field expectations (in EXPECTED.json, cases.<id>.fields.<field>):
  "expect": "v"                exact string
  "expect": ["a", "b"]          any of
  "expect": {"re": "..."}       regular expression (fullmatch)
  "expect": {"eq_field": "f"}   equal to another observed field of the same case
  "expect": {"lt_field": "f"}   integer less than another observed field (used to guard against vacuous passes)
  "expect": null               observation only (no claim)
  "when": {"field": "f", "expect": ...}  apply only when another observed field satisfies the expectation
  "predicted_as_built": ...     optional code-read prediction (same forms), compared separately
"""
import hashlib
import json
import os
import re
import sys
from datetime import datetime, timezone

OBS_RE = re.compile(r"(?:^|>)\s*OBS (\S+) ([^=\s]+)=(.*)$")
BEGIN_RE = re.compile(r"(?:^|>)\s*CASE-BEGIN (\S+)")
END_RE = re.compile(r"(?:^|>)\s*CASE-END (\S+)")
ENV_PATTERNS = [
    ("license", re.compile(r"LICENSE LIMIT|License limit exceeded", re.I)),
    ("disk", re.compile(r"<DISKHARD>|<FILEFULL>|<DATABASE>|No space left", re.I)),
    ("store", re.compile(r"<STORE>")),
]


def parse_log(path):
    obs, begun, ended, env, dup = {}, [], [], set(), []
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for raw in fh:
            line = raw.rstrip("\r\n")
            m = OBS_RE.search(line)
            if m:
                case, field, value = m.group(1), m.group(2), m.group(3).rstrip()
                if field in obs.setdefault(case, {}):
                    dup.append(f"{case}:{field}")
                obs[case][field] = value
                continue
            m = BEGIN_RE.search(line)
            if m:
                begun.append(m.group(1))
                continue
            m = END_RE.search(line)
            if m:
                ended.append(m.group(1))
                continue
            for name, pat in ENV_PATTERNS:
                if pat.search(line):
                    env.add(name)
    return obs, begun, ended, sorted(env), dup


def check(spec, observed, case_obs):
    """Return True/False for a comparison, None when there is no claim."""
    if spec is None:
        return None
    if observed is None:
        return False
    if isinstance(spec, list):
        return observed in [str(s) for s in spec]
    if isinstance(spec, dict):
        if "re" in spec:
            return re.fullmatch(spec["re"], observed) is not None
        if "eq_field" in spec:
            other = case_obs.get(spec["eq_field"])
            return other is not None and other == observed
        if "ne_field" in spec:
            other = case_obs.get(spec["ne_field"])
            return other is not None and other != observed
        if "lt_field" in spec:
            other = case_obs.get(spec["lt_field"])
            try:
                return other is not None and int(observed) < int(other)
            except ValueError:
                return False
        raise ValueError(f"unknown expectation form {spec}")
    return observed == str(spec)


def evaluate_case(cid, cspec, case_obs, complete_markers):
    fields = {}
    missing, mismatched, harness_failed, pred_total, pred_hits = [], [], [], 0, 0
    for fname, fspec in cspec.get("fields", {}).items():
        observed = case_obs.get(fname)
        exp = fspec.get("expect")
        cond = fspec.get("when")
        if cond is not None and not check(cond["expect"], case_obs.get(cond["field"]), case_obs):
            fields[fname] = {"expected": exp, "source": fspec.get("source"), "observed": observed,
                             "match": "N/A", "condition": cond}
            continue
        res = check(exp, observed, case_obs)
        entry = {"expected": exp, "source": fspec.get("source"), "observed": observed, "match": res}
        if "predicted_as_built" in fspec:
            pres = check(fspec["predicted_as_built"], observed, case_obs)
            entry["predicted_as_built"] = fspec["predicted_as_built"]
            entry["prediction_source"] = fspec.get("prediction_source")
            entry["matches_prediction"] = pres
            pred_total += 1
            pred_hits += 1 if pres else 0
        if observed is None:
            missing.append(fname)
        elif res is False and (fspec.get("harness") or fspec.get("source") == "harness"):
            harness_failed.append(fname)
        elif res is False:
            mismatched.append(fname)
        fields[fname] = entry
    extra = {k: v for k, v in case_obs.items() if k not in fields}
    kind = cspec.get("kind", "case")
    if missing or harness_failed or not complete_markers:
        verdict = "INCOMPLETE"
    elif kind == "premise":
        verdict = "REFUTED" if mismatched else "CONFIRMED"
    else:
        verdict = "DEVIATES" if mismatched else "CONFORMS"
    out = {
        "kind": kind,
        "claim": cspec.get("claim"),
        "hypothesis": cspec.get("hypothesis"),
        "maps_to": cspec.get("maps_to"),
        "verdict": verdict,
        "mismatched_fields": mismatched,
        "missing_fields": missing,
        "harness_failed_fields": harness_failed,
        "case_markers_complete": complete_markers,
        "fields": fields,
        "unexpected_observations": extra,
    }
    if pred_total:
        out["as_built_prediction"] = {"fields": pred_total, "matched": pred_hits,
                                      "verdict": "CONFIRMED" if pred_hits == pred_total else "PARTLY_REFUTED"}
    return out


def file_meta(path):
    data = open(path, "rb").read()
    mtime = datetime.fromtimestamp(os.path.getmtime(path), tz=timezone.utc)
    return hashlib.sha256(data).hexdigest(), mtime


def main(argv):
    args = list(argv)
    mutations, index_concern, expect_summary = [], None, None
    use_current = "--use-current" in args
    args = [a for a in args if a != "--use-current"]
    pos = []
    i = 0
    while i < len(args):
        a = args[i]
        if a == "--mutate":
            mutations.append(args[i + 1]); i += 2
        elif a == "--index":
            index_concern = args[i + 1]; i += 2
        elif a == "--expect-summary":
            expect_summary = args[i + 1]; i += 2
        else:
            pos.append(a); i += 1
    if len(pos) < 2:
        print(__doc__)
        return 2
    run_dir, exp_paths = pos[0], pos[1:]
    receipt = json.load(open(os.path.join(run_dir, "receipt.json")))
    obs, begun, ended, env, dup = parse_log(os.path.join(run_dir, "raw.log"))
    started = datetime.strptime(receipt["started_utc"], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)

    cases_spec, exp_meta = {}, []
    for p in exp_paths:
        snap = os.path.join(run_dir, "inputs", os.path.basename(p))
        sha, mtime = file_meta(p)
        meta = {"path": os.path.relpath(p, run_dir), "sha256": sha, "mtime_utc": mtime.strftime("%Y-%m-%dT%H:%M:%SZ")}
        if os.path.isfile(snap) and not use_current:
            # The runner copied this file before the session started: evaluate against that copy.
            ssha, _ = file_meta(snap)
            meta.update({"evaluated_from": os.path.relpath(snap, run_dir), "snapshot_sha256": ssha,
                         "snapshot_matches_current": ssha == sha, "written_before_run": True})
            spec = json.load(open(snap))
        else:
            meta.update({"evaluated_from": meta["path"], "written_before_run": mtime < started,
                         "note": "no snapshot in this run; written_before_run compares the current file's mtime"})
            if os.path.isfile(snap):
                ssha, _ = file_meta(snap)
                meta.update({"snapshot_sha256": ssha, "expectations_changed_after_run": ssha != sha,
                             "note": "re-evaluation against the current expectations file (--use-current); "
                                     "the run's own snapshot is in inputs/"})
            spec = json.load(open(p))
        meta["group"] = spec.get("group")
        exp_meta.append(meta)
        cases_spec.update(spec["cases"])

    process_ok = receipt["process_outcome"] == "COMPLETED" and not env
    results = {}
    for cid, cspec in cases_spec.items():
        if cid not in obs and cid not in begun:
            continue  # case not part of this run
        markers = (cid in begun and cid in ended) or cspec.get("no_markers", False)
        res = evaluate_case(cid, cspec, obs.get(cid, {}), markers and process_ok)
        results[cid] = res

    controls = []
    for m in mutations:
        cid, rest = m.split(":", 1)
        fname, value = rest.split("=", 1)
        if cid not in results:
            controls.append({"mutation": m, "result": "case not in run"}); continue
        mutated = dict(obs.get(cid, {}))
        before = results[cid]
        mutated[fname] = value
        markers = (cid in begun and cid in ended) and process_ok
        after = evaluate_case(cid, cases_spec[cid], mutated, markers)
        detected = (after["verdict"] != before["verdict"]) or (
            after["fields"].get(fname, {}).get("match") != before["fields"].get(fname, {}).get("match"))
        controls.append({"mutation": m, "original_observed": obs.get(cid, {}).get(fname),
                         "verdict_before": before["verdict"], "verdict_after": after["verdict"],
                         "field_match_before": before["fields"].get(fname, {}).get("match"),
                         "field_match_after": after["fields"].get(fname, {}).get("match"),
                         "detected": detected})

    counts = {}
    for r in results.values():
        counts[r["verdict"]] = counts.get(r["verdict"], 0) + 1
    if not process_ok:
        substantive = "INCOMPLETE"
    elif not results:
        substantive = "INCOMPLETE"
    elif any(r["verdict"] == "INCOMPLETE" for r in results.values()):
        substantive = "PARTIAL"
    else:
        substantive = "EVALUATED"
    verdict = {
        "run_id": receipt["run_id"],
        "label": receipt["label"],
        "process_outcome": receipt["process_outcome"],
        "environment_flags": env,
        "duplicate_observations": dup,
        "expected_files": exp_meta,
        "substantive_verdict": substantive,
        "case_verdict_counts": counts,
        "cases": results,
        "mutation_controls": controls,
        "evaluated_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "evaluator": "native/evaluate.py",
    }
    vname = "verdict.json"
    if use_current:
        vname = "verdict-reeval-" + "-".join(e["sha256"][:8] for e in exp_meta) + ".json"
        verdict["re_evaluation"] = "Expectations changed after this run (vacuous-pass guards); the original verdict.json is kept."
    with open(os.path.join(run_dir, vname), "w") as fh:
        json.dump(verdict, fh, indent=2)
    if index_concern:
        idx = os.path.join(run_dir, "..", "..", "index.jsonl")
        line = {"concern": index_concern, "label": receipt["label"],
                "outcome": f"{receipt['process_outcome']}; " + ", ".join(f"{k}={v}" for k, v in sorted(counts.items())),
                "run_id": receipt["run_id"], "kind": "native", "variant": "as-built",
                "expect": expect_summary or ", ".join(e["path"] for e in exp_meta)}
        with open(os.path.normpath(idx), "a") as fh:
            fh.write(json.dumps(line) + "\n")
    print(json.dumps({"run_id": receipt["run_id"], "process": receipt["process_outcome"], "env": env,
                      "substantive": substantive, "counts": counts,
                      "per_case": {k: v["verdict"] + (" mismatch=" + ",".join(v["mismatched_fields"]) if v["mismatched_fields"] else "")
                                   + (" harness_failed=" + ",".join(v["harness_failed_fields"]) if v["harness_failed_fields"] else "")
                                   + (" missing=" + ",".join(v["missing_fields"]) if v["missing_fields"] else "")
                                   for k, v in results.items()},
                      "controls": [(c["mutation"], c.get("detected")) for c in controls]}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
