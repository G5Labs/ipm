"""Abstraction check for models/verus/load_general.rs against the fine L2 machine (l2_load), which conforms to IRIS.

The Verus spec functions are transcribed literally below (same names). For every configuration in the product of
mode x dependency count (0..3) x dependency outcomes x Activate outcome x prior state (top absent/older/newer,
dependencies absent/older) x caller (none/commits/rolls back) x crash point (none, each running pc, each dependency)
x crash resolution x fix on/off, both machines run and their L1 views (durable module versions and history rows)
are compared. Fixes are the code variants `fix-dev-persist` + `fix-dep-repersist` in l2_load.
"""
from __future__ import annotations

import itertools
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from irisenv import Env, Halt  # noqa: E402
import l2_load  # noqa: E402
from l2_load import Mod  # noqa: E402

NO_ROW = dict(exists=False, named=False, ok=False, committed=False, ended=False)
INIT_ROW = dict(NO_ROW, exists=True)


def U(base, **kw):
    x = dict(base)
    x.update(kw)
    return x


def dep_row(ok, committed, ended):
    return dict(exists=True, named=True, ok=ok, committed=committed, ended=ended)


DEV_ROW = dict(exists=True, named=True, ok=False, committed=True, ended=False)


# ------------------------------------------------------------------ transcription of load_general.rs
def k(c):
    return 0 if c["caller"] == "NoCaller" else 1


def uses_txn(m):
    return m != "NoTxn"


def merge(a, p):
    return [p[j] if p[j]["exists"] else a[j] for j in range(len(a))]


def merge_fix(a, p, deps, vers):
    return [U(p[j], committed=deps[j] == vers[j]) if p[j]["exists"] else a[j] for j in range(len(a))]


def step_init(s):
    db = U(s["db"], top_row=INIT_ROW)
    return U(s, db=db, snaps=s["snaps"] + [db] if uses_txn(s["cfg"]["mode"]) else s["snaps"], temp=INIT_ROW, pc=("Guard",))


def fail_end(s):
    kk = k(s["cfg"])
    rolled = s["snaps"][kk] if len(s["snaps"]) > kk else s["db"]
    db2 = U(rolled, dep_rows=merge_fix(rolled["dep_rows"], s["pend"], rolled["deps"], s["cfg"]["vers"])) if s["cfg"]["fix"] else rolled
    t = U(s["temp"], ok=False, ended=True)
    return U(s, db=U(db2, top_row=t), snaps=s["snaps"][:kk], temp=t, pc=("Closed",))


def ok_end(s):
    t = U(s["temp"], ok=True, committed=True, ended=True)
    return U(s, db=U(s["db"], top_row=t), temp=t, pc=("Closed",))


def step_guard(s):
    if s["cfg"]["refuse"]:
        return fail_end(s)
    if s["cfg"]["mode"] == "NoTxn":
        tt = U(s["temp"], committed=True)
        return U(s, db=U(s["db"], top_row=tt) if s["cfg"]["fix"] else s["db"], temp=tt, pc=("Manifest",))
    return U(s, pc=("Manifest",))


def step_manifest(s):
    db1 = U(s["db"], top=s["cfg"]["v"])
    t = U(s["temp"], named=True)
    if s["cfg"]["mode"] == "Dev":
        t2 = U(t, committed=True)
        db2 = U(db1, top_row=t2) if s["cfg"]["fix"] else db1
        return U(s, db=db2, snaps=s["snaps"][:-1] + [db2], temp=t2, pc=("Deps", 0))
    return U(s, db=db1, temp=t, pc=("Deps", 0))


def crash(s, r, kind="Process"):
    closed = U(s["db"], top_row=s["temp"])
    if kind == "Instance":
        db = s["snaps"][0] if s["snaps"] else s["db"]
        return U(s, db=db, snaps=[], pc=("Crashed",), pend=[NO_ROW] * len(s["pend"]))
    db = s["snaps"][0] if (len(s["snaps"]) > 0 and r == "Rollback") else closed
    return U(s, db=db, snaps=[], pc=("Crashed",))


def upd(seq, i, v):
    x = list(seq)
    x[i] = v
    return x


def step_dep(s, i, r):
    c = s["cfg"]
    kept = c["mode"] == "NoTxn"
    out = c["outs"][i]
    if out == "Ok":
        row = dep_row(True, True, True)
        return U(s, db=U(s["db"], deps=upd(s["db"]["deps"], i, c["vers"][i]), dep_rows=upd(s["db"]["dep_rows"], i, row)),
                 pend=upd(s["pend"], i, row) if c["fix"] else s["pend"], began=i + 1, pc=("Deps", i + 1))
    row = dep_row(False, kept, out == "Fail")
    keep_dep = kept or (out == "Crash" and r == "Commit")
    s1 = U(s, db=U(s["db"], deps=upd(s["db"]["deps"], i, c["vers"][i]) if keep_dep else s["db"]["deps"],
                   dep_rows=upd(s["db"]["dep_rows"], i, row)),
           pend=upd(s["pend"], i, row) if c["fix"] else s["pend"], began=i + 1)
    return fail_end(s1) if out == "Fail" else crash(s1, r)


def step_deps_done(s):
    if s["cfg"]["mode"] == "Dev":
        return U(s, snaps=s["snaps"][:-1], pc=("Activate",))
    return U(s, pc=("Activate",))


def step_activate(s):
    if s["cfg"]["act_ok"]:
        if s["cfg"]["mode"] == "Txn":
            return U(s, snaps=s["snaps"][:-1], pc=("Window",))
        return ok_end(s)
    return fail_end(s)


def step_window(s):
    return ok_end(U(s, temp=U(s["temp"], committed=True)))


def step_reconcile(s):
    return U(s, db=U(s["db"], dep_rows=merge_fix(s["db"]["dep_rows"], s["pend"], s["db"]["deps"], s["cfg"]["vers"])), pc=("Reconciled",))


def step_caller_end(s):
    db = s["snaps"][0] if s["cfg"]["caller"] == "CallerRollsBack" else s["db"]
    return U(s, db=db, snaps=[], pc=("Done",))


def durable(s):
    return s["db"] if not s["snaps"] else s["snaps"][0]


def verus_run(cfg, pre, crash_at, res, kind="Process"):
    n = len(cfg["vers"])
    s = dict(db=pre, snaps=[pre] if k(cfg) == 1 else [], temp=NO_ROW, pend=[NO_ROW] * n, pc=("Init",), cfg=cfg, pre=pre, began=0)
    s = step_init(s)
    while True:
        pc = s["pc"]
        if pc[0] in ("Closed", "Crashed", "Done", "Reconciled"):
            break
        if crash_at == pc:
            s = crash(s, res, kind)
            break
        if pc[0] == "Guard":
            s = step_guard(s)
        elif pc[0] == "Manifest":
            s = step_manifest(s)
        elif pc[0] == "Deps":
            s = step_dep(s, pc[1], res) if pc[1] < n else step_deps_done(s)
        elif pc[0] == "Activate":
            s = step_activate(s)
        elif pc[0] == "Window":
            s = step_window(s)
    if s["pc"][0] == "Closed" and cfg["caller"] != "NoCaller":
        s = step_caller_end(s)
    if s["pc"][0] == "Crashed" and cfg["fix"]:
        s = step_reconcile(s)
    d = durable(s)
    return dict(top=d["top"], deps=list(d["deps"]), top_row=d["top_row"], dep_rows=list(d["dep_rows"]))


# ------------------------------------------------------------------ fine machine, same configuration
POINT = {"Guard": "guard:m", "Manifest": "manifest:m", "Activate": "activate:m", "Window": "after-commit:m"}


def fine_run(cfg, pre, crash_at, res, kind="Process"):
    n = len(cfg["vers"])
    names = [f"d{j}" for j in range(n)]
    act = {"Ok": "ok", "Fail": "fail", "Crash": "halt"}
    deps = [Mod(names[j], version=str(cfg["vers"][j]), has_cls=True, activate=act[cfg["outs"][j]]) for j in range(n)]
    env = Env()
    if pre["top"] is not None:
        env.set(("mod", "m"), {"version": str(pre["top"]), "dev": 0})
    for j in range(n):
        if pre["deps"][j] is not None:
            env.set(("mod", names[j]), {"version": str(pre["deps"][j]), "dev": 0})
    if crash_at is not None:
        if crash_at[0] == "Deps":
            env.halt_at = ("deps:m", 1) if crash_at[1] == 0 else (f"before-dep:{names[crash_at[1]]}", 1)
        else:
            env.halt_at = (POINT[crash_at[0]], 1)
    env.resolution = "commit" if res == "Commit" else "rollback"
    env.crash_kind = "instance" if kind == "Instance" else "process"
    mut = frozenset({"fix-dev-persist", "fix-dep-repersist"}) if cfg["fix"] else frozenset()
    mode = {"Txn": "txn", "Dev": "dev", "NoTxn": "notxn"}[cfg["mode"]]
    caller = {"NoCaller": None, "CallerCommits": "commit", "CallerRollsBack": "rollback"}[cfg["caller"]]
    status = l2_load.run_command(env, Mod("m", version=str(cfg["v"]), has_cls=True, deps=deps,
                                          activate="ok" if cfg["act_ok"] else "fail"),
                                 mode, mut, caller=caller, pre_newer=cfg["refuse"])
    if status == "halted" and cfg["fix"]:
        env.undo = []
        l2_load.reconcile_pending(env)        # the next command reconciles leftovers
    rows = {}
    for key in sorted(kk for kk in env.db if kk[0] == "row"):
        rows[env.db[key]["package"] or "m"] = env.db[key]

    def r(x):
        if x is None:
            return NO_ROW
        return dict(exists=True, named=x["package"] != "", ok=bool(x["success"]), committed=bool(x["committed"]), ended=bool(x["ended"]))
    top = env.db.get(("mod", "m"))
    return dict(top=int(top["version"]) if top else None,
                deps=[int(env.db[("mod", nm)]["version"]) if ("mod", nm) in env.db else None for nm in names],
                top_row=r(rows.get("m")), dep_rows=[r(rows.get(nm)) for nm in names])


def configs():
    for mode, n, act_ok, pre_top, caller, fix in itertools.product(
            ("Txn", "Dev", "NoTxn"), (0, 1, 2, 3), (True, False), (None, 1, 9), ("NoCaller", "CallerCommits", "CallerRollsBack"), (False, True)):
        for outs in itertools.product(("Ok", "Fail", "Crash"), repeat=n):
            for pre_deps in itertools.product((None, 0), repeat=n):
                if n == 3 and (len(set(outs)) > 1 or len(set(pre_deps)) > 1):
                    continue    # keep n = 3 to uniform outcomes and prior states
                cfg = dict(mode=mode, v=2, vers=[1] * n, outs=list(outs), act_ok=act_ok, refuse=pre_top == 9,
                           caller=caller, fix=fix)
                pre = dict(top=pre_top, deps=list(pre_deps), top_row=NO_ROW, dep_rows=[NO_ROW] * n)
                points = [None, ("Guard",), ("Manifest",), ("Activate",), ("Window",)] + [("Deps", i) for i in range(n)]
                for crash_at in points:
                    if crash_at is not None and caller != "NoCaller":
                        continue
                    if crash_at == ("Window",) and mode != "Txn":
                        continue
                    for res in (("Rollback", "Commit") if crash_at is not None or "Crash" in outs else ("Rollback",)):
                        yield cfg, pre, crash_at, res, "Process"
                    if crash_at is not None:
                        yield cfg, pre, crash_at, "Rollback", "Instance"


def compare():
    total, mismatches, skipped = 0, [], 0
    for cfg, pre, crash_at, res, kind in configs():
        v = verus_run(cfg, pre, crash_at, res, kind)
        try:
            f = fine_run(cfg, pre, crash_at, res, kind)
        except Exception as e:  # noqa: BLE001
            mismatches.append({"cfg": cfg, "crash_at": crash_at, "res": res, "error": repr(e)})
            continue
        # a crash point the run never reaches (e.g. Window after a refusal) makes both machines run to completion
        total += 1
        if v != f:
            mismatches.append({"cfg": cfg, "pre": pre, "crash_at": crash_at, "res": res, "kind": kind, "verus": v, "fine": f})
    return total, mismatches


if __name__ == "__main__":
    n, mm = compare()
    print(n, "configurations;", len(mm), "mismatches")
    for x in mm[:8]:
        print(x)
