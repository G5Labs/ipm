//! L2: the as-built IPM load path and history log at 571e04a1, over a model of the IRIS
//! process (nested transactions, IRISTEMP, journaling switch, process death).
//!
//! Every function in the `exec_spec_verified!` block is compiled to an executable twin
//! `exec_<name>` that Verus proves equal to the spec function. The explorer and the case
//! generator run those twins. `BINDINGS.md` binds each step to its ObjectScript line.
//!
//! Store layout. Module slot 0 is the module being loaded (`top`), slots 1 and 2 are its
//! dependencies in load order. History rows (`twins`, the `%IPM.General.History` table) and
//! IRISTEMP rows (`temp`, `%IPM.General.HistoryTemp`) use the same slot as the action they log.
//!
//! IRIS transactions are modelled by snapshots: `tx[k]` is the namespace store as it was at
//! the TSTART that opened level k+1, so `$TLEVEL = tx.len()`. TCOMMIT drops the top snapshot
//! (an inner commit is not durable, P-01). TROLLBACK 1 restores the top snapshot (P-01). A write
//! made while process journaling is off is not undone by rollback (P-03), so it is applied to
//! every open snapshot as well. IRISTEMP rows (`temp`) are outside the snapshots (P-02).
//! Process death restores `tx[0]` (P-05) and runs no `%OnClose` (P-06).
use vstd::prelude::*;
use vstd::contrib::exec_spec::*;

verus! {

exec_spec_verified! {

// ------------------------------------------------------------------ inputs

/// A package on disk (its `module.xml` and resources).
pub struct Pkg {
    /// Version ordinal (1 < 2 < 3).
    pub ver: u8,
    /// The manifest declares `<UpdatePackage>`.
    pub upd: bool,
    /// `<SystemRequirements>` are satisfied in this instance.
    pub sysreq: bool,
    /// Activate phase: 0 succeeds, 1 throws (phase finalized with the error),
    /// 2 an Invoke returns an error status (`quit:$$$ISERR` path, phase left open).
    pub act: u8,
    /// Manifest: 0 valid, 1 not parseable (GetModuleObjectFromPath throws), 2 parseable but holds
    /// no module document ("No module definition found."), 3 module document refused by
    /// ImportStream validation (nested transaction rolled back), 4 written, then refused
    /// ("module.xml contains multiple documents.", Utils/Module.cls:1258-1260).
    pub mf: u8,
    /// Snapshot version (a dependency loaded from a filesystem repository under a `-dev` parent
    /// is itself loaded in developer mode).
    pub snap: bool,
    /// A dependency is found in a configured repository (BuildDependencyGraph succeeds).
    pub found: bool,
}

/// A module as stored in the namespace (`^IPM.Storage.ModuleD` plus its `.ZPM` document and its
/// loaded resources).
pub struct Mod {
    /// Module record exists (`%IPM.Storage.Module:NameExists`).
    pub present: bool,
    pub ver: u8,
    /// `DeveloperMode`.
    pub dev: bool,
    /// The module's resources (classes) are loaded.
    pub res: bool,
    /// Version of the stored `.ZPM` document stream (0 none).
    pub doc: u8,
    /// Number of writes to the module record by the command (stands for LastUpdated/Hash).
    pub wr: u8,
}

pub struct Cfg {
    /// 0 default (transactions), 1 `-DNoTransaction=1`, 2 `-DNoJournal=1`.
    pub txm: u8,
    /// 0 neither flag, 1 `-dev`, 2 `-nodev`.
    pub devf: u8,
    pub force: bool,
    /// The path passes LoadInternal's checks (non-empty, absolute or existing).
    pub path_ok: bool,
    /// `tmpRepoMgr.CleanUp()` fails after LoadNewModule returned (OBS-L12).
    pub cleanup_fails: bool,
    /// The caller already holds a transaction (`tstart` then `Shell("load ...")`).
    pub caller_tx: bool,
    pub top: Pkg,
    /// Leaf dependencies in load order (0..2).
    pub deps: Seq<Pkg>,
    /// Installed state before the command, one entry per module slot (3).
    pub init: Seq<Mod>,
    /// 0 as-built. 1..19 controls (one guard or effect removed). 100.. repairs:
    /// 103 = D1 repair (dependency History rows written outside the parent's transaction).
    pub var: u8,
}

// ------------------------------------------------------------------ state

/// A `%IPM.General.History` row (the durable "twin").
pub struct Twin {
    pub ex: bool,
    pub id: u8,
    /// Package set (not the empty placeholder).
    pub pkg: bool,
    /// Version (0 is the placeholder 0.0.0-0).
    pub ver: u8,
    /// Success: 0 in progress (InitialExpression 0), 1 OK, 2 error.
    pub ok: u8,
    pub com: bool,
    /// TimeEnd set.
    pub tend: bool,
    pub fin: bool,
    /// Number of phases started (0..5: Initialize, Reload, Validate, Compile, Activate).
    pub ph: u8,
    /// Status of the last started phase: 0 open (no TimeEnd), 1 OK, 2 error.
    pub phst: u8,
    /// Changed by another writer (SQL UPDATE or object save outside the load).
    pub edited: bool,
}

pub struct Ns {
    pub mods: Seq<Mod>,
    pub twins: Seq<Twin>,
}

/// A `%IPM.General.HistoryTemp` row in IRISTEMP.
pub struct Temp {
    pub live: bool,
    pub pkg: bool,
    pub ver: u8,
    pub ok: u8,
    pub com: bool,
    pub tend: bool,
    pub ph: u8,
    pub phst: u8,
}

/// A `LoadNewModule` activation.
pub struct Lnm {
    pub pc: u8,
    /// `tInitTLevel`.
    pub itl: u8,
    /// `tDeveloperMode`.
    pub dev: bool,
    /// `tSC` is an error.
    pub err: bool,
    /// A JournalManager(0) was created (NoJournal).
    pub nj: bool,
    /// Journaling state the JournalManager restores on close.
    pub jprev: bool,
    /// LoadDependencies cursor (top frame only).
    pub di: u8,
}

pub struct State {
    /// Namespace store as this process sees it (including its uncommitted writes).
    pub ns: Ns,
    /// One snapshot per open transaction level.
    pub tx: Seq<Ns>,
    /// Process journaling enabled.
    pub jrn: bool,
    /// IRISTEMP history rows.
    pub temp: Seq<Temp>,
    /// Control 6 only: IRISTEMP snapshots (temp rows treated as journaled). Empty otherwise.
    pub ttx: Seq<Seq<Temp>>,
    /// History ID counter (`$increment`, not rolled back).
    pub ctr: u8,
    pub alive: bool,
    /// Main:Load.
    pub top_pc: u8,
    pub top_err: bool,
    pub lnm0: Lnm,
    /// LoadModuleReference for the current dependency (0 idle).
    pub lmr_pc: u8,
    pub lmr_err: bool,
    pub lnm1: Lnm,
    /// Caller transaction outcome: 0 none or open, 1 committed, 2 rolled back.
    pub caller: u8,
    /// Auxiliary (not a product variable): per action slot, 0 not started, 1 History row
    /// inserted (action started), 2 Finalize ran (status known), 3 the row was completed by
    /// PersistToTwin after Finalize.
    pub att: Seq<u8>,
    /// How the process ended: 0 alive, 1 killed without %OnClose (SIGKILL-like, N-P18),
    /// 2 terminated with %OnClose (Terminate or HALT, N-P10/N-P11).
    pub how: u8,
}

// ------------------------------------------------------------------ accessors

pub open spec fn dmod() -> Mod { Mod { present: false, ver: 0, dev: false, res: false, doc: 0, wr: 0 } }
pub open spec fn dtwin() -> Twin { Twin { ex: false, id: 0, pkg: false, ver: 0, ok: 0, com: false, tend: false, fin: false, ph: 0, phst: 0, edited: false } }
pub open spec fn dtemp() -> Temp { Temp { live: false, pkg: false, ver: 0, ok: 0, com: false, tend: false, ph: 0, phst: 0 } }
pub open spec fn dpkg() -> Pkg { Pkg { ver: 1, upd: false, sysreq: true, act: 0, mf: 0, snap: false, found: true } }
pub open spec fn dlnm() -> Lnm { Lnm { pc: 0, itl: 0, dev: false, err: false, nj: false, jprev: true, di: 0 } }

pub open spec fn gmod(ms: Seq<Mod>, i: usize) -> Mod { if i < ms.len() { ms[i as int] } else { dmod() } }
pub open spec fn gtwin(ts: Seq<Twin>, i: usize) -> Twin { if i < ts.len() { ts[i as int] } else { dtwin() } }
pub open spec fn gtemp(ts: Seq<Temp>, i: usize) -> Temp { if i < ts.len() { ts[i as int] } else { dtemp() } }
pub open spec fn gpkg(ps: Seq<Pkg>, i: usize) -> Pkg { if i < ps.len() { ps[i as int] } else { dpkg() } }
pub open spec fn gatt(a: Seq<u8>, i: usize) -> u8 { if i < a.len() { a[i as int] } else { 0u8 } }

pub open spec fn inc(x: u8) -> u8 { if x < 250 { (x + 1) as u8 } else { x } }

// ------------------------------------------------------------------ state builders

pub open spec fn with_store(s: State, ns: Ns, tx: Seq<Ns>) -> State {
    State { ns: ns, tx: tx, jrn: s.jrn, temp: s.temp, ttx: s.ttx, ctr: s.ctr, alive: s.alive, top_pc: s.top_pc, top_err: s.top_err,
        lnm0: s.lnm0, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: s.lnm1, caller: s.caller, att: s.att, how: s.how }
}

pub open spec fn with_jrn(s: State, j: bool) -> State {
    State { ns: s.ns, tx: s.tx, jrn: j, temp: s.temp, ttx: s.ttx, ctr: s.ctr, alive: s.alive, top_pc: s.top_pc, top_err: s.top_err,
        lnm0: s.lnm0, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: s.lnm1, caller: s.caller, att: s.att, how: s.how }
}

pub open spec fn with_temp(s: State, t: Seq<Temp>, tt: Seq<Seq<Temp>>) -> State {
    State { ns: s.ns, tx: s.tx, jrn: s.jrn, temp: t, ttx: tt, ctr: s.ctr, alive: s.alive, top_pc: s.top_pc, top_err: s.top_err,
        lnm0: s.lnm0, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: s.lnm1, caller: s.caller, att: s.att, how: s.how }
}

pub open spec fn with_ctr_att(s: State, ctr: u8, att: Seq<u8>) -> State {
    State { ns: s.ns, tx: s.tx, jrn: s.jrn, temp: s.temp, ttx: s.ttx, ctr: ctr, alive: s.alive, top_pc: s.top_pc, top_err: s.top_err,
        lnm0: s.lnm0, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: s.lnm1, caller: s.caller, att: att, how: s.how }
}

pub open spec fn with_top(s: State, pc: u8, err: bool) -> State {
    State { ns: s.ns, tx: s.tx, jrn: s.jrn, temp: s.temp, ttx: s.ttx, ctr: s.ctr, alive: s.alive, top_pc: pc, top_err: err,
        lnm0: s.lnm0, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: s.lnm1, caller: s.caller, att: s.att, how: s.how }
}

pub open spec fn with_lmr(s: State, pc: u8, err: bool) -> State {
    State { ns: s.ns, tx: s.tx, jrn: s.jrn, temp: s.temp, ttx: s.ttx, ctr: s.ctr, alive: s.alive, top_pc: s.top_pc, top_err: s.top_err,
        lnm0: s.lnm0, lmr_pc: pc, lmr_err: err, lnm1: s.lnm1, caller: s.caller, att: s.att, how: s.how }
}

pub open spec fn with_lnm0(s: State, f: Lnm) -> State {
    State { ns: s.ns, tx: s.tx, jrn: s.jrn, temp: s.temp, ttx: s.ttx, ctr: s.ctr, alive: s.alive, top_pc: s.top_pc, top_err: s.top_err,
        lnm0: f, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: s.lnm1, caller: s.caller, att: s.att, how: s.how }
}

pub open spec fn with_lnm1(s: State, f: Lnm) -> State {
    State { ns: s.ns, tx: s.tx, jrn: s.jrn, temp: s.temp, ttx: s.ttx, ctr: s.ctr, alive: s.alive, top_pc: s.top_pc, top_err: s.top_err,
        lnm0: s.lnm0, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: f, caller: s.caller, att: s.att, how: s.how }
}

pub open spec fn with_death(s: State, how: u8) -> State {
    State { ns: s.ns, tx: s.tx, jrn: s.jrn, temp: s.temp, ttx: s.ttx, ctr: s.ctr, alive: false, top_pc: s.top_pc, top_err: s.top_err,
        lnm0: s.lnm0, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: s.lnm1, caller: s.caller, att: s.att, how: how }
}

pub open spec fn with_end(s: State, alive: bool, top_pc: u8, caller: u8) -> State {
    State { ns: s.ns, tx: s.tx, jrn: s.jrn, temp: s.temp, ttx: s.ttx, ctr: s.ctr, alive: alive, top_pc: top_pc, top_err: s.top_err,
        lnm0: s.lnm0, lmr_pc: s.lmr_pc, lmr_err: s.lmr_err, lnm1: s.lnm1, caller: caller, att: s.att, how: s.how }
}

// ------------------------------------------------------------------ IRIS provider: namespace store

pub open spec fn upd_mod(ns: Ns, i: usize, m: Mod) -> Ns {
    if i < ns.mods.len() { Ns { mods: ns.mods.update(i as int, m), twins: ns.twins } } else { ns }
}

pub open spec fn upd_twin(ns: Ns, i: usize, t: Twin) -> Ns {
    if i < ns.twins.len() { Ns { mods: ns.mods, twins: ns.twins.update(i as int, t) } } else { ns }
}

/// The same module write applied to every open snapshot (a write rollback does not undo).
pub open spec fn all_mod(tx: Seq<Ns>, i: usize, m: Mod) -> Seq<Ns> {
    if tx.len() == 0 { tx }
    else if tx.len() == 1 { Seq::empty().push(upd_mod(tx[0], i, m)) }
    else if tx.len() == 2 { Seq::empty().push(upd_mod(tx[0], i, m)).push(upd_mod(tx[1], i, m)) }
    else if tx.len() == 3 { Seq::empty().push(upd_mod(tx[0], i, m)).push(upd_mod(tx[1], i, m)).push(upd_mod(tx[2], i, m)) }
    else { tx }
}

pub open spec fn all_twin(tx: Seq<Ns>, i: usize, t: Twin) -> Seq<Ns> {
    if tx.len() == 0 { tx }
    else if tx.len() == 1 { Seq::empty().push(upd_twin(tx[0], i, t)) }
    else if tx.len() == 2 { Seq::empty().push(upd_twin(tx[0], i, t)).push(upd_twin(tx[1], i, t)) }
    else if tx.len() == 3 { Seq::empty().push(upd_twin(tx[0], i, t)).push(upd_twin(tx[1], i, t)).push(upd_twin(tx[2], i, t)) }
    else { tx }
}

/// SET of a module node by this process (journaled unless process journaling is off).
pub open spec fn wmod(s: State, i: usize, m: Mod) -> State {
    with_store(s, upd_mod(s.ns, i, m), if s.jrn { s.tx } else { all_mod(s.tx, i, m) })
}

/// SET of a History row by this process.
pub open spec fn wtwin(s: State, i: usize, t: Twin) -> State {
    with_store(s, upd_twin(s.ns, i, t), if s.jrn { s.tx } else { all_twin(s.tx, i, t) })
}

/// TSTART. Control 6 also snapshots IRISTEMP.
pub open spec fn tstart(c: Cfg, s: State) -> State {
    let s2 = with_store(s, s.ns, s.tx.push(s.ns));
    if c.var == 6 { with_temp(s2, s.temp, s.ttx.push(s.temp)) } else { s2 }
}

/// TCOMMIT: drops the innermost snapshot. Durable only when it leaves level 0.
pub open spec fn tcommit(c: Cfg, s: State) -> State {
    if s.tx.len() > 0 {
        let s2 = with_store(s, s.ns, s.tx.drop_last());
        if c.var == 6 && s.ttx.len() > 0 { with_temp(s2, s.temp, s.ttx.drop_last()) } else { s2 }
    } else { s }
}

/// TROLLBACK 1: restores the state at the innermost TSTART.
pub open spec fn rollback1(c: Cfg, s: State) -> State {
    if s.tx.len() > 0 {
        let s2 = with_store(s, s.tx.last(), s.tx.drop_last());
        if c.var == 6 && s.ttx.len() > 0 { with_temp(s2, s.ttx.last(), s.ttx.drop_last()) } else { s2 }
    } else { s }
}

/// The durable view: what the namespace holds if this process dies now.
pub open spec fn dv(s: State) -> Ns { if s.tx.len() > 0 { s.tx[0] } else { s.ns } }

// ------------------------------------------------------------------ IRISTEMP history writes

pub open spec fn put_temp(s: State, i: usize, t: Temp) -> State {
    if i < s.temp.len() { with_temp(s, s.temp.update(i as int, t), s.ttx) } else { s }
}

/// HistoryTemp Init (temp row %Save). `pkg`: Init received a package name (LoadInit(pModuleName)
/// for a dependency; LoadInit("") for `load`).
pub open spec fn fresh_temp(pkg: bool) -> Temp { Temp { live: true, pkg: pkg, ver: 0, ok: 0, com: false, tend: false, ph: 0, phst: 0 } }

pub open spec fn t_names(t: Temp, ver: u8) -> Temp { Temp { live: t.live, pkg: true, ver: ver, ok: t.ok, com: t.com, tend: t.tend, ph: t.ph, phst: t.phst } }
pub open spec fn t_ver(t: Temp, ver: u8) -> Temp { Temp { live: t.live, pkg: t.pkg, ver: ver, ok: t.ok, com: t.com, tend: t.tend, ph: t.ph, phst: t.phst } }
pub open spec fn t_com(t: Temp) -> Temp { Temp { live: t.live, pkg: t.pkg, ver: t.ver, ok: t.ok, com: true, tend: t.tend, ph: t.ph, phst: t.phst } }
pub open spec fn t_phase(t: Temp, k: u8, st: u8) -> Temp { Temp { live: t.live, pkg: t.pkg, ver: t.ver, ok: t.ok, com: t.com, tend: t.tend, ph: k, phst: st } }
pub open spec fn t_dead(t: Temp) -> Temp { Temp { live: false, pkg: t.pkg, ver: t.ver, ok: t.ok, com: t.com, tend: t.tend, ph: t.ph, phst: t.phst } }

/// AbstractHistory:Finalize: Success = status, Committed = Committed || ISOK(status), TimeEnd.
pub open spec fn t_final(c: Cfg, t: Temp, err: bool) -> Temp {
    Temp { live: t.live, pkg: t.pkg, ver: t.ver, ok: if err { 2u8 } else { 1u8 },
        com: if c.var == 5 { t.com } else { t.com || !err }, tend: true, ph: t.ph, phst: t.phst }
}

/// AbstractHistory:SetCommitted (IRISTEMP only). Repair 102 also copies it to the History row
/// when no transaction is open.
pub open spec fn set_com(c: Cfg, s: State, i: usize) -> State {
    let s2 = put_temp(s, i, t_com(gtemp(s.temp, i)));
    if c.var == 102 && s.tx.len() == 0 && gtwin(s.ns.twins, i).ex && !gtwin(s.ns.twins, i).fin {
        let tw = gtwin(s.ns.twins, i);
        wtwin(s2, i, Twin { ex: tw.ex, id: tw.id, pkg: tw.pkg, ver: tw.ver, ok: tw.ok, com: true, tend: tw.tend, fin: tw.fin, ph: tw.ph, phst: tw.phst, edited: tw.edited })
    } else { s2 }
}

pub open spec fn fresh_twin(id: u8, pkg: bool) -> Twin { Twin { ex: true, id: id, pkg: pkg, ver: 0, ok: 0, com: false, tend: false, fin: false, ph: 0, phst: 0, edited: false } }

/// A write that no open transaction of this process undoes (applied to the store and to every
/// snapshot). Used only by repair 103 for dependency rows.
pub open spec fn autonomous_twin(s: State, i: usize, t: Twin) -> State {
    with_store(s, upd_twin(s.ns, i, t), all_twin(s.tx, i, t))
}

/// Repair 103 writes the History rows of dependency loads (slots 1 and 2) outside the parent's
/// transaction.
pub open spec fn dep_autonomous(c: Cfg, i: usize) -> bool { c.var == 103 && i != 0 }

/// HistoryTemp:Init second half: the History twin %Save (at the caller's $TLEVEL), copying
/// Package and the placeholder Version from the temp row.
pub open spec fn twin_insert(c: Cfg, s: State, i: usize) -> State {
    let t = fresh_twin(inc(s.ctr), gtemp(s.temp, i).pkg);
    let s2 = if dep_autonomous(c, i) { autonomous_twin(s, i, t) } else { wtwin(s, i, t) };
    with_ctr_att(s2, inc(s.ctr), if i < s.att.len() { s.att.update(i as int, 1u8) } else { s.att })
}

/// PersistToTwin's write: copy every field, Finalized=1, deep %Save at the current $TLEVEL. The
/// UPDATE trigger refuses a row whose stored Finalized is 1 (N-P13, N-P16). A deleted twin is not
/// recreated (N-P15).
pub open spec fn persist_row(c: Cfg, s: State, i: usize) -> State {
    let tw = gtwin(s.ns.twins, i);
    let t = gtemp(s.temp, i);
    if !tw.ex || (tw.fin && c.var != 10) { s }
    else if dep_autonomous(c, i) {
        // repair 103: Committed only if the dependency's writes are already durable (no IPM
        // transaction of the load is open); finalized later, at the top-level close
        autonomous_twin(s, i, Twin { ex: true, id: tw.id, pkg: t.pkg, ver: t.ver, ok: t.ok,
            com: t.com && s.tx.len() <= (s.lnm0.itl as usize), tend: t.tend, fin: false, ph: t.ph, phst: t.phst, edited: tw.edited })
    }
    else {
        wtwin(s, i, Twin { ex: true, id: tw.id, pkg: t.pkg, ver: t.ver, ok: t.ok, com: t.com, tend: t.tend,
            fin: c.var != 7, ph: t.ph, phst: t.phst, edited: tw.edited })
    }
}

/// Repair 103: when the parent's commit makes its dependencies' writes durable, their rows take
/// the dependencies' own Committed.
pub open spec fn dep_mark_one(c: Cfg, s: State, i: usize) -> State {
    let tw = gtwin(s.ns.twins, i);
    if tw.ex && !tw.fin {
        autonomous_twin(s, i, Twin { ex: tw.ex, id: tw.id, pkg: tw.pkg, ver: tw.ver, ok: tw.ok, com: gtemp(s.temp, i).com,
            tend: tw.tend, fin: tw.fin, ph: tw.ph, phst: tw.phst, edited: tw.edited })
    } else { s }
}

pub open spec fn dep_commit_mark(c: Cfg, s: State, w: u8) -> State {
    if c.var == 103 && w == 0 && s.tx.len() <= (s.lnm0.itl as usize) { dep_mark_one(c, dep_mark_one(c, s, 1), 2) } else { s }
}

/// Repair 103: the top-level close finalizes the dependency rows.
pub open spec fn dep_fin_one(s: State, i: usize) -> State {
    let tw = gtwin(s.ns.twins, i);
    if tw.ex && !tw.fin {
        autonomous_twin(s, i, Twin { ex: tw.ex, id: tw.id, pkg: tw.pkg, ver: tw.ver, ok: tw.ok, com: tw.com,
            tend: tw.tend, fin: true, ph: tw.ph, phst: tw.phst, edited: tw.edited })
    } else { s }
}

pub open spec fn dep_finalize(c: Cfg, s: State) -> State {
    if c.var == 103 { dep_fin_one(dep_fin_one(s, 1), 2) } else { s }
}

/// Auxiliary: record that slot `i` reached stage `a`.
pub open spec fn set_att(s: State, i: usize, a: u8) -> State {
    with_ctr_att(s, s.ctr, if i < s.att.len() { s.att.update(i as int, a) } else { s.att })
}

/// PersistToTwin from %OnClose; a row persisted after Finalize is complete (auxiliary 3).
pub open spec fn persist(c: Cfg, s: State, i: usize) -> State {
    let s2 = persist_row(c, s, i);
    if gatt(s.att, i) == 2 { set_att(s2, i, 3) } else { s2 }
}

// Program counters: named after the exec block (the step functions use the literals).

pub open spec fn phk(pc: u8) -> u8 { if pc <= 16 { 1 } else if pc <= 19 { 2 } else if pc <= 22 { 3 } else if pc <= 25 { 4 } else { 5 } }
pub open spec fn phj(pc: u8) -> u8 {
    if pc == 14 || pc == 17 || pc == 20 || pc == 23 || pc == 26 { 0 }
    else if pc == 15 || pc == 18 || pc == 21 || pc == 24 || pc == 27 { 1 } else { 2 }
}

// ------------------------------------------------------------------ LoadNewModule

pub open spec fn lnm_of(s: State, w: u8) -> Lnm { if w == 0 { s.lnm0 } else { s.lnm1 } }
pub open spec fn set_lnm(s: State, w: u8, f: Lnm) -> State { if w == 0 { with_lnm0(s, f) } else { with_lnm1(s, f) } }
pub open spec fn slot_of(s: State, w: u8) -> usize { if w == 0 { 0usize } else if s.lnm0.di == 0 { 1usize } else { 2usize } }
pub open spec fn pkg_of(c: Cfg, s: State, w: u8) -> Pkg { if w == 0 { c.top } else { gpkg(c.deps, s.lnm0.di as usize) } }

/// `params("DeveloperMode")` is defined: for `load` only with `-dev`/`-nodev`; always for a
/// dependency (LoadModuleFromDirectory sets the override).
pub open spec fn devdef(c: Cfg, w: u8) -> bool { if w == 0 { c.devf != 0 } else { true } }
/// Its value: `-dev` for the top; for a dependency, snapshot loaded under a `-dev` parent.
pub open spec fn devval(c: Cfg, s: State, w: u8) -> bool { if w == 0 { c.devf == 1 } else { pkg_of(c, s, w).snap && c.devf == 1 } }

pub open spec fn lpc(f: Lnm, pc: u8) -> Lnm { Lnm { pc: pc, itl: f.itl, dev: f.dev, err: f.err, nj: f.nj, jprev: f.jprev, di: f.di } }
pub open spec fn lfail(f: Lnm) -> Lnm { Lnm { pc: 31, itl: f.itl, dev: f.dev, err: true, nj: f.nj, jprev: f.jprev, di: f.di } }

/// Manifest import (`$system.OBJ.Load module.xml` → ImportStream → UpdatePersistentFromStream):
/// writes the `.ZPM` document and the module record, keeping DeveloperMode and loaded resources.
pub open spec fn m_manifest(cur: Mod, ver: u8) -> Mod {
    Mod { present: true, ver: ver, dev: if cur.present { cur.dev } else { false }, res: if cur.present { cur.res } else { false }, doc: ver, wr: inc(cur.wr) }
}
pub open spec fn m_doc(cur: Mod, ver: u8) -> Mod { Mod { present: cur.present, ver: cur.ver, dev: cur.dev, res: cur.res, doc: ver, wr: cur.wr } }
pub open spec fn m_dev(cur: Mod, dev: bool) -> Mod { Mod { present: cur.present, ver: cur.ver, dev: dev, res: cur.res, doc: cur.doc, wr: inc(cur.wr) } }
pub open spec fn m_touch(cur: Mod) -> Mod { Mod { present: cur.present, ver: cur.ver, dev: cur.dev, res: cur.res, doc: cur.doc, wr: inc(cur.wr) } }
pub open spec fn m_res(cur: Mod) -> Mod { Mod { present: cur.present, ver: cur.ver, dev: cur.dev, res: true, doc: cur.doc, wr: inc(cur.wr) } }

/// The downgrade and update-required guards (Utils/Module.cls:1213-1223).
pub open spec fn guard_refuses(c: Cfg, s: State, w: u8) -> bool {
    let cur = gmod(s.ns.mods, slot_of(s, w));
    let p = pkg_of(c, s, w);
    (cur.present && cur.ver > p.ver && c.var != 8)
    || (cur.present && p.ver > cur.ver && !c.force && !devval(c, s, w) && p.upd && c.var != 9)
}

pub open spec fn all_found(c: Cfg) -> bool {
    (c.deps.len() < 1 || gpkg(c.deps, 0).found) && (c.deps.len() < 2 || gpkg(c.deps, 1).found)
}

/// Dependency `di` is already installed at the requested version (BuildDependencyGraph gives it
/// no ServerName, so LoadDependencies skips it).
pub open spec fn dep_satisfied(c: Cfg, s: State, di: u8) -> bool {
    let m = gmod(s.ns.mods, if di == 0 { 1usize } else { 2usize });
    m.present && m.ver == gpkg(c.deps, di as usize).ver
}

pub open spec fn lnm_enter(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    let p = pkg_of(c, s, w);
    let itl = if s.tx.len() < 250 { s.tx.len() as u8 } else { 250u8 };
    set_lnm(s, w, Lnm { pc: if p.mf == 1 { 31u8 } else { 1u8 }, itl: itl, dev: false, err: p.mf == 1, nj: false, jprev: s.jrn, di: f.di })
}

pub open spec fn lnm_begin(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    if c.txm == 2 {
        set_lnm(with_jrn(s, false), w, Lnm { pc: 2, itl: f.itl, dev: f.dev, err: f.err, nj: true, jprev: s.jrn, di: f.di })
    } else if c.txm == 0 {
        set_lnm(tstart(c, s), w, lpc(f, 2))
    } else {
        set_lnm(s, w, lpc(f, 2))
    }
}

pub open spec fn lnm_load(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    let i = slot_of(s, w);
    let p = pkg_of(c, s, w);
    let cur = gmod(s.ns.mods, i);
    if p.mf == 2 { set_lnm(s, w, lfail(f)) }
    else if p.mf == 3 {
        // ImportStream: tstart, ModuleStream %Save, validation refuses, trollback 1. The
        // document write is undone only if it was journaled.
        set_lnm(if s.jrn { s } else { wmod(s, i, m_doc(cur, p.ver)) }, w, lfail(f))
    } else if p.mf == 4 {
        // both documents are imported (module record written), then the multiple-documents check
        // refuses (1258-1260)
        set_lnm(wmod(s, i, m_manifest(cur, p.ver)), w, lfail(f))
    } else {
        set_lnm(wmod(s, i, m_manifest(cur, p.ver)), w, lpc(f, 5))
    }
}

pub open spec fn lnm_devsave(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    let i = slot_of(s, w);
    let cur = gmod(s.ns.mods, i);
    if devdef(c, w) {
        let d = devval(c, s, w);
        set_lnm(wmod(s, i, m_dev(cur, d)), w, Lnm { pc: 7, itl: f.itl, dev: d, err: f.err, nj: f.nj, jprev: f.jprev, di: f.di })
    } else {
        set_lnm(s, w, Lnm { pc: 7, itl: f.itl, dev: cur.dev, err: f.err, nj: f.nj, jprev: f.jprev, di: f.di })
    }
}

/// LoadDependencies, driven from the top frame at L_DEPS (dependencies are leaves).
pub open spec fn lnm_deps(c: Cfg, s: State) -> State {
    let f = s.lnm0;
    if s.lmr_pc == 9 {
        let s2 = with_lmr(s, 0, false);
        if s.lmr_err { with_lnm0(s2, lfail(f)) }
        else { with_lnm0(s2, Lnm { pc: 11, itl: f.itl, dev: f.dev, err: f.err, nj: f.nj, jprev: f.jprev, di: inc(f.di) }) }
    } else if f.di == 0 && !all_found(c) {
        with_lnm0(s, lfail(f))
    } else if (f.di as usize) < c.deps.len() && f.di < 2 {
        if dep_satisfied(c, s, f.di) {
            with_lnm0(s, Lnm { pc: 11, itl: f.itl, dev: f.dev, err: f.err, nj: f.nj, jprev: f.jprev, di: inc(f.di) })
        } else {
            with_lmr(s, 1, false)
        }
    } else {
        with_lnm0(s, lpc(f, 12))
    }
}

pub open spec fn lnm_phase(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    let i = slot_of(s, w);
    let p = pkg_of(c, s, w);
    let k = phk(f.pc);
    let j = phj(f.pc);
    let t = gtemp(s.temp, i);
    if j == 0 {
        set_lnm(put_temp(s, i, t_phase(t, k, 0)), w, lpc(f, inc(f.pc)))
    } else if j == 1 {
        if k == 2 {
            set_lnm(wmod(s, i, m_res(gmod(s.ns.mods, i))), w, lpc(f, inc(f.pc)))
        } else if k == 5 && p.act == 1 {
            set_lnm(put_temp(s, i, t_phase(t, k, 2)), w, lfail(f))
        } else if k == 5 && p.act == 2 {
            set_lnm(s, w, lfail(f))
        } else {
            set_lnm(s, w, lpc(f, inc(f.pc)))
        }
    } else {
        set_lnm(put_temp(s, i, t_phase(t, k, 1)), w, lpc(f, if k < 5 { inc(f.pc) } else { 29u8 }))
    }
}

pub open spec fn lnm_exit(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    if s.tx.len() > (f.itl as usize) && c.var != 4 {
        rollback1(c, s)
    } else {
        set_lnm(if f.nj { with_jrn(s, f.jprev) } else { s }, w, lpc(f, 32))
    }
}

/// LoadNewModule up to the developer-mode save (Utils/Module.cls:1158-1307).
pub open spec fn lnm_a(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    let i = slot_of(s, w);
    let p = pkg_of(c, s, w);
    if f.pc == 0 { lnm_enter(c, s, w) }
    else if f.pc == 1 { lnm_begin(c, s, w) }
    else if f.pc == 2 { set_lnm(s, w, if guard_refuses(c, s, w) { lfail(f) } else { lpc(f, 3) }) }
    else if f.pc == 3 { set_lnm(if c.txm != 0 && c.var != 3 { set_com(c, s, i) } else { s }, w, lpc(f, 4)) }
    else if f.pc == 4 { lnm_load(c, s, w) }
    else if f.pc == 5 { set_lnm(put_temp(s, i, t_names(gtemp(s.temp, i), p.ver)), w, lpc(f, 6)) }
    else { lnm_devsave(c, s, w) }
}

/// LoadNewModule first developer-mode commit and system requirements (1309-1321).
pub open spec fn lnm_b(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    let i = slot_of(s, w);
    let p = pkg_of(c, s, w);
    if f.pc == 7 {
        if f.dev && c.txm == 0 { set_lnm(tcommit(c, s), w, lpc(f, 8)) } else { set_lnm(s, w, lpc(f, 10)) }
    }
    else if f.pc == 8 { set_lnm(if c.var != 1 { set_com(c, s, i) } else { s }, w, lpc(f, 9)) }
    else if f.pc == 9 { set_lnm(tstart(c, s), w, lpc(f, 10)) }
    else { set_lnm(s, w, if p.sysreq { lpc(f, 11) } else { lfail(f) }) }
}

/// LoadNewModule dependencies, manifest reload and second developer-mode commit (1323-1339).
pub open spec fn lnm_b2(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    let i = slot_of(s, w);
    if f.pc == 11 { if w == 0 { lnm_deps(c, s) } else { set_lnm(s, w, lpc(f, 12)) } }
    else if f.pc == 12 { set_lnm(wmod(s, i, m_touch(gmod(s.ns.mods, i))), w, lpc(f, 13)) }
    // control 12 (reviewer variant 212): developer-mode phases run inside an inner transaction
    else { set_lnm(if f.dev && c.txm == 0 { if c.var == 12 { tstart(c, dep_commit_mark(c, tcommit(c, s), w)) } else { dep_commit_mark(c, tcommit(c, s), w) } } else { s }, w, lpc(f, 14)) }
}

/// LoadNewModule phases, non-developer commit and exit (1340-1363).
pub open spec fn lnm_c(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    let i = slot_of(s, w);
    if f.pc == 29 {
        if !f.dev && c.txm == 0 { set_lnm(dep_commit_mark(c, tcommit(c, s), w), w, lpc(f, 30)) }
        else if f.dev && c.txm == 0 && c.var == 12 { set_lnm(tcommit(c, s), w, lpc(f, 31)) }
        else { set_lnm(s, w, lpc(f, 31)) }
    }
    else if f.pc == 30 { set_lnm(if c.var != 2 { set_com(c, s, i) } else { s }, w, lpc(f, 31)) }
    else if f.pc == 31 { lnm_exit(c, s, w) }
    else { s }
}

pub open spec fn lnm_step(c: Cfg, s: State, w: u8) -> State {
    let f = lnm_of(s, w);
    if f.pc <= 6 { lnm_a(c, s, w) } else if f.pc <= 10 { lnm_b(c, s, w) } else if f.pc <= 13 { lnm_b2(c, s, w) } else if f.pc <= 28 { lnm_phase(c, s, w) } else { lnm_c(c, s, w) }
}

// ------------------------------------------------------------------ LoadModuleReference (dependency)

pub open spec fn dslot(s: State) -> usize { if s.lnm0.di == 0 { 1usize } else { 2usize } }

pub open spec fn lmr_step(c: Cfg, s: State) -> State {
    let i = dslot(s);
    let d = gpkg(c.deps, s.lnm0.di as usize);
    if s.lmr_pc == 1 { with_lmr(put_temp(s, i, fresh_temp(true)), 2, s.lmr_err) }
    else if s.lmr_pc == 2 { with_lmr(twin_insert(c, s, i), 3, s.lmr_err) }
    else if s.lmr_pc == 3 { with_lmr(put_temp(s, i, t_ver(gtemp(s.temp, i), d.ver)), 4, s.lmr_err) }
    else if s.lmr_pc == 4 { with_lmr(with_lnm1(s, dlnm()), 5, s.lmr_err) }
    else if s.lmr_pc == 5 { with_lmr(s, 6, s.lnm1.err) }
    else if s.lmr_pc == 6 { with_lmr(set_att(put_temp(s, i, t_final(c, gtemp(s.temp, i), s.lmr_err)), i, 2), 7, s.lmr_err) }
    else if s.lmr_pc == 7 {
        // %OnClose: PersistToTwin, then the temp row is deleted. A missing twin row is not
        // recreated and the temp row is still deleted (N-P15).
        with_lmr(persist(c, s, i), 8, s.lmr_err)
    }
    else if s.lmr_pc == 8 { with_lmr(put_temp(s, i, t_dead(gtemp(s.temp, i))), 9, s.lmr_err) }
    else { s }
}

// ------------------------------------------------------------------ Main:Load

pub open spec fn top_step(c: Cfg, s: State) -> State {
    if s.top_pc == 0 { with_top(put_temp(s, 0, fresh_temp(false)), 1, false) }
    else if s.top_pc == 1 { with_top(twin_insert(c, s, 0), 2, false) }
    else if s.top_pc == 2 {
        if c.path_ok { with_top(with_lnm0(s, dlnm()), 3, false) } else { with_top(s, 4, true) }
    }
    else if s.top_pc == 3 { with_top(s, 4, s.lnm0.err || c.cleanup_fails) }
    else if s.top_pc == 4 { with_top(set_att(put_temp(s, 0, t_final(c, gtemp(s.temp, 0), s.top_err)), 0, 2), 5, s.top_err) }
    else if s.top_pc == 5 {
        with_top(persist(c, dep_finalize(c, s), 0), 6, s.top_err)
    }
    else if s.top_pc == 6 { with_top(put_temp(s, 0, t_dead(gtemp(s.temp, 0))), 7, s.top_err) }
    else { s }
}

// ------------------------------------------------------------------ the machine

/// Which activation runs next: 0 Main:Load, 1 top LoadNewModule, 2 LoadModuleReference,
/// 3 dependency LoadNewModule.
pub open spec fn active(s: State) -> u8 {
    if s.top_pc == 3 && s.lnm0.pc != 32 {
        if s.lnm0.pc == 11 && s.lmr_pc != 0 && s.lmr_pc != 9 {
            if s.lmr_pc == 5 && s.lnm1.pc != 32 { 3u8 } else { 2u8 }
        } else { 1u8 }
    } else { 0u8 }
}

pub open spec fn running(s: State) -> bool { s.alive && s.top_pc < 7 }

/// One step of the load command (deterministic).
pub open spec fn step(c: Cfg, s: State) -> State {
    if !running(s) { s }
    else if active(s) == 0 { top_step(c, s) }
    else if active(s) == 1 { lnm_step(c, s, 0) }
    else if active(s) == 2 { lmr_step(c, s) }
    else { lnm_step(c, s, 1) }
}

/// Rollback of every open level at process end (N-P12, N-P18). Control 11: kept instead.
pub open spec fn end_rollback(c: Cfg, s: State) -> State {
    let s2 = if s.tx.len() > 0 && c.var != 11 { with_store(s, s.tx[0], Seq::empty()) } else { with_store(s, s.ns, Seq::empty()) };
    if c.var == 6 && s.ttx.len() > 0 { with_temp(s2, s.ttx[0], Seq::empty()) } else { s2 }
}

/// Kill-like end (SIGKILL): no %OnClose runs; IRIS rolls back the open transaction (N-P18);
/// IRISTEMP keeps its rows.
pub open spec fn kill(c: Cfg, s: State) -> State {
    with_death(end_rollback(c, s), 1)
}

/// %OnClose of the log in slot `i` at process termination: PersistToTwin when Init had linked
/// the twin (`linked`), then the temp row is deleted. Unlike the normal close it does not record
/// that the action ended.
pub open spec fn close_log(c: Cfg, s: State, i: usize, linked: bool) -> State {
    let s2 = if linked { persist(c, s, i) } else { s };
    put_temp(s2, i, t_dead(gtemp(s2.temp, i)))
}

/// Terminate-like end (`$SYSTEM.Process.Terminate`, HALT): %OnClose runs for live objects,
/// innermost frame first, at the current $TLEVEL (N-P10, N-P11), then IRIS rolls back the open
/// transaction (N-P12). Frames: the dependency's log (LoadModuleReference), the top
/// LoadNewModule's JournalManager, the top log (Main:Load).
pub open spec fn terminate(c: Cfg, s: State) -> State {
    let s1 = if s.top_pc == 3 && s.lnm0.pc == 11 && 2 <= s.lmr_pc && s.lmr_pc <= 8 {
        close_log(c, s, dslot(s), 3 <= s.lmr_pc && s.lmr_pc <= 7)
    } else { s };
    let s2 = if s.top_pc == 3 && s.lnm0.nj && s.lnm0.pc != 32 { with_jrn(s1, s.lnm0.jprev) } else { s1 };
    let s3 = if 1 <= s.top_pc && s.top_pc <= 6 { close_log(c, s2, 0, 2 <= s.top_pc && s.top_pc <= 5) } else { s2 };
    with_death(end_rollback(c, s3), 2)
}

/// The caller, after `Shell("load ...")` returned, commits or rolls back its transaction.
pub open spec fn caller_end(c: Cfg, s: State, commit: bool) -> State {
    if commit {
        with_end(with_store(s, s.ns, Seq::empty()), s.alive, 8, 1)
    } else {
        let s2 = if s.tx.len() > 0 { with_store(s, s.tx[0], Seq::empty()) } else { s };
        with_end(s2, s.alive, 8, 2)
    }
}

/// Another process edits History row `i` (SQL UPDATE or object save). Not part of this
/// process's transaction; the trigger refuses it when the stored row is finalized.
pub open spec fn user_edit(c: Cfg, s: State, i: usize) -> State {
    let tw = gtwin(s.ns.twins, i);
    if tw.fin && c.var != 10 { s }
    else {
        let t2 = Twin { ex: tw.ex, id: tw.id, pkg: tw.pkg, ver: tw.ver, ok: tw.ok, com: tw.com, tend: tw.tend, fin: tw.fin, ph: tw.ph, phst: tw.phst, edited: true };
        with_store(s, upd_twin(s.ns, i, t2), all_twin(s.tx, i, t2))
    }
}

/// A foreign edit is modelled only for a durable row this process has not modified in its open
/// transaction (where snapshot restore and IRIS undo agree).
pub open spec fn user_edit_enabled(s: State, i: usize) -> bool {
    i < 3 && gtwin(dv(s).twins, i).ex && gtwin(s.ns.twins, i) == gtwin(dv(s).twins, i)
}

pub open spec fn init(c: Cfg) -> State {
    let ns = Ns { mods: c.init, twins: Seq::empty().push(dtwin()).push(dtwin()).push(dtwin()) };
    let temps = Seq::empty().push(dtemp()).push(dtemp()).push(dtemp());
    State {
        ns: ns,
        tx: if c.caller_tx { Seq::empty().push(ns) } else { Seq::empty() },
        jrn: true,
        temp: temps,
        ttx: if c.caller_tx && c.var == 6 { Seq::empty().push(temps) } else { Seq::empty() },
        ctr: 0, alive: true, top_pc: 0, top_err: false, lnm0: dlnm(), lmr_pc: 0, lmr_err: false, lnm1: dlnm(), caller: 0,
        att: Seq::empty().push(0u8).push(0u8).push(0u8),
        how: 0,
    }
}

/// Configurations in the modelled domain.
pub open spec fn pkg_wf(p: Pkg) -> bool { 1 <= p.ver && p.ver <= 3 && p.act <= 2 && p.mf <= 4 }
pub open spec fn mod_wf(m: Mod) -> bool { m.ver <= 3 && m.wr == 0 && (m.present ==> (1 <= m.ver && m.doc == m.ver)) && (!m.present ==> (!m.dev && !m.res && m.doc == 0 && m.ver == 0)) }
pub open spec fn cfg_wf(c: Cfg) -> bool {
    &&& c.txm <= 2 && c.devf <= 2
    &&& pkg_wf(c.top)
    &&& c.deps.len() <= 2
    &&& (c.deps.len() >= 1 ==> pkg_wf(gpkg(c.deps, 0)))
    &&& (c.deps.len() >= 2 ==> pkg_wf(gpkg(c.deps, 1)))
    &&& c.init.len() == 3
    &&& mod_wf(gmod(c.init, 0)) && mod_wf(gmod(c.init, 1)) && mod_wf(gmod(c.init, 2))
    &&& (gmod(c.init, 1).present ==> (c.deps.len() >= 1 && gmod(c.init, 1).ver == gpkg(c.deps, 0).ver && !gmod(c.init, 1).dev))
    &&& (gmod(c.init, 2).present ==> (c.deps.len() >= 2 && gmod(c.init, 2).ver == gpkg(c.deps, 1).ver && !gmod(c.init, 2).dev))
    &&& (c.var == 0 || (1 <= c.var && c.var <= 12) || c.var == 102 || c.var == 103)
}

} // exec_spec_verified!

// Program counter names (the step functions use the literals).

// Main:Load (top_pc)
pub open spec fn T_TEMPINS() -> u8 { 0 }
pub open spec fn T_TWININS() -> u8 { 1 }
pub open spec fn T_PATH() -> u8 { 2 }
pub open spec fn T_INLNM() -> u8 { 3 }
pub open spec fn T_FIN() -> u8 { 4 }
pub open spec fn T_PERSIST() -> u8 { 5 }
pub open spec fn T_TDEL() -> u8 { 6 }
pub open spec fn T_DONE() -> u8 { 7 }
pub open spec fn T_CALLER() -> u8 { 8 }

// LoadModuleReference (lmr_pc)
pub open spec fn M_IDLE() -> u8 { 0 }
pub open spec fn M_TEMPINS() -> u8 { 1 }
pub open spec fn M_TWININS() -> u8 { 2 }
pub open spec fn M_SRC() -> u8 { 3 }
pub open spec fn M_CALL() -> u8 { 4 }
pub open spec fn M_INLNM() -> u8 { 5 }
pub open spec fn M_FIN() -> u8 { 6 }
pub open spec fn M_PERSIST() -> u8 { 7 }
pub open spec fn M_TDEL() -> u8 { 8 }
pub open spec fn M_RET() -> u8 { 9 }

// LoadNewModule (Lnm.pc). Phases: start/body/end of phase k at 14+3(k-1)+{0,1,2}.
pub open spec fn L_ENTER() -> u8 { 0 }
pub open spec fn L_BEGIN() -> u8 { 1 }
pub open spec fn L_GUARD() -> u8 { 2 }
pub open spec fn L_MARK() -> u8 { 3 }
pub open spec fn L_LOAD() -> u8 { 4 }
pub open spec fn L_NAMES() -> u8 { 5 }
pub open spec fn L_DEVSAVE() -> u8 { 6 }
pub open spec fn L_C1() -> u8 { 7 }
pub open spec fn L_C1MARK() -> u8 { 8 }
pub open spec fn L_C1START() -> u8 { 9 }
pub open spec fn L_SYSREQ() -> u8 { 10 }
pub open spec fn L_DEPS() -> u8 { 11 }
pub open spec fn L_RELOAD() -> u8 { 12 }
pub open spec fn L_C2() -> u8 { 13 }
pub open spec fn L_PH() -> u8 { 14 }
pub open spec fn L_C3() -> u8 { 29 }
pub open spec fn L_C3MARK() -> u8 { 30 }
pub open spec fn L_EXIT() -> u8 { 31 }
pub open spec fn L_RET() -> u8 { 32 }


} // verus!
