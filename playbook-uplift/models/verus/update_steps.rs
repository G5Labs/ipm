// L1/L2 models of IPM's install/update guard, step seeding and update-step execution (ipm 571e04a1),
// with per-operation refinement proofs, an inductive invariant and proved defect witnesses.
//
// L2 is one LoadNewModule run (Utils/Module.cls#L1152-L1364) for one module whose new manifest names an update
// package, over the IRIS contract of models/explorer/irisenv.py: journaled globals rolled back to the level the
// command started at (P1: it starts at $TLEVEL 0; P2: a halt with an open transaction rolls back), effects outside
// the database are never undone, and `lock +^IPM.UpdateLock(name):3` either succeeds or times out (a
// nondeterministic choice; observed: nc-inst-3-update-lock-busy). Step records live in journaled globals
// (^IPM.UpdateStep.AnyMemberD / PrimaryOnlyD, unique on class + method), so they follow the transaction.
//
// The update package is given flat: `steps` lists the (class number, list, method) keys in the order the code walks
// them: classes by ascending number ($order over GetUpdatePackageClassList, VersionBase.cls#L34-L60; probe
// nc-inst-model-class-order), in each class the all-member list then the primary-only list (Storage/Module.cls
// #L131-L138), each list in the author's order (#L181). Arbitrary length; no bound on classes or steps.
//
// L1 is the operator-level contract of modules 03 (INST): what the namespace shows once the command is over. The
// reported outcome is two-way: a command ends OK only when no phase and no step it ran threw, and ends Failed only when
// one did (`fault`, a ghost record of the environment's failure choice); a crash never reports success. The developer
// mode of a run is the operator's -dev / -nodev when typed, else the mode stored on the installed module
// (Utils/Module.cls#L1298-L1307; LOAD-B21, DEC-LOAD-2); seeding tests the stored mode (Lifecycle/Module.cls#L122).
//
// Proved (no assume/admit/external_body):
//   - the invariant is inductive and every L2 step refines an L1 step or a stutter under premises PL and PF5
//     (inst_refines and one lemma per step function, each revealing only its own step);
//   - an update that ends OK ran exactly the scheduled steps, in walk order, and left every step it may run done;
//   - after any interruption, an update whose steps succeed finishes them (lock granted).
// Proved defect witnesses:
//   - inst_witness_busy_lock_reports_success (F-INST-1): without PL, update reports OK and runs nothing;
//   - inst_witness_dev_update_from_registry_rolls_back (F-INST-5): without PF5, `update -dev` from a registry is undone;
//   - inst_witness_dev_install_leaves_steps_due (DEC-INST-2): a dev install seeds nothing, so a later update runs
//     steps of older classes;
//   - inst_witness_failed_update_repeats_effects (DEC-INST-5): a rolled-back update reruns steps that worked;
//   - inst_witness_helper_class_hides_steps (F-INST-6) and inst_witness_class_without_number_fails (F-INST-7): other
//     classes of the update package whose names start with V break the class list (VersionBase.cls#L34-L60).
// Named premises:
//   PL  the update lock is granted (Storage/Module.cls#L105-L108 returns OK when it is not: F-INST-1);
//   PF5 the run's developer mode is the one the operator asked for, or the stored one when none is typed. It can fail
//       only for registry loads (Utils/Module.cls#L128-L136, #L307-L310; Main.cls#L2558: F-INST-5);
//   P1, P2 as in load_history.rs (IRIS transaction semantics, not spec functions).
// PREMISE premise_pl = PL
// PREMISE premise_pf5 = PF5
use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------- data

// A step record key: class number (V<c>), primary-only table or not, method name.
pub struct Key { pub c: nat, pub prim: bool, pub m: nat }

// A step record: TimeStampStart set, TimeStampEnd set, Status OK.
pub struct Rec { pub started: bool, pub end: bool, pub ok: bool }

// Journaled state of the namespace for this module: installed version, developer mode, step records, and the
// journaled effects of the steps that ran (ghost; rolled back with the transaction).
pub struct Db { pub ver: Option<nat>, pub dev: bool, pub recs: Map<Key, Rec>, pub jlog: Seq<Key> }

pub enum Kind { Install, Load, Update }

pub enum Source { Path, Registry }

pub struct Op {
    pub kind: Kind,
    pub force: bool,
    pub dev: Option<bool>, // the DeveloperMode the operator typed: -dev Some(true), -nodev Some(false), neither None
    pub txn: bool,        // not -DNoTransaction / -DNoJournal
    pub top: bool,        // the module the operator named (pParams("Install") / Main:Update checks)
    pub source: Source,   // -path or a folder (Path), or a registry
    pub snapshot: bool,   // the target version is a +snapshot
    pub to: nat,          // target version
    pub has_pkg: bool,    // the new manifest names an update package
    pub skip_prim: bool,  // mirror member that is not the primary (Storage/Module.cls#L228)
}

pub enum Pc { Init, Guard, Manifest, Phases, Lock, Walk, Closed, Crashed }

pub enum Out { Running, Ok, Refused, Failed }

pub struct L2 {
    pub db: Db,           // current values
    pub snap: Option<Db>, // value at the open TSTART (none open: None)
    pub pre: Db,          // ghost: the namespace when the command started
    pub pc: Pc,
    pub op: Op,
    pub steps: Seq<Key>,  // the new version's update package, in walk order
    pub seeding: bool,    // the walk seeds (Activate) rather than runs (ApplyUpdateSteps)
    pub i: nat,           // walk cursor
    pub flag: bool,       // newStepToApply
    pub log: Seq<Key>,    // ghost: every step method that ran, in order (effects outside the database stay)
    pub fault: bool,      // ghost: a phase, or a step that ran, threw (the environment's failure choice)
    pub out: Out,
}

// ---------------------------------------------------------------- records

pub open spec fn rec(recs: Map<Key, Rec>, k: Key) -> Option<Rec> {
    if recs.contains_key(k) { Some(recs[k]) } else { None }
}

// L1 (INST-B55, B60, B63): a step is due when it never ran or was seeded, or failed last time.
pub open spec fn due(o: Option<Rec>) -> bool {
    match o { Option::None => true, Option::Some(x) => !x.end || !x.ok }
}

// L2, Storage/Module.cls#L190: the code's test 'stepObj.TimeStampEnd || $$$ISERR(stepObj.Status).
pub open spec fn run_needed(x: Rec) -> bool { !x.end || !x.ok }

pub open spec fn done(o: Option<Rec>) -> bool { !due(o) }

// GetAddUpdateStep (#L258-L272): open the record or create it (Status = $$$OK, no time stamps) and save it.
pub open spec fn visit(o: Option<Rec>) -> Rec {
    match o { Option::None => Rec { started: false, end: false, ok: true }, Option::Some(x) => x }
}

// SeedUpdateStep (#L203-L220).
pub open spec fn seeded(x: Rec) -> Rec { if !x.end || !x.ok { Rec { end: true, ok: true, ..x } } else { x } }

// ExecuteUpdateStep (#L224-L250): start stamp, run, end stamp and status, save.
pub open spec fn ran(ok: bool) -> Rec { Rec { started: true, end: true, ok: ok } }

// The flat walk order: distinct keys, classes ascending, all-member list before primary-only list in a class.
pub open spec fn wf(steps: Seq<Key>) -> bool {
    forall|a: int, b: int| #![trigger steps[a], steps[b]] 0 <= a < b < steps.len() ==>
        steps[a] != steps[b] && steps[a].c <= steps[b].c && (steps[a].c == steps[b].c && steps[a].prim ==> steps[b].prim)
}

pub open spec fn cstart(steps: Seq<Key>, i: int) -> bool { i == 0 || steps[i - 1].c != steps[i].c }

pub open spec fn due_at(r: Map<Key, Rec>, steps: Seq<Key>, j: int) -> bool { due(rec(r, steps[j])) }

pub open spec fn skipped(op: Op, k: Key) -> bool { k.prim && op.skip_prim }

// ---------------------------------------------------------------- L1 schedule (declarative)

// Step i runs in an update when it is not skipped on this mirror member and some step of its class, at or before
// it, was due when the update started (INST-B55, B60-B63).
pub open spec fn sched(r: Map<Key, Rec>, steps: Seq<Key>, op: Op, i: int) -> bool {
    !skipped(op, steps[i])
    && exists|j: int| #![trigger due_at(r, steps, j)] 0 <= j <= i && steps[j].c == steps[i].c && due_at(r, steps, j)
}

// The scheduled steps among steps[0..n), in walk order.
pub open spec fn sched_upto(r: Map<Key, Rec>, steps: Seq<Key>, op: Op, n: nat) -> Seq<Key>
    decreases n
{
    if n == 0 { Seq::<Key>::empty() } else {
        let p = sched_upto(r, steps, op, (n - 1) as nat);
        if sched(r, steps, op, n - 1) { p.push(steps[n - 1]) } else { p }
    }
}

// ---------------------------------------------------------------- L2 (one function per code region)

// The DeveloperMode parameter LoadNewModule receives. Main:Install always sets it, to 1 with -dev and 0 without
// (Main.cls#L2558, #L2623); LoadModuleReference sets 0 for a registry unless snapshot and -dev (#L128-L136), and
// LoadModuleFromDirectory applies that to every load but a top-level install (#L307-L310). `load` and `update -path`
// (Main:LoadInternal) pass it only when -dev or -nodev is typed.
pub open spec fn param_dev(op: Op) -> Option<bool> {
    match op.source {
        Source::Path => op.dev,
        Source::Registry => Option::Some(
            if op.kind == Kind::Install && op.top { op.dev == Option::Some(true) }
            else { op.snapshot && op.dev == Option::Some(true) }),
    }
}

// The install/load guard reads $get(params("DeveloperMode"),0) (#L1219): the parameter only, never the stored mode.
pub open spec fn dev_guard(op: Op) -> bool { param_dev(op) == Option::Some(true) }

// The run's developer mode (#L1298-L1307): the parameter when set, else the DeveloperMode of the record the manifest
// load leaves, which keeps the installed module's mode (LOAD-B21); a module not installed has none.
pub open spec fn dev_run(op: Op, d: Db) -> bool {
    match param_dev(op) { Option::Some(x) => x, Option::None => d.ver is Some && d.dev }
}

pub open spec fn older(from: Option<nat>, to: nat) -> bool { match from { Option::Some(f) => f > to, Option::None => false } }

pub open spec fn newer(from: Option<nat>, to: nat) -> bool { match from { Option::Some(f) => to > f, Option::None => false } }

// Main.cls#L4423 (update of a module not installed), Utils/Module.cls#L1213-L1216 (downgrade),
// #L1218-L1222 (install/load of a newer version with an update package, without -force or developer mode).
pub open spec fn refuses(op: Op, dev: bool, from: Option<nat>) -> bool {
    ||| (op.kind == Kind::Update && op.top && from is None)
    ||| older(from, op.to)
    ||| ((op.kind == Kind::Install || op.kind == Kind::Load) && !op.force && !dev && newer(from, op.to) && op.has_pkg)
}

pub open spec fn durable(s: L2) -> Db { match s.snap { Option::Some(d) => d, Option::None => s.db } }

// OK: TCOMMIT if a transaction is open (#L1347-L1351). Error: TROLLBACK to the initial level (#L1356-L1361).
pub open spec fn finish(s: L2, out: Out) -> L2 {
    let d = if out == Out::Ok { s.db } else { durable(s) };
    L2 { db: d, snap: Option::None, pc: Pc::Closed, out: out, ..s }
}

// TSTART unless NoTransaction/NoJournal (#L1195).
#[verifier::opaque]
pub open spec fn step_init(s: L2) -> L2 {
    L2 { pre: s.db, snap: if s.op.txn { Option::Some(s.db) } else { Option::None }, pc: Pc::Guard,
         log: Seq::<Key>::empty(), fault: false, out: Out::Running, ..s }
}

#[verifier::opaque]
pub open spec fn step_guard(s: L2) -> L2 {
    if refuses(s.op, dev_guard(s.op), s.db.ver) { finish(s, Out::Refused) } else { L2 { pc: Pc::Manifest, ..s } }
}

// OBJ.Load of module.xml (#L1251) with DeveloperMode (#L1298-L1307); in developer mode TCOMMIT, TSTART (#L1309-L1319),
// dependencies, TCOMMIT (#L1335-L1339): the phases then run outside a transaction.
#[verifier::opaque]
pub open spec fn step_manifest(s: L2) -> L2 {
    L2 { db: Db { ver: Option::Some(s.op.to), dev: dev_run(s.op, s.db), ..s.db },
         snap: if dev_run(s.op, s.db) { Option::None } else { s.snap }, pc: Pc::Phases, ..s }
}

// Initialize .. Activate (#L1345-L1346). Activate seeds unless Update or the module record is in developer mode
// (`..Module.DeveloperMode`, Lifecycle/Module.cls#L122-L127); an update goes on to ApplyUpdateSteps
// (Lifecycle/Base.cls#L1923-L1936). A phase fails by throwing (or returning an error status); the command then ends.
#[verifier::opaque]
pub open spec fn step_phases(s: L2, ok: bool) -> L2 {
    if !ok { finish(L2 { fault: true, ..s }, Out::Failed) }
    else if s.op.kind == Kind::Update { L2 { pc: Pc::Lock, ..s } }
    else if !s.db.dev { L2 { pc: Pc::Walk, seeding: true, i: 0, flag: false, ..s } }
    else { finish(s, Out::Ok) }
}

// $$$LockUpdateModule with a 3 s timeout; on timeout `return sc` with sc = $$$OK (Storage/Module.cls#L105-L108).
#[verifier::opaque]
pub open spec fn step_lock(s: L2, granted: bool) -> L2 {
    if !granted { finish(s, Out::Ok) }
    else { L2 { pc: Pc::Walk, seeding: false, i: 0, flag: false, ..s } }
}

// One iteration of HandleUpdateStepsFromList (#L178-L197) or the end of HandleAllUpdateSteps (#L156-L164).
// newStepToApply is reset at each class (#L130). A step fails only by throwing: ExecuteUpdateStep calls it with
// `do $classmethod(...)` and discards any return value (#L237-L242); the exception ends the walk.
#[verifier::opaque]
pub open spec fn step_walk(s: L2, ok: bool) -> L2 {
    if s.i >= s.steps.len() { finish(s, Out::Ok) }
    else {
        let k = s.steps[s.i as int];
        let v = visit(rec(s.db.recs, k));
        if s.seeding {
            L2 { db: Db { recs: s.db.recs.insert(k, seeded(v)), ..s.db }, i: s.i + 1, ..s }
        } else {
            let f = (!cstart(s.steps, s.i as int) && s.flag) || run_needed(v);
            if f && !skipped(s.op, k) {
                if ok {
                    L2 { db: Db { recs: s.db.recs.insert(k, ran(true)), jlog: s.db.jlog.push(k), ..s.db },
                         i: s.i + 1, flag: f, log: s.log.push(k), ..s }
                } else {
                    finish(L2 { db: Db { recs: s.db.recs.insert(k, ran(false)), ..s.db }, log: s.log.push(k), fault: true, ..s }, Out::Failed)
                }
            } else {
                L2 { db: Db { recs: s.db.recs.insert(k, v), ..s.db }, i: s.i + 1, flag: f, ..s }
            }
        }
    }
}

// Abnormal end of the process: the open transaction rolls back (P2).
#[verifier::opaque]
pub open spec fn step_crash(s: L2) -> L2 { L2 { db: durable(s), snap: Option::None, pc: Pc::Crashed, ..s } }

pub enum Choice { Init, Guard, Manifest, Phases(bool), Lock(bool), Walk(bool), Crash }

pub open spec fn running(pc: Pc) -> bool {
    pc == Pc::Guard || pc == Pc::Manifest || pc == Pc::Phases || pc == Pc::Lock || pc == Pc::Walk
}

pub open spec fn enabled(s: L2, c: Choice) -> bool {
    match c {
        Choice::Init => s.pc == Pc::Init,
        Choice::Guard => s.pc == Pc::Guard,
        Choice::Manifest => s.pc == Pc::Manifest,
        Choice::Phases(_) => s.pc == Pc::Phases,
        Choice::Lock(_) => s.pc == Pc::Lock,
        Choice::Walk(_) => s.pc == Pc::Walk,
        Choice::Crash => running(s.pc),
    }
}

pub open spec fn step(s: L2, c: Choice) -> L2 {
    match c {
        Choice::Init => step_init(s),
        Choice::Guard => step_guard(s),
        Choice::Manifest => step_manifest(s),
        Choice::Phases(ok) => step_phases(s, ok),
        Choice::Lock(g) => step_lock(s, g),
        Choice::Walk(ok) => step_walk(s, ok),
        Choice::Crash => step_crash(s),
    }
}

pub open spec fn initial(s: L2) -> bool { s.pc == Pc::Init && s.snap is None && s.pre == s.db }

// ---------------------------------------------------------------- premises

pub open spec fn premise_pl(c: Choice) -> bool { c != Choice::Lock(false) }

// PF5, stated on the inputs: a load from a folder or -path always qualifies; a registry load qualifies unless it is
// `-dev` for a non-snapshot version other than a top-level install, or it types no -dev while the installed module is
// in developer mode (Main:Install then passes DeveloperMode=0; F-INST-5. A top-level `install` of such a module is
// refused earlier by the developer-mode guard, Utils/Module.cls#L109, which is not modelled).
pub open spec fn premise_pf5(s: L2) -> bool {
    ||| s.op.source == Source::Path
    ||| ((s.op.dev == Option::Some(true) ==> (s.op.kind == Kind::Install && s.op.top) || s.op.snapshot)
         && (s.op.dev is None ==> !stored_dev(s.pre)))
}

// ---------------------------------------------------------------- L1

// The developer mode stored on the installed module (INST-D2).
pub open spec fn stored_dev(d: Db) -> bool { d.ver is Some && d.dev }

// INST-D2, LOAD-B21 (DEC-LOAD-2): the developer mode of a command is the one typed (-dev, -nodev), else the stored one.
pub open spec fn want_dev(op: Op, pre: Db) -> bool {
    match op.dev { Option::Some(x) => x, Option::None => stored_dev(pre) }
}

pub open spec fn typed_dev(op: Op) -> bool { op.dev == Option::Some(true) }

// INST-B24, B26 (no move to an older version, whatever the flags), B27, B28, B30 (install or load of a newer version
// that names an update package needs -force or a typed -dev), B45 (update of a module that is not installed).
pub open spec fn blocked(op: Op, from: Option<nat>) -> bool {
    match from {
        Option::None => op.kind == Kind::Update && op.top,
        Option::Some(f) => f > op.to || (op.kind != Kind::Update && op.to > f && op.has_pkg && !op.force && !typed_dev(op)),
    }
}

pub enum Phase { NotStarted, Running, Closed, Crashed }

pub struct L1 {
    pub d: Db, pub pre: Db, pub log: Seq<Key>, pub phase: Phase, pub out: Out, pub op: Op, pub steps: Seq<Key>,
    pub fault: bool,  // a phase or a step that ran threw
}

pub open spec fn phase_of(pc: Pc) -> Phase {
    match pc { Pc::Init => Phase::NotStarted, Pc::Closed => Phase::Closed, Pc::Crashed => Phase::Crashed, _ => Phase::Running }
}

pub open spec fn abs(s: L2) -> L1 {
    L1 { d: durable(s), pre: s.pre, log: s.log, phase: phase_of(s.pc), out: s.out, op: s.op, steps: s.steps, fault: s.fault }
}

// The steps that ran are the scheduled steps before some point, in order (INST-B64: nothing after a failure).
pub open spec fn log_is(a: L1, m: nat) -> bool { m <= a.steps.len() && a.log == sched_upto(a.pre.recs, a.steps, a.op, m) }

pub open spec fn sched_prefix(a: L1) -> bool { exists|m: nat| #![trigger log_is(a, m)] log_is(a, m) }

// No command moves a module to an older version (INST-B24).
pub open spec fn ver_ok(a: L1) -> bool {
    a.d.ver == a.pre.ver || (a.d.ver == Option::Some(a.op.to) && match a.pre.ver { Option::Some(f) => f <= a.op.to, Option::None => true })
}

pub open spec fn run_ok(a: L1) -> bool {
    &&& ver_ok(a)
    &&& (a.op.txn && !want_dev(a.op, a.pre) ==> a.d == a.pre)     // INST-B14, B65, B66
    &&& sched_prefix(a)
    &&& (a.op.kind != Kind::Update ==> a.log == Seq::<Key>::empty()) // install and load run no step
}

// OK: no phase or step threw (the command worked), and the namespace shows the work done.
pub open spec fn end_ok(a: L1) -> bool {
    &&& !a.fault
    &&& a.d.ver == Option::Some(a.op.to) && a.d.dev == want_dev(a.op, a.pre)
    &&& !blocked(a.op, a.pre.ver)
    &&& if a.op.kind == Kind::Update {
            // INST-B44, B71: every step that was due ran; every step this member may run is now done
            &&& a.log == sched_upto(a.pre.recs, a.steps, a.op, a.steps.len())
            &&& forall|j: int| #![trigger a.steps[j]] 0 <= j < a.steps.len() && !skipped(a.op, a.steps[j]) ==> done(rec(a.d.recs, a.steps[j]))
        } else if !want_dev(a.op, a.pre) {
            // INST-B17, B29: the steps of the version put in place are seeded, none runs
            &&& a.log == Seq::<Key>::empty()
            &&& forall|j: int| #![trigger a.steps[j]] 0 <= j < a.steps.len() ==> done(rec(a.d.recs, a.steps[j]))
        } else {
            // INST-B18 (DEC-INST-2): developer mode leaves the records as they were
            a.log == Seq::<Key>::empty() && a.d.recs == a.pre.recs
        }
}

pub open spec fn end_refused(a: L1) -> bool {
    !a.fault && blocked(a.op, a.pre.ver) && a.d == a.pre && a.log == Seq::<Key>::empty()
}

// Failed: a phase or a step threw (INST-B64), and nothing else is reported as failure.
pub open spec fn end_failed(a: L1) -> bool {
    &&& a.fault
    &&& run_ok(a)
    &&& !blocked(a.op, a.pre.ver)
    // INST-B68: in developer mode a failure is not undone, and the steps that worked keep their records
    &&& (want_dev(a.op, a.pre) ==> a.d.ver == Option::Some(a.op.to) && a.d.dev
          && forall|k: int| #![trigger a.log[k]] 0 <= k < a.log.len() - 1 ==> done(rec(a.d.recs, a.log[k])))
}

// Two-way: Ok needs !fault, Failed needs fault, Refused needs blocked; so a command that worked and was not blocked
// can only report OK, and one that threw can only report Failed.
pub open spec fn honest_end(a: L1) -> bool {
    match a.out { Out::Ok => end_ok(a), Out::Refused => end_refused(a), Out::Failed => end_failed(a), Out::Running => false }
}

pub open spec fn same_consts(a: L1, b: L1) -> bool { b.op == a.op && b.steps == a.steps }

pub open spec fn l1_next(a: L1, b: L1) -> bool {
    ||| a == b
    ||| (a.phase == Phase::NotStarted && b.phase == Phase::Running && same_consts(a, b) && b.pre == a.d && b.d == a.d
         && b.log == Seq::<Key>::empty() && b.out == Out::Running && !b.fault)
    ||| (a.phase == Phase::Running && b.phase == Phase::Running && same_consts(a, b) && b.pre == a.pre
         && b.out == Out::Running && !b.fault && run_ok(b))
    ||| (a.phase == Phase::Running && b.phase == Phase::Closed && same_consts(a, b) && b.pre == a.pre && honest_end(b))
    // a crash reports nothing: in particular not success
    ||| (a.phase == Phase::Running && b.phase == Phase::Crashed && same_consts(a, b) && b.pre == a.pre
         && b.out == Out::Running && run_ok(b))
}

// ---------------------------------------------------------------- invariant

pub open spec fn manifested(s: L2) -> Db { Db { ver: Option::Some(s.op.to), dev: dev_run(s.op, s.pre), ..s.pre } }

pub open spec fn snap_pre(s: L2) -> Option<Db> { if s.op.txn { Option::Some(s.pre) } else { Option::None } }

pub open spec fn snap_post(s: L2) -> Option<Db> { if s.op.txn && !dev_run(s.op, s.pre) { Option::Some(s.pre) } else { Option::None } }

pub open spec fn seen(steps: Seq<Key>, i: int, k: Key) -> bool { exists|j: int| #![trigger steps[j]] 0 <= j < i && steps[j] == k }

// newStepToApply after the walk visited steps[0..i) (ghost value the code variable must hold).
pub open spec fn flag_spec(r: Map<Key, Rec>, steps: Seq<Key>, i: int) -> bool {
    i > 0 && exists|j: int| #![trigger due_at(r, steps, j)] 0 <= j < i && steps[j].c == steps[i - 1].c && due_at(r, steps, j)
}

pub open spec fn after_exec(r: Map<Key, Rec>, steps: Seq<Key>, op: Op, j: int) -> Rec {
    if sched(r, steps, op, j) { ran(true) } else { visit(rec(r, steps[j])) }
}

pub open spec fn after_seed(r: Map<Key, Rec>, steps: Seq<Key>, j: int) -> Rec { seeded(visit(rec(r, steps[j]))) }

pub open spec fn walk_inv(s: L2) -> bool {
    &&& s.i <= s.steps.len()
    &&& s.db.ver == Option::Some(s.op.to) && s.db.dev == dev_run(s.op, s.pre)
    &&& s.snap == snap_post(s)
    &&& forall|k: Key| #![trigger rec(s.db.recs, k)] !seen(s.steps, s.i as int, k) ==> rec(s.db.recs, k) == rec(s.pre.recs, k)
    &&& if s.seeding {
            &&& s.op.kind != Kind::Update && !dev_run(s.op, s.pre)
            &&& s.log == Seq::<Key>::empty() && s.db.jlog == s.pre.jlog
            &&& forall|j: int| #![trigger s.steps[j]] 0 <= j < s.i ==> rec(s.db.recs, s.steps[j]) == Option::Some(after_seed(s.pre.recs, s.steps, j))
        } else {
            &&& s.op.kind == Kind::Update
            &&& s.log == sched_upto(s.pre.recs, s.steps, s.op, s.i)
            &&& s.db.jlog == s.pre.jlog + s.log
            &&& s.flag == flag_spec(s.pre.recs, s.steps, s.i as int)
            &&& forall|j: int| #![trigger s.steps[j]] 0 <= j < s.i ==> rec(s.db.recs, s.steps[j]) == Option::Some(after_exec(s.pre.recs, s.steps, s.op, j))
        }
}

pub open spec fn inv(s: L2) -> bool {
    &&& wf(s.steps)
    &&& (running(s.pc) ==> !s.fault)
    &&& match s.pc {
        Pc::Init => s.snap is None && s.pre == s.db,
        Pc::Guard => s.db == s.pre && s.snap == snap_pre(s) && s.log == Seq::<Key>::empty() && s.out == Out::Running,
        Pc::Manifest => s.db == s.pre && s.snap == snap_pre(s) && s.log == Seq::<Key>::empty() && s.out == Out::Running
            && !refuses(s.op, dev_guard(s.op), s.pre.ver),
        Pc::Phases => s.db == manifested(s) && s.snap == snap_post(s) && s.log == Seq::<Key>::empty() && s.out == Out::Running
            && !refuses(s.op, dev_guard(s.op), s.pre.ver),
        Pc::Lock => s.db == manifested(s) && s.snap == snap_post(s) && s.log == Seq::<Key>::empty() && s.out == Out::Running
            && !refuses(s.op, dev_guard(s.op), s.pre.ver) && s.op.kind == Kind::Update,
        Pc::Walk => s.out == Out::Running && !refuses(s.op, dev_guard(s.op), s.pre.ver) && walk_inv(s),
        Pc::Closed => s.snap is None && s.out != Out::Running,
        Pc::Crashed => s.snap is None,
    }
}

// ---------------------------------------------------------------- schedule lemmas

proof fn inst_lemma_sched_upto_zero(r: Map<Key, Rec>, steps: Seq<Key>, op: Op)
    ensures sched_upto(r, steps, op, 0) == Seq::<Key>::empty()
{}

// Every element of sched_upto(n) is a scheduled step before n.
pub open spec fn elem_ok(r: Map<Key, Rec>, steps: Seq<Key>, op: Op, n: nat, p: int) -> bool {
    exists|j: int| #![trigger steps[j]] 0 <= j < n && sched(r, steps, op, j) && sched_upto(r, steps, op, n)[p] == steps[j]
}

proof fn inst_lemma_sched_elem(r: Map<Key, Rec>, steps: Seq<Key>, op: Op, n: nat, p: int)
    requires n <= steps.len(), 0 <= p < sched_upto(r, steps, op, n).len()
    ensures elem_ok(r, steps, op, n, p)
    decreases n
{
    if n > 0 {
        let q = sched_upto(r, steps, op, (n - 1) as nat);
        if p < q.len() {
            inst_lemma_sched_elem(r, steps, op, (n - 1) as nat, p);
            assert(sched_upto(r, steps, op, n)[p] == q[p]);
            let j = choose|j: int| #![trigger steps[j]] 0 <= j < n - 1 && sched(r, steps, op, j) && q[p] == steps[j];
            assert(0 <= j < n && sched(r, steps, op, j) && sched_upto(r, steps, op, n)[p] == steps[j]);
        } else {
            assert(sched(r, steps, op, n - 1));
            assert(sched_upto(r, steps, op, n)[p] == steps[n - 1]);
        }
    }
}

proof fn inst_lemma_sched_elems(r: Map<Key, Rec>, steps: Seq<Key>, op: Op, n: nat)
    requires n <= steps.len()
    ensures forall|p: int| #![trigger elem_ok(r, steps, op, n, p)] 0 <= p < sched_upto(r, steps, op, n).len() ==> elem_ok(r, steps, op, n, p)
{
    assert forall|p: int| #![trigger elem_ok(r, steps, op, n, p)] 0 <= p < sched_upto(r, steps, op, n).len() implies elem_ok(r, steps, op, n, p) by {
        inst_lemma_sched_elem(r, steps, op, n, p);
    }
}

// sched_upto(m) is a prefix of sched_upto(n) for m <= n.
proof fn inst_lemma_sched_prefix(r: Map<Key, Rec>, steps: Seq<Key>, op: Op, m: nat, n: nat)
    requires m <= n
    ensures sched_upto(r, steps, op, m).len() <= sched_upto(r, steps, op, n).len(),
        forall|p: int| #![trigger sched_upto(r, steps, op, m)[p]] 0 <= p < sched_upto(r, steps, op, m).len() ==>
            sched_upto(r, steps, op, n)[p] == sched_upto(r, steps, op, m)[p]
    decreases n - m
{
    if m < n {
        inst_lemma_sched_prefix(r, steps, op, m, (n - 1) as nat);
    }
}

// The flag the code keeps equals the declarative "some step of this class, so far, was due".
proof fn inst_lemma_flag_step(r: Map<Key, Rec>, steps: Seq<Key>, i: int)
    requires wf(steps), 0 <= i < steps.len()
    ensures
        ((!cstart(steps, i) && flag_spec(r, steps, i)) || due_at(r, steps, i)) == flag_spec(r, steps, i + 1),
        flag_spec(r, steps, i + 1) == (exists|j: int| #![trigger due_at(r, steps, j)] 0 <= j <= i && steps[j].c == steps[i].c && due_at(r, steps, j)),
{
    let lhs = (!cstart(steps, i) && flag_spec(r, steps, i)) || due_at(r, steps, i);
    let ex = exists|j: int| #![trigger due_at(r, steps, j)] 0 <= j <= i && steps[j].c == steps[i].c && due_at(r, steps, j);
    assert(flag_spec(r, steps, i + 1) == ex);
    if lhs {
        if due_at(r, steps, i) {
            assert(ex);
        } else {
            let j = choose|j: int| 0 <= j < i && steps[j].c == steps[i - 1].c && due_at(r, steps, j);
            assert(0 <= j <= i && steps[j].c == steps[i].c && due_at(r, steps, j));
        }
    }
    if ex {
        let j = choose|j: int| 0 <= j <= i && steps[j].c == steps[i].c && due_at(r, steps, j);
        if j < i {
            assert(steps[j].c <= steps[i - 1].c) by {
                if j < i - 1 { assert(steps[j].c <= steps[i - 1].c); }
            }
            assert(steps[i - 1].c <= steps[i].c);
            assert(!cstart(steps, i));
            assert(flag_spec(r, steps, i));
        }
    }
}

// ---------------------------------------------------------------- per-step lemmas (each reveals only its step)

// Under PF5 the run's developer mode is the one L1 expects, and the guard sees -dev as typed.
proof fn inst_lemma_pf5(s: L2)
    requires premise_pf5(s)
    ensures dev_run(s.op, s.pre) == want_dev(s.op, s.pre), dev_guard(s.op) == typed_dev(s.op)
{}

proof fn inst_step_init(s: L2)
    requires inv(s), s.pc == Pc::Init
    ensures inv(step_init(s)), l1_next(abs(s), abs(step_init(s)))
{
    reveal(step_init);
}

proof fn inst_step_guard(s: L2)
    requires inv(s), s.pc == Pc::Guard, premise_pf5(s)
    ensures inv(step_guard(s)), l1_next(abs(s), abs(step_guard(s)))
{
    inst_lemma_pf5(s);
    reveal(step_guard);
    inst_lemma_sched_upto_zero(s.pre.recs, s.steps, s.op);
}

proof fn inst_step_manifest(s: L2)
    requires inv(s), s.pc == Pc::Manifest, premise_pf5(s)
    ensures inv(step_manifest(s)), l1_next(abs(s), abs(step_manifest(s)))
{
    inst_lemma_pf5(s);
    reveal(step_manifest);
    inst_lemma_sched_upto_zero(s.pre.recs, s.steps, s.op);
    assert(log_is(abs(step_manifest(s)), 0));
}

proof fn inst_step_phases(s: L2, ok: bool)
    requires inv(s), s.pc == Pc::Phases, premise_pf5(s)
    ensures inv(step_phases(s, ok)), l1_next(abs(s), abs(step_phases(s, ok)))
{
    inst_lemma_pf5(s);
    reveal(step_phases);
    inst_lemma_sched_upto_zero(s.pre.recs, s.steps, s.op);
    let t = step_phases(s, ok);
    if !ok {
        assert(log_is(abs(t), 0));
        assert(ver_ok(abs(t)));
        assert(sched_prefix(abs(t)));
        assert(run_ok(abs(t)));
        assert(end_failed(abs(t)));
    } else if s.op.kind == Kind::Update {
        assert(abs(t) == abs(s));
    } else if !s.db.dev {
        assert forall|k: Key| #![trigger rec(t.db.recs, k)] !seen(t.steps, t.i as int, k) implies rec(t.db.recs, k) == rec(t.pre.recs, k) by {}
        assert(abs(t) == abs(s));
    } else {
        assert(end_ok(abs(t)));
    }
}

proof fn inst_step_lock(s: L2, granted: bool)
    requires inv(s), s.pc == Pc::Lock, premise_pf5(s), premise_pl(Choice::Lock(granted))
    ensures inv(step_lock(s, granted)), l1_next(abs(s), abs(step_lock(s, granted)))
{
    inst_lemma_pf5(s);
    reveal(step_lock);
    inst_lemma_sched_upto_zero(s.pre.recs, s.steps, s.op);
    let t = step_lock(s, granted);
    assert(s.pre.jlog + Seq::<Key>::empty() =~= s.pre.jlog);
    assert(abs(t) == abs(s));
}

proof fn inst_step_crash(s: L2)
    requires inv(s), running(s.pc), premise_pf5(s)
    ensures inv(step_crash(s)), l1_next(abs(s), abs(step_crash(s)))
{
    inst_lemma_pf5(s);
    reveal(step_crash);
    inst_lemma_sched_upto_zero(s.pre.recs, s.steps, s.op);
    if s.pc == Pc::Walk && !s.seeding {
        assert(log_is(abs(step_crash(s)), s.i));
    } else {
        assert(log_is(abs(step_crash(s)), 0));
    }
}

// The end of a walk (i = len): commit, report OK.
proof fn inst_walk_end(s: L2, ok: bool)
    requires inv(s), s.pc == Pc::Walk, premise_pf5(s), s.i >= s.steps.len()
    ensures inv(step_walk(s, ok)), l1_next(abs(s), abs(step_walk(s, ok)))
{
    inst_lemma_pf5(s);
    reveal(step_walk);
    let t = step_walk(s, ok);
    assert(s.i == s.steps.len());
    if s.seeding {
        assert forall|j: int| #![trigger s.steps[j]] 0 <= j < s.steps.len() implies done(rec(t.db.recs, s.steps[j])) by {
            assert(rec(s.db.recs, s.steps[j]) == Option::Some(after_seed(s.pre.recs, s.steps, j)));
        }
    } else {
        assert forall|j: int| #![trigger s.steps[j]] 0 <= j < s.steps.len() && !skipped(s.op, s.steps[j]) implies done(rec(t.db.recs, s.steps[j])) by {
            assert(rec(s.db.recs, s.steps[j]) == Option::Some(after_exec(s.pre.recs, s.steps, s.op, j)));
            if !sched(s.pre.recs, s.steps, s.op, j) {
                assert(!due_at(s.pre.recs, s.steps, j));
            }
        }
    }
}

// One step of a seeding walk.
proof fn inst_walk_seed(s: L2, ok: bool)
    requires inv(s), s.pc == Pc::Walk, premise_pf5(s), s.i < s.steps.len(), s.seeding
    ensures inv(step_walk(s, ok)), l1_next(abs(s), abs(step_walk(s, ok)))
{
    inst_lemma_pf5(s);
    reveal(step_walk);
    let t = step_walk(s, ok);
    let i = s.i as int;
    let k = s.steps[i];
    assert(!seen(s.steps, i, k)) by {
        assert forall|j: int| 0 <= j < i implies s.steps[j] != k by { assert(s.steps[j] != s.steps[i]); }
    }
    assert(rec(s.db.recs, k) == rec(s.pre.recs, k));
    assert forall|j: int| #![trigger t.steps[j]] 0 <= j < t.i implies rec(t.db.recs, t.steps[j]) == Option::Some(after_seed(t.pre.recs, t.steps, j)) by {
        if j < i { assert(s.steps[j] != s.steps[i]); }
    }
    assert forall|k2: Key| #![trigger rec(t.db.recs, k2)] !seen(t.steps, t.i as int, k2) implies rec(t.db.recs, k2) == rec(t.pre.recs, k2) by {
        assert(k2 != k) by { if k2 == k { assert(seen(t.steps, t.i as int, k2)) by { assert(t.steps[i] == k2); } } }
        assert(!seen(s.steps, i, k2)) by {
            if seen(s.steps, i, k2) {
                let j = choose|j: int| 0 <= j < i && s.steps[j] == k2;
                assert(t.steps[j] == k2);
            }
        }
        assert(rec(s.db.recs, k2) == rec(s.pre.recs, k2));
    }
    assert(abs(t).d == abs(s).d || !s.op.txn);
    inst_lemma_sched_upto_zero(s.pre.recs, s.steps, s.op);
    assert(log_is(abs(t), 0));
}

// Inserting the record of steps[i] keeps every key not yet visited unchanged.
proof fn inst_lemma_frame_insert(recs: Map<Key, Rec>, pre: Map<Key, Rec>, steps: Seq<Key>, i: int, x: Rec)
    requires wf(steps), 0 <= i < steps.len(),
        forall|k: Key| #![trigger rec(recs, k)] !seen(steps, i, k) ==> rec(recs, k) == rec(pre, k)
    ensures
        !seen(steps, i, steps[i]),
        rec(recs, steps[i]) == rec(pre, steps[i]),
        forall|k: Key| #![trigger rec(recs.insert(steps[i], x), k)] !seen(steps, i + 1, k) ==> rec(recs.insert(steps[i], x), k) == rec(pre, k),
{
    let ki = steps[i];
    assert(!seen(steps, i, ki)) by {
        assert forall|j: int| 0 <= j < i implies steps[j] != ki by { assert(steps[j] != steps[i]); }
    }
    assert(rec(recs, ki) == rec(pre, ki));
    assert forall|k2: Key| #![trigger rec(recs.insert(ki, x), k2)] !seen(steps, i + 1, k2) implies rec(recs.insert(ki, x), k2) == rec(pre, k2) by {
        assert(k2 != ki) by { if k2 == ki { assert(seen(steps, i + 1, k2)) by { assert(steps[i] == k2); } } }
        assert(!seen(steps, i, k2)) by {
            if seen(steps, i, k2) {
                let j = choose|j: int| 0 <= j < i && steps[j] == k2;
                assert(steps[j] == k2);
            }
        }
        assert(rec(recs, k2) == rec(pre, k2));
        assert(rec(recs.insert(ki, x), k2) == rec(recs, k2));
    }
}

// Inserting after_exec(i) extends the visited prefix by one.
proof fn inst_lemma_prefix_exec(recs: Map<Key, Rec>, r0: Map<Key, Rec>, steps: Seq<Key>, op: Op, i: int, x: Rec)
    requires wf(steps), 0 <= i < steps.len(), x == after_exec(r0, steps, op, i),
        forall|j: int| #![trigger steps[j]] 0 <= j < i ==> rec(recs, steps[j]) == Option::Some(after_exec(r0, steps, op, j))
    ensures forall|j: int| #![trigger steps[j]] 0 <= j < i + 1 ==> rec(recs.insert(steps[i], x), steps[j]) == Option::Some(after_exec(r0, steps, op, j))
{
    assert forall|j: int| #![trigger steps[j]] 0 <= j < i + 1 implies rec(recs.insert(steps[i], x), steps[j]) == Option::Some(after_exec(r0, steps, op, j)) by {
        if j < i { assert(steps[j] != steps[i]); }
    }
}

// Facts shared by both outcomes of an update walk step: the code's flag and run test equal the L1 schedule.
proof fn inst_walk_exec_facts(s: L2)
    requires inv(s), s.pc == Pc::Walk, s.i < s.steps.len(), !s.seeding
    ensures ({
        let i = s.i as int;
        let k = s.steps[i];
        let r0 = s.pre.recs;
        let f = (!cstart(s.steps, i) && s.flag) || run_needed(visit(rec(s.db.recs, k)));
        &&& !seen(s.steps, i, k)
        &&& rec(s.db.recs, k) == rec(r0, k)
        &&& f == flag_spec(r0, s.steps, i + 1)
        &&& (f && !skipped(s.op, k)) == sched(r0, s.steps, s.op, i)
        &&& sched_upto(r0, s.steps, s.op, (s.i + 1) as nat) == (if sched(r0, s.steps, s.op, i) { s.log.push(k) } else { s.log })
    })
{
    let i = s.i as int;
    let k = s.steps[i];
    let r0 = s.pre.recs;
    inst_lemma_frame_insert(s.db.recs, r0, s.steps, i, ran(true));
    inst_lemma_flag_step(r0, s.steps, i);
    assert(run_needed(visit(rec(s.db.recs, k))) == due_at(r0, s.steps, i));
}

// An update walk step that does not fail: the walk goes on.
proof fn inst_walk_exec_go(s: L2, ok: bool)
    requires inv(s), s.pc == Pc::Walk, premise_pf5(s), s.i < s.steps.len(), !s.seeding,
        !(sched(s.pre.recs, s.steps, s.op, s.i as int) && !ok)
    ensures inv(step_walk(s, ok)), l1_next(abs(s), abs(step_walk(s, ok)))
{
    inst_lemma_pf5(s);
    reveal(step_walk);
    let t = step_walk(s, ok);
    let i = s.i as int;
    let k = s.steps[i];
    let r0 = s.pre.recs;
    inst_walk_exec_facts(s);
    let x = after_exec(r0, s.steps, s.op, i);
    assert(t.db.recs == s.db.recs.insert(k, x));
    inst_lemma_frame_insert(s.db.recs, r0, s.steps, i, x);
    inst_lemma_prefix_exec(s.db.recs, r0, s.steps, s.op, i, x);
    if sched(r0, s.steps, s.op, i) {
        assert(s.pre.jlog + s.log.push(k) =~= (s.pre.jlog + s.log).push(k));
    }
    assert(t.log == sched_upto(r0, s.steps, s.op, t.i));
    assert(walk_inv(t));
    assert(log_is(abs(t), t.i));
}

// A failing step ends the command with an error.
proof fn inst_walk_exec_fail(s: L2)
    requires inv(s), s.pc == Pc::Walk, premise_pf5(s), s.i < s.steps.len(), !s.seeding,
        sched(s.pre.recs, s.steps, s.op, s.i as int)
    ensures inv(step_walk(s, false)), l1_next(abs(s), abs(step_walk(s, false)))
{
    inst_lemma_pf5(s);
    reveal(step_walk);
    let t = step_walk(s, false);
    let i = s.i as int;
    let k = s.steps[i];
    let r0 = s.pre.recs;
    inst_walk_exec_facts(s);
    let n1 = (s.i + 1) as nat;
    assert(t.log == sched_upto(r0, s.steps, s.op, n1));
    assert(log_is(abs(t), n1));
    if want_dev(s.op, s.pre) {
        inst_lemma_sched_elems(r0, s.steps, s.op, s.i);
        assert forall|p: int| #![trigger t.log[p]] 0 <= p < t.log.len() - 1 implies done(rec(t.db.recs, t.log[p])) by {
            assert(t.log[p] == s.log[p]);
            assert(s.log[p] == sched_upto(r0, s.steps, s.op, s.i)[p]);
            assert(elem_ok(r0, s.steps, s.op, s.i, p));
            let j = choose|j: int| 0 <= j < s.i && sched(r0, s.steps, s.op, j) && sched_upto(r0, s.steps, s.op, s.i)[p] == s.steps[j];
            assert(s.steps[j] != s.steps[i]);
            assert(rec(s.db.recs, s.steps[j]) == Option::Some(after_exec(r0, s.steps, s.op, j)));
        }
    }
    assert(end_failed(abs(t)));
}

proof fn inst_walk_exec(s: L2, ok: bool)
    requires inv(s), s.pc == Pc::Walk, premise_pf5(s), s.i < s.steps.len(), !s.seeding
    ensures inv(step_walk(s, ok)), l1_next(abs(s), abs(step_walk(s, ok)))
{
    if sched(s.pre.recs, s.steps, s.op, s.i as int) && !ok { inst_walk_exec_fail(s); } else { inst_walk_exec_go(s, ok); }
}

proof fn inst_step_walk(s: L2, ok: bool)
    requires inv(s), s.pc == Pc::Walk, premise_pf5(s)
    ensures inv(step_walk(s, ok)), l1_next(abs(s), abs(step_walk(s, ok)))
{
    if s.i >= s.steps.len() { inst_walk_end(s, ok); }
    else if s.seeding { inst_walk_seed(s, ok); }
    else { inst_walk_exec(s, ok); }
}

// ---------------------------------------------------------------- refinement and invariant (dispatcher)

pub proof fn inst_inv_initial(s: L2)
    requires initial(s), wf(s.steps)
    ensures inv(s)
{}

pub proof fn inst_refines(s: L2, c: Choice)
    requires inv(s), enabled(s, c), premise_pl(c), premise_pf5(s)
    ensures inv(step(s, c)), l1_next(abs(s), abs(step(s, c)))
{
    match c {
        Choice::Init => inst_step_init(s),
        Choice::Guard => inst_step_guard(s),
        Choice::Manifest => inst_step_manifest(s),
        Choice::Phases(ok) => inst_step_phases(s, ok),
        Choice::Lock(g) => inst_step_lock(s, g),
        Choice::Walk(ok) => inst_step_walk(s, ok),
        Choice::Crash => inst_step_crash(s),
    }
}

// ---------------------------------------------------------------- consequences of the L1 contract

// INST-B55, B60, B61, B62, B63: an update that ends OK ran step j exactly when j was scheduled.
pub proof fn inst_ok_update_runs_iff_scheduled(a: L1, j: int)
    requires end_ok(a), a.op.kind == Kind::Update, wf(a.steps), 0 <= j < a.steps.len()
    ensures a.log.contains(a.steps[j]) == sched(a.pre.recs, a.steps, a.op, j)
{
    let r = a.pre.recs;
    let n = a.steps.len();
    inst_lemma_sched_elems(r, a.steps, a.op, n);
    if sched(r, a.steps, a.op, j) {
        inst_lemma_sched_prefix(r, a.steps, a.op, (j + 1) as nat, n);
        let q = sched_upto(r, a.steps, a.op, (j + 1) as nat);
        assert(q[q.len() - 1] == a.steps[j]);
        assert(a.log[q.len() - 1] == a.steps[j]);
    }
    if a.log.contains(a.steps[j]) {
        let p = choose|p: int| 0 <= p < a.log.len() && a.log[p] == a.steps[j];
        assert(elem_ok(r, a.steps, a.op, n, p));
        let j2 = choose|j2: int| 0 <= j2 < n && sched(r, a.steps, a.op, j2) && sched_upto(r, a.steps, a.op, n)[p] == a.steps[j2];
        if j2 != j {
            if j2 < j { assert(a.steps[j2] != a.steps[j]); } else { assert(a.steps[j] != a.steps[j2]); }
        }
    }
}

// INST-B56..B59: the steps an OK update ran appear in walk order (class number, all-member list first, author order).
pub proof fn inst_ok_update_runs_in_walk_order(a: L1, x: int, y: int)
    requires end_ok(a), a.op.kind == Kind::Update, 0 <= x < y < a.steps.len(),
        sched(a.pre.recs, a.steps, a.op, x), sched(a.pre.recs, a.steps, a.op, y)
    ensures exists|p: int, q: int| 0 <= p < q < a.log.len() && a.log[p] == a.steps[x] && a.log[q] == a.steps[y]
{
    let r = a.pre.recs;
    let n = a.steps.len();
    let qx = sched_upto(r, a.steps, a.op, (x + 1) as nat);
    let qy = sched_upto(r, a.steps, a.op, (y + 1) as nat);
    inst_lemma_sched_prefix(r, a.steps, a.op, (x + 1) as nat, y as nat);
    inst_lemma_sched_prefix(r, a.steps, a.op, (x + 1) as nat, n);
    inst_lemma_sched_prefix(r, a.steps, a.op, (y + 1) as nat, n);
    let p = qx.len() - 1;
    let q = qy.len() - 1;
    assert(qx[p] == a.steps[x]);
    assert(qy[q] == a.steps[y]);
    assert(qy.len() == sched_upto(r, a.steps, a.op, y as nat).len() + 1);
    assert(a.log[p] == a.steps[x]);
    assert(a.log[q] == a.steps[y]);
}

// INST-B49, B29: when every step of the package is done, an update runs nothing.
pub proof fn inst_all_done_runs_nothing(r: Map<Key, Rec>, steps: Seq<Key>, op: Op, n: nat)
    requires n <= steps.len(), forall|j: int| #![trigger steps[j]] 0 <= j < steps.len() ==> done(rec(r, steps[j]))
    ensures sched_upto(r, steps, op, n) == Seq::<Key>::empty()
    decreases n
{
    if n > 0 {
        inst_all_done_runs_nothing(r, steps, op, (n - 1) as nat);
        if sched(r, steps, op, n - 1) {
            let j = choose|j: int| 0 <= j <= n - 1 && steps[j].c == steps[n - 1].c && due_at(r, steps, j);
            assert(done(rec(r, steps[j])));
        }
    }
}

// INST-B53 (DEC-INST-8): with no records at all, an OK update runs every step this member may run.
pub proof fn inst_fresh_update_runs_all(a: L1, j: int)
    requires end_ok(a), a.op.kind == Kind::Update, wf(a.steps), a.pre.recs == Map::<Key, Rec>::empty(),
        0 <= j < a.steps.len(), !skipped(a.op, a.steps[j])
    ensures a.log.contains(a.steps[j])
{
    assert(due_at(a.pre.recs, a.steps, j));
    inst_ok_update_runs_iff_scheduled(a, j);
}

// ---------------------------------------------------------------- an interrupted update can be finished (INST-B69)

pub open spec fn walk_all(s: L2, n: nat) -> L2
    decreases n
{
    if n == 0 { s } else { walk_all(step(s, Choice::Walk(true)), (n - 1) as nat) }
}

proof fn inst_walk_completes(s: L2)
    requires inv(s), s.pc == Pc::Walk, !s.seeding, premise_pf5(s)
    ensures ({
        let e = walk_all(s, (s.steps.len() - s.i + 1) as nat);
        e.pc == Pc::Closed && e.out == Out::Ok && end_ok(abs(e)) && e.op == s.op && e.steps == s.steps && e.pre == s.pre
    })
    decreases s.steps.len() - s.i
{
    reveal(step_walk);
    reveal_with_fuel(walk_all, 2);
    let s1 = step(s, Choice::Walk(true));
    inst_refines(s, Choice::Walk(true));
    if s.i < s.steps.len() {
        assert(s1.pc == Pc::Walk && s1.i == s.i + 1 && !s1.seeding);
        inst_walk_completes(s1);
        assert(walk_all(s, (s.steps.len() - s.i + 1) as nat) == walk_all(s1, (s1.steps.len() - s1.i + 1) as nat));
    } else {
        assert(walk_all(s, 1) == s1);
    }
}

// From any step records (an earlier update cut off at any point), an update whose lock is granted and whose
// steps succeed ends OK with every step this member may run done.
pub proof fn inst_update_after_interruption_completes(s: L2)
    requires initial(s), wf(s.steps), s.op.kind == Kind::Update, premise_pf5(s), !blocked(s.op, s.db.ver)
    ensures ({
        let s5 = step(step(step(step(step(s, Choice::Init), Choice::Guard), Choice::Manifest), Choice::Phases(true)), Choice::Lock(true));
        let e = walk_all(s5, (s5.steps.len() + 1) as nat);
        &&& e.pc == Pc::Closed && e.out == Out::Ok && end_ok(abs(e))
        &&& forall|j: int| #![trigger e.steps[j]] 0 <= j < e.steps.len() && !skipped(e.op, e.steps[j]) ==> done(rec(abs(e).d.recs, e.steps[j]))
    })
{
    let s1 = step(s, Choice::Init);
    inst_refines(s, Choice::Init);
    reveal(step_init);
    let s2 = step(s1, Choice::Guard);
    inst_refines(s1, Choice::Guard);
    reveal(step_guard);
    let s3 = step(s2, Choice::Manifest);
    inst_refines(s2, Choice::Manifest);
    reveal(step_manifest);
    let s4 = step(s3, Choice::Phases(true));
    inst_refines(s3, Choice::Phases(true));
    reveal(step_phases);
    let s5 = step(s4, Choice::Lock(true));
    inst_refines(s4, Choice::Lock(true));
    reveal(step_lock);
    assert(s5.pc == Pc::Walk && s5.i == 0 && !s5.seeding && s5.op == s.op);
    inst_walk_completes(s5);
}

// ---------------------------------------------------------------- the class list (VersionBase.cls#L34-L60)

// One class of the update package as GetUpdatePackageClassList sees it, in query row order: the number left by
// $zstrip(<last piece of the name>, "*E'N") (None when no digit is left) and whether it defines the step lists.
pub struct PkgClass { pub num: Option<nat>, pub step_class: bool, pub id: nat }

// updatePackageClassList(classNum) = className, row by row: a later row with the same number replaces an earlier one;
// an empty classNum is a <SUBSCRIPT> error that fails the command (None).
pub open spec fn class_list(cs: Seq<PkgClass>) -> Option<Map<nat, PkgClass>>
    decreases cs.len()
{
    if cs.len() == 0 { Option::Some(Map::<nat, PkgClass>::empty()) } else {
        match class_list(cs.drop_last()) {
            Option::None => Option::None,
            Option::Some(m) => match cs.last().num {
                Option::None => Option::None,
                Option::Some(n) => Option::Some(m.insert(n, cs.last())),
            },
        }
    }
}

// L1 (INST-D3, D4, B44): the update walks every step class of the update package.
pub open spec fn walks_class(m: Map<nat, PkgClass>, c: PkgClass) -> bool {
    match c.num { Option::Some(n) => m.contains_key(n) && m[n] == c, Option::None => false }
}

pub open spec fn all_step_classes_walked(cs: Seq<PkgClass>) -> bool {
    match class_list(cs) {
        Option::None => false,
        Option::Some(m) => forall|i: int| #![trigger cs[i]] 0 <= i < cs.len() && cs[i].step_class ==> walks_class(m, cs[i]),
    }
}

// ---------------------------------------------------------------- defect witnesses (proved)

pub open spec fn k(c: nat, m: nat) -> Key { Key { c: c, prim: false, m: m } }

pub open spec fn db0(ver: Option<nat>) -> Db { Db { ver: ver, dev: false, recs: Map::<Key, Rec>::empty(), jlog: Seq::<Key>::empty() } }

pub open spec fn upd_op(dev: Option<bool>, source: Source) -> Op {
    Op { kind: Kind::Update, force: false, dev: dev, txn: true, top: true, source: source, snapshot: false,
         to: 2, has_pkg: true, skip_prim: false }
}

pub open spec fn start(db: Db, op: Op, steps: Seq<Key>) -> L2 {
    L2 { db: db, snap: Option::None, pre: db, pc: Pc::Init, op: op, steps: steps, seeding: false, i: 0, flag: false,
         log: Seq::<Key>::empty(), fault: false, out: Out::Running }
}

pub open spec fn upto_lock(s: L2) -> L2 {
    step(step(step(step(s, Choice::Init), Choice::Guard), Choice::Manifest), Choice::Phases(true))
}

// F-INST-1: the update lock is busy. The update reports OK, the module is at the new version and no step ran,
// although a step was due (IRIS: nc-inst-3-update-lock-busy).
pub proof fn inst_witness_busy_lock_reports_success()
    ensures ({
        let steps = seq![k(1, 0)];
        let s4 = upto_lock(start(db0(Option::Some(1)), upd_op(Option::None, Source::Registry), steps));
        let s5 = step(s4, Choice::Lock(false));
        &&& inv(s4) && enabled(s4, Choice::Lock(false)) && !premise_pl(Choice::Lock(false)) && premise_pf5(s4)
        &&& s5.out == Out::Ok && abs(s5).d.ver == Option::Some(2nat) && abs(s5).log.len() == 0
        &&& due(rec(abs(s5).d.recs, k(1, 0)))
        &&& !l1_next(abs(s4), abs(s5))
    })
{
    reveal(step_init); reveal(step_guard); reveal(step_manifest); reveal(step_phases); reveal(step_lock);
    let steps = seq![k(1, 0)];
    let s0 = start(db0(Option::Some(1)), upd_op(Option::None, Source::Registry), steps);
    let s4 = upto_lock(s0);
    let s5 = step(s4, Choice::Lock(false));
    assert(due_at(s4.pre.recs, steps, 0));
    assert(sched(s4.pre.recs, steps, s4.op, 0));
    reveal_with_fuel(sched_upto, 2);
    assert(sched_upto(s4.pre.recs, steps, s4.op, 1) =~= seq![k(1, 0)]);
    assert(abs(s5).log != sched_upto(s4.pre.recs, steps, s4.op, 1));
    assert forall|a: int, b: int| #![trigger steps[a], steps[b]] 0 <= a < b < steps.len() implies
        steps[a] != steps[b] && steps[a].c <= steps[b].c && (steps[a].c == steps[b].c && steps[a].prim ==> steps[b].prim) by {}
}

// F-INST-5: `update -dev` of a non-snapshot version from a registry runs without developer mode, so a failing step
// rolls everything back: the module stays at the old version and not in developer mode (IRIS:
// nc-inst-4e-dev-update-from-registry, nc-dep-19f-dev-registry-update).
pub proof fn inst_witness_dev_update_from_registry_rolls_back()
    ensures ({
        let steps = seq![k(1, 0)];
        let s4 = upto_lock(start(db0(Option::Some(1)), upd_op(Option::Some(true), Source::Registry), steps));
        let s5 = step(s4, Choice::Lock(true));
        let s6 = step(s5, Choice::Walk(false));
        &&& !premise_pf5(s5) && enabled(s5, Choice::Walk(false))
        &&& s6.out == Out::Failed && abs(s6).d.ver == Option::Some(1nat) && !abs(s6).d.dev
        &&& !l1_next(abs(s5), abs(s6))
    })
{
    reveal(step_init); reveal(step_guard); reveal(step_manifest); reveal(step_phases); reveal(step_lock); reveal(step_walk);
}

// DEC-INST-2: an install in developer mode seeds nothing, so a later update runs the steps of every class,
// including classes older than the installed version; a non-developer install seeds them all (IRIS:
// nc-inst-2b-dev-fresh-then-update, nc-inst-6b-fresh-install-control).
pub proof fn inst_witness_dev_install_leaves_steps_due()
    ensures ({
        let steps = seq![k(1, 0), k(2, 0)];
        let dev_op = Op { kind: Kind::Install, force: false, dev: Option::Some(true), txn: true, top: true, source: Source::Registry,
                          snapshot: false, to: 2, has_pkg: true, skip_prim: false };
        let e = step(step(step(step(start(db0(Option::None), dev_op, steps), Choice::Init), Choice::Guard), Choice::Manifest), Choice::Phases(true));
        let plain_op = Op { dev: Option::None, ..dev_op };
        let w = step(step(step(step(start(db0(Option::None), plain_op, steps), Choice::Init), Choice::Guard), Choice::Manifest), Choice::Phases(true));
        let p = step(step(step(w, Choice::Walk(true)), Choice::Walk(true)), Choice::Walk(true));
        &&& e.out == Out::Ok && abs(e).d.ver == Option::Some(2nat) && abs(e).d.dev
        &&& sched(abs(e).d.recs, steps, upd_op(Option::None, Source::Registry), 0)
        &&& p.out == Out::Ok && !sched(abs(p).d.recs, steps, upd_op(Option::None, Source::Registry), 0)
        &&& !sched(abs(p).d.recs, steps, upd_op(Option::None, Source::Registry), 1)
    })
{
    reveal(step_init); reveal(step_guard); reveal(step_manifest); reveal(step_phases); reveal(step_walk);
    let steps = seq![k(1, 0), k(2, 0)];
    let dev_op = Op { kind: Kind::Install, force: false, dev: Option::Some(true), txn: true, top: true, source: Source::Registry,
                      snapshot: false, to: 2, has_pkg: true, skip_prim: false };
    let e = step(step(step(step(start(db0(Option::None), dev_op, steps), Choice::Init), Choice::Guard), Choice::Manifest), Choice::Phases(true));
    assert(due_at(abs(e).d.recs, steps, 0));
    let plain_op = Op { dev: Option::None, ..dev_op };
    let w = step(step(step(step(start(db0(Option::None), plain_op, steps), Choice::Init), Choice::Guard), Choice::Manifest), Choice::Phases(true));
    let p = step(step(step(w, Choice::Walk(true)), Choice::Walk(true)), Choice::Walk(true));
    let r = abs(p).d.recs;
    assert(r.contains_key(k(1, 0)) && r[k(1, 0)].end && r[k(1, 0)].ok);
    assert(r.contains_key(k(2, 0)) && r[k(2, 0)].end && r[k(2, 0)].ok);
    assert(!sched(r, steps, upd_op(Option::None, Source::Registry), 0)) by {
        assert forall|j: int| 0 <= j <= 0 && steps[j].c == steps[0].c implies !due_at(r, steps, j) by { assert(j == 0); }
    }
    assert(!sched(r, steps, upd_op(Option::None, Source::Registry), 1)) by {
        assert forall|j: int| 0 <= j <= 1 && steps[j].c == steps[1].c implies !due_at(r, steps, j) by {
            assert(j == 1);
        }
    }
}

// DEC-INST-5: a transactional update whose second step fails is rolled back, records included; the first step's
// effects outside the database stay, and the retry schedules that step again, so its effects happen twice
// (IRIS: nc-inst-4-nondev-failed-update, nc-inst-4b-nondev-retry).
pub proof fn inst_witness_failed_update_repeats_effects()
    ensures ({
        let steps = seq![k(2, 0), k(2, 1)];
        let s5 = step(upto_lock(start(db0(Option::Some(1)), upd_op(Option::None, Source::Path), steps)), Choice::Lock(true));
        let s7 = step(step(s5, Choice::Walk(true)), Choice::Walk(false));
        &&& s7.out == Out::Failed && abs(s7).d == db0(Option::Some(1))
        &&& abs(s7).log == seq![k(2, 0), k(2, 1)]
        &&& sched(abs(s7).d.recs, steps, upd_op(Option::None, Source::Path), 0)
    })
{
    reveal(step_init); reveal(step_guard); reveal(step_manifest); reveal(step_phases); reveal(step_lock); reveal(step_walk);
    let steps = seq![k(2, 0), k(2, 1)];
    let s5 = step(upto_lock(start(db0(Option::Some(1)), upd_op(Option::None, Source::Path), steps)), Choice::Lock(true));
    let s6 = step(s5, Choice::Walk(true));
    assert(due(rec(s5.db.recs, k(2, 0))));
    assert(s6.log == seq![k(2, 0)]);
    assert(!cstart(steps, 1));
    let s7 = step(s6, Choice::Walk(false));
    assert(s7.log =~= seq![k(2, 0), k(2, 1)]);
    assert(due_at(abs(s7).d.recs, steps, 0));
}

// F-INST-6: a helper class V3Helper, listed after the step class V3, takes class number 3, so V3's steps are never
// walked; the helper has no step lists, and the update reports OK (IRIS: nc-inst-model-class-number-collision).
pub proof fn inst_witness_helper_class_hides_steps()
    ensures ({
        let v3 = PkgClass { num: Option::Some(3), step_class: true, id: 0 };
        let helper = PkgClass { num: Option::Some(3), step_class: false, id: 1 };
        let cs = seq![v3, helper];
        &&& class_list(cs) == Option::Some(Map::<nat, PkgClass>::empty().insert(3, helper))
        &&& !all_step_classes_walked(cs)
    })
{
    let v3 = PkgClass { num: Option::Some(3), step_class: true, id: 0 };
    let helper = PkgClass { num: Option::Some(3), step_class: false, id: 1 };
    let cs = seq![v3, helper];
    reveal_with_fuel(class_list, 3);
    assert(cs.drop_last() =~= seq![v3]);
    assert(seq![v3].drop_last() =~= Seq::<PkgClass>::empty());
    let m = Map::<nat, PkgClass>::empty().insert(3, helper);
    assert(class_list(cs) == Option::Some(m));
    assert(cs[0] == v3);
    assert(!walks_class(m, v3));
}

// F-INST-7: a class of the update package whose name leaves no digit (for example Validator) makes the class list
// fail, so every update of the module fails (IRIS: nc-inst-model-class-without-number).
pub proof fn inst_witness_class_without_number_fails()
    ensures ({
        let v3 = PkgClass { num: Option::Some(3), step_class: true, id: 0 };
        let validator = PkgClass { num: Option::None, step_class: false, id: 1 };
        &&& class_list(seq![v3, validator]) is None
        &&& !all_step_classes_walked(seq![v3, validator])
    })
{
    let v3 = PkgClass { num: Option::Some(3), step_class: true, id: 0 };
    let validator = PkgClass { num: Option::None, step_class: false, id: 1 };
    reveal_with_fuel(class_list, 3);
    assert(seq![v3, validator].drop_last() =~= seq![v3]);
    assert(seq![v3].drop_last() =~= Seq::<PkgClass>::empty());
}

fn main() {}

} // verus!
