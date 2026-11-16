// Generalized L1/L2 for IPM package loading and transaction history (ipm 571e04a1).
//
// Extends models/verus/load_history.rs (one module, at most one dependency, empty namespace, premises P1-P3 built in):
//   - any number n of dependencies (Seq), loaded in the resolver's flat order (Utils/Module.cls#L1399-L1413);
//   - any prior namespace: the top module and each dependency may already be installed at any version; unrelated
//     modules (`other`) and older history rows (`hist`) are arbitrary and must be left untouched (frame);
//   - a caller transaction that commits or rolls back (P1 becomes a theorem);
//   - a crash at every running point, resolved by rollback or by an operator commit (P2 becomes a theorem);
//   - the window between the outer TCOMMIT and SetCommitted as its own step (P3 becomes a theorem);
//   - the recommended fixes as a configuration flag `fix` (DEC-HIST-2 option A: persist the twin after the
//     developer-mode commit; DEC-HIST-1: keep dependency rows in IRISTEMP and re-persist them at the parent's
//     level after its rollback, and reconcile them at the next command after a crash).
//
// IRIS contract (validated in models/explorer/irisenv.py against 9 probes): journaled writes are undone by
// TROLLBACK; IRISTEMP is not; a crash runs %OnClose at the open level and then ends the process's transactions
// (job: rollback; interactive terminal: the operator may commit). At most one load level is open at a time
// above the caller's level (the dependency's nested TSTART/TCOMMIT is folded; checked by models/explorer/fold_check_general.py).
// PREMISE p1c = P1
// PREMISE p2 = P2
// PREMISE p3 = P3
// PREMISE pc_premise = PC
// PREMISE pi = PI
// PREMISE pd = PD
// PREMISE pi_dep = PI-dep
use vstd::prelude::*;

verus! {

pub struct Row { pub exists: bool, pub named: bool, pub ok: bool, pub committed: bool, pub ended: bool }

pub open spec fn no_row() -> Row { Row { exists: false, named: false, ok: false, committed: false, ended: false } }
pub open spec fn init_row() -> Row { Row { exists: true, named: false, ok: false, committed: false, ended: false } }
pub open spec fn dep_row(ok: bool, committed: bool, ended: bool) -> Row {
    Row { exists: true, named: true, ok: ok, committed: committed, ended: ended }
}
// The top record after the developer-mode commit when the fix persists it.
pub open spec fn dev_row() -> Row { Row { exists: true, named: true, ok: false, committed: true, ended: false } }

pub struct Db {
    pub top: Option<int>,          // installed version of the module being loaded
    pub deps: Seq<Option<int>>,    // installed version of each dependency in load order
    pub top_row: Row,              // this command's history record
    pub dep_rows: Seq<Row>,        // the dependency loads' history records
    pub other: Map<int, int>,      // every other installed module
    pub hist: Map<int, Row>,       // every older history record
}

pub enum Mode { Txn, Dev, NoTxn }
pub enum Out { Ok, Fail, Crash }
pub enum Res { Rollback, Commit }
// A process crash (HALT, Terminate, lost terminal) runs %OnClose before ending the transaction; an instance crash
// (iris force, container restart, power loss) runs nothing, wipes IRISTEMP, and rolls back at startup
// (observed: nc-gap-P1-*).
pub enum Kind { Process, Instance }
pub enum Caller { NoCaller, CallerCommits, CallerRollsBack }
pub enum Pc { Init, Guard, Manifest, Deps(nat), Activate, Window, Closed, Crashed, Done, Reconciled }

pub struct Cfg {
    pub mode: Mode,
    pub v: int,               // version in the manifest
    pub vers: Seq<int>,       // version chosen for each dependency
    pub outs: Seq<Out>,       // what each dependency load does
    pub act_ok: bool,         // Activate succeeds
    pub refuse: bool,         // the downgrade/update guard refuses (Utils/Module.cls#L1213-L1222)
    pub caller: Caller,
    pub fix: bool,
}

pub struct L2 {
    pub db: Db,
    pub snaps: Seq<Db>,       // value at each open TSTART, outermost first
    pub temp: Row,            // this command's IRISTEMP record
    pub pend: Seq<Row>,       // (fix) dependency rows kept in IRISTEMP
    pub pc: Pc,
    pub cfg: Cfg,
    pub pre: Db,              // ghost: durable state when the command starts
    pub began: nat,           // ghost: dependency loads started
    pub clean: bool,          // ghost: every crash so far respected P2, P3 and (unless fixed) PC
    pub lost_temp: bool,      // ghost: an instance crash wiped IRISTEMP
}

pub open spec fn n(c: Cfg) -> nat { c.vers.len() }
pub open spec fn uses_txn(m: Mode) -> bool { m != Mode::NoTxn }
pub open spec fn k(c: Cfg) -> nat { if c.caller == Caller::NoCaller { 0 } else { 1 } }
pub open spec fn nothing(len: nat) -> Seq<Row> { Seq::new(len, |j: int| no_row()) }

pub open spec fn wf(c: Cfg, pre: Db) -> bool {
    &&& c.outs.len() == c.vers.len()
    &&& pre.deps.len() == n(c)
    &&& pre.dep_rows == nothing(n(c))
    &&& pre.top_row == no_row()
}

pub open spec fn caller_snaps(s: L2) -> Seq<Db> { if k(s.cfg) == 1 { seq![s.pre] } else { Seq::empty() } }

pub open spec fn initial(s: L2) -> bool {
    &&& wf(s.cfg, s.pre)
    &&& s.db == s.pre && s.snaps == caller_snaps(s) && s.temp == no_row() && s.pend == nothing(n(s.cfg))
    &&& s.pc == Pc::Init && s.began == 0 && s.clean && !s.lost_temp
}

// ---------------------------------------------------------------- L2 steps

pub open spec fn step_init(s: L2) -> L2 {
    let db = Db { top_row: init_row(), ..s.db };
    L2 { db: db, snaps: if uses_txn(s.cfg.mode) { s.snaps.push(db) } else { s.snaps }, temp: init_row(), pc: Pc::Guard, ..s }
}

// Catch, roll back to the level the load started at (#L1358), re-persist pending dependency rows (fix),
// Finalize(error), %OnClose.
pub open spec fn merge(a: Seq<Row>, p: Seq<Row>) -> Seq<Row> { Seq::new(a.len(), |j: int| if p[j].exists { p[j] } else { a[j] }) }
// (fix) Re-persisting a pending dependency row sets Committed from whether the dependency is installed at the logged
// version now: after the parent's rollback it is not. (The first version of the fix kept the logged value; the
// explorer found that dishonest: models/explorer/REPORT.md, section G.)
pub open spec fn merge_fix(a: Seq<Row>, p: Seq<Row>, deps: Seq<Option<int>>, vers: Seq<int>) -> Seq<Row> {
    Seq::new(a.len(), |j: int| if p[j].exists { Row { committed: deps[j] == Some(vers[j]), ..p[j] } } else { a[j] })
}

pub open spec fn fail_end(s: L2) -> L2 {
    let kk = k(s.cfg);
    let rolled = if s.snaps.len() > kk { s.snaps[kk as int] } else { s.db };
    let db2 = if s.cfg.fix { Db { dep_rows: merge_fix(rolled.dep_rows, s.pend, rolled.deps, s.cfg.vers), ..rolled } } else { rolled };
    let t = Row { ok: false, ended: true, ..s.temp };
    L2 { db: Db { top_row: t, ..db2 }, snaps: s.snaps.subrange(0, kk as int), temp: t, pc: Pc::Closed, ..s }
}

pub open spec fn ok_end(s: L2) -> L2 {
    let t = Row { ok: true, committed: true, ended: true, ..s.temp };
    L2 { db: Db { top_row: t, ..s.db }, temp: t, pc: Pc::Closed, ..s }
}

pub open spec fn step_guard(s: L2) -> L2 {
    if s.cfg.refuse { fail_end(s) }
    else if s.cfg.mode == Mode::NoTxn {
        let t = Row { committed: true, ..s.temp };
        // fix: SetCommitted at $TLEVEL 0 also saves the twin (#L1226)
        L2 { db: if s.cfg.fix { Db { top_row: t, ..s.db } } else { s.db }, temp: t, pc: Pc::Manifest, ..s }
    }
    else { L2 { pc: Pc::Manifest, ..s } }
}

pub open spec fn step_manifest(s: L2) -> L2 {
    let db1 = Db { top: Some(s.cfg.v), ..s.db };
    let t = Row { named: true, ..s.temp };
    if s.cfg.mode == Mode::Dev {
        let t2 = Row { committed: true, ..t };
        let db2 = if s.cfg.fix { Db { top_row: t2, ..db1 } } else { db1 };
        // TCOMMIT (#L1311), SetCommitted (#L1313), [fix: persist the twin], TSTART (#L1318)
        L2 { db: db2, snaps: s.snaps.drop_last().push(db2), temp: t2, pc: Pc::Deps(0), ..s }
    } else {
        L2 { db: db1, temp: t, pc: Pc::Deps(0), ..s }
    }
}

pub open spec fn crash(s: L2, r: Res) -> L2 { crash_k(s, r, Kind::Process) }

// PI: where an instance crash leaves an honest record without the fix.
pub open spec fn instance_safe(s: L2) -> bool {
    s.cfg.fix || s.cfg.mode == Mode::Txn || s.pc == Pc::Guard || s.pc == Pc::Manifest
}

pub open spec fn crash_k(s: L2, r: Res, kind: Kind) -> L2 {
    let closed = Db { top_row: s.temp, ..s.db };
    let db = if kind == Kind::Instance { if s.snaps.len() > 0 { s.snaps[0] } else { s.db } }
             else if s.snaps.len() > 0 && r == Res::Rollback { s.snaps[0] } else { closed };
    let fine = (kind == Kind::Instance || r == Res::Rollback) && s.pc != Pc::Window && s.cfg.caller == Caller::NoCaller
               && (s.cfg.fix || !(s.cfg.mode == Mode::Dev && s.pc is Deps))
               && (kind == Kind::Process || instance_safe(s));
    L2 { db: db, snaps: Seq::empty(), pc: Pc::Crashed, clean: s.clean && fine,
         pend: if kind == Kind::Instance { nothing(n(s.cfg)) } else { s.pend },
         lost_temp: s.lost_temp || kind == Kind::Instance, ..s }
}

pub open spec fn step_dep(s: L2, i: nat, r: Res) -> L2 {
    let kept = s.cfg.mode == Mode::NoTxn;
    match s.cfg.outs[i as int] {
        Out::Ok => {
            let row = dep_row(true, true, true);
            L2 { db: Db { deps: s.db.deps.update(i as int, Some(s.cfg.vers[i as int])), dep_rows: s.db.dep_rows.update(i as int, row), ..s.db },
                 pend: if s.cfg.fix { s.pend.update(i as int, row) } else { s.pend }, began: i + 1, pc: Pc::Deps(i + 1), ..s }
        },
        Out::Fail => {
            let row = dep_row(false, kept, true);
            fail_end(L2 { db: Db { deps: if kept { s.db.deps.update(i as int, Some(s.cfg.vers[i as int])) } else { s.db.deps },
                                   dep_rows: s.db.dep_rows.update(i as int, row), ..s.db },
                          pend: if s.cfg.fix { s.pend.update(i as int, row) } else { s.pend }, began: i + 1, ..s })
        },
        Out::Crash => {
            // The dependency's module write is made inside its own level; an operator commit at the crash prompt
            // commits every open level, so it is kept then too (found by models/explorer/fold_check_general.py).
            let row = dep_row(false, kept, false);
            crash(L2 { db: Db { deps: if kept || r == Res::Commit { s.db.deps.update(i as int, Some(s.cfg.vers[i as int])) } else { s.db.deps },
                                dep_rows: s.db.dep_rows.update(i as int, row), ..s.db },
                       pend: if s.cfg.fix { s.pend.update(i as int, row) } else { s.pend }, began: i + 1, ..s }, r)
        },
    }
}

pub open spec fn step_deps_done(s: L2) -> L2 {
    if s.cfg.mode == Mode::Dev { L2 { snaps: s.snaps.drop_last(), pc: Pc::Activate, ..s } }   // TCOMMIT #L1338
    else { L2 { pc: Pc::Activate, ..s } }
}

pub open spec fn step_activate(s: L2) -> L2 {
    if s.cfg.act_ok {
        if s.cfg.mode == Mode::Txn { L2 { snaps: s.snaps.drop_last(), pc: Pc::Window, ..s } }  // TCOMMIT #L1348
        else { ok_end(s) }
    } else { fail_end(s) }
}

pub open spec fn step_window(s: L2) -> L2 { ok_end(L2 { temp: Row { committed: true, ..s.temp }, ..s }) }  // SetCommitted #L1350

pub open spec fn step_reconcile(s: L2) -> L2 { L2 { db: Db { dep_rows: merge_fix(s.db.dep_rows, s.pend, s.db.deps, s.cfg.vers), ..s.db }, pc: Pc::Reconciled, ..s } }

pub open spec fn step_caller_end(s: L2) -> L2 {
    let db = if s.cfg.caller == Caller::CallerRollsBack { s.snaps[0] } else { s.db };
    L2 { db: db, snaps: Seq::empty(), pc: Pc::Done, ..s }
}

pub enum Choice { Init, Guard, Manifest, Dep(Res), DepsDone, Activate, Window, Crash(Res), InstanceCrash, Reconcile, CallerEnd }

pub open spec fn running(pc: Pc) -> bool {
    pc == Pc::Guard || pc == Pc::Manifest || pc is Deps || pc == Pc::Activate || pc == Pc::Window
}

pub open spec fn enabled(s: L2, c: Choice) -> bool {
    match c {
        Choice::Init => s.pc == Pc::Init,
        Choice::Guard => s.pc == Pc::Guard,
        Choice::Manifest => s.pc == Pc::Manifest,
        Choice::Dep(_) => s.pc is Deps && s.pc->Deps_0 < n(s.cfg),
        Choice::DepsDone => s.pc is Deps && s.pc->Deps_0 == n(s.cfg),
        Choice::Activate => s.pc == Pc::Activate,
        Choice::Window => s.pc == Pc::Window,
        Choice::Crash(_) => running(s.pc),
        Choice::InstanceCrash => running(s.pc),
        Choice::Reconcile => s.pc == Pc::Crashed && s.cfg.fix,
        Choice::CallerEnd => s.pc == Pc::Closed && s.cfg.caller != Caller::NoCaller,
    }
}

pub open spec fn step(s: L2, c: Choice) -> L2 {
    match c {
        Choice::Init => step_init(s),
        Choice::Guard => step_guard(s),
        Choice::Manifest => step_manifest(s),
        Choice::Dep(r) => step_dep(s, s.pc->Deps_0, r),
        Choice::DepsDone => step_deps_done(s),
        Choice::Activate => step_activate(s),
        Choice::Window => step_window(s),
        Choice::Crash(r) => crash(s, r),
        Choice::InstanceCrash => crash_k(s, Res::Rollback, Kind::Instance),
        Choice::Reconcile => step_reconcile(s),
        Choice::CallerEnd => step_caller_end(s),
    }
}

// ---------------------------------------------------------------- L1

pub enum Phase { NotStarted, Running, Ended, Crashed, Reconciled }

pub struct L1 { pub d: Db, pub phase: Phase, pub cfg: Cfg, pub pre: Db, pub began: nat }

pub open spec fn durable(s: L2) -> Db { if s.snaps.len() == 0 { s.db } else { s.snaps[0] } }

pub open spec fn phase_of(s: L2) -> Phase {
    match s.pc {
        Pc::Init => Phase::NotStarted,
        Pc::Closed => if s.cfg.caller == Caller::NoCaller { Phase::Ended } else { Phase::Running },
        Pc::Done => Phase::Ended,
        Pc::Crashed => Phase::Crashed,
        Pc::Reconciled => Phase::Reconciled,
        _ => Phase::Running,
    }
}

pub open spec fn abs(s: L2) -> L1 { L1 { d: durable(s), phase: phase_of(s), cfg: s.cfg, pre: s.pre, began: s.began } }

pub open spec fn frame(a: L1) -> bool { a.d.other == a.pre.other && a.d.hist == a.pre.hist && a.d.deps.len() == n(a.cfg) && a.d.dep_rows.len() == n(a.cfg) }

pub open spec fn unchanged(a: L1) -> bool { a.d.top == a.pre.top && a.d.deps == a.pre.deps }

// The command worked: not refused, every dependency loaded, Activate succeeded.
pub open spec fn worked(c: Cfg) -> bool {
    !c.refuse && c.act_ok && (forall|j: int| 0 <= j < c.outs.len() ==> c.outs[j] == Out::Ok)
}

pub open spec fn all_deps_installed(a: L1) -> bool { forall|j: int| 0 <= j < n(a.cfg) ==> a.d.deps[j] == Some(a.cfg.vers[j]) }

// What an operator can rely on once the command is over (or crashed).
pub open spec fn honest(a: L1) -> bool {
    &&& frame(a)
    &&& a.d.top_row.exists
    &&& (!a.d.top_row.committed ==> unchanged(a))                                         // Committed = 0: nothing kept
    &&& (a.d.top_row.committed && a.cfg.mode != Mode::NoTxn ==> a.d.top == Some(a.cfg.v))  // Committed = 1: the manifest was kept
    &&& (a.d.top_row.ok ==> a.d.top_row.committed && a.d.top == Some(a.cfg.v) && all_deps_installed(a))
    &&& (a.cfg.mode == Mode::Txn && !a.d.top_row.ok ==> unchanged(a))                       // transactional failure is atomic
    &&& (a.phase == Phase::Ended <==> a.d.top_row.ended)
    &&& (a.phase == Phase::Ended ==> (a.d.top_row.ok <==> worked(a.cfg)))                  // Success says whether it worked
    &&& (a.phase != Phase::Ended ==> !a.d.top_row.ok)                                       // a crash never records success
    &&& (forall|j: int| 0 <= j < n(a.cfg) && #[trigger] a.d.dep_rows[j].exists ==>
            (a.d.dep_rows[j].ok <==> a.cfg.outs[j] == Out::Ok))                             // a dependency record says whether it worked
    &&& (forall|j: int| 0 <= j < n(a.cfg) && a.d.deps[j] != a.pre.deps[j] ==> a.d.dep_rows[j].exists)  // kept dependency has a record
    &&& (forall|j: int| 0 <= j < n(a.cfg) && #[trigger] a.d.dep_rows[j].exists ==>                      // its record is honest
            (a.d.dep_rows[j].committed ==> a.d.deps[j] == Some(a.cfg.vers[j]))
            && (!a.d.dep_rows[j].committed ==> a.d.deps[j] == a.pre.deps[j]))
}

pub open spec fn honest_strict(a: L1) -> bool {
    honest(a) && (forall|j: int| 0 <= j < a.began && j < n(a.cfg) ==> a.d.dep_rows[j].exists)
}

pub open spec fn same_run(a: L1, b: L1) -> bool { b.cfg == a.cfg && b.pre == a.pre && a.began <= b.began }

pub open spec fn progress_row(b: L1) -> bool {
    b.d.top_row.exists && !b.d.top_row.ended && !b.d.top_row.ok
    && (b.d.top_row.committed ==> b.d.top == Some(b.cfg.v) || b.cfg.mode == Mode::NoTxn)
}

pub open spec fn keep_allowed(a: L1, b: L1) -> bool {
    if a.cfg.mode == Mode::Txn { (b.d.top == a.d.top && b.d.deps == a.d.deps) || (b.d.top == Some(a.cfg.v) && all_deps_installed(b)) }
    else {
        (b.d.top == a.d.top || b.d.top == Some(a.cfg.v))
        && (forall|j: int| 0 <= j < n(a.cfg) ==> b.d.deps[j] == a.d.deps[j] || b.d.deps[j] == Some(a.cfg.vers[j]))
    }
}

pub open spec fn l1_next(a: L1, b: L1, strict: bool) -> bool {
    ||| a == b
    ||| (a.phase == Phase::NotStarted && b.phase == Phase::Running && same_run(a, b) && frame(b)
         && (b.d == a.d || b.d == Db { top_row: init_row(), ..a.d }))
    ||| (a.phase == Phase::Running && b.phase == Phase::Running && same_run(a, b) && frame(b)
         && (b.d.top_row == a.d.top_row || progress_row(b)) && keep_allowed(a, b)
         && (forall|j: int| 0 <= j < n(a.cfg) ==> b.d.dep_rows[j] == a.d.dep_rows[j] || !a.d.dep_rows[j].exists))
    ||| (a.phase == Phase::Running && b.phase == Phase::Ended && same_run(a, b) && (if strict { honest_strict(b) } else { honest(b) }))
    ||| (a.phase == Phase::Running && b.phase == Phase::Crashed && same_run(a, b) && honest(b))
    ||| (a.phase == Phase::Crashed && b.phase == Phase::Reconciled && same_run(a, b) && (if strict { honest_strict(b) } else { honest(b) }))
}

// ---------------------------------------------------------------- invariant

pub open spec fn init_db(s: L2) -> Db { Db { top_row: init_row(), ..s.pre } }
pub open spec fn guard_db(s: L2) -> Db {
    if s.cfg.fix && s.cfg.mode == Mode::NoTxn { Db { top_row: Row { committed: true, ..init_row() }, ..s.pre } } else { init_db(s) }
}
pub open spec fn applied(s: L2, i: nat) -> Seq<Option<int>> {
    Seq::new(n(s.cfg), |j: int| if j < i { Some(s.cfg.vers[j]) } else { s.pre.deps[j] })
}
pub open spec fn rows_applied(s: L2, i: nat) -> Seq<Row> {
    Seq::new(n(s.cfg), |j: int| if j < i { dep_row(true, true, true) } else { no_row() })
}
pub open spec fn top_after_manifest(s: L2) -> Row {
    if s.cfg.fix && s.cfg.mode == Mode::Dev { dev_row() }
    else if s.cfg.fix && s.cfg.mode == Mode::NoTxn { Row { committed: true, ..init_row() } }
    else { init_row() }
}
pub open spec fn db_at(s: L2, i: nat) -> Db {
    Db { top: Some(s.cfg.v), deps: applied(s, i), top_row: top_after_manifest(s), dep_rows: rows_applied(s, i), ..s.pre }
}
pub open spec fn running_temp(s: L2) -> Row { Row { named: true, committed: s.cfg.mode != Mode::Txn, ..init_row() } }
pub open spec fn level_snaps(s: L2, i: nat) -> Seq<Db> {
    match s.cfg.mode {
        Mode::Txn => caller_snaps(s).push(init_db(s)),
        Mode::Dev => caller_snaps(s).push(Db { top: Some(s.cfg.v), top_row: top_after_manifest(s), ..s.pre }),
        Mode::NoTxn => caller_snaps(s),
    }
}

// A state at the end of a command (Closed): honest as seen from its own database.
pub open spec fn closed_view(s: L2) -> L1 { L1 { d: s.db, phase: Phase::Ended, cfg: s.cfg, pre: s.pre, began: s.began } }

pub open spec fn frame_db(d: Db, p: Db) -> bool {
    d.other == p.other && d.hist == p.hist && d.deps.len() == p.deps.len() && d.dep_rows.len() == p.deps.len()
}
pub open spec fn dep_shape(d: Db, s: L2) -> bool {
    forall|j: int| 0 <= j < n(s.cfg) ==> #[trigger] d.deps[j] == s.pre.deps[j] || d.deps[j] == Some(s.cfg.vers[j])
}

pub open spec fn inv(s: L2) -> bool {
    &&& wf(s.cfg, s.pre)
    &&& s.pend.len() == n(s.cfg)
    &&& (s.lost_temp ==> s.pc == Pc::Crashed || s.pc == Pc::Reconciled)
    &&& frame_db(s.db, s.pre) && (forall|q: int| 0 <= q < s.snaps.len() ==> frame_db(#[trigger] s.snaps[q], s.pre))
    &&& dep_shape(s.db, s) && (forall|q: int| 0 <= q < s.snaps.len() ==> dep_shape(#[trigger] s.snaps[q], s))
    &&& match s.pc {
        Pc::Init => s.db == s.pre && s.snaps == caller_snaps(s) && s.began == 0 && s.pend == nothing(n(s.cfg)),
        Pc::Guard => s.db == init_db(s) && s.temp == init_row() && s.began == 0 && s.pend == nothing(n(s.cfg))
            && s.snaps == (if uses_txn(s.cfg.mode) { caller_snaps(s).push(init_db(s)) } else { caller_snaps(s) }),
        Pc::Manifest => s.db == guard_db(s) && s.temp == Row { committed: s.cfg.mode == Mode::NoTxn, ..init_row() }
            && s.began == 0 && s.pend == nothing(n(s.cfg)) && !s.cfg.refuse
            && s.snaps == (if uses_txn(s.cfg.mode) { caller_snaps(s).push(init_db(s)) } else { caller_snaps(s) }),
        Pc::Deps(i) => i <= n(s.cfg) && s.began == i && s.db == db_at(s, i) && s.temp == running_temp(s) && !s.cfg.refuse
            && (forall|j: int| 0 <= j < i ==> #[trigger] s.cfg.outs[j] == Out::Ok)
            && s.snaps == level_snaps(s, i) && s.pend == (if s.cfg.fix { rows_applied(s, i) } else { nothing(n(s.cfg)) }),
        Pc::Activate => s.began == n(s.cfg) && s.db == db_at(s, n(s.cfg)) && s.temp == running_temp(s) && !s.cfg.refuse
            && (forall|j: int| 0 <= j < n(s.cfg) ==> #[trigger] s.cfg.outs[j] == Out::Ok)
            && s.snaps == (if s.cfg.mode == Mode::Txn { caller_snaps(s).push(init_db(s)) } else { caller_snaps(s) })
            && s.pend == (if s.cfg.fix { rows_applied(s, n(s.cfg)) } else { nothing(n(s.cfg)) }),
        Pc::Window => s.cfg.mode == Mode::Txn && s.cfg.act_ok && s.began == n(s.cfg) && s.db == db_at(s, n(s.cfg))
            && (forall|j: int| 0 <= j < n(s.cfg) ==> #[trigger] s.cfg.outs[j] == Out::Ok)
            && s.temp == running_temp(s) && s.snaps == caller_snaps(s) && !s.cfg.refuse
            && s.pend == (if s.cfg.fix { rows_applied(s, n(s.cfg)) } else { nothing(n(s.cfg)) }),
        Pc::Closed => s.snaps == caller_snaps(s) && honest(closed_view(s)),
        Pc::Crashed => s.snaps.len() == 0 && (s.clean ==> honest(abs(s)))
            && (s.cfg.fix && s.clean ==> honest(L1 { d: Db { dep_rows: merge_fix(s.db.dep_rows, s.pend, s.db.deps, s.cfg.vers), ..s.db }, ..abs(s) }))
            && (s.cfg.fix && !s.lost_temp ==> forall|j: int| 0 <= j < s.began && j < n(s.cfg) ==> s.pend[j].exists)
            && (s.lost_temp ==> s.pend == nothing(n(s.cfg))),
        Pc::Done => s.snaps.len() == 0,
        Pc::Reconciled => s.snaps.len() == 0,
    }
}

// ---------------------------------------------------------------- premises

// P1' (weakened from P1): the caller does not roll back. (g_caller_rollback_leaves_no_trace proves what happens if it does.)
pub open spec fn p1(s: L2) -> bool { s.cfg.caller != Caller::CallerRollsBack }
// A crash ends the process and rolls back the caller's level as well, so a crash needs no caller at all.
pub open spec fn p1c(s: L2, c: Choice) -> bool { p1(s) && (crash_res(s, c) is Some ==> s.cfg.caller == Caller::NoCaller) }
// P2: a crash rolls back. P3: no crash in the commit-flag window. PC: no crash while a developer-mode load has its
// dependency transaction open, unless fixed.
pub open spec fn crash_res(s: L2, c: Choice) -> Option<Res> {
    match c {
        Choice::Crash(r) => Some(r),
        Choice::InstanceCrash => Some(Res::Rollback),
        Choice::Dep(r) => if s.pc is Deps && s.pc->Deps_0 < n(s.cfg) && s.cfg.outs[s.pc->Deps_0 as int] == Out::Crash { Some(r) } else { None },
        _ => None,
    }
}
pub open spec fn p2(s: L2, c: Choice) -> bool { crash_res(s, c) != Some(Res::Commit) }
pub open spec fn p3(s: L2, c: Choice) -> bool { !(s.pc == Pc::Window && crash_res(s, c) is Some) }
// PI: an instance crash happens only where it leaves an honest record (always true with the fix).
pub open spec fn pi(s: L2, c: Choice) -> bool { c != Choice::InstanceCrash || instance_safe(s) }
// PI-dep: with the fix, no instance crash after a dependency load began (its pending row lives in IRISTEMP).
pub open spec fn pi_dep(s: L2, c: Choice) -> bool {
    !(c == Choice::InstanceCrash && s.began > 0) && !(c == Choice::Reconcile && s.lost_temp && s.began > 0)
}
pub open spec fn pc_premise(s: L2, c: Choice) -> bool { s.cfg.fix || !(s.cfg.mode == Mode::Dev && s.pc is Deps && crash_res(s, c) is Some) }
// PD: no dependency loaded inside a transaction that later rolls back, unless fixed.
pub open spec fn pd(s: L2, c: Choice) -> bool {
    s.cfg.fix || !(uses_txn(s.cfg.mode) && s.snaps.len() > k(s.cfg) && (
        (s.pc is Deps && s.pc->Deps_0 < n(s.cfg) && s.cfg.outs[s.pc->Deps_0 as int] != Out::Ok && c is Dep)
        || (s.began > 0 && (c is Crash || (c == Choice::Activate && !s.cfg.act_ok)))))
}

// ---------------------------------------------------------------- lemmas (one per step; playbook 3.3 layout)

proof fn lemma_seq_facts(s: L2, i: nat)
    requires wf(s.cfg, s.pre), i < n(s.cfg)
    ensures
        applied(s, i).update(i as int, Some(s.cfg.vers[i as int])) == applied(s, i + 1),
        rows_applied(s, i).update(i as int, dep_row(true, true, true)) == rows_applied(s, i + 1),
{
    assert(applied(s, i).update(i as int, Some(s.cfg.vers[i as int])) =~= applied(s, i + 1));
    assert(rows_applied(s, i).update(i as int, dep_row(true, true, true)) =~= rows_applied(s, i + 1));
}

proof fn lemma_seq_zero(s: L2)
    requires wf(s.cfg, s.pre)
    ensures applied(s, 0) == s.pre.deps, rows_applied(s, 0) == nothing(n(s.cfg)),
            merge(nothing(n(s.cfg)), nothing(n(s.cfg))) == nothing(n(s.cfg)),
            caller_snaps(s).push(init_db(s)).drop_last() == caller_snaps(s),
            caller_snaps(s).push(init_db(s)).subrange(0, k(s.cfg) as int) == caller_snaps(s),
            caller_snaps(s).subrange(0, k(s.cfg) as int) == caller_snaps(s),
{
    assert(applied(s, 0) =~= s.pre.deps);
    assert(rows_applied(s, 0) =~= nothing(n(s.cfg)));
    assert(merge(nothing(n(s.cfg)), nothing(n(s.cfg))) =~= nothing(n(s.cfg)));
    assert(caller_snaps(s).push(init_db(s)).drop_last() =~= caller_snaps(s));
    assert(caller_snaps(s).push(init_db(s)).subrange(0, k(s.cfg) as int) =~= caller_snaps(s));
    assert(caller_snaps(s).subrange(0, k(s.cfg) as int) =~= caller_snaps(s));
}

proof fn lemma_level_facts(s: L2, d: Db)
    ensures caller_snaps(s).push(d).drop_last() == caller_snaps(s),
            caller_snaps(s).push(d).subrange(0, k(s.cfg) as int) == caller_snaps(s),
            caller_snaps(s).push(d)[k(s.cfg) as int] == d,
            caller_snaps(s).len() == k(s.cfg),
{
    assert(caller_snaps(s).push(d).drop_last() =~= caller_snaps(s));
    assert(caller_snaps(s).push(d).subrange(0, k(s.cfg) as int) =~= caller_snaps(s));
}

proof fn lemma_merge_nothing(a: Seq<Row>, len: nat)
    requires a.len() == len
    ensures merge(a, nothing(len)) == a
{
    assert(merge(a, nothing(len)) =~= a);
}

// Re-persisting pending dependency rows (with Committed taken from the installed state) keeps a view honest.
proof fn lemma_honest_merge(a: L1, p: Seq<Row>)
    requires honest(a), p.len() == a.d.dep_rows.len(),
             forall|j: int| 0 <= j < n(a.cfg) ==> #[trigger] a.d.deps[j] == a.pre.deps[j] || a.d.deps[j] == Some(a.cfg.vers[j]),
             forall|j: int| 0 <= j < n(a.cfg) && #[trigger] p[j].exists ==> (p[j].ok <==> a.cfg.outs[j] == Out::Ok)
    ensures honest(L1 { d: Db { dep_rows: merge_fix(a.d.dep_rows, p, a.d.deps, a.cfg.vers), ..a.d }, ..a })
{
    let b = L1 { d: Db { dep_rows: merge_fix(a.d.dep_rows, p, a.d.deps, a.cfg.vers), ..a.d }, ..a };
    assert forall|j: int| 0 <= j < n(b.cfg) && b.d.deps[j] != b.pre.deps[j] implies b.d.dep_rows[j].exists by {
        assert(a.d.dep_rows[j].exists);
    };
    assert forall|j: int| 0 <= j < n(b.cfg) && #[trigger] b.d.dep_rows[j].exists implies
        (b.d.dep_rows[j].committed ==> b.d.deps[j] == Some(b.cfg.vers[j])) && (!b.d.dep_rows[j].committed ==> b.d.deps[j] == b.pre.deps[j]) by {
        if !p[j].exists { assert(a.d.dep_rows[j].exists); }
    };
}

proof fn inv_init(s: L2)
    requires inv(s), s.pc == Pc::Init
    ensures inv(step_init(s))
{
    lemma_level_facts(s, init_db(s));
}

proof fn inv_guard(s: L2)
    requires inv(s), s.pc == Pc::Guard
    ensures inv(step_guard(s))
{
    lemma_seq_zero(s);
    lemma_level_facts(s, init_db(s));
    if s.cfg.refuse {
        lemma_merge_nothing(init_db(s).dep_rows, n(s.cfg));
        let e = step_guard(s);
        assert(e.snaps == caller_snaps(s));
        assert(honest(closed_view(e)));
    }
}

proof fn inv_manifest(s: L2)
    requires inv(s), s.pc == Pc::Manifest
    ensures inv(step_manifest(s))
{
    lemma_seq_zero(s);
    lemma_level_facts(s, init_db(s));
    let e = step_manifest(s);
    assert(e.db == db_at(s, 0));
    if s.cfg.mode == Mode::Dev {
        lemma_level_facts(s, Db { top: Some(s.cfg.v), top_row: top_after_manifest(s), ..s.pre });
        assert(e.snaps =~= level_snaps(s, 0));
    }
    assert(e.pend == (if s.cfg.fix { rows_applied(s, 0) } else { nothing(n(s.cfg)) }));
}

proof fn inv_dep(s: L2, r: Res)
    requires inv(s), s.pc is Deps, s.pc->Deps_0 < n(s.cfg)
    ensures inv(step_dep(s, s.pc->Deps_0, r))
{
    let i = s.pc->Deps_0;
    let nn = n(s.cfg);
    lemma_seq_facts(s, i);
    lemma_seq_zero(s);
    let kept = s.cfg.mode == Mode::NoTxn;
    match s.cfg.outs[i as int] {
        Out::Ok => {
            let e = step_dep(s, i, r);
            assert(e.db == db_at(s, i + 1));
        },
        Out::Fail => {
            assert(!worked(s.cfg));
            let row = dep_row(false, kept, true);
            let s1 = L2 { db: Db { deps: if kept { s.db.deps.update(i as int, Some(s.cfg.vers[i as int])) } else { s.db.deps },
                                   dep_rows: s.db.dep_rows.update(i as int, row), ..s.db },
                          pend: if s.cfg.fix { s.pend.update(i as int, row) } else { s.pend }, began: i + 1, ..s };
            let e = fail_end(s1);
            match s.cfg.mode {
                Mode::Txn => { lemma_level_facts(s, init_db(s)); },
                Mode::Dev => { lemma_level_facts(s, Db { top: Some(s.cfg.v), top_row: top_after_manifest(s), ..s.pre }); },
                Mode::NoTxn => { assert(caller_snaps(s).subrange(0, k(s.cfg) as int) =~= caller_snaps(s)); },
            }
            if !s.cfg.fix {
                let rolled = if s1.snaps.len() > k(s1.cfg) { s1.snaps[k(s1.cfg) as int] } else { s1.db };
                assert(e.db.dep_rows == rolled.dep_rows);
            }
            assert(honest(closed_view(e)));
        },
        Out::Crash => {
            let row = dep_row(false, kept, false);
            let s1 = L2 { db: Db { deps: if kept { s.db.deps.update(i as int, Some(s.cfg.vers[i as int])) } else { s.db.deps },
                                   dep_rows: s.db.dep_rows.update(i as int, row), ..s.db },
                          pend: if s.cfg.fix { s.pend.update(i as int, row) } else { s.pend }, began: i + 1, ..s };
            let e = crash(s1, r);
            if s.cfg.fix {
                assert(forall|j: int| 0 <= j < e.began && j < nn ==> e.pend[j].exists);
            }
        },
    }
}

proof fn inv_deps_done(s: L2)
    requires inv(s), s.pc is Deps, s.pc->Deps_0 == n(s.cfg)
    ensures inv(step_deps_done(s))
{
    if s.cfg.mode == Mode::Dev {
        lemma_level_facts(s, Db { top: Some(s.cfg.v), top_row: top_after_manifest(s), ..s.pre });
    }
}

proof fn inv_activate(s: L2)
    requires inv(s), s.pc == Pc::Activate
    ensures inv(step_activate(s))
{
    lemma_seq_zero(s);
    lemma_level_facts(s, init_db(s));
    let e = step_activate(s);
    if !s.cfg.act_ok {
        assert(honest(closed_view(e)));
    } else if s.cfg.mode != Mode::Txn {
        assert(honest(closed_view(e)));
    }
}

proof fn inv_window(s: L2)
    requires inv(s), s.pc == Pc::Window
    ensures inv(step_window(s))
{
    assert(honest(closed_view(step_window(s))));
}

proof fn inv_instance_crash(s: L2)
    requires inv(s), running(s.pc)
    ensures inv(crash_k(s, Res::Rollback, Kind::Instance))
{
    lemma_seq_zero(s);
    let e = crash_k(s, Res::Rollback, Kind::Instance);
    assert(frame_db(e.db, e.pre)) by { if s.snaps.len() > 0 { assert(frame_db(s.snaps[0], s.pre)); } };
    assert(dep_shape(e.db, e)) by { if s.snaps.len() > 0 { assert(dep_shape(s.snaps[0], s)); } };
    lemma_merge_nothing(e.db.dep_rows, n(s.cfg));
    assert(merge_fix(e.db.dep_rows, e.pend, e.db.deps, e.cfg.vers) =~= e.db.dep_rows);
    if e.clean {
        assert(honest(abs(e)));
    }
}

proof fn inv_crash(s: L2, r: Res)
    requires inv(s), running(s.pc)
    ensures inv(crash(s, r))
{
    lemma_seq_zero(s);
    let e = crash(s, r);
    if s.cfg.fix {
        if s.pc == Pc::Guard || s.pc == Pc::Manifest {
            lemma_merge_nothing(e.db.dep_rows, n(s.cfg));
        }
    }
    assert(e.snaps.len() == 0);
    assert(frame_db(e.db, e.pre)) by { if s.snaps.len() > 0 { assert(frame_db(s.snaps[0], s.pre)); } };
    if e.clean {
        assert(honest(abs(e)));
    }
    if s.cfg.fix && e.clean {
        assert(dep_shape(e.db, e)) by { if s.snaps.len() > 0 && r == Res::Rollback { assert(dep_shape(s.snaps[0], s)); } };
        assert(forall|j: int| 0 <= j < n(e.cfg) && #[trigger] e.pend[j].exists ==> (e.pend[j].ok <==> e.cfg.outs[j] == Out::Ok));
        lemma_honest_merge(abs(e), e.pend);
        assert(abs(e).d == e.db);
    }
    if s.cfg.fix {
        assert(forall|j: int| 0 <= j < e.began && j < n(e.cfg) ==> e.pend[j].exists);
    }
}

pub proof fn g_inv_initial(s: L2)
    requires initial(s)
    ensures inv(s)
{}

pub proof fn g_inv_step(s: L2, c: Choice)
    requires inv(s), enabled(s, c)
    ensures inv(step(s, c))
{
    match c {
        Choice::Init => inv_init(s),
        Choice::Guard => inv_guard(s),
        Choice::Manifest => inv_manifest(s),
        Choice::Dep(r) => inv_dep(s, r),
        Choice::DepsDone => inv_deps_done(s),
        Choice::Activate => inv_activate(s),
        Choice::Window => inv_window(s),
        Choice::Crash(r) => inv_crash(s, r),
        Choice::InstanceCrash => inv_instance_crash(s),
        Choice::Reconcile => {},
        Choice::CallerEnd => {},
    }
}

// ---------------------------------------------------------------- refinement (one lemma per step)

pub open spec fn premises(s: L2, c: Choice, strict: bool) -> bool {
    &&& p1c(s, c) && p2(s, c) && p3(s, c) && pc_premise(s, c) && pi(s, c) && s.clean
    &&& (strict ==> pd(s, c) && pi_dep(s, c) && s.cfg.caller == Caller::NoCaller)
}

proof fn refine_ends_honest(s: L2, e: L2, strict: bool)
    requires inv(e), e.pc == Pc::Closed, e.cfg.caller == Caller::NoCaller, e.cfg == s.cfg, e.pre == s.pre,
             strict ==> (forall|j: int| 0 <= j < e.began && j < n(e.cfg) ==> e.db.dep_rows[j].exists)
    ensures (if strict { honest_strict(abs(e)) } else { honest(abs(e)) })
{
    assert(abs(e) == closed_view(e));
}

pub proof fn g_step_refines(s: L2, c: Choice, strict: bool)
    requires inv(s), enabled(s, c), premises(s, c, strict)
    ensures l1_next(abs(s), abs(step(s, c)), strict)
{
    g_inv_step(s, c);
    lemma_seq_zero(s);
    let e = step(s, c);
    match c {
        Choice::Init => { lemma_level_facts(s, init_db(s)); },
        Choice::Guard => {
            lemma_level_facts(s, init_db(s));
            if s.cfg.refuse && s.cfg.caller == Caller::NoCaller { refine_ends_honest(s, e, strict); }
        },
        Choice::Manifest => {
            lemma_level_facts(s, init_db(s));
            if s.cfg.mode == Mode::Dev {
                lemma_level_facts(s, Db { top: Some(s.cfg.v), top_row: top_after_manifest(s), ..s.pre });
            }
        },
        Choice::Dep(r) => {
            let i = s.pc->Deps_0;
            lemma_seq_facts(s, i);
            match s.cfg.mode {
                Mode::Txn => { lemma_level_facts(s, init_db(s)); },
                Mode::Dev => { lemma_level_facts(s, Db { top: Some(s.cfg.v), top_row: top_after_manifest(s), ..s.pre }); },
                Mode::NoTxn => {},
            }
            if e.pc == Pc::Closed && s.cfg.caller == Caller::NoCaller { refine_ends_honest(s, e, strict); }
        },
        Choice::DepsDone => {
            if s.cfg.mode == Mode::Dev {
                lemma_level_facts(s, Db { top: Some(s.cfg.v), top_row: top_after_manifest(s), ..s.pre });
            }
        },
        Choice::Activate => {
            lemma_level_facts(s, init_db(s));
            if e.pc == Pc::Closed && s.cfg.caller == Caller::NoCaller { refine_ends_honest(s, e, strict); }
        },
        Choice::Window => {
            if s.cfg.caller == Caller::NoCaller { refine_ends_honest(s, e, strict); }
        },
        Choice::Crash(r) => {},
        Choice::InstanceCrash => {},
        Choice::Reconcile => {
            if s.lost_temp { assert(merge_fix(s.db.dep_rows, s.pend, s.db.deps, s.cfg.vers) =~= s.db.dep_rows); }
        },
        Choice::CallerEnd => {},
    }
}

// Every L2 step refines L1 (weak contract) under P1c, P2, P3 and PC: any number of dependencies, any prior state.
pub proof fn g_refines(s: L2, c: Choice)
    requires inv(s), enabled(s, c), p1c(s, c), p2(s, c), p3(s, c), pc_premise(s, c), pi(s, c), s.clean
    ensures l1_next(abs(s), abs(step(s, c)), false)
{
    g_step_refines(s, c, false);
}

// With PD as well (and no caller), the strict contract holds: every dependency load leaves a record.
pub proof fn g_refines_strict(s: L2, c: Choice)
    requires inv(s), enabled(s, c), p1c(s, c), p2(s, c), p3(s, c), pc_premise(s, c), pi(s, c), pd(s, c), pi_dep(s, c), s.clean,
             s.cfg.caller == Caller::NoCaller
    ensures l1_next(abs(s), abs(step(s, c)), true)
{
    g_step_refines(s, c, true);
}

// The fixed design needs neither PC, PD nor PI; only PI-dep remains for the strict contract
// (pending dependency rows live in IRISTEMP, which an instance crash wipes: g_instance_crash_loses_pending).
pub proof fn g_fixed_refines_strict(s: L2, c: Choice)
    requires inv(s), enabled(s, c), s.cfg.fix, p1c(s, c), p2(s, c), p3(s, c), pi_dep(s, c), s.clean, s.cfg.caller == Caller::NoCaller
    ensures l1_next(abs(s), abs(step(s, c)), true)
{
    g_step_refines(s, c, true);
}

// ---------------------------------------------------------------- characterizations (premises as theorems)

// P1: if the caller rolls back, the command leaves no trace: modules, history and other data are as before.
pub proof fn g_caller_rollback_leaves_no_trace(s: L2)
    requires inv(s), s.pc == Pc::Closed, s.cfg.caller == Caller::CallerRollsBack
    ensures durable(step_caller_end(s)) == s.pre, !durable(step_caller_end(s)).top_row.exists
{}

// P1: if the caller commits, the outcome is honest exactly as with no caller.
pub proof fn g_caller_commit_is_honest(s: L2)
    requires inv(s), s.pc == Pc::Closed, s.cfg.caller == Caller::CallerCommits
    ensures honest(abs(step_caller_end(s)))
{
    assert(abs(step_caller_end(s)) == closed_view(s));
}

// P3: in a transactional load, a crash in the window after the outer TCOMMIT is always dishonest when the load
// changed the module, whatever the prior state and dependencies.
pub proof fn g_window_crash_dishonest(s: L2)
    requires inv(s), s.pc == Pc::Window, s.cfg.caller == Caller::NoCaller, s.pre.top != Some(s.cfg.v)
    ensures !honest(abs(crash(s, Res::Rollback)))
{}

// P2: an operator commit at the crash prompt of a transactional load, after the manifest write, is dishonest
// whenever the load changed the module.
pub proof fn g_operator_commit_dishonest(s: L2)
    requires inv(s), s.cfg.mode == Mode::Txn, s.cfg.caller == Caller::NoCaller, s.pc is Deps || s.pc == Pc::Activate,
             s.pre.top != Some(s.cfg.v)
    ensures !honest(abs(crash(s, Res::Commit)))
{}

// P2, rollback branch, is not a premise: a crash that rolls back is honest in every mode and at every point,
// except the developer-mode dependency phase (PC) and the window (P3).
pub proof fn g_rollback_crash_honest(s: L2)
    requires inv(s), running(s.pc), s.cfg.caller == Caller::NoCaller, s.pc != Pc::Window, s.clean,
             s.cfg.fix || !(s.cfg.mode == Mode::Dev && s.pc is Deps)
    ensures honest(abs(crash(s, Res::Rollback)))
{
    inv_crash(s, Res::Rollback);
}

// PC, unfixed: every crash of a developer-mode load during its dependency phase that changes the module is dishonest.
pub proof fn g_dev_crash_dishonest_unless_fixed(s: L2)
    requires inv(s), s.cfg.mode == Mode::Dev, !s.cfg.fix, s.pc is Deps, s.pre.top != Some(s.cfg.v), s.cfg.caller == Caller::NoCaller
    ensures !honest(abs(crash(s, Res::Rollback)))
{}

// PC, fixed: the same crash is honest.
pub proof fn g_dev_crash_honest_when_fixed(s: L2)
    requires inv(s), s.cfg.mode == Mode::Dev, s.cfg.fix, s.pc is Deps, s.cfg.caller == Caller::NoCaller, s.clean
    ensures honest(abs(crash(s, Res::Rollback)))
{
    inv_crash(s, Res::Rollback);
}

// PD, unfixed: in a transactional load, once dependencies were loaded, an Activate failure always loses their records.
pub proof fn g_dependency_records_lost_unless_fixed(s: L2)
    requires inv(s), s.cfg.mode == Mode::Txn, !s.cfg.fix, s.pc == Pc::Activate, !s.cfg.act_ok, n(s.cfg) > 0,
             s.cfg.caller == Caller::NoCaller
    ensures !honest_strict(abs(fail_end(s))), honest(abs(fail_end(s)))
{
    lemma_level_facts(s, init_db(s));
    inv_activate(s);
    assert(abs(fail_end(s)) == closed_view(fail_end(s)));
    assert(!abs(fail_end(s)).d.dep_rows[0].exists);
}

// PD, fixed: the same failure keeps every dependency record.
pub proof fn g_dependency_records_kept_when_fixed(s: L2)
    requires inv(s), s.cfg.mode == Mode::Txn, s.cfg.fix, s.pc == Pc::Activate, !s.cfg.act_ok, s.cfg.caller == Caller::NoCaller
    ensures honest_strict(abs(fail_end(s)))
{
    lemma_level_facts(s, init_db(s));
    inv_activate(s);
    assert(abs(fail_end(s)) == closed_view(fail_end(s)));
}

// Instance crash, unfixed: once a no-transaction or developer-mode load has written its manifest and no
// transaction protects it, an instance crash leaves the module changed and the record saying Committed = 0.
pub proof fn g_instance_crash_dishonest_unless_fixed(s: L2)
    requires inv(s), !s.cfg.fix, s.cfg.caller == Caller::NoCaller, s.pre.top != Some(s.cfg.v),
             (s.cfg.mode == Mode::NoTxn && (s.pc is Deps || s.pc == Pc::Activate)) || (s.cfg.mode == Mode::Dev && (s.pc is Deps || s.pc == Pc::Activate))
    ensures !honest(abs(crash_k(s, Res::Rollback, Kind::Instance)))
{}

// Instance crash, fixed: honest at every point except the window (P3).
pub proof fn g_instance_crash_honest_when_fixed(s: L2)
    requires inv(s), s.cfg.fix, running(s.pc), s.pc != Pc::Window, s.cfg.caller == Caller::NoCaller, s.clean
    ensures honest(abs(crash_k(s, Res::Rollback, Kind::Instance)))
{
    inv_instance_crash(s);
}

// No in-process design keeps pending dependency rows across an instance crash: after a transactional load that
// already loaded dependencies crashes this way, even reconciliation leaves them without records.
pub proof fn g_instance_crash_loses_pending(s: L2)
    requires inv(s), s.cfg.fix, s.cfg.mode == Mode::Txn, s.pc == Pc::Activate, n(s.cfg) > 0, s.cfg.caller == Caller::NoCaller
    ensures !honest_strict(abs(step_reconcile(crash_k(s, Res::Rollback, Kind::Instance))))
{
    let e = crash_k(s, Res::Rollback, Kind::Instance);
    assert(merge_fix(e.db.dep_rows, e.pend, e.db.deps, e.cfg.vers) =~= e.db.dep_rows);
    assert(!abs(step_reconcile(e)).d.dep_rows[0].exists);
}

// Frame: the command never changes unrelated modules or older history, in any run (no premise).
pub proof fn g_frame_every_step(s: L2, c: Choice)
    requires inv(s), enabled(s, c)
    ensures durable(step(s, c)).other == s.pre.other, durable(step(s, c)).hist == s.pre.hist
{
    g_inv_step(s, c);
    let e = step(s, c);
    if e.snaps.len() > 0 { assert(frame_db(e.snaps[0], e.pre)); }
}

// ---------------------------------------------------------------- reachability witnesses (non-vacuity)
// Concrete runs built step by step from an initial state, with more than one dependency and a non-empty prior
// namespace. Each shows that the invariant chain applies to real multi-dependency runs and that the defect or the
// fix is reached, not just stated.

pub open spec fn cfg2(mode: Mode, outs: Seq<Out>, act_ok: bool, fix: bool) -> Cfg {
    Cfg { mode: mode, v: 2, vers: seq![1int, 1int], outs: outs, act_ok: act_ok, refuse: false, caller: Caller::NoCaller, fix: fix }
}
pub open spec fn pre2() -> Db {
    // dependency 0 already installed at an older version; an unrelated module and an older history row exist
    Db { top: Some(1), deps: seq![Some(0int), None], top_row: no_row(), dep_rows: seq![no_row(), no_row()],
         other: Map::empty().insert(7int, 3int), hist: Map::empty().insert(1int, init_row()) }
}
pub open spec fn start(c: Cfg) -> L2 {
    L2 { db: pre2(), snaps: Seq::empty(), temp: no_row(), pend: seq![no_row(), no_row()], pc: Pc::Init, cfg: c, pre: pre2(), began: 0, clean: true, lost_temp: false }
}

pub proof fn witness_two_deps_dev_crash_unfixed()
    ensures ({
        let s0 = start(cfg2(Mode::Dev, seq![Out::Ok, Out::Crash], true, false));
        let s1 = step(s0, Choice::Init); let s2 = step(s1, Choice::Guard); let s3 = step(s2, Choice::Manifest);
        let s4 = step(s3, Choice::Dep(Res::Rollback)); let s5 = step(s4, Choice::Dep(Res::Rollback));
        &&& initial(s0) && inv(s4) && s5.pc == Pc::Crashed
        &&& abs(s5).d.top == Some(2int) && !abs(s5).d.top_row.committed && !honest(abs(s5))
    })
{
    let s0 = start(cfg2(Mode::Dev, seq![Out::Ok, Out::Crash], true, false));
    assert(s0.pre.dep_rows =~= nothing(2)); assert(s0.pend =~= nothing(2));
    g_inv_initial(s0);
    let s1 = step(s0, Choice::Init); g_inv_step(s0, Choice::Init);
    let s2 = step(s1, Choice::Guard); g_inv_step(s1, Choice::Guard);
    let s3 = step(s2, Choice::Manifest); g_inv_step(s2, Choice::Manifest);
    g_inv_step(s3, Choice::Dep(Res::Rollback));
}

pub proof fn witness_two_deps_dev_crash_fixed()
    ensures ({
        let s0 = start(cfg2(Mode::Dev, seq![Out::Ok, Out::Crash], true, true));
        let s1 = step(s0, Choice::Init); let s2 = step(s1, Choice::Guard); let s3 = step(s2, Choice::Manifest);
        let s4 = step(s3, Choice::Dep(Res::Rollback)); let s5 = step(s4, Choice::Dep(Res::Rollback));
        let s6 = step(s5, Choice::Reconcile);
        &&& initial(s0) && s5.pc == Pc::Crashed && honest(abs(s5)) && s6.pc == Pc::Reconciled && honest_strict(abs(s6))
    })
{
    let s0 = start(cfg2(Mode::Dev, seq![Out::Ok, Out::Crash], true, true));
    assert(s0.pre.dep_rows =~= nothing(2)); assert(s0.pend =~= nothing(2));
    g_inv_initial(s0);
    let s1 = step(s0, Choice::Init); g_inv_step(s0, Choice::Init);
    let s2 = step(s1, Choice::Guard); g_inv_step(s1, Choice::Guard);
    let s3 = step(s2, Choice::Manifest); g_inv_step(s2, Choice::Manifest);
    let s4 = step(s3, Choice::Dep(Res::Rollback)); g_inv_step(s3, Choice::Dep(Res::Rollback));
    let s5 = step(s4, Choice::Dep(Res::Rollback)); g_inv_step(s4, Choice::Dep(Res::Rollback));
    g_fixed_refines_strict(s5, Choice::Reconcile);
}

pub proof fn witness_two_deps_txn_parent_fails()
    ensures ({
        let s0 = start(cfg2(Mode::Txn, seq![Out::Ok, Out::Ok], false, false));
        let s1 = step(s0, Choice::Init); let s2 = step(s1, Choice::Guard); let s3 = step(s2, Choice::Manifest);
        let s4 = step(s3, Choice::Dep(Res::Rollback)); let s5 = step(s4, Choice::Dep(Res::Rollback));
        let s6 = step(s5, Choice::DepsDone); let s7 = step(s6, Choice::Activate);
        &&& initial(s0) && s7.pc == Pc::Closed && honest(abs(s7)) && !honest_strict(abs(s7))
        &&& abs(s7).d.deps == pre2().deps && abs(s7).d.other == pre2().other && abs(s7).d.hist == pre2().hist
    })
{
    let s0 = start(cfg2(Mode::Txn, seq![Out::Ok, Out::Ok], false, false));
    assert(s0.pre.dep_rows =~= nothing(2)); assert(s0.pend =~= nothing(2));
    g_inv_initial(s0);
    let s1 = step(s0, Choice::Init); g_inv_step(s0, Choice::Init);
    let s2 = step(s1, Choice::Guard); g_inv_step(s1, Choice::Guard);
    let s3 = step(s2, Choice::Manifest); g_inv_step(s2, Choice::Manifest);
    let s4 = step(s3, Choice::Dep(Res::Rollback)); g_inv_step(s3, Choice::Dep(Res::Rollback));
    let s5 = step(s4, Choice::Dep(Res::Rollback)); g_inv_step(s4, Choice::Dep(Res::Rollback));
    let s6 = step(s5, Choice::DepsDone); g_inv_step(s5, Choice::DepsDone);
    g_dependency_records_lost_unless_fixed(s6);
}

pub proof fn witness_two_deps_txn_success_strict()
    ensures ({
        let s0 = start(cfg2(Mode::Txn, seq![Out::Ok, Out::Ok], true, false));
        let s1 = step(s0, Choice::Init); let s2 = step(s1, Choice::Guard); let s3 = step(s2, Choice::Manifest);
        let s4 = step(s3, Choice::Dep(Res::Rollback)); let s5 = step(s4, Choice::Dep(Res::Rollback));
        let s6 = step(s5, Choice::DepsDone); let s7 = step(s6, Choice::Activate); let s8 = step(s7, Choice::Window);
        &&& initial(s0) && s7.pc == Pc::Window && s8.pc == Pc::Closed && honest_strict(abs(s8))
        &&& abs(s8).d.deps == seq![Some(1int), Some(1int)] && !honest(abs(crash(s7, Res::Rollback)))
    })
{
    let s0 = start(cfg2(Mode::Txn, seq![Out::Ok, Out::Ok], true, false));
    assert(s0.pre.dep_rows =~= nothing(2)); assert(s0.pend =~= nothing(2));
    g_inv_initial(s0);
    let s1 = step(s0, Choice::Init); g_inv_step(s0, Choice::Init);
    let s2 = step(s1, Choice::Guard); g_inv_step(s1, Choice::Guard);
    let s3 = step(s2, Choice::Manifest); g_inv_step(s2, Choice::Manifest);
    let s4 = step(s3, Choice::Dep(Res::Rollback)); g_inv_step(s3, Choice::Dep(Res::Rollback));
    let s5 = step(s4, Choice::Dep(Res::Rollback)); g_inv_step(s4, Choice::Dep(Res::Rollback));
    let s6 = step(s5, Choice::DepsDone); g_inv_step(s5, Choice::DepsDone);
    let s7 = step(s6, Choice::Activate); g_inv_step(s6, Choice::Activate);
    g_step_refines(s7, Choice::Window, true);
    assert(abs(step(s7, Choice::Window)).d.deps =~= seq![Some(1int), Some(1int)]);
    g_window_crash_dishonest(s7);
}

fn main() {}

} // verus!
