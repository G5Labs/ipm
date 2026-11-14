#!/usr/bin/env python3
"""Explicit-state exploration of the IPM load/history L2 machine, plus validation against IRIS.

  python3 -I models/explorer/explore.py        (from the experiment root)

Writes evidence/model-checks.json and models/explorer/REPORT.md. Parts:
 A. Environment differential: each IRIS probe replayed on the environment contract; mutants must be killed.
 B. Trace conformance: each IPM scenario observed on IRIS replayed on the L2 machine.
 C. Exhaustive exploration of L1 rules over every configuration and every crash point.
 D. Broken-implementation controls (code mutants) must be rejected by C or B.
"""
from __future__ import annotations

import itertools
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))

from irisenv import MUTANTS as ENV_MUTANTS, Env, Halt, SaveError  # noqa: E402
from l2_load import CODE_MUTANTS, HistoryLog, Mod, run_command  # noqa: E402
import l2_load  # noqa: E402

OBS = {r["name"]: r for r in json.loads((ROOT / "evidence" / "iris-observations.json").read_text())["records"]}

# ----------------------------------------------------------------------------- A. environment differential


class Rec:
    """Uplift.Probe.TempRec analogue (an IRISTEMP log with a twin row)."""

    def __init__(self, env, name):
        self.id = env.incr("temp")
        self.twin = env.incr("rec")
        env.tset(("t", self.id), {"name": name, "committed": 0})
        env.save_row(("rec", self.twin), {"name": name, "committed": 0, "finalized": 0}, False)
        self.saved = True
        env.open_obj(self)

    def on_close(self, env):
        n = env.incr("onclose")
        env.set(("onclose", n), env.tl)
        d = dict(env.temp.get(("t", self.id), {"name": "?", "committed": 0}))
        try:
            env.save_row(("rec", self.twin), {"name": d["name"], "committed": d["committed"], "finalized": 1}, self.saved)
        except SaveError:
            pass
        env.tkill(("t", self.id))


def count(env, kind):
    return sum(1 for k in env.db if k[0] == kind)


def first_rec(env, prop):
    keys = sorted(k for k in env.db if k[0] == "rec")
    return env.db[keys[0]][prop] if keys else "<none>"


def probe_env(mut):
    out = {}
    e = Env(mut); e.tstart(); e.set("outer", 1); e.tstart(); e.set("inner", 1); e.tcommit()
    t1 = e.tl; e.trollback1()
    out["env-nested-commit-outer-rollback"] = {"tlevelAfterInnerCommit": t1, "tlevelAfterRollback": e.tl,
                                               "outerKept": int("outer" in e.db), "innerKept": int("inner" in e.db)}
    e = Env(mut); e.tstart(); e.set("outer", 1); e.tstart(); e.set("inner", 1); e.trollback1()
    r = {"tlevelAfterFirst": e.tl, "outerKeptAfterFirst": int("outer" in e.db), "innerKeptAfterFirst": int("inner" in e.db)}
    if e.tl:
        e.trollback1()
    r.update({"tlevelAfterSecond": e.tl, "outerKeptAfterSecond": int("outer" in e.db)})
    out["env-rollback-one-level"] = r
    e = Env(mut); e.tstart(); e.set("outer", 1); e.tstart(); e.set("inner", 1); e.trollback_all()
    out["env-rollback-all"] = {"tlevel": e.tl, "outerKept": int("outer" in e.db), "innerKept": int("inner" in e.db)}
    e = Env(mut); e.tstart(); e.tset("x", 1); e.set("x", 1); e.trollback_all()
    out["env-iristemp-survives-rollback"] = {"tempKept": int("x" in e.temp), "ordinaryKept": int("x" in e.db)}
    # %OnClose inside a caller transaction
    e = Env(mut); e.tstart()
    rec = Rec(e, "dep"); e.temp[("t", rec.id)]["committed"] = 1
    e.drop_obj(rec)
    closed = e.ctr.get("onclose", 0)
    tl_seen = e.db.get(("onclose", 1), "")
    before = count(e, "rec"); e.trollback_all()
    e.finish()
    out["env-onclose-inside-caller-transaction"] = {"closedBeforeCallerContinues": closed, "tlevelSeenInOnClose": tl_seen,
                                                    "twinRowsBeforeRollback": before, "twinRowsAfterRollback": count(e, "rec"),
                                                    "tempRowsAfterRollback": sum(1 for k in e.temp if k[0] == "t")}
    e = Env(mut)
    rec = Rec(e, "top"); e.temp[("t", rec.id)]["committed"] = 1
    e.drop_obj(rec); e.finish()
    out["env-onclose-outside-transaction"] = {"closed": e.ctr.get("onclose", 0), "twinRows": count(e, "rec"),
                                              "twinCommitted": first_rec(e, "committed"), "twinFinalized": first_rec(e, "finalized"),
                                              "tempRows": sum(1 for k in e.temp if k[0] == "t")}
    e = Env(mut); e.tstart(); rid = e.incr("rec"); e.save_row(("rec", rid), {"name": "ghost", "finalized": 0}, False); e.trollback_all()
    after_rb = count(e, "rec")
    try:
        e.save_row(("rec", rid), {"name": "ghost2", "finalized": 0}, True); ok = 1
    except SaveError:
        ok = 0
    out["env-resave-after-rollback"] = {"rowsAfterRollback": after_rb, "idStillOnOref": 1, "resaveOK": ok,
                                        "rowsAfterResave": count(e, "rec"), "existsById": int(("rec", rid) in e.db)}
    e = Env(mut); e.save_row(("rec", 1), {"name": "r", "finalized": 1}, False)
    try:
        e.save_row(("rec", 1), {"name": "r", "finalized": 1}, True); same = 1
    except SaveError:
        same = 0
    try:
        e.save_row(("rec", 1), {"name": "changed", "finalized": 1}, True); changed = 1
    except SaveError:
        changed = 0
    out["env-finalized-trigger"] = {"modifiedAfterSameValue": 0, "saveSameValueOK": same, "saveChangedOK": changed, "sqlDeleteSQLCODE": 0}
    # child job halts with an open transaction
    e = Env(mut)
    rec = Rec(e, "child"); e.temp[("t", rec.id)]["committed"] = 1
    e.tstart(); e.set("childTxnWrite", 1)
    e.halt()
    keys = sorted(k for k in e.temp if k[0] == "t")
    out["env-halt-open-transaction"] = {"childGone": 1, "childTxnWriteKept": int("childTxnWrite" in e.db),
                                        "onCloseRanInChild": e.ctr.get("onclose", 0), "twinRows": count(e, "rec"),
                                        "twinCommitted": first_rec(e, "committed"), "twinFinalized": first_rec(e, "finalized"),
                                        "tempRowsLeft": len(keys), "onCloseTlevel": e.db.get(("onclose", 1), "")}
    return out


def diff(pred: dict, name: str):
    obs = OBS[name]["values"]
    bad = []
    for k, v in pred.items():
        if k in obs and str(obs[k]) != str(v):
            bad.append(f"{k}: model {v} vs IRIS {obs[k]}")
    return bad

# ----------------------------------------------------------------------------- B. scenario conformance


FIX_HOOKS = "UpParent.Hooks"


def M(name, **kw):
    return Mod(name, **kw)


def scenario_runs():
    """Each IRIS scenario as a list of commands (one process each). Returns {obs name: (commands, class map)}."""
    dep = lambda act="ok": M("up-dep", has_cls=True, activate=act)  # noqa: E731
    S = {}
    S["ipm-load-ok"] = [dict(mod=M("simplest-module"), mode="txn")]
    S["ipm-dev-downgrade-refused"] = [dict(mod=M("simplest-module", version="2.0.0"), mode="txn"),
                                      dict(mod=M("simplest-module"), mode="dev", pre_newer=True)]
    S["ipm-dev-activate-fails"] = [dict(mod=M("up-res-fail", has_cls=True, activate="fail"), mode="dev")]
    S["ipm-activate-fails"] = [dict(mod=M("up-res-fail", has_cls=True, activate="fail"), mode="txn")]
    S["ipm-nojournal-activate-fails"] = [dict(mod=M("up-res-fail", has_cls=True, activate="fail"), mode="notxn")]
    S["ipm-notxn-activate-fails"] = [dict(mod=M("up-res-fail", has_cls=True, activate="fail"), mode="notxn")]
    S["ipm-dep-ok"] = [dict(mod=M("up-parent-ok", has_cls=True, deps=[dep()]), mode="txn")]
    S["ipm-dep-parent-fails"] = [dict(mod=M("up-parent-fail", has_cls=True, deps=[dep()], activate="fail"), mode="txn")]
    S["ipm-dev-dep-parent-fails"] = [dict(mod=M("up-dev-dep-fail", has_cls=True, deps=[dep()], activate="fail"), mode="dev")]
    S["ipm-dev-missing-dependency"] = [dict(mod=M("up-parent-ok", has_cls=True, resolvable=False), mode="dev")]
    S["ipm-missing-dependency"] = [dict(mod=M("up-parent-ok", has_cls=True, resolvable=False), mode="txn")]
    S["ipm-install-from-fs-repo"] = [dict(mod=dep(), mode="txn", action="install", known_name="up-dep")]
    S["ipm-update-with-path"] = [dict(mod=M("simplest-module"), mode="txn"),
                                 dict(mod=M("simplest-module", version="2.0.0"), mode="txn", action="update", known_name="simplest-module")]
    S["ipm-load-inside-caller-transaction-rolled-back"] = [dict(mod=M("simplest-module"), mode="txn", caller="rollback")]
    S["ipm-failed-load-inside-caller-transaction-committed"] = [dict(mod=M("up-res-fail", has_cls=True, activate="fail"), mode="txn", caller="commit")]
    S["ipm-load-relative-path-refused"] = [dict(mod=M("simplest-module"), mode="txn", path_ok=False)]
    for mode in ("txn", "notxn", "dev"):
        S[f"ipm-halt-{mode}"] = [dict(mod=M("up-halt", has_cls=True, activate="halt"), mode=mode)]
        S[f"ipm-terminate-{mode}"] = [dict(mod=M("up-hang", has_cls=True, activate="halt"), mode=mode)]
    S["ipm-halt-txn-operator-commits"] = [dict(mod=M("up-halt", has_cls=True, activate="halt"), mode="txn", resolution="commit")]
    crash_dep = M("up-dep-halt", has_cls=True, activate="halt")
    S["ipm-halt-dev-crash-in-dependency"] = [dict(mod=M("up-dev-parent-crash", has_cls=True, deps=[crash_dep]), mode="dev")]
    S["ipm-halt-txn-crash-in-dependency"] = [dict(mod=M("up-dev-parent-crash", has_cls=True, deps=[crash_dep]), mode="txn")]
    # Native-gap lane (validation/native-gaps-n2.md): instance crashes (P1) and dependency chains/upgrades (P9).
    for mode in ("txn", "notxn", "dev"):
        S[f"nc-gap-P1-docker-{mode}-after"] = [dict(mod=M("n2-wait", has_cls=True), mode=mode, halt_at=("activate:n2-wait", 1), crash_kind="instance")]
    S["nc-gap-P1-force-txn-after"] = [dict(mod=M("n2-wait", has_cls=True), mode="txn", halt_at=("activate:n2-wait", 1), crash_kind="instance")]
    for mode in ("txn", "notxn", "dev"):
        cc = lambda act="ok": M("n2-cc", has_cls=True, activate=act)  # noqa: E731
        S[f"nc-gap-P9-chain-ok-{mode}"] = [dict(mod=M("n2-ca", has_cls=True, deps=[cc(), M("n2-cb", has_cls=True)]), mode=mode)]
        S[f"nc-gap-P9-chain-c-fails-{mode}"] = [dict(mod=M("n2-ca", has_cls=True, deps=[cc("fail"), M("n2-cb", has_cls=True)]), mode=mode)]
        S[f"nc-gap-P9-two-deps-parent-fails-{mode}"] = [dict(mod=M("n2-tp", has_cls=True, activate="fail",
                                                                deps=[M("n2-d1", has_cls=True), M("n2-d2", has_cls=True)]), mode=mode)]
        pre = dict(mod=M("n2-pd", has_cls=True), mode="txn")
        S[f"nc-gap-P9-upgrade-ok-{mode}"] = [pre, dict(mod=M("n2-pa", has_cls=True, deps=[M("n2-pd", version="1.1.0", has_cls=True)]), mode=mode)]
        S[f"nc-gap-P9-upgrade-dep-fails-{mode}"] = [pre, dict(mod=M("n2-pa", has_cls=True, deps=[M("n2-pd", version="1.1.0", has_cls=True, activate="fail")]), mode=mode)]
        S[f"nc-gap-P9-upgrade-parent-fails-{mode}"] = [pre, dict(mod=M("n2-pa", has_cls=True, activate="fail", deps=[M("n2-pd", version="1.1.0", has_cls=True)]), mode=mode)]
    return S


CLASS_OF = {"up-res-fail": "UpParent.Hooks", "up-parent-ok": "UpParent.Hooks", "up-parent-fail": "UpParent.Hooks",
            "up-dev-dep-fail": "UpParent.Hooks", "up-halt": "UpParent.Hooks", "up-hang": "UpParent.Hooks",
            "up-dev-parent-crash": "UpParent.Hooks", "up-dep": "UpDep.Hello", "up-dep-halt": "UpDepHalt.Hooks"}


def observe(env: Env, status):
    o = {"cmdStatus": {True: "ok", False: "err", "halted": "n/a"}[status]}
    rows = [env.db[k] for k in sorted(k for k in env.db if k[0] == "row")]
    for i, r in enumerate(rows, 1):
        o[f"row{i}"] = (f"{r['action']}/{r['package']}/{r['version']}/success:{'ok' if r['success'] else 'err'}"
                        f"/committed:{r['committed']}/ended:{r['ended']}/finalized:{r['finalized']}")
    o["historyRows"] = len(rows)
    o["historyTempLeft"] = sum(1 for k in env.temp if k[0] == "htemp") + sum(1 for k in env.db if k[0] == "htemp")
    for k, v in env.db.items():
        if k[0] == "mod":
            o[f"mod:{k[1]}"] = v["version"] + ("/dev" if v["dev"] else "")
    for cls in ("UpParent.Hooks", "UpDep.Hello"):
        o[f"cls:{cls}"] = "absent"
    for k in env.db:
        if k[0] == "cls" and CLASS_OF.get(k[1]) in ("UpParent.Hooks", "UpDep.Hello"):
            o[f"cls:{CLASS_OF[k[1]]}"] = "compiled"
    return o


def run_scenario(cmds, mut=frozenset(), env_mut=frozenset()):
    env = Env(env_mut)
    status = None
    for c in cmds:
        c = dict(c)
        env.resolution = c.pop("resolution", "rollback")
        env.undo, env.objs, env.points, env.halt_at = [], [], [], c.pop("halt_at", None)
        env.crash_kind = c.pop("crash_kind", "process")
        status = run_command(env, c.pop("mod"), c.pop("mode"), mut, caller=c.pop("caller", None), **c)
    return env, status


def conformance(mut=frozenset(), env_mut=frozenset()):
    results = {}
    for name, cmds in scenario_runs().items():
        env, status = run_scenario(cmds, mut, env_mut)
        pred = observe(env, status)
        obs = OBS[name]["values"]
        bad = []
        keys = set(pred) | {k for k in obs if k.startswith(("row", "mod:", "cls:")) or k in ("historyRows", "historyTempLeft", "cmdStatus")}
        for k in sorted(keys):
            o = obs.get(k, "<absent>")
            if k.startswith("row") and o != "<absent>":
                o = re.sub(r"^id\d+/", "", o.split("/phases:")[0])
            if k.startswith("rowId"):
                continue
            if k.startswith("cls:") and (str(o).startswith("compiled/V:") or name.startswith("nc-gap")):
                continue   # the native lane's class probes carry versions; module records and rows are compared
            p = pred.get(k, "<absent>")
            if k.startswith("cls:") and (o == "<absent>" or (p == "<absent>" and o == "absent")):
                continue
            if str(p) != str(o):
                bad.append(f"{k}: model {p} vs IRIS {o}")
        results[name] = bad
    return results

# ----------------------------------------------------------------------------- C. exploration of L1 rules


RULES = {
    "L1-REC-TOP": "every command leaves one history record",
    "L1-REC-DEP": "every dependency load that started leaves a history record",
    "L1-COMMIT-SOUND": "Committed = 0 means none of the action's changes were kept",
    "L1-COMMIT-EXACT": "Committed = 1 means some change was kept (a load without a transaction may over-report)",
    "L1-SUCCESS-COMMITTED": "a successful command is marked committed (top-level record)",
    "L1-ENDED": "an end time is recorded exactly when the command ended normally",
    "L1-SUCCESS-EXACT": "Success is recorded exactly when the command worked; never after a crash",
    "L1-ATOMIC": "a failed or crashed transactional load leaves installed modules unchanged",
    "L1-DEV-KEEP": "a developer-mode load keeps the module record once it is saved",
    "L1-SUCCESS-STATE": "a successful load installs the module and its dependencies",
    "L1-REFUSE": "a refused load changes no installed module",
    "L1-NO-TEMP": "no temporary history record is left behind",
    "L1-PKG-NAMED": "a record names the module once its manifest was read",
}


def module_changed(pre: dict, post: dict, name: str) -> bool:
    return pre.get(("mod", name)) != post.get(("mod", name)) or pre.get(("cls", name)) != post.get(("cls", name))


class Tracker:
    """Ghost: which logs were created, for which module, and which program points ran."""

    def __init__(self):
        self.logs = []

    def install(self):
        orig = HistoryLog.__init__
        tracker = self

        def init(self_, env, action, package, mut):
            orig(self_, env, action, package, mut)
            tracker.logs.append(self_)
        HistoryLog.__init__ = init
        return orig


def check(cfg, env: Env, pre_db, status, halted, logs, mode):
    v = set()
    top, deps = logs[0], logs[1:]
    mod = cfg["mod"]
    rowof = lambda lg: env.db.get(("row", lg.twin_id))  # noqa: E731
    modof = {top.id: mod.name}
    for lg, d in zip(deps, [d for d in mod.deps]):
        modof[lg.id] = d.name
    if rowof(top) is None:
        v.add("L1-REC-TOP")
    for lg in deps:
        if rowof(lg) is None:
            v.add("L1-REC-DEP")
    for lg in logs:
        r = rowof(lg)
        if r is None:
            continue
        name = modof.get(lg.id, mod.name)
        kept = module_changed(pre_db, env.db, name)
        if lg is top:
            kept = kept or any(module_changed(pre_db, env.db, d.name) for d in mod.deps)
        if r["committed"] == 0 and kept:
            v.add("L1-COMMIT-SOUND")
        if r["committed"] == 1 and not kept and not (mode == "notxn"):
            v.add("L1-COMMIT-EXACT")
        if lg is top and r["success"] == 1 and r["committed"] == 0:   # top-level rule; see DEC-HIST-1 for dependency rows
            v.add("L1-SUCCESS-COMMITTED")
        if lg is top and r["ended"] != (0 if halted else 1):
            v.add("L1-ENDED")
        if lg is top:
            worked = (not halted and cfg["pre"] != "newer" and cfg["path_ok"] and mod.resolvable and mod.activate == "ok"
                      and all(d.activate == "ok" for d in mod.deps))
            if bool(r["success"]) != worked:
                v.add("L1-SUCCESS-EXACT")
        elif r["success"] and halted and r["ended"] == 0:
            v.add("L1-SUCCESS-EXACT")
        manifest_read = any(p == f"deps:{name}" or p.startswith(f"activate:{name}") for p in env.points) or \
            (lg is top and cfg["pre"] == "newer")
        if manifest_read and r["package"] != name:
            v.add("L1-PKG-NAMED")
    changed_any = module_changed(pre_db, env.db, mod.name) or any(module_changed(pre_db, env.db, d.name) for d in mod.deps)
    if mode == "txn" and status is not True and changed_any:
        v.add("L1-ATOMIC")
    if mode == "dev" and f"deps:{mod.name}" in env.points and ("mod", mod.name) not in env.db:
        v.add("L1-DEV-KEEP")
    if status is True and (env.db.get(("mod", mod.name), {}).get("version") != mod.version or
                           any(("mod", d.name) not in env.db for d in mod.deps)):
        v.add("L1-SUCCESS-STATE")
    if cfg["pre"] == "newer" and any(env.db.get(k) != pre_db.get(k) for k in set(env.db) | set(pre_db) if k[0] == "mod"):
        v.add("L1-REFUSE")
    if any(k[0] == "htemp" for k in env.temp) or any(k[0] == "htemp" for k in env.db):
        v.add("L1-NO-TEMP")
    return v


def configs():
    dep_opts = {"none": [], "ok": [Mod("d", has_cls=True)], "fail": [Mod("d", has_cls=True, activate="fail")],
                "halt": [Mod("d", has_cls=True, activate="halt")]}
    for mode, pre, deps, resolvable, act, path_ok, caller in itertools.product(
            ("txn", "dev", "notxn"), ("absent", "newer"), dep_opts, (True, False), ("ok", "fail"), (True, False),
            (None, "commit", "rollback")):
        if not path_ok and (pre, deps, resolvable, act) != ("absent", "none", True, "ok"):
            continue
        if not resolvable and deps != "none":
            continue
        yield dict(mode=mode, pre=pre, deps=deps, resolvable=resolvable, act=act, path_ok=path_ok, caller=caller,
                   mod=Mod("m", has_cls=True, deps=[Mod(d.name, has_cls=True, activate=d.activate) for d in dep_opts[deps]],
                           activate=act, resolvable=resolvable))


def one_run(cfg, mut, halt_at=None, resolution="rollback", close_order="inner-first"):
    env = Env(frozenset())
    if cfg["pre"] == "newer":
        env.set(("mod", "m"), {"version": "9.0.0", "dev": 0})
    pre_db = dict(env.db)
    env.halt_at = halt_at
    env.resolution = resolution
    tracker = Tracker()
    orig = tracker.install()
    orig_halt = env.halt
    env.halt = lambda close_order_=close_order: orig_halt(close_order_)
    try:
        status = run_command(env, cfg["mod"], cfg["mode"], mut, caller=cfg["caller"], pre_newer=cfg["pre"] == "newer",
                             path_ok=cfg["path_ok"])
        if status == "halted" and "fix-dep-repersist" in mut:
            env.undo = []
            l2_load.reconcile_pending(env)   # the next command reconciles pending dependency rows
    finally:
        HistoryLog.__init__ = orig
    halted = status == "halted"
    return env, pre_db, status, halted, tracker.logs


def premises(cfg, halt_label, resolution):
    p = set()
    if cfg["caller"] is not None:
        p.add("P1-caller-transaction")
    if halt_label is not None and resolution == "commit":
        p.add("P2-operator-commits-at-halt")
    if halt_label is not None and halt_label.startswith("after-commit"):
        p.add("P3-commit-flag-window")
    return p


def explore(mut=frozenset()):
    viol = defaultdict(list)          # (rule, premise-set) -> examples
    runs = 0
    finals = set()
    for cfg in configs():
        env, pre_db, status, halted, logs = one_run(cfg, mut)
        runs += 1
        points = list(env.points)
        plans = [(None, "rollback", "inner-first")]
        seen = defaultdict(int)
        for p in points:
            seen[p] += 1
            for res in ("rollback", "commit"):
                for order in ("inner-first", "outer-first"):
                    plans.append(((p, seen[p]), res, order))
        for halt_at, res, order in plans:
            if halt_at is None:
                e, pdb, st, h, lg = env, pre_db, status, halted, logs
            else:
                e, pdb, st, h, lg = one_run(cfg, mut, halt_at, res, order)
                runs += 1
            finals.add(json.dumps(observe(e, st), sort_keys=True))
            label = halt_at[0] if halt_at else (("activate:" + cfg["mod"].deps[0].name) if st == "halted" and cfg["mod"].deps else ("activate:m" if st == "halted" else None))
            prem = frozenset(premises(cfg, label, res))
            for rule in check(cfg, e, pdb, st, h, lg, cfg["mode"]):
                key = (rule, prem)
                if len(viol[key]) < 400:
                    viol[key].append({"mode": cfg["mode"], "pre": cfg["pre"], "deps": cfg["deps"], "resolvable": cfg["resolvable"],
                                      "activate": cfg["act"], "path_ok": cfg["path_ok"], "caller": cfg["caller"],
                                      "halt": label, "resolution": res if label else None})
    return viol, runs, len(finals)


def summarize(viol):
    """rule -> {"premise-free": n, "only-under": {premise: n}} plus a small set of distinct example shapes."""
    out = {}
    for (rule, prem), ex in viol.items():
        s = out.setdefault(rule, {"without_premise": 0, "with_premise": defaultdict(int), "examples": []})
        if prem:
            for p in prem:
                s["with_premise"][p] += len(ex)
        else:
            s["without_premise"] += len(ex)
            shapes = {(e["mode"], e["deps"], e["activate"], e["halt"].split(":")[0] if e["halt"] else None) for e in ex}
            for sh in sorted(shapes, key=lambda x: (x[3] is not None, x[0], str(x)))[:12]:
                if sh not in s["examples"]:
                    s["examples"].append(sh)
    for s in out.values():
        s["with_premise"] = dict(s["with_premise"])
    return out

# ----------------------------------------------------------------------------- main


def main():
    checks = []
    report = ["# Explorer report (generated by models/explorer/explore.py)", ""]

    # A
    base = probe_env(frozenset())
    env_bad = {n: diff(p, n) for n, p in base.items()}
    ok_a = all(not b for b in env_bad.values())
    checks.append({"name": "env-contract-differential", "status": "pass" if ok_a else "fail",
                   "detail": {n: (b or "match") for n, b in env_bad.items()}})
    report += ["## A. Environment contract vs IRIS", "", f"Probes compared: {len(base)}; mismatches: {sum(len(b) for b in env_bad.values())}.", ""]
    for n, b in env_bad.items():
        report.append(f"- `{n}`: {'match' if not b else '; '.join(b)}")
    report.append("")
    kills = {}
    for m in ENV_MUTANTS:
        mp = probe_env(frozenset({m}))
        killers = [n for n, p in mp.items() if diff(p, n)]
        conf = conformance(env_mut=frozenset({m}))
        conf_kill = [n for n, b in conf.items() if b]
        kills[m] = {"probes": killers, "scenarios": conf_kill}
    all_killed = all(k["probes"] for k in kills.values())
    checks.append({"name": "env-mutants-killed", "status": "pass" if all_killed else "fail", "detail": kills})
    report += ["Environment mutants (each must differ from IRIS on at least one probe):", ""]
    for m, k in kills.items():
        report.append(f"- `{m}` ({ENV_MUTANTS[m]}): {'KILLED by ' + ', '.join(k['probes']) if k['probes'] else 'SURVIVED'}"
                      f"; scenarios also differing: {len(k['scenarios'])}")
    report.append("")

    # B
    conf = conformance()
    ok_b = all(not b for b in conf.values())
    for n, b in conf.items():
        checks.append({"name": f"conf-{n}", "status": "pass" if not b else "fail", "detail": b or "match"})
    checks.append({"name": "trace-conformance", "status": "pass" if ok_b else "fail", "detail": f"{len(conf)} scenarios"})
    report += ["## B. L2 machine vs IPM scenarios on IRIS", "", f"Scenarios: {len(conf)}; failing: {sum(1 for b in conf.values() if b)}.", ""]
    for n, b in conf.items():
        report.append(f"- `{n}`: {'match' if not b else '; '.join(b)}")
    report.append("")

    # C
    viol, runs, nfinal = explore()
    summ = summarize(viol)
    report += ["## C. Exploration of the L1 rules", "", f"Runs: {runs}; distinct final observations: {nfinal}.",
               "Premises: P1 the command starts inside a caller's transaction; P2 an interactive operator commits at a halt; "
               "P3 the process ends between the outer TCOMMIT and SetCommitted.", ""]
    report.append("| Rule | Meaning | Violations with no premise | Violations only under premises | Example shapes (mode, deps, activate, crash point) |")
    report.append("|---|---|---|---|---|")
    for rule, meaning in RULES.items():
        s = summ.get(rule)
        if not s:
            report.append(f"| {rule} | {meaning} | 0 | 0 | |")
            status = "pass"
        else:
            report.append(f"| {rule} | {meaning} | {s['without_premise']} | {s['with_premise'] or 0} | {'; '.join(map(str, s['examples'][:6]))} |")
            status = "pass" if s["without_premise"] == 0 else "fail"
        checks.append({"name": f"explore-{rule}", "status": status,
                       "detail": {"without_premise": s["without_premise"] if s else 0,
                                  "with_premise": s["with_premise"] if s else {},
                                  "examples": [list(map(str, x)) for x in (s["examples"] if s else [])]},
                       "conditions": "all configurations; a pass means no counterexample outside premises P1-P3"})
        # same rule with all crashes excluded
    viol_nc = {k: [e for e in v if e["halt"] is None] for k, v in viol.items()}
    report.append("")
    report.append("Without any crash (no halt injected), rules violated outside premises: " +
                  (", ".join(sorted({r for (r, p), ex in viol_nc.items() if ex and not p})) or "none") + ".")
    for rule in RULES:
        bad = sum(len(ex) for (r, p), ex in viol_nc.items() if r == rule and not p)
        checks.append({"name": f"explore-nocrash-{rule}", "status": "pass" if bad == 0 else "fail", "detail": {"violations": bad},
                       "conditions": "no abnormal process end; no caller transaction"})
    report.append("")

    # D
    base_sigs = {(r, p) for (r, p), ex in viol.items() if ex}
    mres = {}
    for m in CODE_MUTANTS:
        mv, _, _ = explore(frozenset({m}))
        new = sorted({r for (r, p), ex in mv.items() if ex and (r, p) not in base_sigs} |
                     {r for (r, p), ex in mv.items() if ex and len(ex) > len(viol.get((r, p), []))})
        cb = [n for n, b in conformance(mut=frozenset({m})).items() if b]
        mres[m] = {"new_rule_violations": new, "conformance_failures": cb}
    rejected = all(r["new_rule_violations"] or r["conformance_failures"] for r in mres.values())
    checks.append({"name": "code-mutants-rejected", "status": "pass" if rejected else "fail", "detail": mres})
    report += ["## D. Broken-implementation controls", ""]
    for m, r in mres.items():
        verdict = "REJECTED" if (r["new_rule_violations"] or r["conformance_failures"]) else "ACCEPTED (control failed)"
        report.append(f"- `{m}` ({CODE_MUTANTS[m]}): {verdict}; new rule violations: {', '.join(r['new_rule_violations']) or 'none'}; "
                      f"IRIS scenarios contradicting it: {len(r['conformance_failures'])}")
    report.append("")

    # E
    import fold_check
    n, mm = fold_check.compare()
    checks.append({"name": "fold-abstraction-check", "status": "pass" if not mm else "fail",
                   "detail": {"configurations": n, "mismatches": mm[:20]},
                   "conditions": "Verus L2 (models/verus/load_history.rs, transcribed) vs fine L2; L1 view compared"})
    report += ["## E. Folded Verus L2 vs fine L2", "", f"Configurations: {n}; mismatches: {len(mm)}.", ""]

    # F
    import fold_check_general
    from l2_load import FIXES
    n2, mm2 = fold_check_general.compare()
    checks.append({"name": "fold-abstraction-check-general", "status": "pass" if not mm2 else "fail",
                   "detail": {"configurations": n2, "mismatches": mm2[:20]},
                   "conditions": "models/verus/load_general.rs (transcribed) vs fine L2: 0-3 dependencies, prior state, caller "
                                 "commit/rollback, every crash point, both crash resolutions, fixes on and off"})
    report += ["## F. Generalized Verus L2 vs fine L2", "", f"Configurations: {n2}; mismatches: {len(mm2)}.", ""]
    fv, fruns, _ = explore(frozenset(FIXES))
    fsum = summarize(fv)
    report += ["## G. The proposed fixes on the fine L2", "", f"Runs: {fruns}. Rules violated outside premises P1-P3 with both fixes:", ""]
    for rule in RULES:
        s = fsum.get(rule)
        bad = s["without_premise"] if s else 0
        checks.append({"name": f"explore-fixed-{rule}", "status": "pass" if bad == 0 else "fail", "detail": {"without_premise": bad},
                       "conditions": "fine L2 with fix-dev-persist and fix-dep-repersist; reconcile after a crash is the next command"})
        report.append(f"- {rule}: {bad}")
    report.append("")

    (ROOT / "evidence" / "model-checks.json").write_text(json.dumps({"tool": "models/explorer/explore.py", "records": checks}, indent=2, default=str) + "\n")
    (HERE / "REPORT.md").write_text("\n".join(report) + "\n")
    print("\n".join(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
