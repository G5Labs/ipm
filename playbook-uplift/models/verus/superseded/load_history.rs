// L1/L2 models of IPM package loading and transaction history (ipm 571e04a1), with refinement proofs.
//
// L2 is the algorithm of Utils/Module.cls LoadNewModule (#L1152-L1364), Main.cls Load (#L2415) and
// HistoryTemp/AbstractHistory over the IRIS environment contract validated in models/explorer
// (journaled globals with nested TSTART levels; IRISTEMP not journaled; %OnClose runs before a halt's
// rollback). One top-level module M, at most one dependency D loaded from a registry, modules absent
// before the command. A dependency's own nested TSTART/TCOMMIT is folded into one step: the explorer
// checks that folding against the fine model (nested levels are never durable on their own).
//
// L1 is the operator-level contract: what the namespace and its history show once the command is over.
//
// Proved:
//   - every L2 step refines an L1 step or a stutter, under premises PC and PD (lemma_refines_*);
//   - the step invariant is inductive (lemma_inv_*);
// Proved defect witnesses (refinement fails without the premise):
//   - witness_dev_crash_commit_flag: without PC, a crash leaves kept changes with Committed = 0;
//   - witness_dependency_record_lost: without PD, a dependency's record disappears.
// Named premises (not proved; each mapped to a test or an IRIS observation in STATUS.md):
//   P1 the command starts at $TLEVEL 0;  P2 a halt with an open transaction rolls back;
//   P3 the outer TCOMMIT and SetCommitted are not separated by a process end.
use vstd::prelude::*;

verus! {

pub struct Row { pub exists: bool, pub named: bool, pub ok: bool, pub committed: bool, pub ended: bool }

pub struct Db { pub m: bool, pub m_cls: bool, pub d: bool, pub top: Row, pub dep: Row }

pub enum Mode { Txn, Dev, NoTxn }

pub enum Pc { Init, Guard, Manifest, Deps, Activate, Closed, Crashed }

pub enum DepOutcome { NoDep, Ok, Fail, Crash }

pub struct L2 {
    pub db: Db,            // namespace database (journaled globals), current values
    pub snap: Option<Db>,  // value at the open TSTART (at most one level open in the parent at a time)
    pub temp: Row,         // the top-level HistoryTemp record in IRISTEMP
    pub pc: Pc,
    pub mode: Mode,
    pub dep_began: bool,   // ghost: a dependency load started
}

pub open spec fn no_row() -> Row { Row { exists: false, named: false, ok: false, committed: false, ended: false } }
pub open spec fn init_row() -> Row { Row { exists: true, named: false, ok: false, committed: false, ended: false } }
pub open spec fn empty_db() -> Db { Db { m: false, m_cls: false, d: false, top: no_row(), dep: no_row() } }

pub open spec fn initial(s: L2) -> bool {
    s.db == empty_db() && s.snap is None && s.temp == no_row() && s.pc == Pc::Init && !s.dep_began
}

pub open spec fn uses_txn(m: Mode) -> bool { m != Mode::NoTxn }

// ---------------------------------------------------------------- L2 steps (one per code region)

// Main.Load: LoadInit saves the IRISTEMP log and its twin row at $TLEVEL 0; LoadNewModule then TSTARTs.
pub open spec fn step_init(s: L2) -> L2 {
    let db = Db { top: init_row(), ..s.db };
    L2 { db: db, snap: if uses_txn(s.mode) { Some(db) } else { None }, temp: init_row(), pc: Pc::Guard, ..s }
}

// Close the log: %OnClose copies the IRISTEMP record into the twin at the current level.
pub open spec fn close(db: Db, temp: Row) -> Db { Db { top: temp, ..db } }

// Failure path: catch, roll back to the initial level, Finalize(error), %OnClose.
pub open spec fn fail_end(s: L2) -> L2 {
    let rolled = match s.snap { Some(d) => d, None => s.db };
    let t = Row { ok: false, ended: true, ..s.temp };
    L2 { db: close(rolled, t), snap: None, temp: t, pc: Pc::Closed, ..s }
}

// Success path: Finalize(OK) sets Committed || OK; %OnClose at level 0.
pub open spec fn ok_end(s: L2) -> L2 {
    let t = Row { ok: true, committed: true, ended: true, ..s.temp };
    L2 { db: close(s.db, t), snap: None, temp: t, pc: Pc::Closed, ..s }
}

// Downgrade or update-package refusal (#L1213-L1222); without a transaction SetCommitted follows (#L1226).
pub open spec fn step_guard(s: L2, refuse: bool) -> L2 {
    if refuse { fail_end(s) }
    else {
        let t = if s.mode == Mode::NoTxn { Row { committed: true, ..s.temp } } else { s.temp };
        L2 { temp: t, pc: Pc::Manifest, ..s }
    }
}

// OBJ.Load of module.xml (#L1251), SetName (#L1293), developer-mode save (#L1303),
// developer-mode TCOMMIT + SetCommitted + TSTART (#L1309-L1319).
pub open spec fn step_manifest(s: L2) -> L2 {
    let db = Db { m: true, ..s.db };
    let t = Row { named: true, ..s.temp };
    if s.mode == Mode::Dev {
        L2 { db: db, snap: Some(db), temp: Row { committed: true, ..t }, pc: Pc::Deps, ..s }
    } else {
        L2 { db: db, temp: t, pc: Pc::Deps, ..s }
    }
}

// The dependency's own log row as %OnClose leaves it at the parent's current level.
// Without a transaction (NoTransaction propagates to dependencies) the dependency marks itself committed early.
pub open spec fn dep_row(ok: bool, ended: bool) -> Row { Row { exists: true, named: true, ok: ok, committed: ok, ended: ended } }
pub open spec fn dep_row_m(mode: Mode, ok: bool, ended: bool) -> Row {
    Row { exists: true, named: true, ok: ok, committed: ok || mode == Mode::NoTxn, ended: ended }
}

// Abnormal end of the process: %OnClose first (at the open level), then the halt's rollback (P2).
pub open spec fn crash(s: L2) -> L2 {
    let closed = close(s.db, s.temp);
    let rolled = match s.snap { Some(d) => d, None => closed };
    L2 { db: rolled, snap: None, pc: Pc::Crashed, ..s }
}

// LoadDependencies (#L1328) then the developer-mode TCOMMIT (#L1335-L1339).
pub open spec fn step_deps(s: L2, o: DepOutcome) -> L2 {
    match o {
        DepOutcome::NoDep => {
            let s1 = L2 { pc: Pc::Activate, ..s };
            if s.mode == Mode::Dev { L2 { snap: None, ..s1 } } else { s1 }
        },
        DepOutcome::Ok => {
            let s1 = L2 { db: Db { d: true, dep: dep_row(true, true), ..s.db }, dep_began: true, pc: Pc::Activate, ..s };
            if s.mode == Mode::Dev { L2 { snap: None, ..s1 } } else { s1 }
        },
        // A failed dependency rolls back its own nested level; without a transaction its writes stay.
        DepOutcome::Fail => fail_end(L2 { db: Db { d: s.mode == Mode::NoTxn, dep: dep_row_m(s.mode, false, true), ..s.db }, dep_began: true, ..s }),
        DepOutcome::Crash => crash(L2 { db: Db { d: s.mode == Mode::NoTxn, dep: dep_row_m(s.mode, false, false), ..s.db }, dep_began: true, ..s }),
    }
}

// Compile and Activate (#L1346); on success a transactional load commits and marks the log (#L1347-L1351, P3).
pub open spec fn step_activate(s: L2, ok: bool) -> L2 {
    let s1 = L2 { db: Db { m_cls: true, ..s.db }, ..s };
    if ok {
        if s.mode == Mode::Txn { ok_end(L2 { snap: None, temp: Row { committed: true, ..s1.temp }, ..s1 }) } else { ok_end(s1) }
    } else { fail_end(s1) }
}

pub enum Choice { Init, Guard(bool), Manifest, Deps(DepOutcome), Activate(bool), Crash }

pub open spec fn enabled(s: L2, c: Choice) -> bool {
    match c {
        Choice::Init => s.pc == Pc::Init,
        Choice::Guard(_) => s.pc == Pc::Guard,
        Choice::Manifest => s.pc == Pc::Manifest,
        Choice::Deps(_) => s.pc == Pc::Deps,
        Choice::Activate(_) => s.pc == Pc::Activate,
        Choice::Crash => s.pc == Pc::Guard || s.pc == Pc::Manifest || s.pc == Pc::Deps || s.pc == Pc::Activate,
    }
}

pub open spec fn step(s: L2, c: Choice) -> L2 {
    match c {
        Choice::Init => step_init(s),
        Choice::Guard(r) => step_guard(s, r),
        Choice::Manifest => step_manifest(s),
        Choice::Deps(o) => step_deps(s, o),
        Choice::Activate(ok) => step_activate(s, ok),
        Choice::Crash => crash(s),
    }
}

// ---------------------------------------------------------------- premises on steps

// PC: no abnormal end while a developer-mode load has its second transaction open
// (after the manifest commit at #L1311, before the dependency commit at #L1338).
pub open spec fn premise_pc(s: L2, c: Choice) -> bool {
    !(s.mode == Mode::Dev && s.pc == Pc::Deps &&
      ((c == Choice::Crash) || (c == Choice::Deps(DepOutcome::Crash))))
}

// PD: no dependency is loaded inside a transaction that is later rolled back.
pub open spec fn premise_pd(s: L2, c: Choice) -> bool {
    !(uses_txn(s.mode) && s.snap is Some && (
        c == Choice::Deps(DepOutcome::Fail) || c == Choice::Deps(DepOutcome::Crash) ||
        (s.dep_began && (c == Choice::Crash || c == Choice::Activate(false)))))
}

// ---------------------------------------------------------------- L1

pub enum Phase { NotStarted, Running, Closed, Crashed }

pub struct L1 { pub m: bool, pub d: bool, pub top: Row, pub dep: Row, pub phase: Phase, pub dep_began: bool, pub mode: Mode }

pub open spec fn durable(s: L2) -> Db { match s.snap { Some(d) => d, None => s.db } }

pub open spec fn phase_of(pc: Pc) -> Phase {
    match pc { Pc::Init => Phase::NotStarted, Pc::Closed => Phase::Closed, Pc::Crashed => Phase::Crashed, _ => Phase::Running }
}

pub open spec fn abs(s: L2) -> L1 {
    let d = durable(s);
    L1 { m: d.m, d: d.d, top: d.top, dep: d.dep, phase: phase_of(s.pc), dep_began: s.dep_began, mode: s.mode }
}

// The record an operator reads once the command is over is honest.
pub open spec fn honest_end(a: L1) -> bool {
    &&& a.top.exists
    &&& (!a.top.committed ==> !a.m && !a.d)                       // Committed = 0: nothing kept
    &&& (a.top.committed && a.mode != Mode::NoTxn ==> a.m || a.d)   // Committed = 1: something kept
    &&& (a.top.ok ==> a.top.committed && a.m && (a.dep_began ==> a.d))
    &&& (a.mode == Mode::Txn && !a.top.ok ==> !a.m && !a.d)        // transactional failure is atomic
    &&& (a.phase == Phase::Closed <==> a.top.ended)
    &&& (a.d ==> a.dep.exists)                                       // a kept dependency has its record
}

// Strict variant: every dependency load that started leaves a record (the class comment's intent).
pub open spec fn honest_end_strict(a: L1) -> bool { honest_end(a) && (a.dep_began ==> a.dep.exists) }

pub open spec fn keep_allowed(a: L1, b: L1) -> bool {
    if a.mode == Mode::Txn { b.m == a.m && b.d == a.d } else { (a.m ==> b.m) && (a.d ==> b.d) }
}

pub open spec fn l1_next(a: L1, b: L1) -> bool {
    ||| a == b
    ||| (a.phase == Phase::NotStarted && b.phase == Phase::Running && b.top == init_row() && !b.m && !b.d
         && b.dep == a.dep && b.mode == a.mode && !b.dep_began)
    ||| (a.phase == Phase::Running && b.phase == Phase::Running && b.top == a.top && b.mode == a.mode
         && (b.dep == a.dep || !a.dep.exists) && (a.dep_began ==> b.dep_began) && keep_allowed(a, b))
    ||| (a.phase == Phase::Running && (b.phase == Phase::Closed || b.phase == Phase::Crashed) && b.mode == a.mode
         && (a.dep_began ==> b.dep_began) && honest_end(b))
}

pub open spec fn l1_next_strict(a: L1, b: L1) -> bool {
    l1_next(a, b) && ((b.phase == Phase::Closed || b.phase == Phase::Crashed) && a.phase == Phase::Running ==> honest_end_strict(b))
}

// ---------------------------------------------------------------- invariant

pub open spec fn guard_db() -> Db { Db { top: init_row(), ..empty_db() } }

pub open spec fn inv(s: L2) -> bool {
    match s.pc {
        Pc::Init => s.db == empty_db() && s.snap is None && !s.dep_began,
        Pc::Guard => s.db == guard_db() && s.temp == init_row() && !s.dep_began
            && s.snap == (if uses_txn(s.mode) { Some(guard_db()) } else { None }),
        Pc::Manifest => s.db == guard_db() && !s.dep_began && !s.temp.named && s.temp.exists && !s.temp.ok && !s.temp.ended
            && s.temp.committed == (s.mode == Mode::NoTxn)
            && s.snap == (if uses_txn(s.mode) { Some(guard_db()) } else { None }),
        Pc::Deps => s.db == Db { m: true, ..guard_db() } && !s.dep_began
            && s.temp == Row { named: true, committed: s.mode != Mode::Txn, ..init_row() }
            && s.snap == (match s.mode { Mode::Txn => Some(guard_db()), Mode::Dev => Some(Db { m: true, ..guard_db() }), Mode::NoTxn => None }),
        Pc::Activate => s.db.m && !s.db.m_cls && s.db.top == init_row()
            && s.temp == Row { named: true, committed: s.mode != Mode::Txn, ..init_row() }
            && (s.dep_began <==> s.db.d) && (s.dep_began ==> s.db.dep == dep_row(true, true)) && (!s.dep_began ==> s.db.dep == no_row())
            && s.snap == (if s.mode == Mode::Txn { Some(guard_db()) } else { None }),
        Pc::Closed => s.snap is None,
        Pc::Crashed => s.snap is None,
    }
}

proof fn lemma_inv_initial(s: L2)
    requires initial(s)
    ensures inv(s)
{}

proof fn lemma_inv_step(s: L2, c: Choice)
    requires inv(s), enabled(s, c)
    ensures inv(step(s, c))
{}

// ---------------------------------------------------------------- refinement

pub proof fn lemma_refines(s: L2, c: Choice)
    requires inv(s), enabled(s, c), premise_pc(s, c), premise_pd(s, c)
    ensures l1_next(abs(s), abs(step(s, c)))
{}

pub proof fn lemma_refines_strict(s: L2, c: Choice)
    requires inv(s), enabled(s, c), premise_pc(s, c), premise_pd(s, c)
    ensures l1_next_strict(abs(s), abs(step(s, c)))
{}

// Without PD, the weak contract still holds: a dependency's record is lost only when its changes were undone.
pub proof fn lemma_refines_without_pd(s: L2, c: Choice)
    requires inv(s), enabled(s, c), premise_pc(s, c)
    ensures l1_next(abs(s), abs(step(s, c)))
{}

// No premise is needed for a transactional load: PC only concerns developer mode.
pub proof fn lemma_refines_txn_mode(s: L2, c: Choice)
    requires inv(s), enabled(s, c), s.mode == Mode::Txn
    ensures l1_next(abs(s), abs(step(s, c)))
{}

pub proof fn lemma_refines_notxn_mode(s: L2, c: Choice)
    requires inv(s), enabled(s, c), s.mode == Mode::NoTxn
    ensures l1_next_strict(abs(s), abs(step(s, c)))
{}

// ---------------------------------------------------------------- defect witnesses (proved)

pub open spec fn s0(mode: Mode) -> L2 {
    L2 { db: empty_db(), snap: None, temp: no_row(), pc: Pc::Init, mode: mode, dep_began: false }
}

// Developer mode: init, guard, manifest (commits the manifest), then the process ends while loading
// dependencies. The module stays installed but the record says Committed = 0 (IRIS: ipm-halt-dev-crash-in-dependency).
pub proof fn witness_dev_crash_commit_flag()
    ensures ({
        let s1 = step(s0(Mode::Dev), Choice::Init);
        let s2 = step(s1, Choice::Guard(false));
        let s3 = step(s2, Choice::Manifest);
        let s4 = step(s3, Choice::Crash);
        &&& inv(s3) && enabled(s3, Choice::Crash) && !premise_pc(s3, Choice::Crash)
        &&& abs(s4).m && !abs(s4).top.committed
        &&& !l1_next(abs(s3), abs(s4))
    })
{}

// Transactional mode: the dependency loads, then the parent's Activate fails and the parent rolls back.
// The dependency's record is gone (IRIS: ipm-dep-parent-fails).
pub proof fn witness_dependency_record_lost()
    ensures ({
        let s1 = step(s0(Mode::Txn), Choice::Init);
        let s2 = step(s1, Choice::Guard(false));
        let s3 = step(s2, Choice::Manifest);
        let s4 = step(s3, Choice::Deps(DepOutcome::Ok));
        let s5 = step(s4, Choice::Activate(false));
        &&& inv(s4) && !premise_pd(s4, Choice::Activate(false))
        &&& abs(s5).dep_began && !abs(s5).dep.exists
        &&& l1_next(abs(s4), abs(s5)) && !l1_next_strict(abs(s4), abs(s5))
    })
{}

// The refused developer-mode load (IRIS: ipm-dev-downgrade-refused) and the plain failure both refine L1.
pub proof fn witness_refusal_is_clean()
    ensures ({
        let s1 = step(s0(Mode::Dev), Choice::Init);
        let s2 = step(s1, Choice::Guard(true));
        &&& !abs(s2).m && !abs(s2).top.committed && abs(s2).top.ended && !abs(s2).top.named
        &&& l1_next_strict(abs(s1), abs(s2))
    })
{}

fn main() {}

} // verus!
