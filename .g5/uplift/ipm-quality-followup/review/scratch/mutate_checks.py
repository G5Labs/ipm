#!/usr/bin/env python3
"""Reviewer scratch: remove one check at a time from copies of l1sem.py / evaluate_replay2.py and
rerun the follow-up's run_controls.py against each copy. Run with python3 -I -B.

  python3 -I -B mutate_checks.py VV_DIR WORK_DIR

VV_DIR is .g5/Verus_verification. WORK_DIR receives one tree per mutant; the original files are
never modified. Each mutant tree symlinks cases/, native/, results/ to VV_DIR so the controls read
the same saved evidence.
"""
import json, os, shutil, subprocess, sys

VV, WORK = os.path.abspath(sys.argv[1]), os.path.abspath(sys.argv[2])
SRC = os.path.join(VV, "followup", "replay")

# (name, file, old, new): each old text must occur exactly once.
MUTANTS = [
    ("M1-installed-no-res", "l1sem.py",
     'return AND(p, m["res"], dev_ok(dev, m["dev"]))',
     'return AND(p, dev_ok(dev, m["dev"]))'),
    ("M2-modeq-no-wr-proxy", "l1sem.py",
     'EQ(i["dev"], m["dev"]), EQ(i["wr_proxy"], m["wr_proxy"]))',
     'EQ(i["dev"], m["dev"]))'),
    ("M3-kept-any-slot-for-dep", "l1sem.py",
     'return NOT(mod_eq(before[i], after[i]))',
     'return OR(*[NOT(mod_eq(before[s], after[s])) for s in slots])'),
    ("M4-kind2-no-doc", "l1sem.py",
     'return AND(p, EQ(m["doc"], ver), EQ(m["res"], i["res"]), dev_ok(dev, m["dev"]))',
     'return AND(p, EQ(m["res"], i["res"]), dev_ok(dev, m["dev"]))'),
    ("M5-kind2-no-res-as-before", "l1sem.py",
     'return AND(p, EQ(m["doc"], ver), EQ(m["res"], i["res"]), dev_ok(dev, m["dev"]))',
     'return AND(p, EQ(m["doc"], ver), dev_ok(dev, m["dev"]))'),
    ("M6-alts-first-only", "l1sem.py",
     'per = [alt_ok(a, before, after, slots) for a in alts[:3]]',
     'per = [alt_ok(a, before, after, slots) for a in alts[:1]]'),
    ("M7-missing-is-pass", "l1sem.py",
     '    if a is None or b is None or isbad(a) or isbad(b):\n        return None\n    return a == b',
     '    if a is None or b is None or isbad(a) or isbad(b):\n        return True\n    return a == b'),
    ("M8-no-conflict-check", "evaluate_replay2.py",
     '        if len(distinct) > 1:\n            conflicts[k]',
     '        if False:\n            conflicts[k]'),
    ("M9-no-stale-check", "evaluate_replay2.py",
     'stale = snap_case is None or snap_case["inputs"] != case["inputs"]',
     'stale = False'),
    ("M10-no-script-hash", "evaluate_replay2.py",
     'if sha256_file(cos) != rec["script_sha256"]:',
     'if False:'),
    ("M11-no-marker-check", "evaluate_replay2.py",
     '    mk = marks_ok(marks)\n',
     '    mk = None\n'),
    ("M12-committed-ignores-kept", "l1sem.py",
     'AND(e["com"] == 3, EQ(t["com"], k))',
     '(e["com"] == 3)'),
    ("M13-fields-no-finalized", "l1sem.py",
     '"finalized": v3(e["fin"], t["fin"]),',
     '"finalized": True,'),
    ("M14-modeq-no-res-when-absent", "l1sem.py",
     'base = AND(EQ(i["present"], m["present"]), EQ(i["res"], m["res"]), EQ(i["doc"], m["doc"]))',
     'base = AND(EQ(i["present"], m["present"]), EQ(i["doc"], m["doc"]))'),
    ("M15-modeq-no-doc", "l1sem.py",
     'base = AND(EQ(i["present"], m["present"]), EQ(i["res"], m["res"]), EQ(i["doc"], m["doc"]))',
     'base = AND(EQ(i["present"], m["present"]), EQ(i["res"], m["res"]))'),
]


def build(name, fname, old, new):
    root = os.path.join(WORK, name, "Verus_verification")
    if os.path.exists(os.path.join(WORK, name)):
        shutil.rmtree(os.path.join(WORK, name))
    rp = os.path.join(root, "followup", "replay")
    os.makedirs(rp)
    for d in ("cases", "native", "results"):
        os.symlink(os.path.join(VV, d), os.path.join(root, d))
    for f in ("l1sem.py", "evaluate_replay2.py", "run_controls.py"):
        shutil.copy(os.path.join(SRC, f), os.path.join(rp, f))
    p = os.path.join(rp, fname)
    text = open(p).read()
    assert text.count(old) == 1, (name, text.count(old))
    open(p, "w").write(text.replace(old, new))
    return rp


summary = {}
for name, fname, old, new in MUTANTS:
    rp = build(name, fname, old, new)
    out = os.path.join(WORK, name, "out")
    env = dict(os.environ, TMPDIR=os.path.join(WORK, name, "tmp"))
    os.makedirs(env["TMPDIR"], exist_ok=True)
    p = subprocess.run([sys.executable, "-I", "-B", os.path.join(rp, "run_controls.py"), out], capture_output=True, text=True, env=env)
    try:
        c = json.load(open(os.path.join(out, "controls.json")))
        failed = [f"{r['family']} {r['id']} {r['name']}" for r in c["failures"]]
        summary[name] = {"rc": p.returncode, "counts": c["counts"], "failed_controls": failed}
    except Exception as ex:  # a crash is also a detection, but record it
        summary[name] = {"rc": p.returncode, "error": repr(ex), "stderr_tail": p.stderr[-600:]}
    print(name, "rc=%d" % p.returncode, json.dumps(summary[name].get("counts")), "failed:", len(summary[name].get("failed_controls", [])))
    for f in summary[name].get("failed_controls", [])[:6]:
        print("   ", f)
json.dump(summary, open(os.path.join(WORK, "mutants-summary.json"), "w"), indent=1)
