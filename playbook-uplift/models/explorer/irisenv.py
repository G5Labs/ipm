"""Executable IRIS environment contract (playbook 3.2) for the IPM load and history models.

What it models (each item validated by a probe in validation/iris/Uplift/Probe/Env.cls):
- journaled globals with nested TSTART levels kept as undo logs;
  TCOMMIT at level > 1 merges the level's undo log into its parent (nothing is durable until level 0);
  TROLLBACK 1 undoes one level; TROLLBACK undoes all levels;
- IRISTEMP (^IRIS.Temp.*) is not journaled, so TROLLBACK never undoes it;
- $INCREMENT is never undone (row ids are allocated with it);
- %OnClose runs when the last reference to an object is dropped (method return), at the current $TLEVEL,
  so its writes join any open transaction;
- HALT (or a process termination) with an open transaction first closes the process's objects
  (running %OnClose at the open level) and then ends the transaction: a job rolls back; an interactive
  terminal asks, and the operator may commit;
- saving an object again after the insert was rolled back re-creates the row under the same id;
- an object save that changes no value is a no-op (no UPDATE trigger fires); a changed save of a row whose
  Finalized flag is set is refused by the trigger.

MUTANTS (names) switch one rule off. Every mutant must be killed by the differential check against IRIS.
"""
from __future__ import annotations

import copy

MISSING = object()

MUTANTS = {
    "inner-commit-durable": "TCOMMIT at level > 1 makes the level's writes durable",
    "iristemp-transactional": "IRISTEMP writes are undone by TROLLBACK",
    "halt-rollback-before-onclose": "HALT rolls back before running %OnClose",
    "rollback1-undoes-all": "TROLLBACK 1 undoes every level",
    "onclose-deferred": "%OnClose runs only at process end, not at method return",
    "resave-fails": "re-saving an object whose insert was rolled back fails",
    "increment-rolled-back": "$INCREMENT is undone by TROLLBACK",
    "halt-commits": "a job that halts with an open transaction commits it",
    "same-value-save-fires-trigger": "an unchanged object save still fires the UPDATE trigger",
}


class Halt(Exception):
    """The process ended abnormally (HALT, terminal disconnect, or Terminate)."""


class SaveError(Exception):
    pass


class Env:
    def __init__(self, mutants=frozenset(), interactive_resolution="rollback"):
        self.db = {}          # journaled globals of the namespace database: key -> value
        self.undo = []        # one undo log (list of (key, old)) per open TSTART level
        self.temp = {}        # IRISTEMP
        self.ctr = {}         # $INCREMENT counters
        self.objs = []        # open objects with an on_close() method, innermost last
        self.deferred = []    # objects whose close was deferred (mutant onclose-deferred)
        self.mut = frozenset(mutants)
        self.resolution = interactive_resolution
        self.points = []      # labels of program points reached (for halt injection)
        self.halt_at = None   # (label, occurrence) at which to halt
        self.ended = False

    # --- transactions
    @property
    def tl(self) -> int:
        return len(self.undo)

    def tstart(self):
        self.undo.append([])

    def tcommit(self):
        assert self.undo, "TCOMMIT without TSTART"
        top = self.undo.pop()
        if self.undo and "inner-commit-durable" not in self.mut:
            self.undo[-1].extend(top)

    def _undo_level(self, log):
        for store, key, old in reversed(log):
            target = self.db if store == "db" else (self.temp if store == "temp" else self.ctr)
            if old is MISSING:
                target.pop(key, None)
            else:
                target[key] = old

    def trollback1(self):
        if "rollback1-undoes-all" in self.mut:
            return self.trollback_all()
        self._undo_level(self.undo.pop())

    def trollback_all(self):
        while self.undo:
            self._undo_level(self.undo.pop())

    # --- stores
    def set(self, key, value):
        if self.undo:
            self.undo[-1].append(("db", key, self.db.get(key, MISSING)))
        self.db[key] = copy.deepcopy(value)

    def kill(self, key):
        if key in self.db:
            if self.undo:
                self.undo[-1].append(("db", key, self.db[key]))
            del self.db[key]

    def tset(self, key, value):
        if self.undo and "iristemp-transactional" in self.mut:
            self.undo[-1].append(("temp", key, self.temp.get(key, MISSING)))
        self.temp[key] = copy.deepcopy(value)

    def tkill(self, key):
        if key in self.temp:
            if self.undo and "iristemp-transactional" in self.mut:
                self.undo[-1].append(("temp", key, self.temp[key]))
            del self.temp[key]

    def incr(self, name) -> int:
        if self.undo and "increment-rolled-back" in self.mut:
            self.undo[-1].append(("ctr", name, self.ctr.get(name, MISSING)))
        self.ctr[name] = self.ctr.get(name, 0) + 1
        return self.ctr[name]

    # --- object save with the History trigger rule (Finalized row refuses a changed save)
    def save_row(self, key, value, oref_believes_saved: bool):
        old = self.db.get(key, MISSING)
        if old is MISSING and oref_believes_saved and "resave-fails" in self.mut:
            raise SaveError("object row vanished")
        if old is not MISSING:
            if old == value and "same-value-save-fires-trigger" not in self.mut:
                return  # %IsModified() is false: nothing is written, no trigger
            if old.get("finalized") == 1:
                raise SaveError("Cannot edit finalized history log records")
        self.set(key, value)

    # --- object lifetime
    def open_obj(self, obj):
        self.objs.append(obj)

    def drop_obj(self, obj):
        """Last reference dropped at method return."""
        self.objs.remove(obj)
        if "onclose-deferred" in self.mut:
            self.deferred.append(obj)
        else:
            obj.on_close(self)

    # --- halt injection
    def point(self, label):
        self.points.append(label)
        n = sum(1 for p in self.points if p == label)
        if self.halt_at == (label, n):
            self.active_kind = getattr(self, "crash_kind", "process")   # only the injected crash has the requested kind
            raise Halt(label)

    def halt(self, close_order="inner-first"):
        if getattr(self, "active_kind", "process") == "instance":
            # instance crash (iris force, restart): no object is closed, IRISTEMP is wiped, startup rolls back
            # (observed: nc-gap-P1-*)
            self.objs, self.deferred = [], []
            self.temp = {}
            self.trollback_all()
            self.ended = True
            return
        objs = list(self.objs) + list(self.deferred)
        self.objs, self.deferred = [], []
        order = list(reversed(objs)) if close_order == "inner-first" else objs
        if "halt-rollback-before-onclose" in self.mut:
            self._end_txn()
            for o in order:
                o.on_close(self)
        else:
            for o in order:
                o.on_close(self)
            self._end_txn()
        self.ended = True

    def _end_txn(self):
        if not self.undo:
            return
        if self.resolution == "commit" or "halt-commits" in self.mut:
            self.undo = []
        else:
            self.trollback_all()

    def finish(self):
        """Normal process end (the scenario process itself halts with no transaction open)."""
        for o in reversed(self.deferred):
            o.on_close(self)
        self.deferred = []
