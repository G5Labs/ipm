"""Abstraction check: the folded L2 in models/verus/load_history.rs against the fine L2 machine (l2_load).

The Verus model folds a dependency's nested transaction into one step and has one parent level open at a time.
This file transcribes its step functions literally and compares, for every shared configuration and crash point,
the L1 view (durable modules and history rows) of both machines. A mismatch means the proof is about a
different machine than the one validated against IRIS.
"""
from __future__ import annotations

import itertools
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from irisenv import Env
from l2_load import Mod, run_command

NO_ROW = dict(exists=False, named=False, ok=False, committed=False, ended=False)
INIT_ROW = dict(NO_ROW, exists=True)
EMPTY = dict(m=False, m_cls=False, d=False, top=NO_ROW, dep=NO_ROW)


def upd(base, **kw):
    x = dict(base)
    x.update(kw)
    return x


# ---- literal transcription of the Verus spec functions
def uses_txn(mode):
    return mode != "NoTxn"


def step_init(s):
    db = upd(s["db"], top=INIT_ROW)
    return upd(s, db=db, snap=db if uses_txn(s["mode"]) else None, temp=INIT_ROW, pc="Guard")


def close(db, temp):
    return upd(db, top=temp)


def fail_end(s):
    rolled = s["snap"] if s["snap"] is not None else s["db"]
    t = upd(s["temp"], ok=False, ended=True)
    return upd(s, db=close(rolled, t), snap=None, temp=t, pc="Closed")


def ok_end(s):
    t = upd(s["temp"], ok=True, committed=True, ended=True)
    return upd(s, db=close(s["db"], t), snap=None, temp=t, pc="Closed")


def step_guard(s, refuse):
    if refuse:
        return fail_end(s)
    t = upd(s["temp"], committed=True) if s["mode"] == "NoTxn" else s["temp"]
    return upd(s, temp=t, pc="Manifest")


def step_manifest(s):
    db = upd(s["db"], m=True)
    t = upd(s["temp"], named=True)
    if s["mode"] == "Dev":
        return upd(s, db=db, snap=db, temp=upd(t, committed=True), pc="Deps")
    return upd(s, db=db, temp=t, pc="Deps")


def dep_row(ok, ended):
    return dict(exists=True, named=True, ok=ok, committed=ok, ended=ended)


def dep_row_m(mode, ok, ended):
    return dict(exists=True, named=True, ok=ok, committed=ok or mode == "NoTxn", ended=ended)


def crash(s):
    closed = close(s["db"], s["temp"])
    rolled = s["snap"] if s["snap"] is not None else closed
    return upd(s, db=rolled, snap=None, pc="Crashed")


def step_deps(s, o):
    if o == "NoDep":
        s1 = upd(s, pc="Activate")
        return upd(s1, snap=None) if s["mode"] == "Dev" else s1
    if o == "Ok":
        s1 = upd(s, db=upd(s["db"], d=True, dep=dep_row(True, True)), dep_began=True, pc="Activate")
        return upd(s1, snap=None) if s["mode"] == "Dev" else s1
    if o == "Fail":
        return fail_end(upd(s, db=upd(s["db"], d=s["mode"] == "NoTxn", dep=dep_row_m(s["mode"], False, True)), dep_began=True))
    return crash(upd(s, db=upd(s["db"], d=s["mode"] == "NoTxn", dep=dep_row_m(s["mode"], False, False)), dep_began=True))


def step_activate(s, ok):
    s1 = upd(s, db=upd(s["db"], m_cls=True))
    if ok:
        if s["mode"] == "Txn":
            return ok_end(upd(s1, snap=None, temp=upd(s1["temp"], committed=True)))
        return ok_end(s1)
    return fail_end(s1)


def verus_run(mode, refuse, dep, act, crash_at):
    s = dict(db=EMPTY, snap=None, temp=NO_ROW, pc="Init", mode=mode, dep_began=False)
    s = step_init(s)
    order = [("Guard", lambda s: step_guard(s, refuse)), ("Manifest", step_manifest),
             ("Deps", lambda s: step_deps(s, dep)), ("Activate", lambda s: step_activate(s, act))]
    for pc, f in order:
        if s["pc"] != pc:
            break
        if crash_at == pc:
            return crash(s)
        s = f(s)
    return s


def durable(s):
    return s["snap"] if s["snap"] is not None else s["db"]


# ---- fine machine, same configuration
MODE = {"Txn": "txn", "Dev": "dev", "NoTxn": "notxn"}
POINT = {"Guard": "guard:m", "Manifest": "manifest:m", "Deps": "deps:m", "Activate": "activate:m"}


def fine_run(mode, refuse, dep, act, crash_at):
    deps = {"NoDep": [], "Ok": [Mod("d", has_cls=True)], "Fail": [Mod("d", has_cls=True, activate="fail")],
            "Crash": [Mod("d", has_cls=True, activate="halt")]}[dep]
    env = Env()
    if refuse:
        env.set(("mod", "m"), {"version": "9.0.0", "dev": 0})
    env.halt_at = (POINT[crash_at], 1) if crash_at else None
    run_command(env, Mod("m", has_cls=True, deps=deps, activate="ok" if act else "fail"), MODE[mode], pre_newer=refuse)
    return env


def fine_view(env, refuse):
    rows = [env.db[k] for k in sorted(k for k in env.db if k[0] == "row")]
    top = rows[0] if rows else None
    dep = rows[1] if len(rows) > 1 else None

    def r(x):
        if x is None:
            return NO_ROW
        return dict(exists=True, named=x["package"] != "", ok=bool(x["success"]), committed=bool(x["committed"]), ended=bool(x["ended"]))
    m_now = env.db.get(("mod", "m"))
    m = (m_now is not None) if not refuse else (m_now != {"version": "9.0.0", "dev": 0})
    return dict(m=m, d=("mod", "d") in env.db, top=r(top), dep=r(dep))


def verus_view(s):
    d = durable(s)
    return dict(m=d["m"], d=d["d"], top=d["top"], dep=d["dep"])


def compare():
    mismatches, n = [], 0
    for mode, refuse, dep, act, crash_at in itertools.product(("Txn", "Dev", "NoTxn"), (False, True),
                                                              ("NoDep", "Ok", "Fail", "Crash"), (True, False),
                                                              (None, "Guard", "Manifest", "Deps", "Activate")):
        v = verus_view(verus_run(mode, refuse, dep, act, crash_at))
        f = fine_view(fine_run(mode, refuse, dep, act, crash_at), refuse)
        n += 1
        if v != f:
            mismatches.append({"cfg": [mode, refuse, dep, act, crash_at], "verus": v, "fine": f})
    return n, mismatches


if __name__ == "__main__":
    n, mm = compare()
    print(n, "configurations;", len(mm), "mismatches")
    for x in mm[:10]:
        print(x)
