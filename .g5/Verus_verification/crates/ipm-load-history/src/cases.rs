//! Per-case lemmas: the as-built observation of a named case (B1-Cnn in `cases/cases.json`) and
//! its L1 verdict, proved by evaluating the spec machine (`by (compute_only)`).
//!
//! A conformance lemma shows L1 holds for that run. A violation lemma shows that the run is a
//! trace of L2 from its initial state (`run` iterates `step`; the kill/terminate variants stop
//! at the named point and apply the end), and that L1 (provisional Q) fails on its end state,
//! together with the Q alternative under which it would conform, where one exists.
//!
//! The configurations use `ACTIVE_VARIANT()`, so a control build re-evaluates every case on the
//! mutated machine.
use vstd::prelude::*;
use crate::model::*;
use crate::contract::*;

verus! {

/// Runs the deterministic machine for at most `fuel` steps.
pub open spec fn run(c: Cfg, s: State, fuel: nat) -> State
    decreases fuel
{
    if fuel == 0 || !running(s) { s } else { run(c, step(c, s), (fuel - 1) as nat) }
}

/// The state where activation `a` (1 top LoadNewModule, 3 dependency) is about to run the body of
/// its Activate phase (InitPhase done): the kill points of B1-C44..C50.
pub open spec fn at_activate(s: State, a: u8) -> bool {
    running(s) && active(s) == a && (if a == 1 { s.lnm0.pc == 27 } else { s.lnm1.pc == 27 })
}

pub open spec fn run_to_activate(c: Cfg, s: State, a: u8, fuel: nat) -> State
    decreases fuel
{
    if fuel == 0 || !running(s) || at_activate(s, a) { s } else { run_to_activate(c, step(c, s), a, (fuel - 1) as nat) }
}

pub open spec fn end_of(c: Cfg) -> State { run(c, init(c), 300) }

pub open spec fn mod0() -> Mod { dmod() }
pub open spec fn inst_m(v: u8, dev: bool) -> Mod { Mod { present: true, ver: v, dev: dev, res: true, doc: v, wr: 0 } }
pub open spec fn pk(v: u8) -> Pkg { Pkg { ver: v, upd: false, sysreq: true, act: 0, mf: 0, snap: false, found: true } }
pub open spec fn pk_act(v: u8, act: u8) -> Pkg { Pkg { ver: v, upd: false, sysreq: true, act: act, mf: 0, snap: false, found: true } }
pub open spec fn none3() -> Seq<Mod> { Seq::empty().push(mod0()).push(mod0()).push(mod0()) }

pub open spec fn mk(txm: u8, devf: u8, force: bool, path_ok: bool, cleanup: bool, caller: bool, top: Pkg, deps: Seq<Pkg>, init: Seq<Mod>) -> Cfg {
    Cfg { txm: txm, devf: devf, force: force, path_ok: path_ok, cleanup_fails: cleanup, caller_tx: caller, top: top, deps: deps, init: init, var: crate::proofs::ACTIVE_VARIANT() }
}

pub open spec fn cfg0() -> Cfg { mk(0, 0, false, true, false, false, pk(2), Seq::empty(), none3()) }

pub open spec fn qset(q01: bool, q05: bool, q06b: bool, q08: bool, q14: bool, q17: bool, qfin: bool) -> Q {
    Q { q01_keep: q01, q02_dev_bypass: true, q05_caller_exact: q05, q06b_caller_survive: q06b, q08_exact: q08, q14_end: q14, q17_before: q17, qfin_unended: qfin }
}
pub open spec fn q_with17(b: bool) -> Q { qset(true, false, true, true, true, b, false) }
pub open spec fn q_with14(b: bool) -> Q { qset(true, false, true, true, b, true, false) }
pub open spec fn q_with06b(b: bool) -> Q { qset(true, false, b, true, true, true, false) }
pub open spec fn q_with08(b: bool) -> Q { qset(true, false, true, b, true, true, false) }
pub open spec fn q_withfin(b: bool) -> Q { qset(true, false, true, true, true, true, b) }

// ------------------------------------------------------------------ conforming cases

/// B1-C01: a clean load returns OK, installs the module, one finalized row OK/Committed.
pub proof fn case_c01_conforms() {
    assert({
        let s = end_of(cfg0());
        s.top_pc == 7 && !s.top_err && s.ns.mods[0].present && s.ns.mods[0].res
        && s.ns.twins[0].ex && s.ns.twins[0].com && s.ns.twins[0].fin && s.ns.twins[0].ok == 1
        && !s.temp[0].live && l1_ok(cfg0(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c26() -> Cfg { mk(0, 1, false, true, false, false, pk_act(2, 1), Seq::empty(), none3()) }

/// Placed first among the lemmas that a developer-mode mutation can break: Verus evaluates
/// `compute_only` lemmas in source order and stops at the first failure.
/// B1-C26 (`load.outcome.dev`, `dev-mode.keep-on-failure`): a -dev load whose Activate fails
/// keeps the manifest and the resources its Reload phase loaded; L1 holds. Control 12 (reviewer
/// variant 212: developer-mode phases inside an inner transaction) breaks this lemma.
pub proof fn case_c26_dev_failure_keeps_phase_work() {
    assert({
        let s = end_of(cfg_c26());
        s.top_err && s.ns.mods[0].present && s.ns.mods[0].dev && s.ns.mods[0].res && s.ns.twins[0].com
        && l1_ok(cfg_c26(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c27() -> Cfg { mk(1, 0, false, true, false, false, pk_act(2, 1), Seq::empty(), none3()) }

/// B1-C27 (`load.outcome`, `transaction.opt-out`): a NoTransaction load whose Activate fails
/// keeps what it wrote: the module installed with its resources, the row Committed=1; L1 holds.
/// Control 13 (reviewer variant 15: NoTransaction ignored) breaks this lemma. Placed before the
/// other NoTransaction lemmas (evaluation order).
pub proof fn case_c27_notx_failure_keeps_partial() {
    assert({
        let s = end_of(cfg_c27());
        s.top_err && s.ns.mods[0].present && s.ns.mods[0].res && s.ns.twins[0].com
        && l1_ok(cfg_c27(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c11() -> Cfg { mk(0, 0, false, true, false, false, pk(2), Seq::empty(), Seq::empty().push(inst_m(3, false)).push(mod0()).push(mod0())) }

/// B1-C11: downgrade refused: no module data written, row error/Committed=0 (these clauses
/// conform), but the row's Package is empty although the manifest was read (violation of the
/// R1-ONT-03 clause).
pub proof fn case_c11_downgrade_refused() {
    assert({
        let s = end_of(cfg_c11());
        s.top_err && s.ns.mods == cfg_c11().init && !s.ns.twins[0].com && s.ns.twins[0].tend && !s.ns.twins[0].pkg
        && ns_ok(cfg_c11(), l1_expect(cfg_c11(), qprov()), s) && !l1_ok(cfg_c11(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c15() -> Cfg {
    mk(0, 0, false, true, false, false, Pkg { ver: 2, upd: true, sysreq: true, act: 0, mf: 0, snap: false, found: true }, Seq::empty(),
        Seq::empty().push(inst_m(1, false)).push(mod0()).push(mod0()))
}

/// B1-C15: upgrade over a module declaring UpdatePackage refused without -force: no module data
/// written, Committed=0; Package empty (violation of the R1-ONT-03 clause).
pub proof fn case_c15_update_refused() {
    assert({
        let s = end_of(cfg_c15());
        s.top_err && s.ns.mods == cfg_c15().init && !s.ns.twins[0].com && !s.ns.twins[0].pkg
        && ns_ok(cfg_c15(), l1_expect(cfg_c15(), qprov()), s) && !l1_ok(cfg_c15(), qprov(), s)
    }) by (compute_only);
}

// ------------------------------------------------------------------ as-built violations

pub open spec fn cfg_c08() -> Cfg { mk(1, 0, false, true, false, false, Pkg { ver: 2, upd: false, sysreq: true, act: 0, mf: 2, snap: false, found: true }, Seq::empty(), none3()) }

/// B1-C08 (violation of INT-HIST-05 exactness): NoTransaction, manifest with no module document:
/// nothing kept, yet the row reads Committed=1.
pub proof fn case_c08_committed_without_change() {
    assert({
        let s = end_of(cfg_c08());
        s.top_pc == 7 && s.top_err && !kept(cfg_c08().init, s.ns.mods, 0) && s.ns.twins[0].com && s.ns.twins[0].fin
        && !l1_ok(cfg_c08(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c09() -> Cfg { mk(1, 0, false, true, false, false, Pkg { ver: 2, upd: false, sysreq: true, act: 0, mf: 3, snap: false, found: true }, Seq::empty(), none3()) }

/// B1-C09 (violation of INT-HIST-05 exactness): NoTransaction, module document refused by
/// ImportStream validation (its nested transaction rolls back): nothing kept, Committed=1.
pub proof fn case_c09_committed_without_change() {
    assert({
        let s = end_of(cfg_c09());
        s.top_pc == 7 && s.top_err && !kept(cfg_c09().init, s.ns.mods, 0) && s.ns.twins[0].com && !l1_ok(cfg_c09(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c23() -> Cfg { mk(0, 1, false, true, false, false, Pkg { ver: 2, upd: false, sysreq: false, act: 0, mf: 0, snap: false, found: true }, Seq::empty(), none3()) }

/// B1-C23 (violation of INT-LOAD-11 under Q-17 provisional): a -dev load refused by system
/// requirements keeps the module manifest (developer mode, no resources) and reads Committed=1.
/// Conforms when Q-17's alternative is chosen.
pub proof fn case_c23_sysreq_dev_keeps_manifest() {
    assert({
        let s = end_of(cfg_c23());
        s.top_err && s.ns.mods[0].present && s.ns.mods[0].dev && !s.ns.mods[0].res && s.ns.twins[0].com
        && !l1_ok(cfg_c23(), qprov(), s) && l1_ok(cfg_c23(), q_with17(false), s)
    }) by (compute_only);
}

pub open spec fn cfg_c28() -> Cfg { mk(0, 0, false, true, false, false, pk_act(2, 2), Seq::empty(), none3()) }

/// B1-C28 (violation under Q-14 provisional): an Activate-phase Invoke that returns an error
/// status leaves the Activate phase without an end in the row. Conforms under Q-14's alternative.
pub proof fn case_c28_phase_left_open() {
    assert({
        let s = end_of(cfg_c28());
        s.top_err && s.ns.twins[0].ph == 5 && s.ns.twins[0].phst == 0 && s.ns.mods == cfg_c28().init
        && !l1_ok(cfg_c28(), qprov(), s) && l1_ok(cfg_c28(), q_with14(false), s)
    }) by (compute_only);
}

pub open spec fn cfg_c29() -> Cfg { mk(0, 0, false, true, true, false, pk(2), Seq::empty(), none3()) }

/// B1-C29 (violation of INT-LOAD-08): an error after the final commit (CleanUp) is reported while
/// the module stays installed in default mode.
pub proof fn case_c29_error_after_commit() {
    assert({
        let s = end_of(cfg_c29());
        s.top_err && s.ns.mods[0].present && s.ns.mods[0].res && s.ns.twins[0].com && !l1_ok(cfg_c29(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c31() -> Cfg { mk(0, 0, false, true, false, false, pk_act(2, 1), Seq::empty().push(pk(1)), none3()) }

/// B1-C31 (violation of INT-HIST-02/-08, issue #366): the dependency load ran and completed its
/// row, then the parent's Activate failed and its rollback removed the dependency's row.
pub proof fn case_c31_dep_record_lost() {
    assert({
        let s = end_of(cfg_c31());
        s.top_err && s.att[1] == 3 && !s.ns.twins[1].ex && s.ns.twins[0].ex && s.ctr == 2
        && !l1_ok(cfg_c31(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c33() -> Cfg { mk(0, 1, false, true, false, false, pk(2), Seq::empty().push(pk_act(1, 1)), none3()) }

/// B1-C33: -dev parent, dependency fails: the parent manifest is kept (committed at C1), the
/// dependency's failure row is rolled back with T2.
pub proof fn case_c33_dev_dep_record_lost() {
    assert({
        let s = end_of(cfg_c33());
        s.top_err && s.ns.mods[0].present && s.ns.mods[0].dev && !s.ns.mods[1].present && s.att[1] == 3 && !s.ns.twins[1].ex
        && s.ns.twins[0].com && !l1_ok(cfg_c33(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c45() -> Cfg { mk(0, 0, false, true, false, false, pk(2), Seq::empty().push(pk(1)), none3()) }

/// B1-C45K (violation of INT-HIST-03 under Q-06): SIGKILL-like end inside the parent's Activate
/// after the dependency completed: the namespace is rolled back, the top row keeps its Init values,
/// the dependency's row is gone and the top IRISTEMP row is left.
pub proof fn case_c45_kill_dep_record_lost() {
    assert({
        let p = run_to_activate(cfg_c45(), init(cfg_c45()), 1, 300);
        let s = kill(cfg_c45(), p);
        at_activate(p, 1) && s.ns.mods == cfg_c45().init && s.ns.twins[0].ex && s.ns.twins[0].ok == 0 && !s.ns.twins[0].tend
        && !s.ns.twins[0].pkg && s.att[1] == 3 && !s.ns.twins[1].ex && s.temp[0].live && !l1_ok(cfg_c45(), qprov(), s)
    }) by (compute_only);
}

/// B1-C45T: the same with a Terminate-like end: %OnClose persists the top row inside T1 and the
/// rollback undoes it, so the row keeps its Init values; the IRISTEMP row is deleted.
pub proof fn case_c45_terminate_dep_record_lost() {
    assert({
        let p = run_to_activate(cfg_c45(), init(cfg_c45()), 1, 300);
        let s = terminate(cfg_c45(), p);
        at_activate(p, 1) && s.ns.mods == cfg_c45().init && s.ns.twins[0].ex && s.ns.twins[0].ok == 0 && !s.ns.twins[0].fin
        && !s.ns.twins[1].ex && !s.temp[0].live && !l1_ok(cfg_c45(), qprov(), s)
    }) by (compute_only);
}

pub open spec fn cfg_c46() -> Cfg { mk(0, 1, false, true, false, false, pk(2), Seq::empty().push(pk(1)), none3()) }

/// B1-C46K/T: -dev, process ends inside Activate after C2. SIGKILL-like: changes kept, the row
/// reads Committed=0 (H-COMMITTED violation under Q-08's provisional option, conforms under
/// "unknown") and lists no phases although five started (H-RECORD-FIELDS, R1-MOD-03).
/// Terminate-like: %OnClose persists the row at $TLEVEL 0, so it is Finalized, in progress,
/// Committed=1 (violation under Q-FIN's provisional option, conforms under its alternative).
pub proof fn case_c46_dev_end_inside_activate() {
    assert({
        let p = run_to_activate(cfg_c46(), init(cfg_c46()), 1, 300);
        let k = kill(cfg_c46(), p);
        let t = terminate(cfg_c46(), p);
        at_activate(p, 1)
        && k.ns.mods[0].present && k.ns.mods[1].present && !k.ns.twins[0].com && k.ns.twins[1].ex
        && !l1_ok(cfg_c46(), qprov(), k)
        && !rec_com_ok(grx(expect_for(cfg_c46(), qprov(), k).recs, 0), k.ns.twins[0], kept(cfg_c46().init, k.ns.mods, 0))
        && rec_com_ok(grx(expect_for(cfg_c46(), q_with08(false), k).recs, 0), k.ns.twins[0], kept(cfg_c46().init, k.ns.mods, 0))
        && !rec_fields_ok(grx(expect_for(cfg_c46(), qprov(), k).recs, 0), k.ns.twins[0]) && k.temp[0].ph == 5 && k.ns.twins[0].ph == 0
        && t.ns.twins[0].fin && t.ns.twins[0].com && t.ns.twins[0].ok == 0 && !t.ns.twins[0].tend
        && !l1_ok(cfg_c46(), qprov(), t) && l1_ok(cfg_c46(), q_withfin(true), t)
    }) by (compute_only);
}

pub open spec fn cfg_c42() -> Cfg { mk(0, 0, false, true, false, true, pk(2), Seq::empty(), none3()) }

/// B1-C42 (violation under Q-06b provisional): with a caller-held transaction, the load returns
/// OK with its row reading Committed=1; the caller's rollback then removes the module and the row.
/// Conforms under Q-06b's alternative.
pub proof fn case_c42_caller_rollback_record_lost() {
    assert({
        let r = end_of(cfg_c42());
        let s = caller_end(cfg_c42(), r, false);
        r.top_pc == 7 && !r.top_err && r.tx.len() == 1 && r.ns.twins[0].com && r.ns.twins[0].fin
        && s.ns.mods == cfg_c42().init && !s.ns.twins[0].ex
        && !l1_ok(cfg_c42(), qprov(), s) && l1_ok(cfg_c42(), q_with06b(false), s)
    }) by (compute_only);
}

pub open spec fn cfg_c52() -> Cfg { mk(1, 0, false, true, false, false, Pkg { ver: 2, upd: false, sysreq: true, act: 0, mf: 4, snap: false, found: true }, Seq::empty(), none3()) }

/// B1-C52 (violation of `load.refusal`, R1-MOD-02): NoTransaction, module.xml with two documents:
/// the module record is written, then the multiple-documents check refuses; the record stays and
/// the row reads Committed=1 with an empty Package.
pub proof fn case_c52_multiple_documents_kept() {
    assert({
        let s = end_of(cfg_c52());
        s.top_err && s.ns.mods[0].present && !s.ns.mods[0].res && s.ns.twins[0].com && !s.ns.twins[0].pkg
        && !ns_ok(cfg_c52(), l1_expect(cfg_c52(), qprov()), s) && !l1_ok(cfg_c52(), qprov(), s)
    }) by (compute_only);
}

// ------------------------------------------------------------------ D1 repair (variant 103)

/// The configuration with the D1 repair: dependency History rows are written outside the parent's
/// transaction, read Committed only once the dependency's writes are durable, and are finalized
/// at the top-level close. Not the as-built product.
pub open spec fn r103(c: Cfg) -> Cfg {
    Cfg { txm: c.txm, devf: c.devf, force: c.force, path_ok: c.path_ok, cleanup_fails: c.cleanup_fails, caller_tx: c.caller_tx,
        top: c.top, deps: c.deps, init: c.init, var: 103 }
}

pub open spec fn cfg_c38() -> Cfg { mk(0, 1, false, true, false, false, pk_act(2, 1), Seq::empty().push(pk(1)), none3()) }

/// REPAIR 103 on B1-C31 (normal mode, parent Activate fails after the dependency loaded): the
/// dependency row survives the rollback, reads Success OK, Committed=0, Finalized; L1 holds.
pub proof fn repair103_c31() {
    assert({
        let c = r103(cfg_c31());
        let s = end_of(c);
        s.ns.twins[1].ex && s.ns.twins[1].ok == 1 && !s.ns.twins[1].com && s.ns.twins[1].fin && !s.ns.mods[1].present
        && l1_ok(c, qprov(), s)
    }) by (compute_only);
}

/// REPAIR 103 on B1-C33 (-dev parent, dependency fails inside T2): the failed dependency's row
/// survives T2's rollback with Success error, Committed=0; L1 holds.
pub proof fn repair103_c33() {
    assert({
        let c = r103(cfg_c33());
        let s = end_of(c);
        s.ns.twins[1].ex && s.ns.twins[1].ok == 2 && !s.ns.twins[1].com && s.ns.twins[1].fin && s.ns.mods[0].present
        && l1_ok(c, qprov(), s)
    }) by (compute_only);
}

/// REPAIR 103 on B1-C38 / native A5 (-dev parent fails after C2): the dependency stays installed
/// and its row reads Committed=1 although the parent failed; L1 holds.
pub proof fn repair103_c38() {
    assert({
        let c = r103(cfg_c38());
        let s = end_of(c);
        s.ns.twins[1].ex && s.ns.twins[1].ok == 1 && s.ns.twins[1].com && s.ns.twins[1].fin && s.ns.mods[1].present
        && s.ns.twins[0].ok == 2 && l1_ok(c, qprov(), s)
    }) by (compute_only);
}

/// REPAIR 103 on B1-C45K/T (process end inside the parent's Activate after the dependency loaded):
/// the dependency row survives with Success OK and Committed=0 (its writes were rolled back), so
/// every dependency clause holds. The top row still violates the phase rule (D10, not addressed
/// by this repair).
pub proof fn repair103_c45() {
    assert({
        let c = r103(cfg_c45());
        let p = run_to_activate(c, init(c), 1, 300);
        let k = kill(c, p);
        let t = terminate(c, p);
        let ek = expect_for(c, qprov(), k);
        let et = expect_for(c, qprov(), t);
        at_activate(p, 1)
        && k.ns.twins[1].ex && k.ns.twins[1].ok == 1 && !k.ns.twins[1].com && !k.ns.mods[1].present
        && rec_exists_ok(grx(ek.recs, 1), k.ns.twins[1]) && rec_fields_ok(grx(ek.recs, 1), k.ns.twins[1])
        && rec_com_ok(grx(ek.recs, 1), k.ns.twins[1], kept(c.init, k.ns.mods, 1))
        && t.ns.twins[1].ex && !t.ns.twins[1].com
        && rec_exists_ok(grx(et.recs, 1), t.ns.twins[1]) && rec_fields_ok(grx(et.recs, 1), t.ns.twins[1])
        && rec_com_ok(grx(et.recs, 1), t.ns.twins[1], kept(c.init, t.ns.mods, 1))
        && ns_ok(c, ek, k) && ns_ok(c, et, t)
        && !rec_fields_ok(grx(ek.recs, 0), k.ns.twins[0])
    }) by (compute_only);
}

/// Sanity check of the evaluator (R1-MOD-14): a deliberately false claim about B1-C31 (that the
/// dependency row survives as built). Compiled only with feature `sanity-false`; that run must
/// fail with "expression simplifies to false".
#[cfg(feature = "sanity-false")]
pub proof fn sanity_false_claim() {
    assert({
        let s = end_of(cfg_c31());
        s.ns.twins[1].ex
    }) by (compute_only);
}

} // verus!
