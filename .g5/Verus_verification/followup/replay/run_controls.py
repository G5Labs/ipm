#!/usr/bin/env python3
"""Controls for the corrected replay evaluator. Run with python3 -I -B from .g5/Verus_verification:

  python3 -I -B followup/replay/run_controls.py OUT_DIR

Writes OUT_DIR/controls.json and exits 1 if any control's corrected result differs from its
expectation. Each L1 control also records what the original evaluator (native/evaluate_replay.py)
says about the same observation, so the delta between the two is part of the evidence.

Families
  A  decoder: l1_permitted -> contract codes -> l1_permitted is the identity on every case of
     every cases.json revision used by a replay.
  B  differential: each case's l2_predicted, projected into the native vocabulary, must fail
     exactly the clauses the model reports in l1_vs_l2.violated_clauses, with none unknown.
  C  paired L1 controls on saved observations: a valid neighbor that must pass and a mutation that
     must fail a named clause, per expectation kind, record field, Committed slot and status.
  D  evidence controls on copied receipts: missing case, missing field, stale input, conflicting
     and repeated measurements, changed script, missing end marker, unscheduled observations.
"""
import copy, hashlib, importlib.util, json, os, re, shutil, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
VV = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
import l1sem as L  # noqa: E402
import evaluate_replay2 as E2  # noqa: E402

spec = importlib.util.spec_from_file_location("orig", os.path.join(VV, "native", "evaluate_replay.py"))
ORIG = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ORIG)

CASES = os.path.join(VV, "cases", "cases.json")
RUN_B = os.path.join(VV, "results", "native", "20261113T193952Z-replay-return-b")
RUN_T = os.path.join(VV, "results", "native", "20261113T194122Z-replay-terminate-b")
RUN_K = os.path.join(VV, "results", "native", "20261113T194155Z-replay-sigkill-b")
REVISIONS = [os.path.join(VV, "native", "cases", "b1-cases-fc7adec4.json"), os.path.join(VV, "native", "cases", "b1-cases-05842975.json"), CASES]

results = []


def record(family, cid, name, expect, got, **extra):
    ok = got == expect
    results.append({"family": family, "id": cid, "name": name, "expected": expect, "corrected": got, "pass": ok, **extra})
    return ok


def cases_by_id(path=CASES):
    return {c["id"]: c for c in json.load(open(path))["cases"]}


def saved_obs(run, cid):
    obs, marks, env, ne, stray = E2.parse_log(os.path.join(run, "raw.log"))
    vals, conflicts, _ = E2.collapse(obs[cid])
    assert not conflicts
    return vals


def slots_of(case):
    return ["top"] + ["dep1", "dep2"][: len(case["inputs"]["dependencies"])]


def corrected(case, o, expect=None):
    e = expect or L.decode_expect(case["l1_permitted"])
    snap = {s for s, d in zip(["dep1", "dep2"], case["inputs"]["dependencies"]) if d.get("snapshot")}
    cl = L.l1_clauses(e, o, slots_of(case), snap)
    return {k: E2.tri(v["ok"]) for k, v in cl.items()}


def summarize(cl):
    fails = sorted(k for k, v in cl.items() if v == "FAIL")
    unk = sorted(k for k, v in cl.items() if v == "UNKNOWN")
    return "VIOLATES " + ",".join(fails) if fails else ("INDETERMINATE " + ",".join(unk) if unk else "PERMITTED")


BINDING = {b["id"]: b for b in json.load(open(os.path.join(VV, "native", "cases", "replay-binding.json")))["bindings"]}


def original(case, o):
    """The original evaluator's L1 judgement on the same observation (evaluate_replay.py:154-204).
    `_slots` is set as evaluate_replay.py:243 does, for its refusal classification."""
    oo = dict(o)
    oo["_slots"] = BINDING[case["id"]]["slots"]
    res, bad = ORIG.l1_compare(case, oo)
    return "VIOLATES " + ",".join(sorted(bad)) if bad else "PERMITTED"


# ---------------------------------------------------------------- A decoder


def family_a():
    for path in REVISIONS:
        for c in json.load(open(path))["cases"]:
            e = L.decode_expect(c["l1_permitted"])
            record("A", c["id"], f"round trip {os.path.basename(path)}", True, L.encode_expect(e) == c["l1_permitted"])


# ---------------------------------------------------------------- B differential against the model


def model_obs(case):
    """The model's own end state (l2_predicted) and initial state (inputs), in the driver's vocabulary.
    `writes` becomes the LastUpdated proxy: w0 before, w<writes> after."""
    o = {}
    inp, l2 = case["inputs"], case["l2_predicted"]
    befores = {"top": inp["installed_before"]["top"]}
    for s, d in zip(["dep1", "dep2"], inp["dependencies"]):
        befores[s] = d["installed_before"]
    for s, b in befores.items():
        p = f"before.modules.{s}."
        if b == "absent":
            o.update({p + "manifest_present": "0", p + "resources_loaded": "0", p + "zpm_document_version": "<none>"})
        else:
            ver = b.split(" ")[0]
            o.update({p + "manifest_present": "1", p + "version": ver, p + "developer_mode": "1" if "developer mode" in b else "0",
                      p + "x.last_updated": "w0", p + "resources_loaded": "1", p + "zpm_document_version": ver})
    for s, m in l2["modules"].items():
        p = f"modules.{s}."
        doc = m.get("zpm_document_version")
        o[p + "zpm_document_version"] = "<none>" if doc is None else doc
        if m["manifest_present"]:
            o.update({p + "manifest_present": "1", p + "version": m["version"], p + "developer_mode": "1" if m["developer_mode"] else "0",
                      p + "resources_loaded": "1" if m["resources_loaded"] else "0", p + "x.last_updated": f"w{m['writes']}"})
        else:
            # INV-ABSENT: an absent module record has its initial res, ver, dev and wr.
            o.update({p + "manifest_present": "0", p + "resources_loaded": "0"})
    o["status"] = {"OK": "OK", "error": "ERR", None: "<none>"}[l2["status"]]
    for s, t in l2["history"].items():
        p = f"history.{s}."
        o[p + "row"] = "1" if t["row"] else "0"
        if t["row"]:
            o[p + "success"] = "OK" if t["success"] == "OK" else ("ERR" if t["success"] == "error" else "ZERO")
            o[p + "committed"] = str(int(t["committed"]))
            for k in ("time_end_set", "finalized", "package_set"):
                o[p + k] = "1" if t[k] else "0"
            o[p + "phases_started"] = str(t["phases_started"])
            lp = t["last_phase_status"]
            o[p + "last_phase_status"] = {"OK": "OK", "error": "ERR", None: "<none>"}.get(lp, "OPEN" if lp and lp.startswith("open") else lp)
    return o


def family_b():
    for c in json.load(open(CASES))["cases"]:
        o = model_obs(c)
        cl = corrected(c, o)
        fails = sorted(k for k, v in cl.items() if v == "FAIL")
        unk = sorted(k for k, v in cl.items() if v == "UNKNOWN")
        model = sorted(c["l1_vs_l2"]["violated_clauses"])
        record("B", c["id"], "corrected clauses equal the model's violated_clauses", {"failed": model, "unknown": []},
               {"failed": fails, "unknown": unk}, original=original(c, o))


# ---------------------------------------------------------------- C paired L1 controls


def pair(cid, name, run, mutation, expect_clause, cases, expect_override=None, base_expect="PERMITTED"):
    """A valid neighbor (`mutation` = None means the saved observation) and a mutation."""
    c = cases[cid]
    o = saved_obs(run, cid)
    e = expect_override
    if mutation is None:
        cl = corrected(c, o, e)
        return record("C", cid, name, base_expect, summarize(cl), original=original(c, o) if e is None else None)
    o2 = dict(o)
    for k, v in mutation.items():
        if v is None:
            o2.pop(k, None)
        else:
            o2[k] = v
    cl = corrected(c, o2, e)
    return record("C", cid, name, expect_clause, summarize(cl), mutation=mutation, original=original(c, o2) if e is None else None)


def family_c():
    cs = cases_by_id()
    # kind 1 installed (contract.rs:291): resources, version, developer mode
    pair("B1-C01", "kind 1 installed: valid neighbor (saved observation)", RUN_B, None, None, cs)
    pair("B1-C01", "kind 1 installed: resources missing from the namespace", RUN_B, {"modules.top.resources_loaded": "0"}, "VIOLATES L1-NAMESPACE", cs)
    pair("B1-C01", "kind 1 installed: wrong version", RUN_B, {"modules.top.version": "1.0.0"}, "VIOLATES L1-NAMESPACE", cs)
    pair("B1-C01", "kind 1 installed: developer mode on", RUN_B, {"modules.top.developer_mode": "1"}, "VIOLATES L1-NAMESPACE", cs)
    pair("B1-C01", "kind 1 installed: resources not observed", RUN_B, {"modules.top.resources_loaded": None}, "INDETERMINATE L1-NAMESPACE", cs)
    # kind 0 unchanged, absent before and after
    pair("B1-C05", "kind 0 unchanged (absent): valid neighbor", RUN_B, None, None, cs)
    pair("B1-C05", "kind 0 unchanged (absent): module record written", RUN_B,
         {"modules.top.manifest_present": "1", "modules.top.version": "2.0.0", "modules.top.developer_mode": "0", "modules.top.x.last_updated": "t"},
         "VIOLATES H-COMMITTED[top],L1-NAMESPACE", cs)
    pair("B1-C05", "kind 0 unchanged (absent): no module observation at all", RUN_B,
         {k: None for k in saved_obs(RUN_B, "B1-C05") if ".modules." in "." + k}, "INDETERMINATE H-COMMITTED[top],L1-NAMESPACE", cs)
    # kind 0 unchanged, present before and after: the write proxy decides
    pair("B1-C11", "kind 0 unchanged (present): valid neighbor", RUN_B, None, None, cs, base_expect="VIOLATES H-RECORD-FIELDS[top]")
    lu = saved_obs(RUN_B, "B1-C11")["modules.top.x.last_updated"]
    pair("B1-C11", "kind 0 unchanged (present): record rewritten (LastUpdated changed)", RUN_B, {"modules.top.x.last_updated": lu + "x"},
         "VIOLATES H-COMMITTED[top],H-RECORD-FIELDS[top],L1-NAMESPACE", cs)
    pair("B1-C11", "kind 0 unchanged (present): LastUpdated not observed", RUN_B, {"modules.top.x.last_updated": None},
         "VIOLATES H-RECORD-FIELDS[top]", cs)
    # kind 2 manifest only (contract.rs:296): document version and resources as before
    pair("B1-C33", "kind 2 manifest only: saved observation (dependency record lost, D1)", RUN_B, None, None, cs, base_expect="VIOLATES H-RECORD-EXISTS[dep1]")
    pair("B1-C33", "kind 2 manifest only: stored document at another version", RUN_B, {"modules.top.zpm_document_version": "3.0.0"},
         "VIOLATES H-RECORD-EXISTS[dep1],L1-NAMESPACE", cs)
    pair("B1-C33", "kind 2 manifest only: resources loaded although they were absent before", RUN_B, {"modules.top.resources_loaded": "1"},
         "VIOLATES H-RECORD-EXISTS[dep1],L1-NAMESPACE", cs)
    # kind 4 manifest present (resources any) and kind 3 any: B1-C34
    pair("B1-C34", "kind 3/4: valid neighbor", RUN_B, None, None, cs)
    pair("B1-C34", "kind 4 manifest present: module record absent", RUN_B, {"modules.top.manifest_present": "0"},
         "VIOLATES L1-NAMESPACE", cs)
    pair("B1-C34", "kind 3 any: dependency in any state", RUN_B, {"modules.dep1.resources_loaded": "0"}, "PERMITTED", cs)
    # kinds 5 and 6 (developer-mode process end): B1-C46T
    pair("B1-C46T", "kind 5/6: saved observation", RUN_T, None, None, cs, base_expect="VIOLATES H-RECORD-FIELDS[top]")
    pair("B1-C46T", "kind 5 unchanged or present: present at another version", RUN_T, {"modules.top.version": "1.0.0"},
         "VIOLATES H-RECORD-FIELDS[top],L1-NAMESPACE", cs)
    pair("B1-C46T", "kind 6 unchanged or installed: dependency present without resources", RUN_T, {"modules.dep1.resources_loaded": "0"},
         "VIOLATES H-RECORD-FIELDS[top],L1-NAMESPACE", cs)
    # alternatives (default process end: unchanged or success): B1-C44K
    pair("B1-C44K", "two alternatives: saved observation (unchanged)", RUN_K, None, None, cs, base_expect="VIOLATES H-RECORD-FIELDS[top]")
    pair("B1-C44K", "two alternatives: partial state matching neither", RUN_K,
         {"modules.top.manifest_present": "1", "modules.top.version": "2.0.0", "modules.top.developer_mode": "0", "modules.top.x.last_updated": "t",
          "modules.top.resources_loaded": "0", "modules.top.zpm_document_version": "2.0.0"},
         "VIOLATES H-COMMITTED[top],H-RECORD-FIELDS[top],L1-NAMESPACE", cs)
    # record fields and existence
    pair("B1-C01", "record: not finalized", RUN_B, {"history.top.finalized": "0"}, "VIOLATES H-RECORD-FIELDS[top]", cs)
    pair("B1-C01", "record: four phases", RUN_B, {"history.top.phases_started": "4"}, "VIOLATES H-RECORD-FIELDS[top]", cs)
    pair("B1-C01", "record: last phase failed", RUN_B, {"history.top.last_phase_status": "ERR"}, "VIOLATES H-RECORD-FIELDS[top]", cs)
    pair("B1-C01", "record: no end time", RUN_B, {"history.top.time_end_set": "0"}, "VIOLATES H-RECORD-FIELDS[top]", cs)
    pair("B1-C01", "record: success error", RUN_B, {"history.top.success": "ERR"}, "VIOLATES H-RECORD-FIELDS[top]", cs)
    pair("B1-C01", "record: missing", RUN_B, {"history.top.row": "0"}, "VIOLATES H-RECORD-EXISTS[top]", cs)
    pair("B1-C01", "status: error returned", RUN_B, {"status": "ERR"}, "VIOLATES L1-STATUS", cs)
    pair("B1-C01", "status: not observed", RUN_B, {"status": None}, "INDETERMINATE L1-STATUS", cs)
    pair("B1-C01", "status: value outside the driver vocabulary", RUN_B, {"status": "OKAY"}, "INDETERMINATE L1-STATUS", cs)
    pair("B1-C05", "kind 0 unchanged (absent): resources appeared without a module record", RUN_B, {"modules.top.resources_loaded": "1"},
         "VIOLATES H-COMMITTED[top],L1-NAMESPACE", cs)
    pair("B1-C08", "committed: not observed", RUN_B, {"history.top.committed": None}, "INDETERMINATE H-COMMITTED[top]", cs)
    pair("B1-C31", "record exists: dependency row restored as the contract requires (valid neighbor)", RUN_B,
         {"history.dep1.row": "1", "history.dep1.success": "OK", "history.dep1.committed": "0", "history.dep1.time_end_set": "1",
          "history.dep1.finalized": "1", "history.dep1.package_set": "1", "history.dep1.phases_started": "5", "history.dep1.last_phase_status": "OK"},
         "PERMITTED", cs)
    # Committed per slot (contract.rs:318-321): the parent kept changes, the dependency did not
    dep_row = {"history.dep1.row": "1", "history.dep1.success": "ERR", "history.dep1.time_end_set": "1", "history.dep1.finalized": "1",
               "history.dep1.package_set": "1", "history.dep1.phases_started": "5", "history.dep1.last_phase_status": "ERR"}
    pair("B1-C33", "Committed per slot: dependency row present, dependency rolled back, Committed=0 (valid)", RUN_B,
         {**dep_row, "history.dep1.committed": "0"}, "PERMITTED", cs)
    pair("B1-C33", "Committed per slot: dependency row present, dependency rolled back, Committed=1", RUN_B,
         {**dep_row, "history.dep1.committed": "1"}, "VIOLATES H-COMMITTED[dep1]", cs)
    # Isolated R01 controls: the rest of the observation is made conformant so only the mutated
    # clause decides the verdict.
    pair("B1-C33", "kind 2 isolated: valid neighbor (dependency row as the contract requires)", RUN_B,
         {**dep_row, "history.dep1.committed": "0"}, "PERMITTED", cs)
    pair("B1-C33", "kind 2 isolated: stored document at another version", RUN_B,
         {**dep_row, "history.dep1.committed": "0", "modules.top.zpm_document_version": "3.0.0"}, "VIOLATES L1-NAMESPACE", cs)
    pair("B1-C46T", "kind 6 isolated: valid neighbor (top row not finalized)", RUN_T, {"history.top.finalized": "0"}, "PERMITTED", cs)
    pair("B1-C46T", "kind 6 isolated: dependency present without resources", RUN_T,
         {"history.top.finalized": "0", "modules.dep1.resources_loaded": "0"}, "VIOLATES L1-NAMESPACE", cs)
    # Committed per slot, reverse: dependency kept, parent rolled back (B1-C38 is the -dev parent case)
    pair("B1-C38", "Committed per slot: saved observation", RUN_B, None, None, cs)
    # qualified row (ex = 2), which no final case uses: synthetic expectation on B1-C31's observation
    c = cs["B1-C31"]
    e = L.decode_expect(c["l1_permitted"])
    e["recs"]["dep1"]["ex"] = 2
    o = saved_obs(RUN_B, "B1-C31")
    record("C", "B1-C31", "qualified row (ex=2): absent row passes", "PERMITTED", summarize(corrected(c, o, e)))
    o2 = dict(o); o2.pop("history.dep1.row")
    record("C", "B1-C31", "qualified row (ex=2): row not observed leaves its fields unknown", "INDETERMINATE H-COMMITTED[dep1],H-RECORD-FIELDS[dep1]",
           summarize(corrected(c, o2, e)))
    o3 = dict(o); o3.update({"history.dep1.row": "1", "history.dep1.success": "OK", "history.dep1.committed": "1", "history.dep1.time_end_set": "0",
                             "history.dep1.finalized": "1", "history.dep1.package_set": "1", "history.dep1.phases_started": "5", "history.dep1.last_phase_status": "OK"})
    record("C", "B1-C31", "qualified row (ex=2): present row with no end time fails its fields", "VIOLATES H-COMMITTED[dep1],H-RECORD-FIELDS[dep1]",
           summarize(corrected(c, o3, e)))
    # clause identity: the model fails H-COMMITTED[top] (Committed=1 with nothing kept). Making the
    # module present satisfies H-COMMITTED and fails L1-NAMESPACE instead. The overall boolean is the
    # same, so the original aggregation (evaluate_replay.py:291) reports agreement.
    c = cs["B1-C08"]
    o = saved_obs(RUN_B, "B1-C08")
    o2 = dict(o); o2.update({"modules.top.manifest_present": "1", "modules.top.version": "2.0.0",
                             "modules.top.developer_mode": "0", "modules.top.x.last_updated": "t"})
    cl = corrected(c, o2)
    fails = sorted(k for k, v in cl.items() if v == "FAIL")
    orig = original(c, o2)
    record("C", "B1-C08", "clause identity: different failed clause, same overall verdict",
           {"failed": ["L1-NAMESPACE"], "model": ["H-COMMITTED[top]"], "same_clauses": False},
           {"failed": fails, "model": sorted(c["l1_vs_l2"]["violated_clauses"]), "same_clauses": set(fails) == set(c["l1_vs_l2"]["violated_clauses"])},
           original=orig, original_boolean_match=(orig.startswith("VIOLATES")) == (not c["l1_vs_l2"]["conforms"]))


# ---------------------------------------------------------------- D evidence controls on copied receipts


def copy_run(src, tmp, name):
    dst = os.path.join(tmp, name)
    shutil.copytree(src, dst)
    return dst


def run_e2(run, cases, out):
    p = subprocess.run([sys.executable, "-I", "-B", os.path.join(HERE, "evaluate_replay2.py"), run, "--cases", cases, "--out", out],
                       capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(p.stderr)
    rec = json.load(open(os.path.join(run, "receipt.json")))
    return json.load(open(os.path.join(out, rec["run_id"] + ".verdict.json")))


def run_orig_inmem(run, cid):
    """The original evaluator's completeness decision for one case, without writing into the run."""
    obs, begun, ended, env = ORIG.parse(os.path.join(run, "raw.log"))
    present = cid in (set(obs) | begun)
    return "evaluated" if present else "case absent from results (silently dropped)"


def edit_log(run, fn):
    p = os.path.join(run, "raw.log")
    lines = open(p, encoding="utf-8", errors="replace").read().split("\n")
    open(p, "w", encoding="utf-8").write("\n".join(fn(lines)))


def family_d():
    tmp = tempfile.mkdtemp(prefix="replay-controls-")
    out = os.path.join(tmp, "out")
    try:
        # 1 missing case
        r = copy_run(RUN_B, tmp, "missing-case")
        edit_log(r, lambda ls: [l for l in ls if not re.search(r"\bB1-C05\b", l)])
        v = run_e2(r, CASES, out)
        record("D", "B1-C05", "missing case: scheduled but absent from the log", "INCOMPLETE", v["cases"]["B1-C05"]["l1_verdict"],
               original=run_orig_inmem(r, "B1-C05"))
        # 2 missing field
        r = copy_run(RUN_B, tmp, "missing-field")
        edit_log(r, lambda ls: [l for l in ls if "OBS B1-C01 modules.top.resources_loaded=" not in l])
        v = run_e2(r, CASES, out)
        record("D", "B1-C01", "missing field: resources_loaded line removed", "INCOMPLETE/INDETERMINATE",
               f"{v['cases']['B1-C01']['l2_verdict']}/{v['cases']['B1-C01']['l1_verdict']}")
        # 3 stale input
        r = copy_run(RUN_B, tmp, "stale-input")
        d = json.load(open(CASES))
        for c in d["cases"]:
            if c["id"] == "B1-C02":
                c["inputs"]["dev_flag"] = "none"
        stale = os.path.join(tmp, "cases-stale.json")
        json.dump(d, open(stale, "w"))
        v = run_e2(r, stale, out)
        record("D", "B1-C02", "stale input: target cases.json changed the case's inputs", True,
               v["cases"]["B1-C02"]["l1_verdict"].startswith("STALE"))
        record("D", "B1-C01", "stale input: unchanged neighbor stays evaluable", "PERMITTED", v["cases"]["B1-C01"]["l1_verdict"])
        # 4 conflicting evidence
        r = copy_run(RUN_B, tmp, "conflict")
        edit_log(r, lambda ls: sum(([l, "OBS B1-C01 history.top.committed=0"] if l.startswith("OBS B1-C01 history.top.committed=") else [l] for l in ls), []))
        v = run_e2(r, CASES, out)
        record("D", "B1-C01", "conflicting repeated measurement", "INCOMPLETE", v["cases"]["B1-C01"]["l1_verdict"])
        o, _, _, _ = ORIG.parse(os.path.join(r, "raw.log"))
        results[-1]["original"] = "keeps last value only: history.top.committed=" + o["B1-C01"]["history.top.committed"]
        # 5 repeated identical measurement
        r = copy_run(RUN_B, tmp, "repeat")
        edit_log(r, lambda ls: sum(([l, l] if l.startswith("OBS B1-C01 history.top.committed=") else [l] for l in ls), []))
        v = run_e2(r, CASES, out)
        record("D", "B1-C01", "repeated identical measurement", ("PERMITTED", {"history.top.committed": 2}),
               (v["cases"]["B1-C01"]["l1_verdict"], v["cases"]["B1-C01"]["evidence"]["repeated_fields"]))
        # 6 script changed after the receipt
        r = copy_run(RUN_B, tmp, "script")
        with open(os.path.join(r, "input.cos"), "a") as fh:
            fh.write("\n")
        v = run_e2(r, CASES, out)
        record("D", "*", "input.cos differs from the receipt's script hash", "INCOMPLETE",
               sorted({c["l1_verdict"] for c in v["cases"].values()})[0] if len({c["l1_verdict"] for c in v["cases"].values()}) == 1 else "mixed")
        # 7 end marker missing while the receipt says COMPLETED
        r = copy_run(RUN_B, tmp, "no-end")
        edit_log(r, lambda ls: [l for l in ls if not l.startswith("NATIVE-END")])
        v = run_e2(r, CASES, out)
        record("D", "*", "NATIVE-END missing", "INCOMPLETE",
               sorted({c["l1_verdict"] for c in v["cases"].values()})[0] if len({c["l1_verdict"] for c in v["cases"].values()}) == 1 else "mixed")
        # 8 observations for a case the script never scheduled
        r = copy_run(RUN_B, tmp, "unscheduled")
        edit_log(r, lambda ls: ls + ["CASE-BEGIN B1-C99 x", "OBS B1-C99 status=OK", "CASE-END B1-C99 x"])
        v = run_e2(r, CASES, out)
        record("D", "B1-C99", "observed but not scheduled", ["B1-C99"], v["observed_but_not_scheduled"])
        # 9 a field outside its case's markers
        r = copy_run(RUN_B, tmp, "stray")
        edit_log(r, lambda ls: ls + ["OBS B1-C01 history.top.committed=1"])
        v = run_e2(r, CASES, out)
        record("D", "B1-C01", "measurement outside its case's markers", "INCOMPLETE", v["cases"]["B1-C01"]["l1_verdict"])
        # 9b the driver saw two History rows for one slot and reported the first
        r = copy_run(RUN_B, tmp, "two-rows")
        edit_log(r, lambda ls: sum(([l, "OBS B1-C01 history.top.x.rows=2"] if l.startswith("OBS B1-C01 history_rows=") else [l] for l in ls), []))
        v = run_e2(r, CASES, out)
        record("D", "B1-C01", "driver reported one of two History rows for a slot", "INCOMPLETE", v["cases"]["B1-C01"]["l1_verdict"])
        # 9c a case that began twice
        r = copy_run(RUN_B, tmp, "two-begins")
        edit_log(r, lambda ls: sum(([l, l] if l.startswith("CASE-BEGIN B1-C01 ") else [l] for l in ls), []))
        v = run_e2(r, CASES, out)
        record("D", "B1-C01", "case began twice", "INCOMPLETE", v["cases"]["B1-C01"]["l1_verdict"])
        # 9d an expectation snapshot changed after the run
        r = copy_run(RUN_B, tmp, "snapshot")
        with open(os.path.join(r, "inputs", "expected-repro.json"), "a") as fh:
            fh.write(" ")
        v = run_e2(r, CASES, out)
        record("D", "*", "inputs/ differs from the receipt's expectations snapshot hash", "INCOMPLETE",
               sorted({c["l1_verdict"] for c in v["cases"].values()})[0] if len({c["l1_verdict"] for c in v["cases"].values()}) == 1 else "mixed")
        # 10 the real run is complete
        v = run_e2(RUN_B, CASES, out)
        record("D", "*", "unmodified run: every scheduled case is complete", 43, sum(1 for c in v["cases"].values() if c["evidence"]["complete"]))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main(argv):
    out = argv[0]
    family_a(); family_b(); family_c(); family_d()
    os.makedirs(out, exist_ok=True)
    me = os.path.abspath(__file__)
    inputs = {os.path.relpath(p, VV): hashlib.sha256(open(p, "rb").read()).hexdigest()
              for p in [me, os.path.join(HERE, "l1sem.py"), os.path.join(HERE, "evaluate_replay2.py"), os.path.join(VV, "native", "evaluate_replay.py")] + REVISIONS}
    fails = [r for r in results if not r["pass"]]
    by = {}
    for r in results:
        k = r["family"]
        by.setdefault(k, [0, 0])
        by[k][0 if r["pass"] else 1] += 1
    # A verdict-class difference (PERMITTED / VIOLATES / INDETERMINATE) between the two evaluators.
    deltas = [{"id": r["id"], "name": r["name"], "original": r["original"], "corrected": r["corrected"]} for r in results
              if r["family"] == "C" and isinstance(r.get("original"), str) and isinstance(r["corrected"], str)
              and r["original"].split(" ")[0] != r["corrected"].split(" ")[0]]
    json.dump({"inputs_sha256": inputs, "counts": {k: {"pass": v[0], "fail": v[1]} for k, v in sorted(by.items())},
               "failures": fails, "original_vs_corrected_verdict_class_differences": deltas, "controls": results},
              open(os.path.join(out, "controls.json"), "w"), indent=1, sort_keys=True)
    for r in results:
        if r["family"] in ("C", "D") or not r["pass"]:
            print(("PASS" if r["pass"] else "FAIL"), r["family"], r["id"], r["name"], "| corrected:", r["corrected"],
                  "| original:", r.get("original"))
    print("counts", {k: {"pass": v[0], "fail": v[1]} for k, v in sorted(by.items())})
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
