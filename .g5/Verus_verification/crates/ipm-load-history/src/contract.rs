//! L1: what a `load` must leave in the namespace and in the history log, independent of the
//! code at 571e04a1. Sources per clause are in `B/model-notes.md` (section L1).
//!
//! The verdict of a load is computed from its inputs alone (`l1_verdict`). The required final
//! observation is an `Expect`: the command's status, alternatives for the module states, and
//! one record expectation per action slot. Unresolved intent is the `Q` parameter; `qprov()`
//! is the provisional option of each open question.
//!
//! The abstraction from L2 is `dv` (the durable History table and module store) plus the
//! auxiliary `att` (which actions have started). `l1_ok` checks an end state (command
//! returned, caller finished, or process dead). The step obligations R1-R3 are in `proofs.rs`.
use vstd::prelude::*;
use vstd::contrib::exec_spec::*;
use crate::model::*;

verus! {

exec_spec_verified! {

/// Open questions carried as parameters (A1 section 4).
pub struct Q {
    /// Q-01: a plain load keeps the stored developer mode (provisional true).
    pub q01_keep: bool,
    /// Q-02: `-dev` bypasses the UpdatePackage guard (provisional true).
    pub q02_dev_bypass: bool,
    /// Q-05 (scope revised by stage C R1-MOD-01): under a caller-held transaction, Committed must be
    /// exact (provisional false: not constrained there, because the caller decides later). With no
    /// caller transaction Committed is always required to equal "changes kept", for the top
    /// action and for every dependency (`history.committed`, R1-ONT-01).
    pub q05_caller_exact: bool,
    /// Q-06b (stage C R1-ONT-01): the load's records (top and dependencies) must survive a
    /// rollback of a caller-held transaction, by the caller or by process death (provisional true).
    /// Survival of a dependency's record under its parent's rollback is no longer a Q: it is
    /// required (issue #366).
    pub q06b_caller_survive: bool,
    /// Q-08 (default changed by stage C R1-ONT-02): Committed on a record whose action never
    /// ended must reflect whether its changes were kept (provisional true, #1237 and the wiki/09
    /// recipe). Alternative false: such a record's Committed is unconstrained (outcome unknown).
    pub q08_exact: bool,
    /// Q-14: every started phase must end with its status (provisional true).
    pub q14_end: bool,
    /// Q-17: system requirements are checked before anything is written, in every mode
    /// (provisional true).
    pub q17_before: bool,
    /// Q-FIN (stage C R1-ONT-02): a record whose action never ended (no TimeEnd) may be
    /// Finalized (provisional false).
    pub qfin_unended: bool,
}

pub open spec fn qprov() -> Q {
    Q { q01_keep: true, q02_dev_bypass: true, q05_caller_exact: false, q06b_caller_survive: true, q08_exact: true, q14_end: true, q17_before: true, qfin_unended: false }
}

/// One module's required final state.
/// kind: 0 unchanged; 1 installed (present at `ver`, resources loaded); 2 manifest only (present
/// at `ver`, document written, resources as before); 3 any; 4 present at `ver` (resources any);
/// 5 unchanged or present at `ver`; 6 unchanged or installed.
/// dev: 0 DeveloperMode off, 1 on, 2 any.
pub struct MExp { pub kind: u8, pub ver: u8, pub dev: u8 }

/// One action's required History row.
/// ex: 0 no row, 1 a row, 2 a row may be missing (if present the other fields apply).
/// ok: 0 in progress, 1 OK, 2 error, 3 any. com: 0, 1, 2 any, 3 equals "changes kept".
/// tend, fin, pkg: 0, 1, 2 any. ph: number of phases started (255 any). phst: last phase
/// status (0 open, 1 OK, 2 error, 255 any).
pub struct RExp { pub ex: u8, pub ok: u8, pub com: u8, pub tend: u8, pub fin: u8, pub pkg: u8, pub ph: u8, pub phst: u8 }

pub struct Expect {
    /// 0 any, 1 OK, 2 error.
    pub st: u8,
    /// Each alternative is one MExp per module slot (3).
    pub alts: Seq<Seq<MExp>>,
    /// One per action slot (3).
    pub recs: Seq<RExp>,
    /// L1 verdict code (see `l1_verdict`).
    pub verdict: u8,
}

// ------------------------------------------------------------------ verdict from inputs

pub open spec fn inst(c: Cfg, i: usize) -> Mod { gmod(c.init, i) }
pub open spec fn dep(c: Cfg, d: usize) -> Pkg { gpkg(c.deps, d) }

/// The load runs in developer mode: `-dev`, or neither flag on a module stored in developer
/// mode when the stored mode is kept (Q-01).
pub open spec fn dev_mode(c: Cfg, q: Q) -> bool {
    c.devf == 1 || (c.devf == 0 && q.q01_keep && inst(c, 0).present && inst(c, 0).dev)
}

/// The final DeveloperMode of the loaded module.
pub open spec fn final_dev(c: Cfg, q: Q) -> u8 { if dev_mode(c, q) { 1u8 } else { 0u8 } }

pub open spec fn needed(c: Cfg, d: usize) -> bool {
    d < c.deps.len() && d < 2 && !(inst(c, if d == 0 { 1usize } else { 2usize }).present && inst(c, if d == 0 { 1usize } else { 2usize }).ver == dep(c, d).ver)
}

pub open spec fn dep_fails(c: Cfg, d: usize) -> bool {
    dep(c, d).mf != 0 || !dep(c, d).sysreq || dep(c, d).act != 0
}

/// Index of the first needed dependency that fails (2 if none).
pub open spec fn first_dep_fail(c: Cfg) -> usize {
    if needed(c, 0) && dep_fails(c, 0) { 0usize } else if needed(c, 1) && dep_fails(c, 1) { 1usize } else { 2usize }
}

/// 0 success; 1 path refused; 2 manifest refused; 3 downgrade refused; 4 update-required
/// refused; 5 system requirements refused; 6 dependency failure (not found, or a dependency load
/// failed); 7 phase (Activate) failure; 8 failure after the load completed (CleanUp).
pub open spec fn l1_verdict(c: Cfg, q: Q) -> u8 {
    let cur = inst(c, 0);
    if !c.path_ok { 1u8 }
    else if c.top.mf != 0 { 2u8 }
    else if cur.present && cur.ver > c.top.ver { 3u8 }
    else if cur.present && c.top.ver > cur.ver && c.top.upd && !c.force && !(c.devf == 1 && q.q02_dev_bypass) { 4u8 }
    else if !c.top.sysreq { 5u8 }
    else if !all_found(c) || first_dep_fail(c) < 2 { 6u8 }
    else if c.top.act != 0 { 7u8 }
    else if c.cleanup_fails { 8u8 }
    else { 0u8 }
}

/// Dependency `d` is attempted: the load reached dependency loading, all were found, it is
/// needed, and no earlier dependency failed.
pub open spec fn attempted(c: Cfg, q: Q, d: usize) -> bool {
    let v = l1_verdict(c, q);
    (v == 0 || v == 6 || v == 7 || v == 8) && all_found(c) && needed(c, d) && first_dep_fail(c) >= d
}

// ------------------------------------------------------------------ expected namespace

pub open spec fn mx(kind: u8, ver: u8, dev: u8) -> MExp { MExp { kind: kind, ver: ver, dev: dev } }
pub open spec fn unch() -> MExp { mx(0, 0, 2) }
pub open spec fn anym() -> MExp { mx(3, 0, 2) }
pub open spec fn three(a: MExp, b: MExp, d: MExp) -> Seq<MExp> { Seq::empty().push(a).push(b).push(d) }

/// A dependency's state after its successful load.
pub open spec fn dep_installed(c: Cfg, d: usize) -> MExp {
    if needed(c, d) { mx(1, dep(c, d).ver, if dep(c, d).snap && c.devf == 1 { 1u8 } else { 0u8 }) } else { unch() }
}

pub open spec fn success_alt(c: Cfg, q: Q) -> Seq<MExp> {
    three(mx(1, c.top.ver, final_dev(c, q)), dep_installed(c, 0), dep_installed(c, 1))
}

pub open spec fn unchanged_alt() -> Seq<MExp> { three(unch(), unch(), unch()) }
pub open spec fn any_alt() -> Seq<MExp> { three(anym(), anym(), anym()) }

/// Required module states after the command returned with verdict `v`.
pub open spec fn l1_alts(c: Cfg, q: Q) -> Seq<Seq<MExp>> {
    let v = l1_verdict(c, q);
    let dev = dev_mode(c, q);
    let tx = c.txm == 0;
    if v == 0 { Seq::empty().push(success_alt(c, q)) }
    // refusals (path, manifest, downgrade, update required) write no module data in any mode
    // (`load.refusal`, R1-MOD-02)
    else if v == 1 || v == 2 || v == 3 || v == 4 { Seq::empty().push(unchanged_alt()) }
    else if v == 5 && q.q17_before { Seq::empty().push(unchanged_alt()) }
    else if !tx { Seq::empty().push(any_alt()) }
    else if !dev { Seq::empty().push(unchanged_alt()) }
    // developer mode with a transaction: what was committed before the failure point is kept
    else if v == 2 { Seq::empty().push(unchanged_alt()) }
    else if v == 5 || v == 6 { Seq::empty().push(three(mx(2, c.top.ver, 1), unch(), unch())) }
    // a failure in or after the phases (verdict 7, 8): the phases ran outside a transaction, so
    // their work stays: module present at its version with its resources loaded
    // (`load.outcome.dev`, `dev-mode.keep-on-failure`, wiki/00 L126; stage C R2-01)
    else { Seq::empty().push(three(mx(1, c.top.ver, 1), dep_installed(c, 0), dep_installed(c, 1))) }
}

/// Module states a killed load may leave (`att0`: the top action had started).
pub open spec fn l1_alts_kill(c: Cfg, q: Q) -> Seq<Seq<MExp>> {
    let v = l1_verdict(c, q);
    if v == 1 || v == 2 || v == 3 || v == 4 || (v == 5 && q.q17_before) { Seq::empty().push(unchanged_alt()) }
    else if c.txm != 0 { Seq::empty().push(any_alt()) }
    else if dev_mode(c, q) {
        Seq::empty().push(three(mx(5, c.top.ver, 1), mx(6, dep(c, 0).ver, 2), mx(6, dep(c, 1).ver, 2)))
    }
    else if v == 0 || v == 8 { Seq::empty().push(unchanged_alt()).push(success_alt(c, q)) }
    else { Seq::empty().push(unchanged_alt()) }
}

// ------------------------------------------------------------------ expected history

pub open spec fn rx(ex: u8, ok: u8, com: u8, tend: u8, fin: u8, pkg: u8, ph: u8, phst: u8) -> RExp {
    RExp { ex: ex, ok: ok, com: com, tend: tend, fin: fin, pkg: pkg, ph: ph, phst: phst }
}
pub open spec fn norec() -> RExp { rx(0, 3, 2, 2, 2, 2, 255, 255) }

/// The top action's row after the command returned.
pub open spec fn top_rec(c: Cfg, q: Q) -> RExp {
    let v = l1_verdict(c, q);
    let com = if !c.caller_tx || q.q05_caller_exact { 3u8 } else { 2u8 };
    let ok = if v == 0 { 1u8 } else { 2u8 };
    if v == 0 || v == 8 { rx(1, ok, com, 1, 1, 1, 5, 1) }
    else if v == 7 { rx(1, ok, com, 1, 1, 1, 5, if q.q14_end { 2u8 } else { 255u8 }) }
    // refused or failed after the manifest was read: the record names the package (R1-ONT-03)
    else if v >= 3 { rx(1, ok, com, 1, 1, 1, 255, 255) }
    // a manifest refused after GetModuleObjectFromPath read the <Module> element (validation
    // refusal, multiple documents) names the package too (R1-MOD-02)
    else if v == 2 && c.top.mf >= 3 { rx(1, ok, com, 1, 1, 1, 255, 255) }
    else { rx(1, ok, com, 1, 1, 2, 255, 255) }
}

/// A dependency's row after the command returned.
pub open spec fn dep_rec(c: Cfg, q: Q, d: usize) -> RExp {
    if !attempted(c, q, d) { norec() }
    else {
        let fails = dep_fails(c, d);
        let phased = dep(c, d).mf == 0 && dep(c, d).sysreq;
        // required under the parent's rollback (#366); under a caller-held transaction the
        // record is observed before the caller decides
        let ex = 1u8;
        // a dependency undone by its parent's rollback reads uncommitted; one whose writes were
        // kept reads committed (`history.committed`, R1-MOD-01)
        let com = if !c.caller_tx || q.q05_caller_exact { 3u8 } else { 2u8 };
        let ph = if phased { 5u8 } else { 255u8 };
        let phst = if !fails { 1u8 } else if phased && q.q14_end { 2u8 } else { 255u8 };
        rx(ex, if fails { 2u8 } else { 1u8 }, com, 1, 1, 1, ph, phst)
    }
}

pub open spec fn l1_expect(c: Cfg, q: Q) -> Expect {
    let v = l1_verdict(c, q);
    Expect {
        st: if v == 0 { 1u8 } else { 2u8 },
        alts: l1_alts(c, q),
        recs: Seq::empty().push(top_rec(c, q)).push(dep_rec(c, q, 0)).push(dep_rec(c, q, 1)),
        verdict: v,
    }
}

/// Abstraction of the L2 control state for a dead process: `att` says whether each action had
/// started (History row inserted, 1), had its status decided (Finalize ran, 2), and had its row
/// completed by PersistToTwin after that (3). Only a completed row must carry the outcome; a
/// started row must say "in progress" (Success 0, no TimeEnd), whether or not it is Finalized.
pub open spec fn kill_rec(c: Cfg, q: Q, s: State, i: usize) -> RExp {
    let a = gatt(s.att, i);
    let t = gtemp(s.temp, i);
    let must = !c.caller_tx || q.q06b_caller_survive;
    let ex = if must { 1u8 } else { 2u8 };
    let exact = !c.caller_tx || q.q05_caller_exact;
    let st = if i == 0 { if s.top_err { 2u8 } else { 1u8 } } else if dep_fails(c, if i == 1 { 0usize } else { 1usize }) { 2u8 } else { 1u8 };
    if a == 0 { rx(2, 3, 2, 2, 2, 2, 255, 255) }
    // a completed row carries the outcome; a dependency's row may wait for its parent's end to
    // be finalized, because its Committed depends on the parent's outcome
    else if a == 3 { rx(ex, st, if exact { 3u8 } else { 2u8 }, 1, if i == 0 { 1u8 } else { 2u8 }, 2, 255, 255) }
    // an unended row says in progress, lists the phases its action started (the open one without
    // an end), and names the package once it was known (`history.record`, R1-MOD-03)
    else {
        rx(ex, 0, if q.q08_exact && exact { 3u8 } else { 2u8 }, 0, if q.qfin_unended { 2u8 } else { 0u8 },
            if t.pkg { 1u8 } else { 2u8 }, t.ph, if t.ph == 0 { 255u8 } else { t.phst })
    }
}

/// After the caller's transaction is undone (caller rollback, or death with a caller
/// transaction) the namespace is as before, except that writes made with journaling off cannot
/// be undone (no IPM requirement then).
pub open spec fn caller_undo_alts(c: Cfg) -> Seq<Seq<MExp>> {
    if c.txm == 2 { Seq::empty().push(any_alt()) } else { Seq::empty().push(unchanged_alt()) }
}

/// Expectation for a process that died during the load.
pub open spec fn l1_expect_kill(c: Cfg, q: Q, s: State) -> Expect {
    Expect {
        st: 0,
        alts: if c.caller_tx { caller_undo_alts(c) } else { l1_alts_kill(c, q) },
        recs: Seq::empty().push(kill_rec(c, q, s, 0)).push(kill_rec(c, q, s, 1)).push(kill_rec(c, q, s, 2)),
        verdict: l1_verdict(c, q),
    }
}

// ------------------------------------------------------------------ matching

pub open spec fn dev_ok(e: u8, d: bool) -> bool { e == 2 || (e == 1) == d }

pub open spec fn installed_ok(e: MExp, m: Mod) -> bool { m.present && m.ver == e.ver && m.res && dev_ok(e.dev, m.dev) }

pub open spec fn mexp_ok(e: MExp, i: Mod, m: Mod) -> bool {
    if e.kind == 0 { m == i }
    else if e.kind == 1 { installed_ok(e, m) }
    else if e.kind == 2 { m.present && m.ver == e.ver && m.doc == e.ver && m.res == i.res && dev_ok(e.dev, m.dev) }
    else if e.kind == 3 { true }
    else if e.kind == 4 { m.present && m.ver == e.ver && dev_ok(e.dev, m.dev) }
    else if e.kind == 5 { m == i || (m.present && m.ver == e.ver && dev_ok(e.dev, m.dev)) }
    else { m == i || installed_ok(e, m) }
}

pub open spec fn gmx(a: Seq<MExp>, i: usize) -> MExp { if i < a.len() { a[i as int] } else { anym() } }

pub open spec fn alt_ok(a: Seq<MExp>, init: Seq<Mod>, ms: Seq<Mod>) -> bool {
    mexp_ok(gmx(a, 0), gmod(init, 0), gmod(ms, 0)) && mexp_ok(gmx(a, 1), gmod(init, 1), gmod(ms, 1)) && mexp_ok(gmx(a, 2), gmod(init, 2), gmod(ms, 2))
}

pub open spec fn galt(a: Seq<Seq<MExp>>, i: usize) -> Seq<MExp> { if i < a.len() { a[i as int] } else { Seq::empty() } }

pub open spec fn alts_ok(a: Seq<Seq<MExp>>, init: Seq<Mod>, ms: Seq<Mod>) -> bool {
    (a.len() > 0 && alt_ok(galt(a, 0), init, ms)) || (a.len() > 1 && alt_ok(galt(a, 1), init, ms)) || (a.len() > 2 && alt_ok(galt(a, 2), init, ms))
}

pub open spec fn v3(e: u8, b: bool) -> bool { e == 2 || (e == 1) == b }

/// The action in slot `i` changed the namespace (for the top action: any module).
pub open spec fn kept(init: Seq<Mod>, ms: Seq<Mod>, i: usize) -> bool {
    if i == 0 { !(gmod(ms, 0) == gmod(init, 0)) || !(gmod(ms, 1) == gmod(init, 1)) || !(gmod(ms, 2) == gmod(init, 2)) }
    else { !(gmod(ms, i) == gmod(init, i)) }
}

/// Row existence clause.
pub open spec fn rec_exists_ok(e: RExp, t: Twin) -> bool {
    if e.ex == 0 { !t.ex } else if e.ex == 1 { t.ex } else { true }
}

/// Row content clause (status, TimeEnd, Finalized, package, phases) for a present row.
pub open spec fn rec_fields_ok(e: RExp, t: Twin) -> bool {
    !t.ex || (
        (e.ok == 3 || t.ok == e.ok) && v3(e.tend, t.tend) && v3(e.fin, t.fin) && v3(e.pkg, t.pkg)
        && (e.ph == 255 || t.ph == e.ph) && (e.phst == 255 || t.phst == e.phst))
}

/// Committed clause for a present row.
pub open spec fn rec_com_ok(e: RExp, t: Twin, k: bool) -> bool {
    !t.ex || e.com == 2 || (e.com == 3 && t.com == k) || (e.com <= 1 && t.com == (e.com == 1))
}

pub open spec fn grx(r: Seq<RExp>, i: usize) -> RExp { if i < r.len() { r[i as int] } else { norec() } }

/// The observation a native check reads at the end: the store this process sees (equal to the
/// durable store once no transaction is open).
pub open spec fn obs_ns(s: State) -> Ns { s.ns }

pub open spec fn st_ok(e: Expect, s: State) -> bool {
    e.st == 0 || (e.st == 1) == (!s.top_err)
}

pub open spec fn ns_ok(c: Cfg, e: Expect, s: State) -> bool { alts_ok(e.alts, c.init, obs_ns(s).mods) }

pub open spec fn rec_ok(c: Cfg, e: Expect, s: State, i: usize) -> bool {
    let r = grx(e.recs, i);
    let t = gtwin(obs_ns(s).twins, i);
    rec_exists_ok(r, t) && rec_fields_ok(r, t) && rec_com_ok(r, t, kept(c.init, obs_ns(s).mods, i))
}

pub open spec fn expect_ok(c: Cfg, e: Expect, s: State) -> bool {
    st_ok(e, s) && ns_ok(c, e, s) && rec_ok(c, e, s, 0) && rec_ok(c, e, s, 1) && rec_ok(c, e, s, 2)
}

/// The expectation that applies to an end state of L2, or none (still running): 0 none,
/// 1 command returned, 2 process died, 3 caller committed, 4 caller rolled back.
pub open spec fn end_kind(s: State) -> u8 {
    if !s.alive { 2u8 }
    else if s.top_pc == 7 { 1u8 }
    else if s.top_pc == 8 && s.caller == 1 { 3u8 }
    else if s.top_pc == 8 && s.caller == 2 { 4u8 }
    else { 0u8 }
}

pub open spec fn expect_for(c: Cfg, q: Q, s: State) -> Expect {
    let k = end_kind(s);
    if k == 2 { l1_expect_kill(c, q, s) }
    else if k == 4 {
        // the caller rolled back its transaction: the namespace is undone; the load's records
        // survive under Q-06b's provisional option
        if q.q06b_caller_survive { Expect { st: 0, alts: caller_undo_alts(c), recs: l1_expect(c, q).recs, verdict: l1_verdict(c, q) } }
        else { Expect { st: 0, alts: caller_undo_alts(c), recs: Seq::empty().push(rx(2, 3, 2, 2, 2, 2, 255, 255)).push(rx(2, 3, 2, 2, 2, 2, 255, 255)).push(rx(2, 3, 2, 2, 2, 2, 255, 255)), verdict: l1_verdict(c, q) } }
    }
    else { l1_expect(c, q) }
}

/// L1 holds at an end state (vacuous while running).
pub open spec fn l1_ok(c: Cfg, q: Q, s: State) -> bool {
    end_kind(s) == 0 || expect_ok(c, expect_for(c, q, s), s)
}

} // exec_spec_verified!

} // verus!
