//! Explorer harness and case generator over the executable twins.
//!
//! Every transition here calls an `exec_*` function that `exec_spec_verified!` proved equal to
//! the spec function of the same name in `model.rs`/`contract.rs`. The harness adds only the
//! choice of action (step, process death, caller commit/rollback, a foreign edit), the domains
//! and the property checks.
use crate::contract::*;
use crate::model::*;
use explorer::{action_property, invariant, ActionProperty, Invariant, Model};

explorer::state_struct!(ExecPkg { ver, upd, sysreq, act, mf, snap, found });
explorer::state_struct!(ExecMod { present, ver, dev, res, doc, wr });
explorer::state_struct!(ExecCfg { txm, devf, force, path_ok, cleanup_fails, caller_tx, top, deps, init, var });
explorer::state_struct!(ExecTwin { ex, id, pkg, ver, ok, com, tend, fin, ph, phst, edited });
explorer::state_struct!(ExecNs { mods, twins });
explorer::state_struct!(ExecTemp { live, pkg, ver, ok, com, tend, ph, phst });
explorer::state_struct!(ExecLnm { pc, itl, dev, err, nj, jprev, di });
explorer::state_struct!(ExecState { ns, tx, jrn, temp, ttx, ctr, alive, top_pc, top_err, lnm0, lmr_pc, lmr_err, lnm1, caller, att, how });
explorer::state_struct!(ExecQ { q01_keep, q02_dev_bypass, q05_all, q06b_caller_survive, q08_exact, q14_end, q17_before, qfin_unended });
explorer::state_struct!(ExecMExp { kind, ver, dev });
explorer::state_struct!(ExecRExp { ex, ok, com, tend, fin, pkg, ph, phst });
explorer::state_struct!(ExecExpect { st, alts, recs, verdict });

// ------------------------------------------------------------------ builders

pub fn pkg(ver: u8) -> ExecPkg {
    ExecPkg { ver, upd: false, sysreq: true, act: 0, mf: 0, snap: false, found: true }
}
pub fn absent() -> ExecMod {
    ExecMod { present: false, ver: 0, dev: false, res: false, doc: 0, wr: 0 }
}
pub fn installed(ver: u8, dev: bool) -> ExecMod {
    ExecMod { present: true, ver, dev, res: true, doc: ver, wr: 0 }
}
pub fn base_cfg() -> ExecCfg {
    ExecCfg { txm: 0, devf: 0, force: false, path_ok: true, cleanup_fails: false, caller_tx: false, top: pkg(2), deps: vec![], init: vec![absent(), absent(), absent()], var: 0 }
}
pub fn qprov_x() -> ExecQ {
    exec_qprov()
}

/// The provisional Q and, for each open question, the alternative option.
pub fn q_alternatives() -> Vec<(&'static str, ExecQ)> {
    let p = qprov_x();
    let mut v = vec![];
    let mut q = p.clone(); q.q01_keep = false; v.push(("Q-01 reset to non-dev", q));
    let mut q = p.clone(); q.q02_dev_bypass = false; v.push(("Q-02 only -force bypasses", q));
    let mut q = p.clone(); q.q05_all = true; v.push(("Q-05 Committed exact for every action", q));
    let mut q = p.clone(); q.q06b_caller_survive = false; v.push(("Q-06b records may be lost with a caller-held transaction", q));
    let mut q = p.clone(); q.q08_exact = false; v.push(("Q-08 unended record Committed unconstrained (outcome unknown)", q));
    let mut q = p.clone(); q.q14_end = false; v.push(("Q-14 open phases accepted", q));
    let mut q = p.clone(); q.q17_before = false; v.push(("Q-17 kept manifest accepted", q));
    let mut q = p.clone(); q.qfin_unended = true; v.push(("Q-FIN unended record may be Finalized", q));
    v
}

// ------------------------------------------------------------------ runs

pub fn run_to_end(c: &ExecCfg) -> (ExecState, usize) {
    let mut s = exec_init(c);
    let mut n = 0;
    while exec_running(&s) && n < 1000 {
        s = exec_step(c, &s);
        n += 1;
    }
    (s, n)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KillPoint {
    /// Inside the Activate phase of the loaded module (after InitPhase, before its body).
    TopActivate,
    /// Inside the Activate phase of dependency `d`.
    DepActivate(u8),
    /// Right after the non-developer commit C3 of the loaded module (before SetCommitted).
    AfterC3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum End {
    Normal,
    CallerCommit,
    CallerRollback,
    /// SIGKILL-like: rollback, no %OnClose.
    Kill(KillPoint),
    /// Terminate/HALT-like: %OnClose runs, then rollback.
    Terminate(KillPoint),
}

fn at_kill_point(s: &ExecState, k: KillPoint) -> bool {
    let a = exec_active(s);
    match k {
        KillPoint::TopActivate => a == 1 && s.lnm0.pc == 27,
        KillPoint::DepActivate(d) => a == 3 && s.lnm1.pc == 27 && s.lnm0.di == d,
        KillPoint::AfterC3 => a == 1 && s.lnm0.pc == 30,
    }
}

/// Runs a case to its end. `None` if a kill point is never reached.
pub fn run_case(c: &ExecCfg, end: End) -> Option<(ExecState, Vec<ExecState>)> {
    let mut s = exec_init(c);
    let mut trace = vec![s.clone()];
    let mut n = 0;
    while exec_running(&s) && n < 1000 {
        if let End::Kill(k) | End::Terminate(k) = end {
            if at_kill_point(&s, k) {
                let d = if matches!(end, End::Kill(_)) { exec_kill(c, &s) } else { exec_terminate(c, &s) };
                trace.push(d.clone());
                return Some((d, trace));
            }
        }
        s = exec_step(c, &s);
        trace.push(s.clone());
        n += 1;
    }
    match end {
        End::Normal => Some((s, trace)),
        End::CallerCommit | End::CallerRollback => {
            if !c.caller_tx { return None; }
            let d = exec_caller_end(c, &s, end == End::CallerCommit);
            trace.push(d.clone());
            Some((d, trace))
        }
        End::Kill(_) | End::Terminate(_) => None,
    }
}

// ------------------------------------------------------------------ explorer model

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct XS {
    pub ci: u32,
    pub s: ExecState,
}

#[derive(Clone, Debug)]
pub enum Act {
    Step,
    Kill,
    Terminate,
    CallerCommit,
    CallerRollback,
    Edit(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Check {
    /// L1 end-state contract (status, namespace, every action's record).
    L1End,
    /// R1: a row in the durable History table is never removed by the load.
    R1RowKept,
    /// R2: a finalized durable row never changes.
    R2FinalUnchanged,
    /// R3 (top): once the top action started, its row is durable (no caller transaction).
    R3Top,
    /// R3 (dependencies): once a dependency load started, its row is durable.
    R3Dep,
    /// Committed is never 0 for a top-level action whose changes were kept.
    CommitSoundTop,
    /// Committed is never 1 for a top-level action that kept nothing.
    CommitExactTop,
    /// Each LoadNewModule returns at the $TLEVEL it was entered at.
    TLevel,
    /// PersistToTwin always finds its History row.
    PersistTarget,
    /// When Finalize runs with an OK status, the log already says Committed (so Finalize's
    /// `|| ISOK(status)` changes nothing on the load path).
    IsOkRedundant,
    /// L1 (every clause) at the normal end and after the caller ends its transaction.
    L1EndNormal,
    /// L1 status and namespace clauses at every end state, including process ends.
    L1NsAll,
}

pub struct LoadModel {
    pub cfgs: Vec<ExecCfg>,
    pub q: ExecQ,
    pub checks: Vec<Check>,
    pub edits: bool,
}

fn twin(ns: &ExecNs, i: usize) -> ExecTwin {
    exec_gtwin(&ns.twins, i)
}

pub fn check_state(c: &ExecCfg, q: &ExecQ, ch: Check, s: &ExecState) -> bool {
    match ch {
        Check::L1End => exec_l1_ok(c, q, s),
        Check::R3Top => c.caller_tx || exec_gatt(&s.att, 0) == 0 || twin(&exec_dv(s), 0).ex,
        Check::R3Dep => (1..3).all(|i| exec_gatt(&s.att, i) == 0 || twin(&exec_dv(s), i).ex),
        Check::CommitSoundTop => {
            c.caller_tx || exec_end_kind(s) != 1 || !exec_kept(&c.init, &s.ns.mods, 0) || twin(&s.ns, 0).com
        }
        Check::CommitExactTop => {
            c.caller_tx || exec_end_kind(s) != 1 || exec_kept(&c.init, &s.ns.mods, 0) || !twin(&s.ns, 0).com
        }
        Check::TLevel => {
            let a = !(s.alive && s.top_pc == 3 && s.lnm0.pc == 32) || s.tx.len() == s.lnm0.itl as usize;
            let b = !(s.alive && s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc == 5 && s.lnm1.pc == 32) || s.tx.len() == s.lnm1.itl as usize;
            a && b
        }
        Check::PersistTarget => {
            let a = !(s.alive && s.top_pc == 5) || twin(&s.ns, 0).ex;
            let b = !(s.alive && s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc == 7) || twin(&s.ns, exec_dslot(s)).ex;
            a && b
        }
        Check::L1EndNormal => {
            let k = exec_end_kind(s);
            k == 0 || k == 2 || exec_l1_ok(c, q, s)
        }
        Check::L1NsAll => {
            exec_end_kind(s) == 0 || { let e = exec_expect_for(c, q, s); exec_st_ok(&e, s) && exec_ns_ok(c, &e, s) }
        }
        Check::IsOkRedundant => {
            let a = !(s.alive && s.top_pc == 4 && !s.top_err) || exec_gtemp(&s.temp, 0).com;
            let b = !(s.alive && s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc == 6 && !s.lmr_err) || exec_gtemp(&s.temp, exec_dslot(s)).com;
            a && b
        }
        _ => true,
    }
}

/// Every end state of `c`: normal end, caller commit/rollback, and kill-like and terminate-like
/// ends after each step.
pub fn end_states(c: &ExecCfg) -> Vec<ExecState> {
    let mut s = exec_init(c);
    let mut out = vec![];
    let mut n = 0;
    while exec_running(&s) && n < 1000 {
        out.push(exec_kill(c, &s));
        out.push(exec_terminate(c, &s));
        s = exec_step(c, &s);
        n += 1;
    }
    if c.caller_tx {
        out.push(exec_caller_end(c, &s, true));
        out.push(exec_caller_end(c, &s, false));
    }
    out.push(s);
    out
}

/// The configurations of `domain` on which the as-built (variant 0) satisfies L1 at every end
/// state, re-labelled with variant `var`: a control evaluated here detects only what its
/// mutation changes.
pub fn conforming_domain(txms: &[u8], callers: &[bool], var: u8, ch: Check) -> Vec<ExecCfg> {
    let q = qprov_x();
    domain(txms, callers, 0).into_iter().filter(|c| end_states(c).iter().all(|e| check_state(c, &q, ch, e))).map(|mut c| { c.var = var; c }).collect()
}

pub fn check_step(ch: Check, a: &ExecState, b: &ExecState) -> bool {
    let da = exec_dv(a);
    let db = exec_dv(b);
    match ch {
        Check::R1RowKept => (0..3).all(|i| !twin(&da, i).ex || twin(&db, i).ex),
        Check::R2FinalUnchanged => (0..3).all(|i| !twin(&da, i).fin || twin(&db, i) == twin(&da, i)),
        _ => true,
    }
}

impl Model for LoadModel {
    type State = XS;
    type Action = Act;
    fn init_states(&self) -> Vec<XS> {
        self.cfgs.iter().enumerate().map(|(i, c)| XS { ci: i as u32, s: exec_init(c) }).collect()
    }
    fn actions(&self, _s: &XS) -> Vec<Act> {
        let mut v = vec![Act::Step, Act::Kill, Act::Terminate, Act::CallerCommit, Act::CallerRollback];
        if self.edits {
            v.extend((0..3).map(Act::Edit));
        }
        v
    }
    fn next(&self, x: &XS, a: &Act) -> Option<XS> {
        let c = &self.cfgs[x.ci as usize];
        let s = &x.s;
        let n = match a {
            Act::Step => if exec_running(s) { exec_step(c, s) } else { return None },
            Act::Kill => if exec_running(s) { exec_kill(c, s) } else { return None },
            Act::Terminate => if exec_running(s) { exec_terminate(c, s) } else { return None },
            Act::CallerCommit | Act::CallerRollback => {
                if c.caller_tx && s.alive && s.top_pc == 7 { exec_caller_end(c, s, matches!(a, Act::CallerCommit)) } else { return None }
            }
            Act::Edit(i) => {
                if s.alive && exec_user_edit_enabled(s, *i) && !twin(&s.ns, *i).edited { exec_user_edit(c, s, *i) } else { return None }
            }
        };
        Some(XS { ci: x.ci, s: n })
    }
    fn invariants(&self) -> Vec<Invariant<XS>> {
        let mut v = vec![];
        for &ch in &self.checks {
            if matches!(ch, Check::R1RowKept | Check::R2FinalUnchanged) { continue; }
            let cfgs = self.cfgs.clone();
            let q = self.q.clone();
            v.push(invariant(check_name(ch), move |x: &XS| check_state(&cfgs[x.ci as usize], &q, ch, &x.s)));
        }
        v
    }
    fn action_properties(&self) -> Vec<ActionProperty<XS, Act>> {
        let mut v = vec![];
        for &ch in &self.checks {
            if matches!(ch, Check::R1RowKept | Check::R2FinalUnchanged) {
                v.push(action_property(check_name(ch), move |a: &XS, _: &Act, b: &XS| check_step(ch, &a.s, &b.s)));
            }
        }
        v
    }
}

pub fn check_name(ch: Check) -> &'static str {
    match ch {
        Check::L1End => "L1-END (status, namespace and history at every end state)",
        Check::R1RowKept => "R1 durable History row never removed",
        Check::R2FinalUnchanged => "R2 finalized durable History row never changes",
        Check::R3Top => "R3-top started top-level action has a durable row",
        Check::R3Dep => "R3-dep started dependency load has a durable row",
        Check::CommitSoundTop => "COMMIT-SOUND top-level Committed=0 implies nothing kept",
        Check::CommitExactTop => "COMMIT-EXACT top-level Committed=1 implies something kept",
        Check::TLevel => "TLEVEL LoadNewModule returns at its entry $TLEVEL",
        Check::PersistTarget => "PERSIST-TARGET PersistToTwin finds its row",
        Check::IsOkRedundant => "ISOK-REDUNDANT an OK Finalize finds Committed already set",
        Check::L1EndNormal => "L1-END-NORMAL every L1 clause at the normal end and after the caller ends",
        Check::L1NsAll => "L1-NS-ALL status and namespace clauses at every end state (including process ends)",
    }
}

// ------------------------------------------------------------------ domains

/// Faults injected one at a time on top of each mode, nesting and installed state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fault {
    None,
    Path,
    ManifestParse,
    ManifestNoModule,
    ManifestInvalid,
    UpdatePackage,
    UpdatePackageForce,
    SysReq,
    Activate,
    ActivateReturned,
    Cleanup,
    DepActivate(u8),
    DepManifestInvalid,
    DepNotFound,
    DepSysReq,
}

pub const ALL_FAULTS: [Fault; 16] = [
    Fault::None, Fault::Path, Fault::ManifestParse, Fault::ManifestNoModule, Fault::ManifestInvalid,
    Fault::UpdatePackage, Fault::UpdatePackageForce, Fault::SysReq, Fault::Activate, Fault::ActivateReturned,
    Fault::Cleanup, Fault::DepActivate(0), Fault::DepActivate(1), Fault::DepManifestInvalid, Fault::DepNotFound, Fault::DepSysReq,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Deps {
    None,
    One,
    OneInstalled,
    Two,
    OneSnapshot,
}

pub const ALL_DEPS: [Deps; 5] = [Deps::None, Deps::One, Deps::OneInstalled, Deps::Two, Deps::OneSnapshot];

pub fn top_inits() -> Vec<ExecMod> {
    vec![absent(), installed(1, false), installed(2, false), installed(2, true), installed(3, false)]
}

pub fn make_cfg(txm: u8, devf: u8, caller_tx: bool, top_init: &ExecMod, deps: Deps, fault: Fault, var: u8) -> Option<ExecCfg> {
    let mut c = base_cfg();
    c.txm = txm;
    c.devf = devf;
    c.caller_tx = caller_tx;
    c.var = var;
    c.init[0] = top_init.clone();
    match deps {
        Deps::None => {}
        Deps::One => c.deps = vec![pkg(1)],
        Deps::OneInstalled => { c.deps = vec![pkg(1)]; c.init[1] = installed(1, false); }
        Deps::Two => c.deps = vec![pkg(1), pkg(1)],
        Deps::OneSnapshot => { let mut d = pkg(1); d.snap = true; c.deps = vec![d]; }
    }
    let needs_dep = matches!(deps, Deps::One | Deps::Two | Deps::OneSnapshot);
    match fault {
        Fault::None => {}
        Fault::Path => c.path_ok = false,
        Fault::ManifestParse => c.top.mf = 1,
        Fault::ManifestNoModule => c.top.mf = 2,
        Fault::ManifestInvalid => c.top.mf = 3,
        Fault::UpdatePackage => c.top.upd = true,
        Fault::UpdatePackageForce => { c.top.upd = true; c.force = true; }
        Fault::SysReq => c.top.sysreq = false,
        Fault::Activate => c.top.act = 1,
        Fault::ActivateReturned => c.top.act = 2,
        Fault::Cleanup => c.cleanup_fails = true,
        Fault::DepActivate(d) => {
            if !needs_dep || (d as usize) >= c.deps.len() { return None; }
            c.deps[d as usize].act = 1;
        }
        Fault::DepManifestInvalid => { if !needs_dep { return None; } c.deps[0].mf = 3; }
        Fault::DepNotFound => { if !needs_dep { return None; } c.deps[0].found = false; }
        Fault::DepSysReq => { if !needs_dep { return None; } c.deps[0].sysreq = false; }
    }
    // An UpdatePackage fault is meaningful only when upgrading.
    if matches!(fault, Fault::UpdatePackage | Fault::UpdatePackageForce) && !(top_init.present && top_init.ver < c.top.ver) {
        return None;
    }
    Some(c)
}

/// The single-fault domain: every mode (3 transaction modes x 3 dev flags), with and without a
/// caller transaction, 5 installed states of the top module, 5 dependency shapes, 16 faults.
pub fn domain(txms: &[u8], callers: &[bool], var: u8) -> Vec<ExecCfg> {
    let mut v = vec![];
    for &txm in txms {
        for devf in 0..3u8 {
            for &ct in callers {
                for ti in top_inits() {
                    for &d in ALL_DEPS.iter() {
                        for &f in ALL_FAULTS.iter() {
                            if let Some(c) = make_cfg(txm, devf, ct, &ti, d, f, var) {
                                v.push(c);
                            }
                        }
                    }
                }
            }
        }
    }
    v
}

// ------------------------------------------------------------------ observation rendering

pub fn ver_str(v: u8) -> String {
    if v == 0 { "0.0.0-0 (placeholder)".to_string() } else { format!("{}.0.0", v) }
}

pub fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn mod_json(m: &ExecMod) -> String {
    if !m.present {
        format!("{{\"manifest_present\": false, \"zpm_document_version\": {}, \"writes\": {}}}", if m.doc == 0 { "null".to_string() } else { format!("\"{}\"", ver_str(m.doc)) }, m.wr)
    } else {
        format!("{{\"manifest_present\": true, \"version\": \"{}\", \"developer_mode\": {}, \"resources_loaded\": {}, \"zpm_document_version\": \"{}\", \"writes\": {}}}",
            ver_str(m.ver), m.dev, m.res, ver_str(m.doc), m.wr)
    }
}

pub fn status_str(ok: u8) -> &'static str {
    match ok { 0 => "0 (in progress / never finalized)", 1 => "OK", _ => "error" }
}

pub fn phst_str(p: u8) -> &'static str {
    match p { 0 => "open (no TimeEnd)", 1 => "OK", _ => "error" }
}

pub fn twin_json(t: &ExecTwin) -> String {
    if !t.ex {
        "{\"row\": false}".to_string()
    } else {
        format!("{{\"row\": true, \"id_order\": {}, \"package_set\": {}, \"version\": \"{}\", \"success\": \"{}\", \"committed\": {}, \"time_end_set\": {}, \"finalized\": {}, \"phases_started\": {}, \"last_phase_status\": {}}}",
            t.id, t.pkg, ver_str(t.ver), status_str(t.ok), t.com as u8, t.tend, t.fin, t.ph,
            if t.ph == 0 { "null".to_string() } else { format!("\"{}\"", phst_str(t.phst)) })
    }
}

pub fn temp_json(t: &ExecTemp) -> String {
    if !t.live { "{\"row\": false}".to_string() } else {
        format!("{{\"row\": true, \"package_set\": {}, \"success\": \"{}\", \"committed\": {}, \"time_end_set\": {}, \"phases_started\": {}}}", t.pkg, status_str(t.ok), t.com as u8, t.tend, t.ph)
    }
}

pub fn obs_json(c: &ExecCfg, s: &ExecState) -> String {
    let kind = exec_end_kind(s);
    let status = match kind {
        1 | 3 | 4 => if s.top_err { "\"error\"" } else { "\"OK\"" },
        _ => "null",
    };
    let names = ["top", "dep1", "dep2"];
    let mut mods = vec![];
    let mut rows = vec![];
    let mut temps = vec![];
    for i in 0..3 {
        if i > 0 && i > c.deps.len() { continue; }
        mods.push(format!("\"{}\": {}", names[i], mod_json(&s.ns.mods[i])));
        rows.push(format!("\"{}\": {}", names[i], twin_json(&s.ns.twins[i])));
        temps.push(format!("\"{}\": {}", names[i], temp_json(&s.temp[i])));
    }
    let nrows = (0..3).filter(|&i| s.ns.twins[i].ex).count();
    format!("{{\"end\": \"{}\", \"status\": {}, \"open_transaction_levels\": {}, \"modules\": {{{}}}, \"history_rows\": {}, \"history\": {{{}}}, \"history_temp\": {{{}}}}}",
        match kind { 1 => "returned", 2 => if s.how == 2 { "process terminated (%OnClose ran, then rollback)" } else { "process killed (no %OnClose, rollback)" }, 3 => "returned; caller committed", 4 => "returned; caller rolled back", _ => "running" },
        status, s.tx.len(), mods.join(", "), nrows, rows.join(", "), temps.join(", "))
}

pub fn mexp_json(e: &ExecMExp) -> String {
    let dev = match e.dev { 0 => "false", 1 => "true", _ => "\"any\"" };
    match e.kind {
        0 => "\"unchanged\"".to_string(),
        1 => format!("{{\"installed\": \"{}\", \"developer_mode\": {}}}", ver_str(e.ver), dev),
        2 => format!("{{\"manifest_only\": \"{}\", \"developer_mode\": {}, \"resources\": \"as before\"}}", ver_str(e.ver), dev),
        3 => "\"any (partial changes allowed)\"".to_string(),
        4 => format!("{{\"manifest_present\": \"{}\", \"developer_mode\": {}, \"resources\": \"any\"}}", ver_str(e.ver), dev),
        5 => format!("{{\"unchanged_or_present\": \"{}\", \"developer_mode\": {}}}", ver_str(e.ver), dev),
        _ => format!("{{\"unchanged_or_installed\": \"{}\", \"developer_mode\": {}}}", ver_str(e.ver), dev),
    }
}

fn tri(x: u8) -> &'static str { match x { 0 => "false", 1 => "true", _ => "\"any\"" } }

pub fn rexp_json(e: &ExecRExp) -> String {
    if e.ex == 0 { return "{\"row\": false}".to_string(); }
    let ok = match e.ok { 0 => "\"0 (in progress)\"", 1 => "\"OK\"", 2 => "\"error\"", _ => "\"any\"" };
    let com = match e.com { 0 => "0", 1 => "1", 2 => "\"any\"", _ => "\"equals whether the action's changes were kept\"" };
    let ph = if e.ph == 255 { "\"any\"".to_string() } else { e.ph.to_string() };
    let phst = match e.phst { 255 => "\"any\"", 0 => "\"open\"", 1 => "\"OK\"", _ => "\"error\"" };
    format!("{{\"row\": {}, \"success\": {}, \"committed\": {}, \"time_end_set\": {}, \"finalized\": {}, \"package_set\": {}, \"phases_started\": {}, \"last_phase_status\": {}}}",
        if e.ex == 1 { "true" } else { "\"required unless Q alternative; may be missing\"" }, ok, com, tri(e.tend), tri(e.fin), tri(e.pkg), ph, phst)
}

pub fn expect_json(c: &ExecCfg, e: &ExecExpect) -> String {
    let names = ["top", "dep1", "dep2"];
    let alts: Vec<String> = e.alts.iter().map(|a| {
        let parts: Vec<String> = (0..3).filter(|&i| i == 0 || i <= c.deps.len()).map(|i| format!("\"{}\": {}", names[i], mexp_json(&a[i]))).collect();
        format!("{{{}}}", parts.join(", "))
    }).collect();
    let recs: Vec<String> = (0..3).filter(|&i| i == 0 || i <= c.deps.len()).map(|i| format!("\"{}\": {}", names[i], rexp_json(&e.recs[i]))).collect();
    format!("{{\"verdict\": \"{}\", \"status\": {}, \"modules_one_of\": [{}], \"history\": {{{}}}}}",
        verdict_str(e.verdict), match e.st { 0 => "\"any\"", 1 => "\"OK\"", _ => "\"error\"" }, alts.join(", "), recs.join(", "))
}

pub fn verdict_str(v: u8) -> &'static str {
    match v {
        0 => "success", 1 => "refused: path", 2 => "refused: manifest", 3 => "refused: downgrade", 4 => "refused: update required",
        5 => "refused: system requirements", 6 => "failed: dependency", 7 => "failed: Activate phase", _ => "failed after the load completed (CleanUp)",
    }
}

/// Which L1 clauses an end state violates (empty: conforms).
pub fn violated_clauses(c: &ExecCfg, q: &ExecQ, s: &ExecState) -> Vec<String> {
    if exec_end_kind(s) == 0 { return vec![]; }
    let e = exec_expect_for(c, q, s);
    let names = ["top", "dep1", "dep2"];
    let mut v = vec![];
    if !exec_st_ok(&e, s) { v.push("L1-STATUS".to_string()); }
    if !exec_ns_ok(c, &e, s) { v.push("L1-NAMESPACE".to_string()); }
    for i in 0..3 {
        let r = exec_grx(&e.recs, i);
        let t = twin(&s.ns, i);
        if !exec_rec_exists_ok(&r, &t) { v.push(format!("H-RECORD-EXISTS[{}]", names[i])); }
        if !exec_rec_fields_ok(&r, &t) { v.push(format!("H-RECORD-FIELDS[{}]", names[i])); }
        if !exec_rec_com_ok(&r, &t, exec_kept(&c.init, &s.ns.mods, i)) { v.push(format!("H-COMMITTED[{}]", names[i])); }
    }
    v
}

/// The Q alternatives under which this end state's verdict differs from the provisional one.
pub fn q_sensitivity(c: &ExecCfg, s: &ExecState) -> Vec<String> {
    let base = exec_l1_ok(c, &qprov_x(), s);
    q_alternatives().into_iter().filter_map(|(n, q)| {
        let ok = exec_l1_ok(c, &q, s);
        if ok != base { Some(format!("{}: {}", n, if ok { "conforms" } else { "violates" })) } else { None }
    }).collect()
}
