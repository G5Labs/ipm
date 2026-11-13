#!/usr/bin/env python3
"""Compare a native replay run with B1 model cases. Run with python3 -I.

  python3 -I evaluate_replay.py RUN_DIR [--mutate CASE:FIELD=VALUE ...] [--index CONCERN] [--cases NEWER_CASES_JSON]

Uses RUN_DIR/inputs/b1-cases-*.json and RUN_DIR/inputs/replay-binding.json (copied by run-iris.sh before the
session) and RUN_DIR/raw.log. For each case and field it records two comparisons:
  l2: observed vs l2_predicted (does the as-built model predict the product?)
  l1: observed vs l1_permitted (does the product satisfy the contract?)
Values are normalized to the driver's vocabulary: 1/0, OK/ERR/ZERO/OPEN, <none>.
"""
import glob, hashlib, json, os, re, sys
from datetime import datetime, timezone

OBS_RE = re.compile(r"(?:^|>)\s*OBS (\S+) ([^=\s]+)=(.*)$")
MARK_RE = re.compile(r"(?:^|>)\s*CASE-(BEGIN|END) (\S+)")
ENV_RE = re.compile(r"LICENSE LIMIT|License limit exceeded|<DISKHARD>|<FILEFULL>|No space left", re.I)
NOT_COMPARABLE = {"writes": "module record write count is not observable natively"}


def flat(o, p=""):
    out = {}
    if isinstance(o, dict):
        for k, v in o.items():
            out.update(flat(v, p + k + "."))
    else:
        out[p[:-1]] = o
    return out


def norm_pred(key, v, snapshot_slots):
    last = key.split(".")[-1]
    if v is None:
        return "<none>"
    if isinstance(v, bool):
        return "1" if v else "0"
    if isinstance(v, (int, float)):
        return str(int(v))
    s = str(v)
    if last in ("success", "status", "last_phase_status"):
        if s == "OK":
            return "OK"
        if s == "error":
            return "ERR"
        if s.startswith("0 (in progress"):
            return "ZERO"
        if s.startswith("open"):
            return "OPEN"
    if last == "version" and s.startswith("0.0.0-0"):
        return "0.0.0-0"
    return s


def norm_obs(key, v, snapshot_slots):
    if v is None:
        return None
    parts = key.split(".")
    if parts[-1] in ("version", "zpm_document_version") and len(parts) > 1 and parts[1] in snapshot_slots:
        return v.replace("+snapshot", "")
    return v


def parse(log):
    obs, begun, ended, env = {}, set(), set(), False
    for line in open(log, encoding="utf-8", errors="replace"):
        line = line.rstrip("\r\n")
        m = OBS_RE.search(line)
        if m:
            obs.setdefault(m.group(1), {})[m.group(2)] = m.group(3).rstrip()
            continue
        m = MARK_RE.search(line)
        if m:
            (begun if m.group(1) == "BEGIN" else ended).add(m.group(2))
            continue
        if ENV_RE.search(line):
            env = True
    return obs, begun, ended, env


def slot_changed(o, slot):
    keys = ["manifest_present", "version", "developer_mode", "resources_loaded", "zpm_document_version"]
    return any(o.get(f"before.modules.{slot}.{k}") != o.get(f"modules.{slot}.{k}") for k in keys)


def slot_unchanged(o, slot):
    return not slot_changed(o, slot)


def slot_installed(o, slot, ver, dev):
    if o.get(f"modules.{slot}.manifest_present") != "1":
        return False
    if ver is not None and (o.get(f"modules.{slot}.version") or "").replace("+snapshot", "") != ver:
        return False
    if dev not in (None, "any") and o.get(f"modules.{slot}.developer_mode") != ("1" if dev else "0"):
        return False
    return True


def module_alt_ok(o, alt):
    for slot, spec in alt.items():
        if isinstance(spec, str):
            if spec.startswith("any"):
                continue
            if spec == "unchanged":
                if not slot_unchanged(o, slot):
                    return False
                continue
            return False
        if "manifest_present" in spec or "manifest_only" in spec:
            ver = spec.get("manifest_present", spec.get("manifest_only"))
            if not slot_installed(o, slot, ver, spec.get("developer_mode")):
                return False
            if spec.get("resources") == "as before" and o.get(f"modules.{slot}.resources_loaded") != o.get(f"before.modules.{slot}.resources_loaded"):
                return False
        elif "installed" in spec:
            if not slot_installed(o, slot, spec["installed"], spec.get("developer_mode")):
                return False
        elif "unchanged_or_present" in spec or "unchanged_or_installed" in spec:
            ver = spec.get("unchanged_or_present", spec.get("unchanged_or_installed"))
            if not (slot_unchanged(o, slot) or slot_installed(o, slot, ver, spec.get("developer_mode"))):
                return False
        else:
            return False
    return True


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


def l1_compare(case, o):
    l1 = case["l1_permitted"]
    res = {}
    slots = {}
    for k in o:
        m = re.match(r"modules\.(\w+)\.manifest_present$", k)
        if m:
            slots[m.group(1)] = True
    st = l1.get("status")
    if st not in (None, "any"):
        obs_st = o.get("status")
        res["status"] = {"permitted": st, "observed": obs_st, "ok": obs_st == norm_pred("status", st, set())}
    if o.get("end") in ("returned", "returned; caller rolled back"):
        v = classify(o, o.get("_slots", {}))
        res["verdict"] = {"permitted": l1["verdict"], "observed": v, "ok": v == l1["verdict"] or (l1["verdict"].startswith("refused: manifest") and v.startswith("refused: manifest"))}
    alts = l1.get("modules_one_of", [])
    if alts:
        ok = any(module_alt_ok(o, a) for a in alts)
        res["modules_one_of"] = {"permitted": alts, "observed": {k: v for k, v in o.items() if k.startswith("modules.")}, "ok": ok}
    # Changes kept: a module slot's state changed, or its record was rewritten and the rewrite persisted
    # (LastUpdated is stamped by every %Save; the driver waits 1.1 s before the command).
    kept = any(slot_changed(o, s) or o.get(f"before.modules.{s}.x.last_updated") != o.get(f"modules.{s}.x.last_updated") for s in slots)
    for slot, spec in l1.get("history", {}).items():
        row = o.get(f"history.{slot}.row")
        for f, pv in spec.items():
            key = f"history.{slot}.{f}"
            if pv == "any":
                continue
            if f == "row":
                if str(pv).startswith("required"):
                    # "required unless Q alternative; may be missing": B1 permits absence under the stated alternative.
                    entry = {"permitted": pv, "observed": row, "ok": row in ("1", "0"), "qualified": True}
                else:
                    want = "1" if pv is True else "0"
                    entry = {"permitted": pv, "observed": row, "ok": row == want}
                res[key] = entry
                continue
            if row != "1":
                res[key] = {"permitted": pv, "observed": None, "ok": "N/A (no row)"}
                continue
            ov = o.get(key)
            if isinstance(pv, str) and pv.startswith("equals whether"):
                want = "1" if kept else "0"
                res[key] = {"permitted": pv, "observed": ov, "derived_expected": want, "ok": ov == want}
            else:
                want = norm_pred(key, pv, set())
                if f == "success" and want.startswith("0 (in progress"):
                    want = "ZERO"
                res[key] = {"permitted": pv, "observed": ov, "ok": ov == want}
    bad = [k for k, v in res.items() if v["ok"] is False]
    return res, bad


def main(argv):
    run = argv[0]
    mutations = [argv[i + 1] for i, a in enumerate(argv) if a == "--mutate"]
    concern = argv[argv.index("--index") + 1] if "--index" in argv else None
    rec = json.load(open(os.path.join(run, "receipt.json")))
    bind_path = os.path.join(run, "inputs", "replay-binding.json")
    snaps = []
    if os.path.isfile(bind_path):
        # The binding names the cases.json copy it was generated from; other copies may also be in inputs/.
        copy = json.load(open(bind_path)).get("copy")
        if copy and os.path.isfile(os.path.join(run, "inputs", copy)):
            snaps = [os.path.join(run, "inputs", copy)]
    if not snaps or not os.path.isfile(bind_path):
        print("missing snapshot inputs", file=sys.stderr)
        return 2
    run_cases_sha = hashlib.sha256(open(snaps[0], "rb").read()).hexdigest()
    cases_path = snaps[0]
    if "--cases" in argv:
        # Re-evaluation against a newer cases.json without re-running the product (same inputs required).
        cases_path = argv[argv.index("--cases") + 1]
    cases_raw = open(cases_path, "rb").read()
    cases_sha = hashlib.sha256(cases_raw).hexdigest()
    if cases_sha != run_cases_sha:
        new_inputs = {c["id"]: c["inputs"] for c in json.loads(cases_raw)["cases"]}
        old_inputs = {c["id"]: c["inputs"] for c in json.load(open(snaps[0]))["cases"]}
        changed_inputs = sorted(k for k in new_inputs if new_inputs.get(k) != old_inputs.get(k))
    else:
        changed_inputs = []
    cases = {c["id"]: c for c in json.loads(cases_raw)["cases"]}
    binding = {b["id"]: b for b in json.load(open(bind_path))["bindings"]}
    obs, begun, ended, env = parse(os.path.join(run, "raw.log"))
    process_ok = rec["process_outcome"] == "COMPLETED" and not env

    def evaluate(cid, o):
        case, b = cases[cid], binding[cid]
        o = dict(o)
        o["_slots"] = b["slots"]
        snapshot_slots = {s for s, d in zip(["dep1", "dep2"], case["inputs"].get("dependencies", [])) if d.get("snapshot")}
        harness = {k: v for k, v in o.items() if k.startswith("harness.")}
        harness_bad = [k for k, v in harness.items() if k in ("harness.signal", "harness.gone") and v != "1"]
        if "harness.end.rc" in harness:
            want = "1" if b["endmode"] == "terminate" else "0"
            if harness["harness.end.rc"] != want:
                harness_bad.append("harness.end.rc")
        setup_bad = [k for k, v in o.items() if re.match(r"setup\d+\.ok$", k) and v != "1"]
        # The command must have reached IPM (at least the top log's History ID was allocated);
        # otherwise every absence check (no row, no module) would pass vacuously.
        if not re.fullmatch(r"[1-9][0-9]*", o.get("x.ids_allocated", "")):
            harness_bad.append("x.ids_allocated")
        fault_missing = b.get("fault_check") and b["fault_check"] not in o.get("err", "")
        l2 = {}
        for key, pv in flat(case["l2_predicted"]).items():
            last = key.split(".")[-1]
            if last in NOT_COMPARABLE:
                l2[key] = {"predicted": pv, "result": "N/A", "reason": NOT_COMPARABLE[last]}
                continue
            if key == "end":
                ov = o.get("end")
                agree = ov is not None and str(pv).startswith(ov)
                l2[key] = {"predicted": pv, "observed": ov, "result": "AGREE" if agree else "DISAGREE", "binding_field": True}
                continue
            ov = norm_obs(key, o.get(key), snapshot_slots)
            want = norm_pred(key, pv, snapshot_slots)
            if ov is None:
                l2[key] = {"predicted": pv, "observed": None, "result": "MISSING"}
            else:
                l2[key] = {"predicted": pv, "observed": o.get(key), "result": "AGREE" if ov == want else "DISAGREE"}
        dis = [k for k, v in l2.items() if v["result"] == "DISAGREE"]
        miss = [k for k, v in l2.items() if v["result"] == "MISSING"]
        l1, l1bad = l1_compare(case, o)
        complete = process_ok and cid in begun and cid in ended and not harness_bad and not setup_bad \
            and "case.exception" not in o and not fault_missing
        if not complete:
            l2v, l1v = "INCOMPLETE", "INCOMPLETE"
        else:
            l2v = "DISAGREES" if dis else ("INCOMPLETE" if miss else "AGREES")
            l1v = "VIOLATES" if l1bad else "PERMITTED"
        if b["realized"] == "approx" and complete:
            # An approximate realization is never reported as agreeing or permitted.
            l2v, l1v = "APPROXIMATE (" + l2v.lower() + ")", "APPROXIMATE (" + l1v.lower() + ")"
        return {"title": case["title"], "realized": b["realized"], "binding_notes": b["notes"], "command": b.get("command"),
                "l2_verdict": l2v, "l2_disagreements": dis, "l2_missing": miss,
                "l1_verdict": l1v, "l1_violations": l1bad,
                "b1_l1_vs_l2": case["l1_vs_l2"],
                "native_matches_b1_l1_judgement": (l1v == "VIOLATES") == (not case["l1_vs_l2"]["conforms"]) if complete else None,
                "harness_failed": harness_bad, "setup_failed": setup_bad, "fault_not_produced": bool(fault_missing),
                "exception": o.get("case.exception"), "l2_fields": l2, "l1_fields": l1,
                "extra_observations": {k: v for k, v in o.items() if k.startswith(("x.", "binding.", "inflight.", "err")) or ".x." in k}}

    results = {}
    for cid in sorted(set(obs) | begun):
        if cid in cases:
            results[cid] = evaluate(cid, obs.get(cid, {}))
    not_realized = {cid: binding[cid]["notes"] for cid in binding if binding[cid]["realized"] != "yes" and cid not in results}
    controls = []
    for m in mutations:
        cid, rest = m.split(":", 1)
        f, v = rest.split("=", 1)
        mo = dict(obs.get(cid, {}))
        if cid not in results:
            controls.append({"mutation": m, "result": "case not in this run"})
            continue
        before = results[cid]
        mo[f] = v
        after = evaluate(cid, mo)
        controls.append({"mutation": m, "original": obs.get(cid, {}).get(f),
                         "l2_before": before["l2_verdict"], "l2_after": after["l2_verdict"],
                         "l1_before": before["l1_verdict"], "l1_after": after["l1_verdict"],
                         "detected": (before["l2_verdict"], before["l1_verdict"], before["l2_disagreements"]) != (after["l2_verdict"], after["l1_verdict"], after["l2_disagreements"])})
    counts = {}
    for r in results.values():
        k = f"l2:{r['l2_verdict']}"
        counts[k] = counts.get(k, 0) + 1
        k = f"l1:{r['l1_verdict']}"
        counts[k] = counts.get(k, 0) + 1
    out = {"run_id": rec["run_id"], "process_outcome": rec["process_outcome"], "environment_flag": env,
           "cases_json_sha256": cases_sha, "cases_snapshot": os.path.relpath(snaps[0], run),
           "run_cases_json_sha256": run_cases_sha, "evaluated_against": cases_path,
           "cases_with_changed_inputs_need_rerun": changed_inputs,
           "binding_snapshot": os.path.relpath(bind_path, run), "counts": counts,
           "cases": results, "not_realized": not_realized, "mutation_controls": controls,
           "evaluated_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"), "evaluator": "native/evaluate_replay.py"}
    vname = "verdict.json" if cases_sha == run_cases_sha else f"verdict-cases-{cases_sha[:8]}.json"
    json.dump(out, open(os.path.join(run, vname), "w"), indent=1)
    if concern:
        with open(os.path.normpath(os.path.join(run, "..", "..", "index.jsonl")), "a") as fh:
            fh.write(json.dumps({"concern": concern, "label": rec["label"],
                                 "outcome": f"{rec['process_outcome']}; " + ", ".join(f"{k}={v}" for k, v in sorted(counts.items())),
                                 "run_id": rec["run_id"], "kind": "native", "variant": "as-built",
                                 "expect": f"B1 cases.json sha256 {cases_sha} (l2_predicted and l1_permitted) via inputs/replay-binding.json"}) + "\n")
    for cid, r in results.items():
        print(cid, "l2=" + r["l2_verdict"], ",".join(r["l2_disagreements"]) or "-", "| l1=" + r["l1_verdict"], ",".join(r["l1_violations"]) or "-",
              "| harness_bad=" + ",".join(r["harness_failed"] + r["setup_failed"]) if (r["harness_failed"] or r["setup_failed"]) else "",
              "| exc" if r["exception"] else "", "| fault_not_produced" if r["fault_not_produced"] else "")
    print("counts", counts, "controls", [(c["mutation"], c["detected"]) for c in controls])
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
