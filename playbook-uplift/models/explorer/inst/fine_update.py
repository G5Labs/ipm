"""Fine L2 of LoadNewModule's update-step handling, written from the ObjectScript (ipm 571e04a1), over the IRIS
environment contract models/explorer/irisenv.py (validated by its probes).

Structure follows the code, not the Verus model: the update package is a dict class-number -> (all-member list,
primary-only list) walked with nested loops (VersionBase.cls GetUpdatePackageClassList, Storage/Module.cls
HandleAllUpdateSteps / HandleUpdateStepsFromList / SeedUpdateStep / ExecuteUpdateStep); step records are rows in
journaled globals keyed (table, class, method); TSTART/TCOMMIT/TROLLBACK and halts are the environment's.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from irisenv import Env, Halt  # noqa: E402


# Code mutants (each switches one rule; every one must be contradicted by an IRIS observation in check.py).
MUTANTS = {
    "lock-timeout-errors": "a busy update lock fails the update instead of returning OK",
    "flag-carries-over": "newStepToApply is not reset at each class",
    "dev-install-seeds": "an install in developer mode seeds the steps too",
    "lexical-class-order": "classes are walked in string order (V10 before V2)",
    "records-not-transactional": "step records are written outside the transaction",
    "registry-dev-honoured": "update -dev from a registry runs in developer mode",
    "stored-dev-ignored": "with no -dev/-nodev typed, the run ignores the module's stored DeveloperMode",
    "guard-reads-stored-dev": "the install/load guard reads the run's developer mode, stored mode included",
}
MUT: set = set()


class IPMError(Exception):
    def __init__(self, msg, kind="Failed"):
        super().__init__(msg)
        self.kind = kind


def row_key(prim, cls, meth):
    return ("step", "PrimaryOnly" if prim else "AnyMember", cls, meth)


class Ctx:
    """Inputs of one command."""

    def __init__(self, kind, to, package, force=False, dev=None, txn=True, top=True, source="Registry", snapshot=False,
                 lock_free=True, phases_ok=True, fail_steps=(), mirror_member=False, mirror_primary=False):
        # package: classes of the update package in query row order, (name, anylist or None, primlist or None);
        # None when the manifest names no update package
        self.kind, self.to, self.package = kind, to, package
        self.force, self.dev, self.txn, self.top, self.source, self.snapshot = force, dev, txn, top, source, snapshot
        self.lock_free, self.phases_ok, self.fail_steps = lock_free, phases_ok, set(fail_steps)
        self.mirror_member, self.mirror_primary = mirror_member, mirror_primary


def developer_mode_param(c: Ctx):
    """params("DeveloperMode") as LoadNewModule sees it: None when not set. Main:Install always sets it (Main.cls#L2558,
    #L2623); LoadModuleReference (Utils/Module.cls#L128-L136) and LoadModuleFromDirectory (#L307-L310) override it for
    registry loads that are not a top-level install; load / update -path pass it only when -dev or -nodev is typed."""
    if c.source == "Path":
        return c.dev
    if "registry-dev-honoured" in MUT or (c.kind == "Install" and c.top):
        return c.dev is True
    return c.dev is True and c.snapshot


# ---- Storage/Module.cls
def get_add_update_step(env, prim, cls, meth):
    key = row_key(prim, cls, meth)
    row = env.db.get(key)
    if row is None:
        row = {"start": "", "end": "", "status": "OK"}
    save(env, key, row)  # %Save (HomeModule set)
    return key, dict(row)


def save(env, key, row):
    if "records-not-transactional" in MUT:
        env.db[key] = dict(row)
    else:
        env.set(key, row)


def seed_update_step(env, key, row):
    if (not row["end"]) or row["status"] != "OK":
        row["end"] = "ts"
        row["status"] = "OK"
        save(env, key, row)


def execute_update_step(env, c: Ctx, key, row, prim, cls, meth, runlog):
    if prim and c.mirror_member and not c.mirror_primary:
        return
    row["start"] = "ts"
    ok = (prim, cls, meth) not in c.fail_steps
    runlog.append((cls, prim, meth))
    env.incr(("runs", cls, prim, meth))           # effect outside the database ($INCREMENT, files)
    if ok:
        env.set(("eff", cls, prim, meth), env.db.get(("eff", cls, prim, meth), 0) + 1)  # journaled effect
    row["end"] = "ts"
    row["status"] = "OK" if ok else "ERR"
    save(env, key, row)
    if not ok:
        raise IPMError(f"step {cls}.{meth} failed")


def handle_update_steps_from_list(env, c, methods, cls, prim, seed, new_step, runlog, step_points):
    for meth in methods:
        step_points.append(None)
        env.point(f"step:{len(step_points) - 1}")
        key, row = get_add_update_step(env, prim, cls, meth)
        if seed:
            seed_update_step(env, key, row)
        else:
            if not new_step and ((not row["end"]) or row["status"] != "OK"):
                new_step = True
            if new_step:
                execute_update_step(env, c, key, row, prim, cls, meth, runlog)
    return new_step


def zstrip_digits(name):
    return "".join(ch for ch in name if ch.isdigit())


def collate(sub):
    """IRIS subscript order: canonical numbers ascending, then strings."""
    canonical = sub.isdigit() and (sub == "0" or not sub.startswith("0"))
    return (0, int(sub), "") if canonical else (1, 0, sub)


def get_update_package_class_list(c: Ctx):
    """VersionBase.cls#L33-L52: classList(classNum) = className, row by row."""
    out = {}
    for row in (c.package or []):
        num = zstrip_digits(row[0])
        if num == "":
            raise IPMError("<SUBSCRIPT>GetUpdatePackageClassList")
        out[num] = row
    return out


def handle_all_update_steps(env, c: Ctx, seed, runlog, step_points):
    if not seed:
        env.point("lock")
        if not c.lock_free:
            if "lock-timeout-errors" in MUT:
                raise IPMError("lock timeout")
            return "OK"  # lock timeout: return sc with sc = $$$OK (#L105-L108)
    class_list = get_update_package_class_list(c)
    order = sorted(class_list) if "lexical-class-order" in MUT else sorted(class_list, key=collate)
    new_step = False
    for num in order:  # $order(classList(keyVersion))
        name, anylist, primlist = class_list[num]
        if "flag-carries-over" not in MUT:
            new_step = False
        if anylist is not None:  # $system.CLS.IsMthd(className, "GetOrderedMethods")
            new_step = handle_update_steps_from_list(env, c, anylist, name, False, seed, new_step, runlog, step_points)
        if primlist is not None:
            new_step = handle_update_steps_from_list(env, c, primlist, name, True, seed, new_step, runlog, step_points)
    return "OK"


# ---- Utils/Module.cls LoadNewModule (#L1152-L1364), with Main:Update's not-installed check
def load_new_module(env: Env, c: Ctx, runlog):
    """Returns 'Ok', 'Refused', 'Failed' or 'Crashed'."""
    if c.kind == "Update" and c.top and env.db.get(("mod", "ver")) is None:
        return "Refused"
    init_tl = env.tl
    step_points = []
    status = None
    try:
        if c.txn:
            env.tstart()
        env.point("guard")
        frm = env.db.get(("mod", "ver"))
        param = developer_mode_param(c)
        # #L1298-L1307: the parameter if set, else the DeveloperMode of the record (kept from the installed module)
        stored = frm is not None and bool(env.db.get(("mod", "dev"), False))
        if param is not None:
            dev = param
        else:
            dev = False if "stored-dev-ignored" in MUT else stored
        guard_dev = dev if "guard-reads-stored-dev" in MUT else (param is True)  # $get(params("DeveloperMode"),0), #L1219
        if frm is not None:
            if frm > c.to:
                raise IPMError("Cannot downgrade", "Refused")
            if c.kind in ("Install", "Load") and not c.force and not guard_dev and c.to > frm and c.package is not None:
                raise IPMError("Cannot perform an update ... use the 'update' command", "Refused")
        env.point("manifest")
        env.set(("mod", "ver"), c.to)
        env.set(("mod", "dev"), dev)
        if dev and c.txn:
            env.tcommit()
            env.tstart()
        # dependencies: none in this model
        if dev and c.txn:
            env.tcommit()
        env.point("phases")
        if not c.phases_ok:
            raise IPMError("phase failed")
        if c.kind != "Update":
            # Lifecycle/Module.cls#L122 Activate seeding tests the module record's DeveloperMode
            if not env.db.get(("mod", "dev")) or "dev-install-seeds" in MUT:
                handle_all_update_steps(env, c, True, runlog, step_points)
        else:
            handle_all_update_steps(env, c, False, runlog, step_points)
        env.point("walk-end")
        if not dev and c.txn:
            env.tcommit()
        status = "Ok"
    except IPMError as e:
        status = e.kind
    except Halt:
        env.halt()
        return "Crashed"
    while env.tl > init_tl:
        env.trollback1()
    return status


def records(env):
    out = {}
    for k, v in env.db.items():
        if k[0] == "step":
            _, table, cls, meth = k
            out[(cls, table == "PrimaryOnly", meth)] = (v["start"] != "", v["end"] != "", v["status"] == "OK")
    return out
