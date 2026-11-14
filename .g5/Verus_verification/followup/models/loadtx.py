#!/usr/bin/env python3
"""Composable model of `load` with per-activation transaction modes, process ends and retry.

Run with python3 -I -B:  python3 -I -B loadtx.py OUT_DIR   (writes OUT_DIR/loadtx.json)

Scope. One top module and up to two dependencies, each a separate LoadNewModule activation. This
extends the original model (crates/ipm-load-history), whose transaction mode is one global value,
with what the source actually does (all lines at 571e04a1):

  mode       LoadNewModule merges the module's own <Defaults> under the params it was called with
             (Utils/Module.cls:1165-1169). A dependency is called with its parent's merged params
             (LoadDependencies at :1328, LoadModuleReference -> LoadModuleFromDirectory :306-311),
             so caller flags override the parent's defaults, which override the dependency's own.
             NoTransaction and NoJournal are separate keys, merged one by one.
  begin      NoJournal: process journaling off until the activation returns (:1191-1193,
             JournalManager). Otherwise, with transactions: tstart (:1194-1196).
  marker     without transactions the activation's record is marked Committed at once (:1224-1227).
  dev        a dependency's DeveloperMode is overridden: on only for a snapshot under a -dev parent
             (:130-139, :307-311). Developer mode with transactions commits after the developer-mode
             save (C1, :1309-1319) and after dependencies and reload (C2, :1335-1339); its phases
             then run outside a transaction.
  commit     without developer mode, a successful activation commits after Activate (C3, :1347-1351).
  exit       on failure, rollback to the activation's entry level (:1357-1362). Journaling is
             restored when the JournalManager goes out of scope, after that rollback.
  history    top record: Main.Load (Main.cls:2417-2431); dependency record: LoadModuleReference
             (Utils/Module.cls:58-67, closed when that call returns). Init inserts the durable row
             at the current level and journaling (AbstractHistory.cls:51-62, HistoryTemp.cls).
             SetCommitted and Finalize write IRISTEMP (never undone). Finalize sets
             Committed = Committed || OK (AbstractHistory.cls:127-133). %OnClose copies IRISTEMP into
             the row and sets Finalized (HistoryTemp.cls:30-70), if the row still exists.
  dependencies  LoadDependencies of the top module loads the transitive closure in dependency order
             (:1368-1405). A dependency's own LoadDependencies then finds its dependencies installed
             (in the same process, uncommitted) and skips them (Storage/Module.cls:1139-1154). So a
             depth-2 chain m -> d1 -> d2 runs as the flat sequence d2, d1 under m, and d2 receives m's
             params, not d1's. A shared dependency is loaded once. This is the reduction that lets two
             flat dependency slots stand for nested and shared graphs (checked natively, N-R05).

IRIS semantics (premises P-01..P-06, R-01, and N-P21 for the before-image question):
  journaled write inside a transaction: undo entry with the node's before-image; tcommit at level
  > 1 merges into the enclosing level; trollback 1 restores before-images of that level in reverse;
  unjournaled writes leave no undo entry. A process end runs %OnClose (Terminate) or not (SIGKILL),
  then rolls back every open level. Before the rollback, other processes see uncommitted data.

Contract (L1) used for the checks, independent of the journal mechanics:
  REC-EXISTS     every started action has a record at a returned or settled end (#366, Q-06b).
  COMMITTED      a finalized record of an action with no caller transaction reads Committed equal to
                 "its changes were kept" (dependency: its own module; top: any module).
  PREFIX         after a process end, each module is in the state given by the writes whose
                 durability point came before the end (below). At a returned end, the same with the
                 whole run. Durability points: a write outside any transaction or with journaling
                 off is durable at once; a transactional write is durable at the commit that returns
                 the load to the caller's level; with a caller transaction, at the caller's commit.
  Q-22  A: a dependency's own NoJournal default makes its writes durable at once (as built).
        B: a dependency's defaults cannot weaken its parent's transaction.
  Q-23  A: a dependency's own NoTransaction default suppresses the parent's transaction.
        B: it is ineffective inside the parent's transaction (as built: the dependency only skips
           its own tstart).
  TLEVEL         a returned load leaves $TLEVEL at its entry level.
  RETRY          after a process end has settled, rerunning the same load without the fault installs
                 every module and records a successful action.
"""
import itertools, json, os, sys
from collections import Counter

MODES = (None, "NoTransaction", "NoJournal")


# ---------------------------------------------------------------- IRIS store with a journal


class Store:
    def __init__(self, init):
        self.v = dict(init)          # process view
        self.undo = []               # open levels: list of [(node, before)]
        self.jrn = True
        self.temp = {}               # IRISTEMP: never undone
        self.before_image = True     # compare-and-restore (P-14); mutant M-BEFORE-IMAGE sets False

    def tl(self):
        return len(self.undo)

    def write(self, node, val):
        if self.undo and self.jrn:
            self.undo[-1].append((node, self.v.get(node), val))
        if val is None:
            self.v.pop(node, None)
        else:
            self.v[node] = val

    def tstart(self):
        self.undo.append([])

    def tcommit(self):
        lvl = self.undo.pop()
        if self.undo:
            self.undo[-1].extend(lvl)

    def trollback1(self):
        """P-14 (native N-P20c, N-P20c2, N-P21A, N-P21R): a journaled write is undone only if the node
        still holds the value it wrote; a later unjournaled write to the node survives the rollback.
        The mutant M-BEFORE-IMAGE restores every before-image unconditionally."""
        lvl = self.undo.pop()
        for node, old, new in reversed(lvl):
            if self.before_image and self.v.get(node) != new:
                continue
            if old is None:
                self.v.pop(node, None)
            else:
                self.v[node] = old

    def rollback_all(self):
        while self.undo:
            self.trollback1()

    def snapshot(self):
        return {"v": dict(self.v), "undo": [list(l) for l in self.undo], "jrn": self.jrn, "temp": {k: dict(v) for k, v in self.temp.items()}}


# ---------------------------------------------------------------- configuration


def effective(caller, top_def, own_def, is_top, variant):
    """Per-key merge of NoTransaction / NoJournal (Utils/Module.cls:1165-1169)."""
    def key(k):
        if caller == k:
            return 1
        if top_def == k:
            return 1
        if not is_top and own_def == k and variant not in ("M-GLOBAL-MODE", "REPAIR-103-GUARDED-GLOBAL"):
            return 1
        return 0
    return {"NoTransaction": key("NoTransaction"), "NoJournal": key("NoJournal")}


class Cfg:
    def __init__(self, caller, top_def, dep_defs, dev, fault, caller_tx, variant, installed=()):
        self.caller, self.top_def, self.dep_defs, self.dev = caller, top_def, dep_defs, dev
        self.fault, self.caller_tx, self.variant = fault, caller_tx, variant
        self.n = len(dep_defs)

    def name(self):
        return (f"caller={self.caller} top_def={self.top_def} dep_defs={list(self.dep_defs)} dev={self.dev} "
                f"fault={self.fault} caller_tx={self.caller_tx} variant={self.variant}")


# ---------------------------------------------------------------- the program as a trace of atomic steps


def program(cfg):
    """Yields (label, fn(st, ctx)) steps. ctx carries per-activation bookkeeping; a step that throws
    sets ctx['err'] and the generator jumps to the unwinding steps, as the source does."""
    acts = ["top"] + [f"dep{i + 1}" for i in range(cfg.n)]

    def mode(a):
        return effective(cfg.caller, cfg.top_def, None if a == "top" else cfg.dep_defs[int(a[3:]) - 1], a == "top", cfg.variant)

    def uses_tx(a):
        m = mode(a)
        return not (m["NoTransaction"] or m["NoJournal"])

    def dev(a):
        return cfg.dev if a == "top" else False   # dependencies are not snapshots in this domain

    steps = []
    S = steps.append
    rep = cfg.variant in ("REPAIR-103", "REPAIR-103-GUARDED", "REPAIR-103-GUARDED-GLOBAL", "REPAIR-103-M-LATE-MARK")
    naive = cfg.variant == "REPAIR-NAIVE"   # only the row's survival is fixed; Committed as built

    # ---- history helpers
    def log_init(a):
        def f(st, ctx):
            ctx["started"].add(a)
            st.temp[a] = {"com": 0, "ok": 0, "tend": 0}
            if (rep or naive) and a != "top":
                j = st.jrn
                st.jrn = False                                  # repair 103 / naive fix: dependency row unjournaled
                st.write(("hist", a), {"com": 0, "ok": 0, "tend": 0, "fin": 0})
                st.jrn = j
            else:
                st.write(("hist", a), {"com": 0, "ok": 0, "tend": 0, "fin": 0})
        return f

    def set_committed(a):
        def f(st, ctx):
            if rep and a != "top":
                return                                          # repair 103: dependency marked when durable
            st.temp[a]["com"] = 1
        return f

    def close(a):
        """Finalize (if the action ended) is done by the caller of LoadNewModule; %OnClose persists."""
        def f(st, ctx):
            t = st.temp.get(a)
            if t is None or ("hist", a) not in st.v:
                st.temp.pop(a, None)
                return
            row = {"com": t["com"], "ok": t["ok"], "tend": t["tend"], "fin": 1}
            if rep and a != "top":
                # repair 103: dependency rows are finalized at the top-level close; here only the
                # outcome is copied, unjournaled. Committed comes from the durability mark.
                j = st.jrn
                st.jrn = False
                cur = st.v[("hist", a)]
                st.write(("hist", a), {**cur, "ok": t["ok"], "tend": t["tend"]})
                st.jrn = j
                if st.tl() == ctx["entry"] and ctx["kept_now"](st, a):
                    j = st.jrn
                    st.jrn = False
                    st.write(("hist", a), {**st.v[("hist", a)], "com": 1})
                    st.jrn = j
            elif naive and a != "top":
                j = st.jrn
                st.jrn = False
                st.write(("hist", a), row)
                st.jrn = j
            else:
                st.write(("hist", a), row)
            st.temp.pop(a, None)
        return f

    def finalize(a):
        def f(st, ctx):
            t = st.temp[a]
            ok = 0 if a in ctx["failed"] else 1
            t["ok"] = 1 if ok else 2
            if not (rep and a != "top"):
                t["com"] = t["com"] or ok
            t["tend"] = 1
        return f

    def dep_commit_mark(st, ctx):
        """Repair 103: before a commit that returns the load to its entry level, a journaled
        Committed=1 on the dependency rows whose writes that commit makes durable. It must be the
        row's last write before the commit."""
        if not rep or st.tl() != ctx["entry"] + 1:
            return
        for a in acts[1:]:
            if ("hist", a) in st.v and a in ctx["completed"] and st.v[("hist", a)]["com"] == 0:
                st.write(("hist", a), {**st.v[("hist", a)], "com": 1})

    def late_write(st, ctx):
        """Mutant REPAIR-103-M-LATE-MARK: an unjournaled write to the dependency rows between the mark
        and the commit (for example finalizing them there)."""
        if cfg.variant != "REPAIR-103-M-LATE-MARK" or st.tl() != ctx["entry"] + 1:
            return
        j = st.jrn
        st.jrn = False
        for a in acts[1:]:
            if ("hist", a) in st.v:
                st.write(("hist", a), {**st.v[("hist", a)], "fin": 1})
        st.jrn = j

    # ---- module writes
    def wmod(a, field):
        def f(st, ctx):
            cur = st.v.get(("mod", a), {"present": 0, "res": 0, "dev": 0, "wr": 0})
            new = dict(cur)
            if field == "manifest":
                new["present"], new["dev"] = 1, cur["dev"] if cur["present"] else 0
            elif field == "dev":
                new["dev"] = 1 if dev(a) else 0
            elif field == "res":
                new["res"] = 1
            new["wr"] = cur["wr"] + 1
            st.write(("mod", a), new)
        return f

    # ---- one activation
    def activation(a):
        out = []
        A = out.append
        A((f"{a}:enter", lambda st, ctx, a=a: ctx["itl"].__setitem__(a, st.tl())))
        m = mode(a)
        if m["NoJournal"]:
            def jm_off(st, ctx, a=a):
                ctx["jprev"][a] = st.jrn
                ctx["jm_open"].add(a)
                st.jrn = False
            A((f"{a}:journal-off", jm_off))
        elif uses_tx(a):
            A((f"{a}:tstart", lambda st, ctx: st.tstart()))
        if not uses_tx(a):
            A((f"{a}:notx-marker", set_committed(a)))
        A((f"{a}:write-manifest", wmod(a, "manifest")))
        A((f"{a}:write-dev", wmod(a, "dev")))
        if dev(a) and uses_tx(a):
            def c1(st, ctx, a=a):
                st.tcommit()
            A((f"{a}:C1-mark", dep_commit_mark))
            A((f"{a}:C1-late", late_write))
            A((f"{a}:C1-commit", c1))
            A((f"{a}:C1-marker", set_committed(a)))
            A((f"{a}:C1-tstart", lambda st, ctx: st.tstart()))
        if a == "top":
            for d in acts[1:]:
                A((f"{d}:log-init", log_init(d)))
                out.extend(activation(d))
                A((f"{d}:finalize", finalize(d)))
                A((f"{d}:close", close(d)))
                A((f"{d}:check", "THROW-IF-FAILED:" + d))
        A((f"{a}:write-reload", wmod(a, "touch")))
        if dev(a) and uses_tx(a):
            def c2(st, ctx, a=a):
                st.tcommit()
            A((f"{a}:C2-mark", dep_commit_mark))
            A((f"{a}:C2-late", late_write))
            A((f"{a}:C2-commit", c2))
        A((f"{a}:write-res", wmod(a, "res")))
        A((f"{a}:activate", "FAULT:" + a))
        if not dev(a) and uses_tx(a):
            def c3(st, ctx, a=a):
                st.tcommit()
            A((f"{a}:C3-mark", dep_commit_mark))
            A((f"{a}:C3-late", late_write))
            A((f"{a}:C3-commit", c3))
            A((f"{a}:C3-marker", set_committed(a)))
        A((f"{a}:done", lambda st, ctx, a=a: ctx["completed"].add(a)))
        A((f"{a}:exit", "EXIT:" + a))
        return out

    if cfg.caller_tx:
        S(("caller:tstart", lambda st, ctx: st.tstart()))
    S(("top:log-init", log_init("top")))
    steps.extend(activation("top"))
    S(("top:finalize", finalize("top")))
    S(("top:close", close("top")))
    if rep:
        S(("top:finalize-dependency-rows", lambda st, ctx: finalize_dep_rows(st, acts)))
    return steps, acts, mode, uses_tx


def finalize_dep_rows(st, acts):
    """Repair 103: the top-level close finalizes the dependency rows, unjournaled."""
    j = st.jrn
    st.jrn = False
    for a in acts[1:]:
        if ("hist", a) in st.v:
            st.write(("hist", a), {**st.v[("hist", a)], "fin": 1})
    st.jrn = j


def run(cfg, init, cut=None, end="return"):
    """Execute the trace. cut: index of the step before which the process ends (None: no end).
    Returns the observations at the end points."""
    steps, acts, mode, uses_tx = program(cfg)
    st = Store(init)
    if cfg.variant == "M-BEFORE-IMAGE":
        st.before_image = False
    ctx = {"acts": acts, "started": set(), "failed": set(), "completed": set(), "itl": {}, "jprev": {}, "jm_open": set(), "entry": 1 if cfg.caller_tx else 0,
           "kept_now": lambda st, a: True}
    i, unwinding = 0, None
    obs = {}
    trace = []
    while i < len(steps):
        if cut is not None and i == cut:
            obs["transient"] = st.snapshot()
            if end == "terminate":
                # %OnClose of live objects, innermost frame first (P-06c, native N-P19). Frames, outermost
                # first: Main.Load (top log) > top LoadNewModule (top JournalManager) > LoadModuleReference
                # (dependency log) > dependency LoadNewModule (dependency JournalManager). A JournalManager's
                # %OnClose restores the journaling it found (Utils/JournalManager.cls:14, :22-25).
                for a in [x for x in reversed(acts) if x != "top"] + ["top"]:
                    if a in ctx["jm_open"] and cfg.variant != "M-JOURNAL-NOT-RESTORED":
                        st.jrn = ctx["jprev"][a]
                        ctx["jm_open"].discard(a)
                    if a in st.temp:
                        close(a, st, ctx, cfg.variant)
            st.rollback_all()                                    # P-05
            obs["settled"] = st.snapshot()
            obs["end"] = end
            obs["cut_label"] = steps[i][0]
            obs["trace"] = trace
            obs["ctx"] = {k: sorted(v) if isinstance(v, set) else v for k, v in ctx.items() if k not in ("kept_now", "acts")}
            return obs
        label, fn = steps[i]
        if unwinding is not None and fn != "EXIT:" + unwinding:
            # an exception skips forward to the failing activation's exit, then propagates
            i += 1
            continue
        trace.append(label)
        if isinstance(fn, str):
            if fn.startswith("FAULT:"):
                a = fn[6:]
                if cfg.fault == a:
                    ctx["failed"].add(a)
                    unwinding = a
            elif fn.startswith("THROW-IF-FAILED:"):
                d = fn[16:]
                if d in ctx["failed"]:
                    ctx["failed"].add("top")
                    unwinding = "top"
            elif fn.startswith("EXIT:"):
                a = fn[5:]
                while st.tl() > ctx["itl"][a]:
                    st.trollback1()
                if mode(a)["NoJournal"] and a in ctx["jprev"] and cfg.variant != "M-JOURNAL-NOT-RESTORED":
                    st.jrn = ctx["jprev"][a]
                ctx["jm_open"].discard(a)
                if unwinding == a:
                    unwinding = None
        else:
            fn(st, ctx)
        i += 1
    obs["returned"] = st.snapshot()
    obs["tlevel_ok"] = st.tl() == (1 if cfg.caller_tx else 0)
    if cfg.caller_tx:
        st2 = Store({})
        st2.__dict__.update({k: (dict(v) if isinstance(v, dict) else ([list(l) for l in v] if k == "undo" else v)) for k, v in st.__dict__.items()})
        st.tcommit()
        obs["caller_commit"] = st.snapshot()
        st2.rollback_all()
        obs["caller_rollback"] = st2.snapshot()
    obs["end"] = "return"
    obs["trace"] = trace
    obs["ctx"] = {k: sorted(v) if isinstance(v, set) else v for k, v in ctx.items() if k not in ("kept_now", "acts")}
    return obs


def close(a, st, ctx, variant="as-built"):
    """%OnClose at a Terminate: persist the IRISTEMP state into the row (as built, journaled now).
    Repair 103 writes a dependency row unjournaled and marks it Committed only when its module
    writes are durable, that is when no transaction of the load is open."""
    t = st.temp.get(a)
    if t is None or ("hist", a) not in st.v:
        return
    if variant.startswith("REPAIR-103") and a == "top":
        st.write(("hist", a), {"com": t["com"], "ok": t["ok"], "tend": t["tend"], "fin": 1})
        # REPAIR-103-GUARDED: the close finalizes dependency rows only when no transaction of the load
        # is open, so no unjournaled write can follow the journaled Committed mark before its commit.
        if variant not in ("REPAIR-103-GUARDED", "REPAIR-103-GUARDED-GLOBAL") or st.tl() == ctx["entry"]:
            finalize_dep_rows(st, list(ctx.get("acts", [])))
        return
    if variant.startswith("REPAIR-103") and a != "top":
        j = st.jrn
        st.jrn = False
        com = 1 if (st.tl() == ctx["entry"] and ("mod", a) in st.v) else st.v[("hist", a)]["com"]
        st.write(("hist", a), {**st.v[("hist", a)], "ok": t["ok"], "tend": t["tend"], "com": com})
        st.jrn = j
    else:
        st.write(("hist", a), {"com": t["com"], "ok": t["ok"], "tend": t["tend"], "fin": 1})


# ---------------------------------------------------------------- L1 (contract) evaluation


def l1_durable_writes(cfg, q22, q23, cut_trace, completed):
    """Module state promised durable by the contract after the steps in cut_trace (labels).
    Rebuilt from the labels of the as-built trace, with durability points from the contract."""
    acts = ["top"] + [f"dep{i + 1}" for i in range(cfg.n)]

    def own(a):
        return None if a == "top" else cfg.dep_defs[int(a[3:]) - 1]

    def contract_mode(a):
        """The mode the contract gives each activation under the Q options."""
        m = effective(cfg.caller, cfg.top_def, own(a), a == "top", "as-built")
        if a != "top":
            parent_tx = not (effective(cfg.caller, cfg.top_def, None, True, "as-built")["NoTransaction"]
                             or effective(cfg.caller, cfg.top_def, None, True, "as-built")["NoJournal"])
            if parent_tx and own(a) == "NoJournal" and m["NoJournal"] and cfg.caller != "NoJournal" and cfg.top_def != "NoJournal":
                if q22 == "B":
                    m = {"NoTransaction": 0, "NoJournal": 0}
            if parent_tx and own(a) == "NoTransaction" and m["NoTransaction"] and cfg.caller != "NoTransaction" and cfg.top_def != "NoTransaction":
                if q23 == "B":
                    m = {"NoTransaction": 0, "NoJournal": 0}
        return m

    whole_notx = q23 == "A" and any(own(a) == "NoTransaction" for a in acts[1:])

    def immediate(a):
        m = contract_mode(a)
        return whole_notx or bool(m["NoTransaction"] or m["NoJournal"])

    # durable: survives everything; caller: kept until the caller decides; pending: inside IPM's own
    # transaction, made durable (or handed to the caller's level) only by the top's commit.
    durable, caller, pending = {}, [], []
    seen = []
    for label in cut_trace:
        a, _, what = label.partition(":")
        if what.startswith("write-"):
            ev = (a, what[6:])
            dev_phase = a == "top" and cfg.dev and "top:C2-commit" in seen and not immediate("top")
            m = contract_mode(a)
            if immediate(a) or dev_phase:
                if not cfg.caller_tx or (m["NoJournal"] and not whole_notx):
                    apply(durable, [ev])
                else:
                    caller.append(ev)
            else:
                pending.append(ev)
        elif what in ("C1-commit", "C2-commit", "C3-commit") and a == "top" and not whole_notx:
            if cfg.caller_tx:
                caller.extend(pending)
            else:
                apply(durable, pending)
            pending = []
        seen.append(label)
    return durable, caller, pending


def apply(state, evs):
    for a, field in evs:
        cur = state.get(a, {"present": 0, "res": 0, "dev": 0, "wr": 0})
        new = dict(cur)
        if field == "manifest":
            new["present"] = 1
        elif field == "res":
            new["res"] = 1
        new["wr"] = cur["wr"] + 1
        state[a] = new


def mods_of(snap, acts):
    return {a: {k: snap["v"][("mod", a)][k] for k in ("present", "res")} if ("mod", a) in snap["v"] else {"present": 0, "res": 0} for a in acts}


def check(cfg, q22, q23, obs, acts, caller_end="commit"):
    """Returns the list of violated L1 clauses for one end state. With a caller transaction the
    returned end is evaluated after the caller commits (`caller_end="commit"`) or rolls back."""
    v = []
    end = obs["end"]
    snap = obs.get("settled") or obs.get("returned")
    started = obs["ctx"]["started"]
    durable, caller, pending = l1_durable_writes(cfg, q22, q23, obs["trace"], obs["ctx"]["completed"])
    promised = durable
    if end == "return":
        if not obs["tlevel_ok"]:
            v.append("TLEVEL")
        if cfg.caller_tx:
            snap = obs["caller_commit"] if caller_end == "commit" else obs["caller_rollback"]
            if caller_end == "commit":
                promised = {k: dict(x) for k, x in durable.items()}
                apply(promised, caller)
    for a in started:
        if ("hist", a) not in snap["v"]:
            v.append(f"REC-EXISTS[{a}]")
    got = mods_of(snap, acts)
    want = {a: {"present": promised.get(a, {}).get("present", 0), "res": promised.get(a, {}).get("res", 0)} for a in acts}
    for a in acts:
        if got[a] != want[a]:
            v.append(f"PREFIX[{a}]")
    # Committed, without a caller transaction. COMMITTED: finalized, ended records. COMMITTED-UNENDED:
    # every other existing record at a process end, under Q-08's provisional answer ("Committed reflects
    # the changes kept, even on an unfinished record"; contract.rs kill_rec with q08_exact).
    if not cfg.caller_tx:
        for a in acts:
            row = snap["v"].get(("hist", a))
            if not row:
                continue
            kept = any(got[b] != {"present": 0, "res": 0} for b in acts) if a == "top" else got[a] != {"present": 0, "res": 0}
            if row.get("fin") and row.get("tend"):
                if bool(row["com"]) != kept:
                    v.append(f"COMMITTED[{a}]")
            elif end != "return" and bool(row["com"]) != kept:
                v.append(f"COMMITTED-UNENDED[{a}]")
    return sorted(set(v))


# ---------------------------------------------------------------- domain and exploration


def domain(variants):
    for variant in variants:
        for caller in MODES:
            for top_def in MODES:
                for n in (0, 1, 2):
                    for dep_defs in itertools.product(MODES, repeat=n):
                        for dev in (False, True):
                            for fault in [None, "top"] + [f"dep{i + 1}" for i in range(n)]:
                                for caller_tx in (False, True):
                                    yield Cfg(caller, top_def, dep_defs, dev, fault, caller_tx, variant)


def explore(variants, qopts):
    res = {"configs": 0, "end_states": 0, "violations": {}, "examples": {}, "retry_failures": 0, "transient_differs": 0}
    for cfg in domain(variants):
        res["configs"] += 1
        steps, acts, _, _ = program(cfg)
        ends = [("return", None)] + [(e, k) for k in range(1, len(steps)) for e in ("terminate", "sigkill")]
        for end, cut in ends:
            if end != "return" and cfg.caller_tx:
                continue    # a process end with a caller transaction undoes everything: covered by the original model
            obs = run(cfg, {}, cut, end)
            res["end_states"] += 1
            if end != "return":
                if mods_of(obs["transient"], acts) != mods_of(obs["settled"], acts):
                    res["transient_differs"] += 1
                # retry from the settled state without the fault
                retry_cfg = Cfg(cfg.caller, cfg.top_def, cfg.dep_defs, cfg.dev, None, False, cfg.variant)
                init = {k: v for k, v in obs["settled"]["v"].items() if k[0] == "mod"}
                r = run(retry_cfg, init)
                if any(r["returned"]["v"].get(("mod", a), {}).get("present") != 1 or r["returned"]["v"].get(("mod", a), {}).get("res") != 1 for a in acts):
                    res["retry_failures"] += 1
            for q22, q23 in qopts:
                clauses = check(cfg, q22, q23, obs, acts)
                if end == "return" and cfg.caller_tx:
                    clauses = sorted(set(clauses) | {c + "@caller-rollback" for c in check(cfg, q22, q23, obs, acts, "rollback")})
                for clause in clauses:
                    key = f"{cfg.variant} Q22={q22} Q23={q23} {clause.split('[')[0]}"
                    res["violations"][key] = res["violations"].get(key, 0) + 1
                    ex = res["examples"].setdefault(key, [])
                    if len(ex) < 2:
                        ex.append({"config": cfg.name(), "end": end, "cut": obs.get("cut_label"), "clause": clause})
    return res


# ---------------------------------------------------------------- correspondence with the original model and native evidence


def corresponding_cases():
    """Cases shared with cases.json (global modes) and the archived native M5 case, with what the
    original model or the product showed. Each must hold in this model's as-built variant."""
    out = []

    def returned(cfg):
        o = run(cfg, {})
        return o["returned"]["v"]

    v = returned(Cfg(None, None, (None,), False, "top", False, "as-built"))
    out.append(("B1-C31 parent Activate fails after the dependency loaded (default): dependency row lost, dependency rolled back",
                ("hist", "dep1") not in v and ("mod", "dep1") not in v))
    v = returned(Cfg("NoTransaction", None, (None,), False, "dep1", False, "as-built"))
    out.append(("B1-C34 dependency Activate fails (NoTransaction): dependency row kept, Committed=1",
                v.get(("hist", "dep1"), {}).get("com") == 1 and v.get(("mod", "dep1"), {}).get("present") == 1))
    v = returned(Cfg(None, None, (None,), True, "top", False, "as-built"))
    out.append(("B1-C38 -dev parent fails after dependencies committed: dependency row kept, Committed=1",
                v.get(("hist", "dep1"), {}).get("com") == 1 and v.get(("mod", "dep1"), {}).get("present") == 1))
    v = returned(Cfg(None, None, ("NoJournal",), False, "top", False, "as-built"))
    out.append(("native M5 (20261113T204405Z-stagec2): dependency <Defaults> NoJournal, parent fails: dependency stays installed with no row",
                v.get(("mod", "dep1"), {}).get("present") == 1 and v.get(("mod", "dep1"), {}).get("res") == 1 and ("hist", "dep1") not in v))
    v = returned(Cfg(None, None, ("NoJournal",), False, "top", False, "M-GLOBAL-MODE"))
    out.append(("control: the global-mode model (dependency defaults ignored) cannot produce M5",
                not (v.get(("mod", "dep1"), {}).get("present") == 1)))
    v = returned(Cfg(None, None, ("NoTransaction",), False, "top", False, "as-built"))
    out.append(("prediction N-R05-Q23F: dependency <Defaults> NoTransaction, parent fails: dependency rolled back with the parent, row lost",
                ("mod", "dep1") not in v and ("hist", "dep1") not in v))
    v = returned(Cfg(None, None, ("NoTransaction",), False, None, False, "as-built"))
    out.append(("prediction N-R05-Q23S: same, parent succeeds: dependency installed, row Committed=1",
                v.get(("mod", "dep1"), {}).get("present") == 1 and v.get(("hist", "dep1"), {}).get("com") == 1))
    # P-06c (native N-P19, results/native/20261113T204146Z-premise-p19): NoJournal load with a dependency
    # under a caller transaction, Terminate inside the dependency's Activate. The dependency's row
    # survives finalized; the top row is rolled back.
    cfg = Cfg("NoJournal", None, (None,), False, None, True, "as-built")
    steps, acts, _, _ = program(cfg)
    k = [i for i, (l, _) in enumerate(steps) if l == "dep1:activate"][0]
    v = run(cfg, {}, k, "terminate")["settled"]["v"]
    out.append(("N-P19: NoJournal load under a caller transaction, Terminate in the dependency's Activate: dependency row survives finalized, top row rolled back",
                v.get(("hist", "dep1"), {}).get("fin") == 1 and ("hist", "top") not in v))
    # P-14: the store's rollback against the four native probe sequences
    def seq(steps, end):
        st = Store({})
        st.tstart()
        for j, val in steps:
            st.jrn = j
            st.write("n", val)
        st.jrn = True
        (st.trollback1 if end == "rollback" else st.tcommit)()
        return st.v.get("n")
    out.append(("N-P20c / N-P21AN: unjournaled A, journaled B, rollback -> A", seq([(False, "A"), (True, "B")], "rollback") == "A"))
    out.append(("N-P20c2: journaled insert V, unjournaled U, rollback -> U", seq([(True, "V"), (False, "U")], "rollback") == "U"))
    out.append(("N-P21A: unjournaled A, journaled B, unjournaled C, rollback -> C", seq([(False, "A"), (True, "B"), (False, "C")], "rollback") == "C"))
    out.append(("N-P21B: the same, committed -> C", seq([(False, "A"), (True, "B"), (False, "C")], "commit") == "C"))
    return out


def ordering_hazard():
    """R06e. Repair 103's dependency row: unjournaled insert, journaled Committed=1 mark, and the
    question whether a later unjournaled write to the same row survives a rollback of the mark."""
    st = Store({})
    st.tstart()
    st.jrn = False
    st.write(("hist", "d"), {"com": 0, "fin": 0})        # unjournaled insert (R-01 a)
    st.jrn = True
    st.write(("hist", "d"), {"com": 1, "fin": 0})        # journaled mark
    st.jrn = False
    st.write(("hist", "d"), {"com": 1, "fin": 1})        # later unjournaled update of the same node
    st.jrn = True
    st.trollback1()
    with_before_image = st.v[("hist", "d")]
    st = Store({})
    st.before_image = False
    st.tstart()
    st.jrn = False
    st.write(("hist", "d"), {"com": 0, "fin": 0})
    st.jrn = True
    st.write(("hist", "d"), {"com": 1, "fin": 0})
    st.jrn = False
    st.write(("hist", "d"), {"com": 1, "fin": 1})
    st.jrn = True
    st.trollback1()
    return {"compare_and_restore (P-14, model default)": with_before_image, "unconditional_before_image (mutant)": st.v[("hist", "d")],
            "native": "N-P21R (followup/results/native/20261113T234716Z-p21-ordering-r2): committed=1, finalized=1 after the rollback"}


def main(argv):
    out = argv[0]
    os.makedirs(out, exist_ok=True)
    qopts = [("A", "B"), ("B", "B"), ("A", "A")]
    result = {"bounds": "1 top + 0..2 dependencies (flat; nested and shared graphs reduce to it, see docstring); caller flag, top <Defaults>, each "
                        "dependency's <Defaults> in {none, NoTransaction, NoJournal}; -dev or not; at most one Activate fault; caller "
                        "transaction or not; ends: return, and a Terminate or SIGKILL before every step (no caller transaction); "
                        "retry after every process end",
              "q_options": [{"Q-22": a, "Q-23": b} for a, b in qopts]}
    for variants in (["as-built"], ["REPAIR-103"], ["REPAIR-103-GUARDED"], ["REPAIR-103-GUARDED-GLOBAL"], ["REPAIR-103-M-LATE-MARK"], ["REPAIR-NAIVE"], ["M-GLOBAL-MODE"], ["M-BEFORE-IMAGE"], ["M-JOURNAL-NOT-RESTORED"]):
        result[variants[0]] = explore(variants, qopts)
    result["correspondence"] = [{"case": c, "holds": ok} for c, ok in corresponding_cases()]
    result["ordering_hazard"] = ordering_hazard()
    json.dump(result, open(os.path.join(out, "loadtx.json"), "w"), indent=1, sort_keys=True, default=str)
    for k in ("as-built", "REPAIR-103", "REPAIR-103-GUARDED", "REPAIR-103-GUARDED-GLOBAL", "REPAIR-103-M-LATE-MARK", "REPAIR-NAIVE", "M-GLOBAL-MODE", "M-BEFORE-IMAGE", "M-JOURNAL-NOT-RESTORED"):
        r = result[k]
        print(k, "configs", r["configs"], "end_states", r["end_states"], "retry_failures", r["retry_failures"], "transient_differs", r["transient_differs"])
        for vk, n in sorted(r["violations"].items()):
            print("  ", vk, n)
    for c in result["correspondence"]:
        print("CORR", "HOLDS" if c["holds"] else "FAILS", c["case"])
    print("HAZARD", result["ordering_hazard"])
    return 0 if all(c["holds"] for c in result["correspondence"]) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
