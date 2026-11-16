"""L2 machine: IPM package loading and transaction history at 571e04a1, over the IRIS contract (irisenv).

Each function mirrors one ObjectScript routine; comments give the source lines. Program points
(env.point) are where a process may end abnormally; the explorer injects a halt at each one.

Code MUTANTS are broken implementations; each must be rejected by an L1 check or by conformance with IRIS.
"""
from __future__ import annotations

from dataclasses import dataclass, field

from irisenv import Env, Halt, SaveError

# Proposed fixes (DEC-HIST-1, DEC-HIST-2); enabled through the same option set as mutants.
FIXES = {
    "fix-dev-persist": "whenever SetCommitted runs with no transaction open (after the developer-mode TCOMMIT, or in a "
                       "no-transaction load), save the twin with Committed = 1",
    "fix-dep-repersist": "dependency logs keep their IRISTEMP record while a transaction is open; the parent re-persists "
                         "them after its rollback, and the next command reconciles leftovers after a crash",
}

CODE_MUTANTS = {
    "no-rollback-loop": "LoadNewModule does not roll back to the initial level on failure (Utils/Module.cls#L1358)",
    "dev-no-setcommitted": "the developer-mode commit does not call SetCommitted (Utils/Module.cls#L1312)",
    "finalize-overwrites-committed": "Finalize sets Committed = success instead of Committed || success (AbstractHistory.cls#L131)",
    "commit-before-activate": "a transactional load commits before Activate instead of after it",
    "temp-in-namespace-db": "HistoryTemp is stored in the namespace database instead of IRISTEMP (HistoryTemp.cls#L124)",
    "no-init-twin": "Init does not save the persistent twin early (HistoryTemp.cls#L21)",
    "onclose-keeps-temp": "%OnClose does not delete the IRISTEMP record (HistoryTemp.cls#L70)",
    "notxn-no-setcommitted": "a load without a transaction does not mark the log committed (Utils/Module.cls#L1226)",
}


class Throw(Exception):
    """An ObjectScript error that the code catches and turns into a %Status."""


@dataclass
class Mod:
    name: str
    version: str = "1.0.0"
    has_cls: bool = False
    deps: list = field(default_factory=list)   # list of Mod (loaded as dependencies from a registry)
    activate: str = "ok"                         # ok, fail, halt
    resolvable: bool = True                      # dependency graph can be built
    manifest_ok: bool = True


def row(action, package, version="0.0.0"):
    return {"action": action, "package": package, "version": version, "success": 0, "committed": 0,
            "ended": 0, "finalized": 0}


class HistoryLog:
    """%IPM.General.HistoryTemp with its %IPM.General.History twin (AbstractHistory.cls, HistoryTemp.cls)."""

    def __init__(self, env: Env, action, package, mut):
        self.mut = mut
        self.id = env.incr("history")               # both classes allocate ids with $INCREMENT
        self.twin_id = env.incr("twin")
        data = row(action, package)
        self._tput(env, data)                       # Init: log.%Save() (AbstractHistory.cls#L61)
        if "no-init-twin" not in mut:
            env.save_row(("row", self.twin_id), data, False)   # twin.%Save() (HistoryTemp.cls#L21)
            self.twin_saved = True
        else:
            self.twin_saved = False
        env.open_obj(self)

    def _tput(self, env, data):
        if "temp-in-namespace-db" in self.mut:
            env.set(("htemp", self.id), data)
        else:
            env.tset(("htemp", self.id), data)

    def _tget(self, env):
        store = env.db if "temp-in-namespace-db" in self.mut else env.temp
        return dict(store[("htemp", self.id)])

    def update(self, env, **kw):                     # %Reload, set, %Save
        d = self._tget(env)
        d.update(kw)
        self._tput(env, d)

    def set_committed(self, env):
        self.update(env, committed=1)

    def finalize(self, env, ok: bool):
        d = self._tget(env)
        if "finalize-overwrites-committed" in self.mut:
            committed = int(ok)
        else:
            committed = int(d["committed"] or ok)              # AbstractHistory.cls#L131
        self.update(env, success=int(ok), committed=committed, ended=1)

    def persist_to_twin(self, env):
        d = self._tget(env)
        d["finalized"] = 1
        try:
            env.save_row(("row", self.twin_id), d, self.twin_saved)
            self.twin_saved = True
        except SaveError:
            pass                                     # %OnClose ignores the status

    def persist_progress(self, env):                 # fix-dev-persist: save the twin without finalizing it
        d = self._tget(env)
        env.save_row(("row", self.twin_id), d, self.twin_saved)

    def on_close(self, env):                         # HistoryTemp.%OnClose (HistoryTemp.cls#L62)
        self.persist_to_twin(env)
        if "fix-dep-repersist" in self.mut and env.tl > 0 and getattr(self, "is_dep", False):
            env.tset(("pending", self.id), (self.twin_id, self._tget(env), self.dep_name, self.dep_version))
        if "onclose-keeps-temp" not in self.mut:
            if "temp-in-namespace-db" in self.mut:
                env.kill(("htemp", self.id))
            else:
                env.tkill(("htemp", self.id))


def load_new_module(env: Env, mod: Mod, mode: str, log: HistoryLog | None, mut, pre_newer=False, cmd="load"):
    """Utils/Module.cls LoadNewModule (#L1152-L1364). Returns True on success."""
    init_tl = env.tl
    use_txn = mode != "notxn"
    dev = mode == "dev"
    ok = False
    try:
        if use_txn:
            env.tstart()                                                     # #L1195
        env.point(f"guard:{mod.name}")
        if pre_newer:
            raise Throw("Cannot downgrade")                                 # #L1214
        if not use_txn and log and "notxn-no-setcommitted" not in mut:
            log.set_committed(env)                                           # #L1226
            if "fix-dev-persist" in mut and env.tl == 0:
                log.persist_progress(env)                                    # fix: SetCommitted at $TLEVEL 0 saves the twin
        env.point(f"manifest:{mod.name}")
        if not mod.manifest_ok:
            raise Throw("No module definition found")
        prev = env.db.get(("mod", mod.name), {})
        env.set(("mod", mod.name), {"version": mod.version, "dev": prev.get("dev", 0)})   # OBJ.Load #L1251
        if log:
            log.update(env, package=mod.name, version=mod.version)          # SetName/SetVersion #L1293
        if dev:
            env.set(("mod", mod.name), {"version": mod.version, "dev": 1})  # #L1303
        if dev and use_txn:
            env.tcommit()                                                    # #L1311
            if log and "dev-no-setcommitted" not in mut:
                log.set_committed(env)                                       # #L1313
            if log and "fix-dev-persist" in mut:
                log.persist_progress(env)
            env.tstart()                                                     # #L1318
        env.point(f"deps:{mod.name}")
        if not mod.resolvable:
            raise Throw("Could not find satisfactory version")             # BuildDependencyGraph
        for dep in mod.deps:                                                 # LoadDependencies #L1404
            env.point(f"before-dep:{dep.name}")
            if not load_module_reference(env, dep, "notxn" if mode == "notxn" else "txn", mut, cmd):
                raise Throw("dependency failed")
        if dev and use_txn:
            env.tcommit()                                                    # #L1338
        if "commit-before-activate" in mut and use_txn and not dev:
            env.tcommit()
            if log:
                log.set_committed(env)
        if mod.has_cls:
            env.set(("cls", mod.name), "compiled")                          # Compile phase
        env.point(f"activate:{mod.name}")
        if mod.activate == "halt":
            raise Halt(f"activate:{mod.name}")
        if mod.activate == "fail":
            raise Throw("Activate failed")
        if use_txn and not dev and "commit-before-activate" not in mut:
            env.tcommit()                                                    # #L1348
            env.point(f"after-commit:{mod.name}")
            if log:
                log.set_committed(env)                                       # #L1350
        ok = True
    except Throw:
        ok = False
    if "no-rollback-loop" not in mut:
        while env.tl > init_tl:
            env.trollback1()                                                 # #L1358-L1361
    if "fix-dep-repersist" in mut and log is not None and not getattr(log, "is_dep", False):
        reconcile_pending(env)                                               # the top-level load, after its rollback
    return ok


def reconcile_pending(env: Env):
    """fix-dep-repersist: write pending dependency rows at the current level and drop them from IRISTEMP."""
    for key in sorted(k for k in env.temp if k[0] == "pending"):
        twin_id, data, name, version = env.temp[key]
        installed = env.db.get(("mod", name), {}).get("version") == version
        env.set(("row", twin_id), dict(data, committed=int(installed), finalized=1))
        env.tkill(key)


def load_module_reference(env: Env, dep: Mod, mode: str, mut, cmd="load") -> bool:
    """Utils/Module.cls LoadModuleReference (#L40-L211) for a dependency (no log passed in)."""
    log = HistoryLog(env, "install" if cmd == "install" else "load", dep.name, mut)   # #L64-L68
    log.is_dep, log.dep_name, log.dep_version = True, dep.name, dep.version
    log.update(env, version=dep.version)                                              # SetSource/SetVersion #L69
    ok = load_new_module(env, dep, mode, log, mut)
    log.finalize(env, ok)                                                             # #L208
    env.drop_obj(log)                                                                 # method return -> %OnClose
    return ok


def cmd_load(env: Env, mod: Mod, mode: str, mut, pre_newer=False, path_ok=True, action="load", known_name=None):
    """Main.cls Load (#L2415) / Install (#L2554) / Update (#L4438) entry: one history log per command."""
    log = HistoryLog(env, action, known_name or "", mut)            # LoadInit("") or InstallInit(name)
    if action in ("install", "update") and known_name:
        log.update(env, version=mod.version)
    env.point("init")
    ok = False
    try:
        if not path_ok:
            raise Throw("The specified path must be absolute.")    # LoadInternal #L2449
        ok = load_new_module(env, mod, mode, log, mut, pre_newer=pre_newer, cmd=action)
    except Throw:
        ok = False
    log.finalize(env, ok)                                            # #L2427 / #L2430
    env.drop_obj(log)
    return ok


def run_command(env: Env, mod, mode, mut=frozenset(), caller=None, **kw):
    """Run one command in a process; a Halt ends the process (env.halt)."""
    status = None
    try:
        if caller:
            env.tstart()
        status = cmd_load(env, mod, mode, mut, **kw)
        if caller == "commit" and env.tl:   # <COMMAND> error in IRIS when nothing is open; only a mutant gets here
            env.tcommit()
        elif caller == "rollback":
            env.trollback_all()
    except Halt:
        env.halt()
        status = "halted"
    env.finish()
    return status
