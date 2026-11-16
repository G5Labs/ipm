"""Executable twin of models/verus/update_steps.rs: a literal transcription of its spec functions.

Every function below has the name and the body of the Verus spec function it transcribes. Records are dicts
key -> Rec; a missing key is a missing record. Keys are (c, prim, m) tuples. States are plain dicts copied on write.
"""
from __future__ import annotations

from collections import namedtuple

Rec = namedtuple("Rec", "started end ok")
Op = namedtuple("Op", "kind force dev txn top source snapshot to has_pkg skip_prim")  # kind: Install/Load/Update; source: Path/Registry;
# dev: the DeveloperMode typed, True (-dev), False (-nodev) or None (neither)


def Db(ver, dev, recs, jlog):
    return {"ver": ver, "dev": dev, "recs": dict(recs), "jlog": tuple(jlog)}


def upd(x, **kw):
    y = dict(x)
    y.update(kw)
    return y


# ---------------------------------------------------------------- records
def rec(recs, k):
    return recs.get(k)


def due(o):
    return True if o is None else (not o.end or not o.ok)


def done(o):
    return not due(o)


def run_needed(x):
    return not x.end or not x.ok


def visit(o):
    return Rec(False, False, True) if o is None else o


def seeded(x):
    return Rec(x.started, True, True) if (not x.end or not x.ok) else x


def ran(ok):
    return Rec(True, True, ok)


def wf(steps):
    for a in range(len(steps)):
        for b in range(a + 1, len(steps)):
            if steps[a] == steps[b] or not steps[a][0] <= steps[b][0]:
                return False
            if steps[a][0] == steps[b][0] and steps[a][1] and not steps[b][1]:
                return False
    return True


def cstart(steps, i):
    return i == 0 or steps[i - 1][0] != steps[i][0]


def due_at(r, steps, j):
    return due(rec(r, steps[j]))


def skipped(op, k):
    return k[1] and op.skip_prim


# ---------------------------------------------------------------- L1 schedule
def sched(r, steps, op, i):
    return (not skipped(op, steps[i])) and any(steps[j][0] == steps[i][0] and due_at(r, steps, j) for j in range(0, i + 1))


def sched_upto(r, steps, op, n):
    out = ()
    for i in range(n):
        if sched(r, steps, op, i):
            out = out + (steps[i],)
    return out


# ---------------------------------------------------------------- L2
def param_dev(op):
    if op.source == "Path":
        return op.dev
    if op.kind == "Install" and op.top:
        return op.dev is True
    return op.snapshot and op.dev is True


def dev_guard(op):
    return param_dev(op) is True


def dev_run(op, d):
    p = param_dev(op)
    return p if p is not None else (d["ver"] is not None and d["dev"])


def older(frm, to):
    return frm is not None and frm > to


def newer(frm, to):
    return frm is not None and to > frm


def refuses(op, dev, frm):
    return ((op.kind == "Update" and op.top and frm is None) or older(frm, op.to)
            or (op.kind in ("Install", "Load") and not op.force and not dev and newer(frm, op.to) and op.has_pkg))


def durable(s):
    return s["snap"] if s["snap"] is not None else s["db"]


def finish(s, out):
    d = s["db"] if out == "Ok" else durable(s)
    return upd(s, db=d, snap=None, pc="Closed", out=out)


def step_init(s):
    return upd(s, pre=s["db"], snap=s["db"] if s["op"].txn else None, pc="Guard", log=(), fault=False, out="Running")


def step_guard(s):
    if refuses(s["op"], dev_guard(s["op"]), s["db"]["ver"]):
        return finish(s, "Refused")
    return upd(s, pc="Manifest")


def step_manifest(s):
    op = s["op"]
    dv = dev_run(op, s["db"])
    return upd(s, db=upd(s["db"], ver=op.to, dev=dv), snap=None if dv else s["snap"], pc="Phases")


def step_phases(s, ok):
    op = s["op"]
    if not ok:
        return finish(upd(s, fault=True), "Failed")
    if op.kind == "Update":
        return upd(s, pc="Lock")
    if not s["db"]["dev"]:
        return upd(s, pc="Walk", seeding=True, i=0, flag=False)
    return finish(s, "Ok")


def step_lock(s, granted):
    if not granted:
        return finish(s, "Ok")
    return upd(s, pc="Walk", seeding=False, i=0, flag=False)


def step_walk(s, ok):
    steps = s["steps"]
    if s["i"] >= len(steps):
        return finish(s, "Ok")
    k = steps[s["i"]]
    v = visit(rec(s["db"]["recs"], k))
    if s["seeding"]:
        return upd(s, db=upd(s["db"], recs={**s["db"]["recs"], k: seeded(v)}), i=s["i"] + 1)
    f = (not cstart(steps, s["i"]) and s["flag"]) or run_needed(v)
    if f and not skipped(s["op"], k):
        if ok:
            return upd(s, db=upd(s["db"], recs={**s["db"]["recs"], k: ran(True)}, jlog=s["db"]["jlog"] + (k,)),
                       i=s["i"] + 1, flag=f, log=s["log"] + (k,))
        return finish(upd(s, db=upd(s["db"], recs={**s["db"]["recs"], k: ran(False)}), log=s["log"] + (k,), fault=True), "Failed")
    return upd(s, db=upd(s["db"], recs={**s["db"]["recs"], k: v}), i=s["i"] + 1, flag=f)


def step_crash(s):
    return upd(s, db=durable(s), snap=None, pc="Crashed")


def running(pc):
    return pc in ("Guard", "Manifest", "Phases", "Lock", "Walk")


def enabled(s, c):
    kind = c[0]
    return {"Init": s["pc"] == "Init", "Guard": s["pc"] == "Guard", "Manifest": s["pc"] == "Manifest",
            "Phases": s["pc"] == "Phases", "Lock": s["pc"] == "Lock", "Walk": s["pc"] == "Walk",
            "Crash": running(s["pc"])}[kind]


def step(s, c):
    kind = c[0]
    if kind == "Init":
        return step_init(s)
    if kind == "Guard":
        return step_guard(s)
    if kind == "Manifest":
        return step_manifest(s)
    if kind == "Phases":
        return step_phases(s, c[1])
    if kind == "Lock":
        return step_lock(s, c[1])
    if kind == "Walk":
        return step_walk(s, c[1])
    return step_crash(s)


def premise_pl(c):
    return c != ("Lock", False)


def premise_pf5(s):
    op = s["op"]
    if op.source == "Path":
        return True
    return ((op.dev is not True or (op.kind == "Install" and op.top) or op.snapshot)
            and (op.dev is not None or not stored_dev(s["pre"])))


# ---------------------------------------------------------------- L1
def stored_dev(d):
    return d["ver"] is not None and d["dev"]


def want_dev(op, pre):
    return op.dev if op.dev is not None else stored_dev(pre)


def typed_dev(op):
    return op.dev is True


def blocked(op, frm):
    if frm is None:
        return op.kind == "Update" and op.top
    return frm > op.to or (op.kind != "Update" and op.to > frm and op.has_pkg and not op.force and not typed_dev(op))


def phase_of(pc):
    return {"Init": "NotStarted", "Closed": "Closed", "Crashed": "Crashed"}.get(pc, "Running")


def abs_(s):
    return {"d": durable(s), "pre": s["pre"], "log": s["log"], "phase": phase_of(s["pc"]), "out": s["out"],
            "op": s["op"], "steps": s["steps"], "fault": s["fault"]}


def sched_prefix(a):
    return any(a["log"] == sched_upto(a["pre"]["recs"], a["steps"], a["op"], m) for m in range(len(a["steps"]) + 1))


def ver_ok(a):
    return a["d"]["ver"] == a["pre"]["ver"] or (a["d"]["ver"] == a["op"].to and (a["pre"]["ver"] is None or a["pre"]["ver"] <= a["op"].to))


def run_ok(a):
    op = a["op"]
    return (ver_ok(a) and (not (op.txn and not want_dev(op, a["pre"])) or a["d"] == a["pre"]) and sched_prefix(a)
            and (op.kind == "Update" or a["log"] == ()))


def end_ok(a):
    op, d, steps = a["op"], a["d"], a["steps"]
    if a["fault"] or not (d["ver"] == op.to and d["dev"] == want_dev(op, a["pre"]) and not blocked(op, a["pre"]["ver"])):
        return False
    if op.kind == "Update":
        return (a["log"] == sched_upto(a["pre"]["recs"], steps, op, len(steps))
                and all(done(rec(d["recs"], steps[j])) for j in range(len(steps)) if not skipped(op, steps[j])))
    if not want_dev(op, a["pre"]):
        return a["log"] == () and all(done(rec(d["recs"], k)) for k in steps)
    return a["log"] == () and d["recs"] == a["pre"]["recs"]


def end_refused(a):
    return not a["fault"] and blocked(a["op"], a["pre"]["ver"]) and a["d"] == a["pre"] and a["log"] == ()


def end_failed(a):
    op, d = a["op"], a["d"]
    if not (a["fault"] and run_ok(a) and not blocked(op, a["pre"]["ver"])):
        return False
    if want_dev(op, a["pre"]):
        return d["ver"] == op.to and d["dev"] and all(done(rec(d["recs"], a["log"][k])) for k in range(len(a["log"]) - 1))
    return True


def honest_end(a):
    return {"Ok": end_ok, "Refused": end_refused, "Failed": end_failed}.get(a["out"], lambda _a: False)(a)


def same_consts(a, b):
    return b["op"] == a["op"] and b["steps"] == a["steps"]


def l1_next(a, b):
    return (a == b
            or (a["phase"] == "NotStarted" and b["phase"] == "Running" and same_consts(a, b) and b["pre"] == a["d"]
                and b["d"] == a["d"] and b["log"] == () and b["out"] == "Running" and not b["fault"])
            or (a["phase"] == "Running" and b["phase"] == "Running" and same_consts(a, b) and b["pre"] == a["pre"]
                and b["out"] == "Running" and not b["fault"] and run_ok(b))
            or (a["phase"] == "Running" and b["phase"] == "Closed" and same_consts(a, b) and b["pre"] == a["pre"] and honest_end(b))
            or (a["phase"] == "Running" and b["phase"] == "Crashed" and same_consts(a, b) and b["pre"] == a["pre"]
                and b["out"] == "Running" and run_ok(b)))


def start(db, op, steps):
    return {"db": db, "snap": None, "pre": db, "pc": "Init", "op": op, "steps": tuple(steps), "seeding": False,
            "i": 0, "flag": False, "log": (), "fault": False, "out": "Running"}


# ---------------------------------------------------------------- class list (VersionBase.cls#L33-L52)
PkgClass = namedtuple("PkgClass", "num step_class id")


def class_list(cs):
    m = {}
    for c in cs:
        if c.num is None:
            return None
        m[c.num] = c
    return m


def walks_class(m, c):
    return c.num is not None and c.num in m and m[c.num] == c


def all_step_classes_walked(cs):
    m = class_list(cs)
    return m is not None and all(walks_class(m, c) for c in cs if c.step_class)
