#!/usr/bin/env python3
"""Corrected evaluator for native replays of the model cases. Run with python3 -I -B.

  python3 -I -B evaluate_replay2.py RUN_DIR --cases TARGET_CASES_JSON --out NEW_EVIDENCE_DIR
                                    [--mutate CASE:FIELD=VALUE ...] [--drop CASE:FIELD ...]

Reads RUN_DIR/{receipt.json,input.cos,raw.log,inputs/replay-binding.json,inputs/<cases copy>} and
writes NEW_EVIDENCE_DIR/<run_id>.verdict.json. It never writes inside RUN_DIR.

What it checks, per scheduled case:
  evidence  the case is scheduled in input.cos (whose hash must equal the receipt's script hash),
            began and ended exactly once, every repeated measurement agrees, setup and harness
            markers hold, the command reached IPM, the injected fault occurred, the run completed
            with NATIVE-END and no environment flag, and the case's inputs are unchanged between
            the snapshot taken before the run and the target cases.json (else STALE).
  l2        each l2_predicted field against the observation (AGREE / DISAGREE / MISSING / N/A).
  l1        each L1 clause of contract.rs (l1sem.py) as PASS / FAIL / UNKNOWN, and the set of
            failed clauses compared with the model's own violated_clauses for the case.
  aux       the refusal or failure kind read from the error text (not an L1 clause).
"""
import hashlib, json, os, re, sys
from datetime import datetime, timezone

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import l1sem as L  # noqa: E402

OBS_RE = re.compile(r"(?:^|>)\s*OBS (\S+) ([^=\s]+)=(.*)$")
MARK_RE = re.compile(r"(?:^|>)\s*CASE-(BEGIN|END) (\S+)")
ENV_RE = re.compile(r"LICENSE LIMIT|License limit exceeded|<DISKHARD>|<FILEFULL>|No space left", re.I)
SCHED_RE = re.compile(r'##class\(B2Native\.Replay\)\.Case\("([^"]+)"')
NOT_OBSERVABLE = {"writes": "module record write count; LastUpdated is the proxy used by L1 (PROJECTION.md)"}


def sha256_file(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()


# ---------------------------------------------------------------- evidence


def parse_log(path):
    """Keeps every measurement with its line number, and every marker."""
    obs, marks, env, native_end, stray = {}, {}, [], False, []
    open_case = None
    for n, line in enumerate(open(path, encoding="utf-8", errors="replace"), 1):
        line = line.rstrip("\r\n")
        if line.startswith("NATIVE-END"):
            native_end = True
        m = MARK_RE.search(line)
        if m:
            kind, cid = m.group(1), m.group(2)
            marks.setdefault(cid, []).append((kind, n))
            open_case = cid if kind == "BEGIN" else None
            continue
        m = OBS_RE.search(line)
        if m:
            cid, key, val = m.group(1), m.group(2), m.group(3).rstrip()
            obs.setdefault(cid, {}).setdefault(key, []).append((val, n))
            if open_case != cid:
                stray.append({"line": n, "case": cid, "key": key})
            continue
        if ENV_RE.search(line):
            env.append(n)
    return obs, marks, env, native_end, stray


def collapse(meas):
    """One value per field; a field measured with different values is a conflict."""
    vals, conflicts, repeats = {}, {}, {}
    for k, lst in meas.items():
        distinct = sorted({v for v, _ in lst})
        if len(distinct) > 1:
            conflicts[k] = [{"value": v, "line": n} for v, n in lst]
        else:
            vals[k] = distinct[0]
            if len(lst) > 1:
                repeats[k] = len(lst)
    return vals, conflicts, repeats


def marks_ok(lst):
    kinds = [k for k, _ in lst]
    if kinds == ["BEGIN", "END"] and lst[0][1] < lst[1][1]:
        return None
    return "markers " + ",".join(f"{k}@{n}" for k, n in lst) if lst else "no CASE-BEGIN/CASE-END"


# ---------------------------------------------------------------- l2 comparison (as-built model vs product)


def norm_pred(key, v):
    last = key.split(".")[-1]
    if v is None:
        return "<none>"
    if isinstance(v, bool):
        return "1" if v else "0"
    if isinstance(v, (int, float)):
        return str(int(v))
    s = str(v)
    if last in ("success", "status", "last_phase_status"):
        for pre, out in (("OK", "OK"), ("error", "ERR"), ("0 (in progress", "ZERO"), ("open", "OPEN")):
            if s == pre or (pre in ("0 (in progress", "open") and s.startswith(pre)):
                return out
    if last == "version" and s.startswith("0.0.0-0"):
        return "0.0.0-0"
    return s


def norm_obs(key, v, snapshot_slots):
    parts = key.split(".")
    if v is not None and parts[-1] in ("version", "zpm_document_version") and len(parts) > 1 and parts[1] in snapshot_slots:
        return v.replace("+snapshot", "")
    return v


END_MAP = {"returned": "returned", "returned; caller rolled back": "returned; caller rolled back",
           "process terminated": "process terminated (%OnClose ran, then rollback)",
           "process killed": "process killed (no %OnClose, rollback)"}


def l2_compare(case, o, snapshot_slots):
    out = {}
    for key, pv in flat(case["l2_predicted"]).items():
        last = key.split(".")[-1]
        if last in NOT_OBSERVABLE:
            out[key] = {"predicted": pv, "result": "N/A", "reason": NOT_OBSERVABLE[last]}
            continue
        ov = o.get(key)
        if key == "end":
            res = "MISSING" if ov is None else ("AGREE" if END_MAP.get(ov) == pv else "DISAGREE")
            out[key] = {"predicted": pv, "observed": ov, "result": res, "binding_field": True}
            continue
        if ov is None:
            out[key] = {"predicted": pv, "observed": None, "result": "MISSING"}
            continue
        agree = norm_obs(key, ov, snapshot_slots) == norm_pred(key, pv)
        out[key] = {"predicted": pv, "observed": ov, "result": "AGREE" if agree else "DISAGREE"}
    return out


def flat(o, p=""):
    out = {}
    if isinstance(o, dict):
        for k, v in o.items():
            out.update(flat(v, p + k + "."))
    else:
        out[p[:-1]] = o
    return out


# ---------------------------------------------------------------- auxiliary refusal classification (not an L1 clause)

VERDICT_RX = [
    ("refused: path", r"must be absolute|No path has been specified"),
    ("refused: downgrade", r"Cannot downgrade"),
    ("refused: update required", r"Please use the 'update' command"),
    ("refused: system requirements", r"requires the %SYS namespace"),
    ("failed after the load completed (CleanUp)", r"INVALID OREF"),
    ("refused: manifest", r"No module definition|already defined as part of module|ERROR #6301|SAX|XML|module\.xml"),
]


def classify(o, slots):
    if o.get("status") == "OK":
        return "success"
    err = o.get("err", "")
    for name, rx in VERDICT_RX:
        if re.search(rx, err):
            return name
    if slots.get("top") and f"failure in {slots['top']}" in err:
        return "failed: Activate phase"
    for k in ("dep1", "dep2"):
        if slots.get(k) and slots[k] in err:
            return "failed: dependency"
    if re.search(r"[Dd]ependenc|not found|Unable to find|Could not find", err):
        return "failed: dependency"
    return "unclassified: " + err[:120]


# ---------------------------------------------------------------- one case


def tri(x):
    return {True: "PASS", False: "FAIL", None: "UNKNOWN"}[x]


def evaluate_case(cid, case, snap_case, b, meas, marks, run_flags, scheduled):
    o, conflicts, repeats = collapse(meas)
    problems = []
    if cid not in scheduled:
        problems.append("not scheduled in input.cos")
    mk = None
    if mk:
        problems.append(mk)
    if conflicts:
        problems.append("conflicting measurements: " + ",".join(sorted(conflicts)))
    problems += run_flags
    for k, v in o.items():
        if re.match(r"setup\d+\.ok$", k) and v != "1":
            problems.append(f"setup failed: {k}={v}")
    for k in ("harness.signal", "harness.gone"):
        if k in o and o[k] != "1":
            problems.append(f"harness: {k}={o[k]}")
    if b["endmode"] in ("terminate", "sigkill"):
        for k in ("harness.signal", "harness.gone", "harness.end.rc"):
            if k not in o:
                problems.append(f"harness marker missing: {k}")
    if "harness.end.rc" in o and o["harness.end.rc"] != ("1" if b["endmode"] == "terminate" else "0"):
        problems.append(f"harness: harness.end.rc={o['harness.end.rc']}")
    if not re.fullmatch(r"[1-9][0-9]*", o.get("x.ids_allocated", "")):
        problems.append("command did not reach IPM (x.ids_allocated)")
    if b.get("fault_check") and b["fault_check"] not in o.get("err", ""):
        problems.append(f"injected fault not observed ({b['fault_check']})")
    for k, v in o.items():
        m = re.match(r"history\.(\w+)\.x\.rows$", k)
        if m:
            problems.append(f"driver saw {v} History rows for slot {m.group(1)} and reported only the first")
    if "case.exception" in o:
        problems.append("driver exception: " + o["case.exception"][:120])
    stale = snap_case is None or snap_case["inputs"] != case["inputs"]
    slots = [s for s in L.SLOTS if s in b["slots"]]
    snapshot_slots = {s for s, d in zip(["dep1", "dep2"], case["inputs"].get("dependencies", [])) if d.get("snapshot")}
    expect = L.decode_expect(case["l1_permitted"])
    cl = L.l1_clauses(expect, o, slots, snapshot_slots)
    failed = sorted(k for k, v in cl.items() if v["ok"] is False)
    unknown = sorted(k for k, v in cl.items() if v["ok"] is None)
    malformed = sorted(k for k, v in o.items() if k.startswith(("modules.", "before.modules.", "history.")) and k.split(".")[-1] in
                       ("manifest_present", "resources_loaded", "developer_mode", "row", "committed", "time_end_set", "finalized", "package_set")
                       and v not in ("0", "1"))
    l2 = l2_compare(case, o, snapshot_slots)
    dis = sorted(k for k, v in l2.items() if v["result"] == "DISAGREE")
    miss = sorted(k for k, v in l2.items() if v["result"] == "MISSING")
    model_failed = sorted(case["l1_vs_l2"]["violated_clauses"])
    complete = not problems and not stale
    if stale:
        l1v = l2v = "STALE (inputs changed since the run; needs new execution)"
    elif not complete:
        l1v = l2v = "INCOMPLETE"
    else:
        l2v = "DISAGREES" if dis else ("INCOMPLETE" if miss else "AGREES")
        l1v = "VIOLATES" if failed else ("INDETERMINATE" if unknown else "PERMITTED")
    if b["realized"] == "approx" and complete:
        l2v, l1v = f"APPROXIMATE ({l2v.lower()})", f"APPROXIMATE ({l1v.lower()})"
    if not complete:
        relation = None
    elif unknown and not set(failed) - set(model_failed):
        relation = "undecided (unknown: " + ",".join(unknown) + ")"
    elif set(failed) == set(model_failed):
        relation = "same clauses"
    elif set(failed) > set(model_failed):
        relation = "native fails more clauses"
    elif set(failed) < set(model_failed):
        relation = "native fails fewer clauses"
    else:
        relation = "different clauses"
    aux = None
    if o.get("end", "").startswith("returned"):
        got = classify(o, b["slots"])
        want = case["l1_permitted"]["verdict"]
        aux = {"permitted": want, "observed": got, "ok": got == want}
    return {
        "title": case["title"], "realized": b["realized"], "command": b.get("command"),
        "evidence": {"complete": complete, "problems": problems, "stale_inputs": stale, "repeated_fields": repeats,
                     "conflicts": conflicts, "malformed_fields": malformed,
                     "l1_permitted_revised_after_run": snap_case is not None and snap_case["l1_permitted"] != case["l1_permitted"],
                     "l2_predicted_revised_after_run": snap_case is not None and snap_case["l2_predicted"] != case["l2_predicted"]},
        "l2_verdict": l2v, "l2_disagreements": dis, "l2_missing": miss,
        "l1_verdict": l1v, "l1_failed_clauses": failed, "l1_unknown_clauses": unknown,
        "model_violated_clauses": model_failed, "clause_relation_to_model": relation,
        "q_alternatives_that_change_the_verdict": case["l1_vs_l2"].get("q_alternatives_that_change_the_verdict", []),
        "aux_refusal_kind": aux,
        "l1_clauses": {k: {"result": tri(v["ok"]), "detail": v["detail"]} for k, v in cl.items()},
        "l2_fields": l2,
    }


# ---------------------------------------------------------------- run


def load_run(run, target_cases):
    rec = json.load(open(os.path.join(run, "receipt.json")))
    cos = os.path.join(run, "input.cos")
    bind_path = os.path.join(run, "inputs", "replay-binding.json")
    bind = json.load(open(bind_path))
    snap_path = os.path.join(run, "inputs", bind["copy"])
    flags = []
    if sha256_file(cos) != rec["script_sha256"]:
        flags.append("input.cos does not match the receipt's script hash")
    if rec["process_outcome"] != "COMPLETED":
        flags.append("process outcome " + rec["process_outcome"])
    obs, marks, env, native_end, stray = parse_log(os.path.join(run, "raw.log"))
    if not native_end:
        flags.append("no NATIVE-END")
    if env:
        flags.append("environment flag at lines " + ",".join(map(str, env[:5])))
    scheduled = SCHED_RE.findall(open(cos, encoding="utf-8").read())
    snap = {c["id"]: c for c in json.load(open(snap_path))["cases"]}
    target_raw = open(target_cases, "rb").read()
    target = {c["id"]: c for c in json.loads(target_raw)["cases"]}
    binding = {x["id"]: x for x in bind["bindings"]}
    return rec, flags, obs, marks, stray, scheduled, snap, target, binding, hashlib.sha256(target_raw).hexdigest(), sha256_file(snap_path)


def main(argv):
    run = argv[0]
    target_cases = argv[argv.index("--cases") + 1]
    out_dir = argv[argv.index("--out") + 1]
    if os.path.commonpath([os.path.abspath(out_dir), os.path.abspath(run)]) == os.path.abspath(run):
        print("refusing to write inside the run directory", file=sys.stderr)
        return 2
    mutations = [argv[i + 1] for i, a in enumerate(argv) if a == "--mutate"]
    drops = [argv[i + 1] for i, a in enumerate(argv) if a == "--drop"]
    rec, flags, obs, marks, stray, scheduled, snap, target, binding, target_sha, snap_sha = load_run(run, target_cases)
    for m in mutations:
        cid, rest = m.split(":", 1)
        f, v = rest.split("=", 1)
        obs.setdefault(cid, {})[f] = [(v, 0)]
    for d in drops:
        cid, f = d.split(":", 1)
        obs.get(cid, {}).pop(f, None)
    run_flags = list(flags)
    if stray:
        run_flags.append(f"{len(stray)} OBS lines outside their case's markers")
    results, unscheduled_observed = {}, []
    for cid in scheduled:
        if cid not in target:
            results[cid] = {"evidence": {"complete": False, "problems": ["scheduled case absent from target cases.json"]},
                            "l1_verdict": "INCOMPLETE", "l2_verdict": "INCOMPLETE"}
            continue
        results[cid] = evaluate_case(cid, target[cid], snap.get(cid), binding[cid], obs.get(cid, {}), marks.get(cid, []), run_flags, scheduled)
    for cid in sorted(set(obs) | set(marks)):
        if cid not in scheduled:
            unscheduled_observed.append(cid)
    counts = {}
    for r in results.values():
        for k in (f"l2:{r['l2_verdict']}", f"l1:{r['l1_verdict']}"):
            counts[k] = counts.get(k, 0) + 1
    me = os.path.abspath(__file__)
    out = {
        "run_id": rec["run_id"], "process_outcome": rec["process_outcome"], "run_flags": run_flags,
        "scheduled": scheduled, "observed_but_not_scheduled": unscheduled_observed,
        "target_cases_sha256": target_sha, "run_snapshot_cases_sha256": snap_sha,
        "receipt": {k: rec.get(k) for k in ("script_sha256", "helpers_sha256", "expectations_snapshot_sha256", "image_id", "product_rev")},
        "evaluator": {"path": os.path.relpath(me, os.path.dirname(os.path.dirname(os.path.dirname(me)))), "sha256": sha256_file(me),
                      "l1sem_sha256": sha256_file(os.path.join(os.path.dirname(me), "l1sem.py"))},
        "mutations": mutations, "drops": drops, "counts": counts, "cases": results,
        "evaluated_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    os.makedirs(out_dir, exist_ok=True)
    suffix = "" if not (mutations or drops) else ".control"
    path = os.path.join(out_dir, f"{rec['run_id']}{suffix}.verdict.json")
    json.dump(out, open(path, "w"), indent=1, sort_keys=True)
    for cid, r in results.items():
        print(cid, "l2=" + r["l2_verdict"], "| l1=" + r["l1_verdict"], ",".join(r.get("l1_failed_clauses", [])) or "-",
              "| model:" + (",".join(r.get("model_violated_clauses", [])) or "-"), "|", r.get("clause_relation_to_model"),
              "| problems:" + ";".join(r["evidence"]["problems"]) if r["evidence"].get("problems") else "")
    print("counts", json.dumps(counts, sort_keys=True))
    print("wrote", path)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
