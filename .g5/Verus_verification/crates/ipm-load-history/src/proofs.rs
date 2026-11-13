//! Invariants of the as-built machine (`c.var == ACTIVE_VARIANT`, 0 unless a control feature is
//! enabled) and the refinement obligations that hold.
//!
//! Structure:
//! - provider lemmas: what TSTART/TCOMMIT/TROLLBACK/death do to the durable view and IRISTEMP;
//! - `inv`: the inductive invariant (store shape, transaction depth per activation, journaling
//!   flag, durable top record, finalized-row stability, fresh slots, commit soundness);
//! - `lemma_init_inv`, `lemma_step_inv`, `lemma_kill_inv`, `lemma_caller_inv`, `lemma_edit_inv`;
//! - theorems stated on reachable states (`reachable`).
use vstd::prelude::*;
use crate::model::*;
use crate::contract::*;

verus! {

/// The variant whose proofs are being checked. 0 (as-built) unless a control feature is set.
#[cfg(not(any(feature = "control-01", feature = "control-02", feature = "control-03", feature = "control-04", feature = "control-05",
    feature = "control-06", feature = "control-07", feature = "control-08", feature = "control-09", feature = "control-10", feature = "control-11",
    feature = "control-12")))]
pub open spec fn ACTIVE_VARIANT() -> u8 { 0 }
#[cfg(feature = "control-01")] pub open spec fn ACTIVE_VARIANT() -> u8 { 1 }
#[cfg(feature = "control-02")] pub open spec fn ACTIVE_VARIANT() -> u8 { 2 }
#[cfg(feature = "control-03")] pub open spec fn ACTIVE_VARIANT() -> u8 { 3 }
#[cfg(feature = "control-04")] pub open spec fn ACTIVE_VARIANT() -> u8 { 4 }
#[cfg(feature = "control-05")] pub open spec fn ACTIVE_VARIANT() -> u8 { 5 }
#[cfg(feature = "control-06")] pub open spec fn ACTIVE_VARIANT() -> u8 { 6 }
#[cfg(feature = "control-07")] pub open spec fn ACTIVE_VARIANT() -> u8 { 7 }
#[cfg(feature = "control-08")] pub open spec fn ACTIVE_VARIANT() -> u8 { 8 }
#[cfg(feature = "control-09")] pub open spec fn ACTIVE_VARIANT() -> u8 { 9 }
#[cfg(feature = "control-10")] pub open spec fn ACTIVE_VARIANT() -> u8 { 10 }
#[cfg(feature = "control-11")] pub open spec fn ACTIVE_VARIANT() -> u8 { 11 }
#[cfg(feature = "control-12")] pub open spec fn ACTIVE_VARIANT() -> u8 { 12 }

// ------------------------------------------------------------------ shape

pub open spec fn ns_wf(n: Ns) -> bool { n.mods.len() == 3 && n.twins.len() == 3 }

pub open spec fn tx_wf(tx: Seq<Ns>) -> bool {
    tx.len() <= 3
    && (tx.len() > 0 ==> ns_wf(tx[0]))
    && (tx.len() > 1 ==> ns_wf(tx[1]))
    && (tx.len() > 2 ==> ns_wf(tx[2]))
}

pub open spec fn shape(s: State) -> bool {
    ns_wf(s.ns) && tx_wf(s.tx) && s.temp.len() == 3 && s.att.len() == 3
}

// ------------------------------------------------------------------ transaction depth

/// Transaction levels an activation of LoadNewModule has open at its program counter (outside
/// its rollback loop).
pub open spec fn lv(c: Cfg, f: Lnm) -> int {
    if c.txm != 0 { 0 }
    else if f.pc <= 1 { 0 }
    else if f.pc <= 7 { 1 }
    else if f.pc == 8 || f.pc == 9 { 0 }
    else if f.pc <= 13 { 1 }
    else if f.pc <= 29 { if f.dev { 0 } else { 1 } }
    else { 0 }
}

/// Program counter facts of one activation.
pub open spec fn lnm_pc_ok(c: Cfg, f: Lnm) -> bool {
    &&& f.pc <= 32
    &&& ((f.pc == 8 || f.pc == 9) ==> (f.dev && c.txm == 0))
    &&& (f.pc == 30 ==> (!f.dev && c.txm == 0))
    &&& (f.nj ==> c.txm == 2)
    &&& (f.pc <= 1 ==> !f.nj)
    &&& ((c.txm == 2 && 2 <= f.pc && f.pc <= 30) ==> f.nj)
}

/// Levels below the load: the caller's transaction until the caller ends it.
pub open spec fn base(c: Cfg, s: State) -> int { if c.caller_tx && s.top_pc <= 7 { 1 } else { 0 } }

/// The dependency activation is running (inside LoadModuleReference's call).
pub open spec fn dep_running(s: State) -> bool { s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc == 5 }

/// Levels the dependency activation holds.
pub open spec fn dep_levels(c: Cfg, s: State) -> int {
    if !dep_running(s) { 0 }
    else if s.lnm1.pc == 31 { s.tx.len() - base(c, s) - lv(c, s.lnm0) }
    else { lv(c, s.lnm1) }
}

pub open spec fn depth_ok(c: Cfg, s: State) -> bool {
    let b = base(c, s);
    &&& s.top_pc <= 8
    &&& (s.lmr_pc != 0 ==> (s.top_pc == 3 && s.lnm0.pc == 11 && (s.lnm0.di as int) < c.deps.len() && s.lmr_pc <= 9))
    &&& (s.top_pc == 8 ==> s.tx.len() == 0)
    &&& (s.top_pc != 3 && s.top_pc != 8 ==> s.tx.len() == b)
    &&& (s.top_pc == 3 ==> {
        &&& (s.lnm0.pc >= 1 ==> s.lnm0.itl == b)
        &&& lnm_pc_ok(c, s.lnm0)
        &&& s.lnm0.di <= 2
        &&& (s.lnm0.pc == 31 ==> (b <= s.tx.len() <= b + 1))
        &&& (s.lnm0.pc != 31 ==> s.tx.len() == b + lv(c, s.lnm0) + dep_levels(c, s))
        &&& (dep_running(s) ==> {
            &&& (s.lnm1.pc >= 1 ==> s.lnm1.itl == b + lv(c, s.lnm0))
            &&& lnm_pc_ok(c, s.lnm1)
            &&& s.lnm1.di == 0
            &&& (s.lnm1.pc == 31 ==> (s.lnm1.itl <= s.tx.len() <= s.lnm1.itl + 1))
        })
    })
}

/// Journaling is off exactly while a NoJournal LoadNewModule holds its JournalManager.
pub open spec fn jrn_ok(c: Cfg, s: State) -> bool {
    &&& s.jrn == !(s.top_pc == 3 && s.lnm0.nj && s.lnm0.pc != 32)
    &&& (s.top_pc == 3 ==> s.lnm0.jprev)
    &&& (dep_running(s) && s.lnm1.nj ==> !s.lnm1.jprev)
}

// ------------------------------------------------------------------ History rows

/// Row `i` exists in the current store and in every snapshot at level `from` or above.
pub open spec fn row_ex_from(s: State, i: usize, from: int) -> bool {
    &&& gtwin(s.ns.twins, i).ex
    &&& (s.tx.len() > 0 && 0 >= from ==> gtwin(s.tx[0].twins, i).ex)
    &&& (s.tx.len() > 1 && 1 >= from ==> gtwin(s.tx[1].twins, i).ex)
    &&& (s.tx.len() > 2 && 2 >= from ==> gtwin(s.tx[2].twins, i).ex)
}

/// Row `i` is the never-written row in the current store and in every snapshot.
pub open spec fn row_fresh(s: State, i: usize) -> bool {
    &&& gtwin(s.ns.twins, i) == dtwin()
    &&& (s.tx.len() > 0 ==> gtwin(s.tx[0].twins, i) == dtwin())
    &&& (s.tx.len() > 1 ==> gtwin(s.tx[1].twins, i) == dtwin())
    &&& (s.tx.len() > 2 ==> gtwin(s.tx[2].twins, i) == dtwin())
}

/// Level at which LoadModuleReference inserts the current dependency's row.
pub open spec fn dep_level(c: Cfg, s: State) -> int { base(c, s) + lv(c, s.lnm0) }

/// Which actions have started, and their rows.
pub open spec fn rows_ok(c: Cfg, s: State) -> bool {
    &&& (gatt(s.att, 0) == 0 ==> row_fresh(s, 0))
    &&& (gatt(s.att, 1) == 0 ==> row_fresh(s, 1))
    &&& (gatt(s.att, 2) == 0 ==> row_fresh(s, 2))
    &&& gatt(s.att, 0) <= 3 && gatt(s.att, 1) <= 3 && gatt(s.att, 2) <= 3
    &&& (s.alive ==> {
        &&& (s.top_pc <= 1 <==> gatt(s.att, 0) == 0)
        &&& (2 <= s.top_pc && s.top_pc <= 4 ==> gatt(s.att, 0) == 1)
        &&& (s.top_pc == 5 ==> gatt(s.att, 0) == 2)
        &&& (6 <= s.top_pc ==> gatt(s.att, 0) == 3)
        &&& (s.top_pc <= 2 ==> gatt(s.att, 1) == 0 && gatt(s.att, 2) == 0)
        &&& (s.top_pc == 3 && s.lnm0.pc < 11 ==> s.lnm0.di == 0 && gatt(s.att, 1) == 0 && gatt(s.att, 2) == 0)
        &&& (s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc <= 2 ==>
                (s.lnm0.di == 0 ==> gatt(s.att, 1) == 0 && gatt(s.att, 2) == 0) && (s.lnm0.di == 1 ==> gatt(s.att, 2) == 0))
        &&& (s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc >= 3 ==> (s.lnm0.di == 0 ==> gatt(s.att, 2) == 0))
        &&& (s.top_pc == 3 && s.lnm0.pc == 11 && 3 <= s.lmr_pc && s.lmr_pc <= 6 ==> gatt(s.att, dslot(s)) == 1)
        &&& (s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc == 7 ==> gatt(s.att, dslot(s)) == 2)
        // the top action's row survives every rollback down to the level it was inserted at
        &&& (2 <= s.top_pc && s.top_pc <= 7 ==> row_ex_from(s, 0, base(c, s)))
        // the dependency's row exists while LoadModuleReference holds its log
        &&& (s.top_pc == 3 && s.lnm0.pc == 11 && 3 <= s.lmr_pc && s.lmr_pc <= 8 ==> row_ex_from(s, dslot(s), dep_level(c, s)))
    })
    // R3-top: once the top action started, its row is in the durable store
    &&& (!c.caller_tx && gatt(s.att, 0) >= 1 ==> gtwin(dv(s).twins, 0).ex)
}

// ------------------------------------------------------------------ finalized rows (R2)

/// A row finalized in `a` is unchanged in the later store `b`.
pub open spec fn fin_stable(a: Ns, b: Ns) -> bool {
    &&& (gtwin(a.twins, 0).fin ==> gtwin(b.twins, 0) == gtwin(a.twins, 0))
    &&& (gtwin(a.twins, 1).fin ==> gtwin(b.twins, 1) == gtwin(a.twins, 1))
    &&& (gtwin(a.twins, 2).fin ==> gtwin(b.twins, 2) == gtwin(a.twins, 2))
}

pub open spec fn fin_ex(a: Ns) -> bool {
    (gtwin(a.twins, 0).fin ==> gtwin(a.twins, 0).ex) && (gtwin(a.twins, 1).fin ==> gtwin(a.twins, 1).ex) && (gtwin(a.twins, 2).fin ==> gtwin(a.twins, 2).ex)
}

pub open spec fn rows_kept(a: Ns, b: Ns) -> bool {
    (gtwin(a.twins, 0).ex ==> gtwin(b.twins, 0).ex) && (gtwin(a.twins, 1).ex ==> gtwin(b.twins, 1).ex) && (gtwin(a.twins, 2).ex ==> gtwin(b.twins, 2).ex)
}

/// Along the snapshot chain tx[0], tx[1], tx[2], ns a finalized row never changes and an existing
/// row never disappears.
pub open spec fn fin_ok(s: State) -> bool {
    &&& fin_ex(s.ns)
    &&& (s.tx.len() > 0 ==> fin_ex(s.tx[0]) && fin_stable(s.tx[0], s.ns) && rows_kept(s.tx[0], s.ns))
    &&& (s.tx.len() > 1 ==> fin_ex(s.tx[1]) && fin_stable(s.tx[0], s.tx[1]) && fin_stable(s.tx[1], s.ns)
            && rows_kept(s.tx[0], s.tx[1]) && rows_kept(s.tx[1], s.ns))
    &&& (s.tx.len() > 2 ==> fin_ex(s.tx[2]) && fin_stable(s.tx[0], s.tx[2]) && fin_stable(s.tx[1], s.tx[2]) && fin_stable(s.tx[2], s.ns)
            && rows_kept(s.tx[0], s.tx[2]) && rows_kept(s.tx[1], s.tx[2]) && rows_kept(s.tx[2], s.ns))
}

// ------------------------------------------------------------------ commit soundness (top action)

pub open spec fn top_com(s: State) -> bool { gtemp(s.temp, 0).com }

/// Between a TCOMMIT that leaves level 0 and the SetCommitted that follows it.
pub open spec fn cs_window(s: State) -> bool { s.top_pc == 3 && (s.lnm0.pc == 8 || s.lnm0.pc == 30) }

/// For a top-level load with no caller transaction: whenever the durable modules differ from
/// the initial ones, the log already says Committed (outside the window), and the History row
/// carries the log's Committed once persisted.
pub open spec fn cs_ok(c: Cfg, s: State) -> bool {
    !c.caller_tx && s.alive ==> {
        &&& (kept(c.init, dv(s).mods, 0) ==> top_com(s) || cs_window(s))
        &&& (s.top_pc <= 2 ==> !kept(c.init, s.ns.mods, 0))
        &&& (s.top_pc == 3 && s.lnm0.pc <= 3 ==> !kept(c.init, s.ns.mods, 0))
        &&& (c.txm != 0 && s.top_pc == 3 && 4 <= s.lnm0.pc && s.lnm0.pc <= 30 ==> top_com(s))
        &&& (c.txm == 0 && s.top_pc == 3 && s.lnm0.dev && 9 <= s.lnm0.pc && s.lnm0.pc <= 29 ==> top_com(s))
        &&& (s.top_pc <= 5 ==> !gtwin(s.ns.twins, 0).fin)
        &&& (6 <= s.top_pc ==> gtwin(s.ns.twins, 0).com == top_com(s))
        &&& (5 <= s.top_pc ==> gtemp(s.temp, 0).tend && gtemp(s.temp, 0).ok == (if s.top_err { 2u8 } else { 1u8 }))
        &&& (6 <= s.top_pc ==> gtwin(s.ns.twins, 0).fin && gtwin(s.ns.twins, 0).tend && gtwin(s.ns.twins, 0).ok == gtemp(s.temp, 0).ok)
    }
}

pub open spec fn inv(c: Cfg, s: State) -> bool {
    &&& cfg_wf(c)
    &&& c.var == ACTIVE_VARIANT()
    &&& shape(s)
    &&& (s.alive ==> depth_ok(c, s))
    &&& (s.alive ==> jrn_ok(c, s))
    &&& rows_ok(c, s)
    &&& fin_ok(s)
    &&& cs_ok(c, s)
    &&& (s.alive <==> s.how == 0)
}

// ------------------------------------------------------------------ initial state

pub proof fn lemma_init_inv(c: Cfg)
    requires cfg_wf(c), c.var == ACTIVE_VARIANT(),
    ensures inv(c, init(c)),
{
}

// ------------------------------------------------------------------ step preservation

pub proof fn lemma_top_step(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 0,
    ensures inv(c, top_step(c, s)),
{
    let s2 = top_step(c, s);
    if s.top_pc == 0 {
        assert(inv(c, s2));
    } else if s.top_pc == 1 {
        assert(inv(c, s2));
    } else if s.top_pc == 2 {
        assert(inv(c, s2));
    } else if s.top_pc == 3 {
        assert(inv(c, s2));
    } else if s.top_pc == 4 {
        assert(inv(c, s2));
    } else if s.top_pc == 5 {
        assert(inv(c, s2));
    } else {
        assert(inv(c, s2));
    }
}

pub proof fn lemma_lmr_step(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 2,
    ensures inv(c, lmr_step(c, s)),
{
}

pub proof fn lemma_lnm0_a(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1, 0 <= s.lnm0.pc <= 6,
    ensures inv(c, lnm_step(c, s, 0)),
{
    if s.lnm0.pc == 0 { assert(s.tx.len() <= 3); assert((s.tx.len() as u8) as int == s.tx.len()); assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 1 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 2 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 3 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 4 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 5 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 6 { assert(inv(c, lnm_step(c, s, 0))); 
    } else { assert(inv(c, lnm_step(c, s, 0))); }
}

pub proof fn lemma_lnm0_b(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1, 7 <= s.lnm0.pc <= 13,
    ensures inv(c, lnm_step(c, s, 0)),
{
    if s.lnm0.pc == 7 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 8 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 9 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 10 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 11 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 12 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 13 { assert(inv(c, lnm_step(c, s, 0))); 
    } else { assert(inv(c, lnm_step(c, s, 0))); }
}

pub proof fn lemma_lnm0_c(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1, 14 <= s.lnm0.pc <= 28,
    ensures inv(c, lnm_step(c, s, 0)),
{
    if s.lnm0.pc == 14 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 15 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 16 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 17 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 18 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 19 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 20 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 21 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 22 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 23 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 24 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 25 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 26 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 27 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 28 { assert(inv(c, lnm_step(c, s, 0))); 
    } else { assert(inv(c, lnm_step(c, s, 0))); }
}

pub proof fn lemma_lnm0_d(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1, 29 <= s.lnm0.pc <= 32,
    ensures inv(c, lnm_step(c, s, 0)),
{
    if s.lnm0.pc == 29 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 30 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 31 { assert(inv(c, lnm_step(c, s, 0))); 
    } else if s.lnm0.pc == 32 { assert(inv(c, lnm_step(c, s, 0))); 
    } else { assert(inv(c, lnm_step(c, s, 0))); }
}

pub proof fn lemma_lnm0_step(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1,
    ensures inv(c, lnm_step(c, s, 0)),
{
    if s.lnm0.pc <= 6 { lemma_lnm0_a(c, s); }
    else if s.lnm0.pc <= 13 { lemma_lnm0_b(c, s); }
    else if s.lnm0.pc <= 28 { lemma_lnm0_c(c, s); }
    else { lemma_lnm0_d(c, s); }
}

pub proof fn lemma_lnm1_a(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3, 0 <= s.lnm1.pc <= 6,
    ensures inv(c, lnm_step(c, s, 1)),
{
    if s.lnm1.pc == 0 { assert(s.tx.len() <= 3); assert((s.tx.len() as u8) as int == s.tx.len()); assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 1 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 2 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 3 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 4 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 5 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 6 { assert(inv(c, lnm_step(c, s, 1))); 
    } else { assert(inv(c, lnm_step(c, s, 1))); }
}

pub proof fn lemma_lnm1_b(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3, 7 <= s.lnm1.pc <= 13,
    ensures inv(c, lnm_step(c, s, 1)),
{
    if s.lnm1.pc == 7 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 8 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 9 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 10 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 11 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 12 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 13 { assert(inv(c, lnm_step(c, s, 1))); 
    } else { assert(inv(c, lnm_step(c, s, 1))); }
}

pub proof fn lemma_lnm1_c(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3, 14 <= s.lnm1.pc <= 28,
    ensures inv(c, lnm_step(c, s, 1)),
{
    if s.lnm1.pc == 14 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 15 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 16 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 17 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 18 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 19 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 20 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 21 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 22 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 23 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 24 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 25 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 26 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 27 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 28 { assert(inv(c, lnm_step(c, s, 1))); 
    } else { assert(inv(c, lnm_step(c, s, 1))); }
}

pub proof fn lemma_lnm1_d(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3, 29 <= s.lnm1.pc <= 32,
    ensures inv(c, lnm_step(c, s, 1)),
{
    if s.lnm1.pc == 29 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 30 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 31 { assert(inv(c, lnm_step(c, s, 1))); 
    } else if s.lnm1.pc == 32 { assert(inv(c, lnm_step(c, s, 1))); 
    } else { assert(inv(c, lnm_step(c, s, 1))); }
}

pub proof fn lemma_lnm1_step(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3,
    ensures inv(c, lnm_step(c, s, 1)),
{
    if s.lnm1.pc <= 6 { lemma_lnm1_a(c, s); }
    else if s.lnm1.pc <= 13 { lemma_lnm1_b(c, s); }
    else if s.lnm1.pc <= 28 { lemma_lnm1_c(c, s); }
    else { lemma_lnm1_d(c, s); }
}

pub proof fn lemma_step_inv(c: Cfg, s: State)
    requires inv(c, s), running(s),
    ensures inv(c, step(c, s)),
{
    if active(s) == 0 { lemma_top_step(c, s); }
    else if active(s) == 1 { lemma_lnm0_step(c, s); }
    else if active(s) == 2 { lemma_lmr_step(c, s); }
    else { lemma_lnm1_step(c, s); }
}

pub proof fn lemma_kill_inv(c: Cfg, s: State)
    requires inv(c, s), running(s),
    ensures inv(c, kill(c, s)),
{
}

pub proof fn lemma_terminate_inv(c: Cfg, s: State)
    requires inv(c, s), running(s),
    ensures inv(c, terminate(c, s)),
{
}

pub proof fn lemma_caller_inv(c: Cfg, s: State, commit: bool)
    requires inv(c, s), c.caller_tx, s.alive, s.top_pc == 7,
    ensures inv(c, caller_end(c, s, commit)),
{
}

pub proof fn lemma_edit_inv(c: Cfg, s: State, i: usize)
    requires inv(c, s), s.alive, user_edit_enabled(s, i),
    ensures inv(c, user_edit(c, s, i)),
{
}

// ------------------------------------------------------------------ transitions and reachability

/// Every transition of L2: a load step, a kill-like or terminate-like end, the caller ending its
/// transaction, or another process editing a History row.
pub open spec fn next(c: Cfg, s: State, s2: State) -> bool {
    ||| running(s) && s2 == step(c, s)
    ||| running(s) && s2 == kill(c, s)
    ||| running(s) && s2 == terminate(c, s)
    ||| (c.caller_tx && s.alive && s.top_pc == 7 && (s2 == caller_end(c, s, true) || s2 == caller_end(c, s, false)))
    ||| (exists |i: usize| #[trigger] user_edit_enabled(s, i) && s.alive && s2 == user_edit(c, s, i))
}

pub proof fn lemma_next_inv(c: Cfg, s: State, s2: State)
    requires inv(c, s), next(c, s, s2),
    ensures inv(c, s2),
{
    if running(s) && s2 == step(c, s) { lemma_step_inv(c, s); }
    else if running(s) && s2 == kill(c, s) { lemma_kill_inv(c, s); }
    else if running(s) && s2 == terminate(c, s) { lemma_terminate_inv(c, s); }
    else if c.caller_tx && s.alive && s.top_pc == 7 && s2 == caller_end(c, s, true) { lemma_caller_inv(c, s, true); }
    else if c.caller_tx && s.alive && s.top_pc == 7 && s2 == caller_end(c, s, false) { lemma_caller_inv(c, s, false); }
    else {
        let i = choose |i: usize| #[trigger] user_edit_enabled(s, i) && s.alive && s2 == user_edit(c, s, i);
        lemma_edit_inv(c, s, i);
    }
}

/// A trace of L2 from its initial state.
pub open spec fn is_trace(c: Cfg, t: Seq<State>) -> bool {
    t.len() > 0 && t[0] == init(c) && forall |k: int| 0 <= k < t.len() - 1 ==> #[trigger] next(c, t[k], t[k + 1])
}

/// THEOREM (inductive invariant): every state reachable from the initial state of a configuration
/// in the domain satisfies `inv`.
pub proof fn theorem_reachable_inv(c: Cfg, t: Seq<State>, k: int)
    requires cfg_wf(c), c.var == ACTIVE_VARIANT(), is_trace(c, t), 0 <= k < t.len(),
    ensures inv(c, t[k]),
    decreases k,
{
    if k == 0 { lemma_init_inv(c); }
    else {
        theorem_reachable_inv(c, t, k - 1);
        let j = k - 1;
        assert(next(c, t[j], t[j + 1]));
        lemma_next_inv(c, t[j], t[j + 1]);
    }
}

// ------------------------------------------------------------------ consequences (theorems)

/// THEOREM R3-top (INT-HIST-02, INT-HIST-03 for a top-level load with no caller transaction):
/// once the load's History row was inserted, the durable store holds it in every reachable
/// state, including after the load's own rollback and after process death of either kind.
pub proof fn theorem_top_record_durable(c: Cfg, s: State)
    requires inv(c, s), !c.caller_tx, gatt(s.att, 0) >= 1,
    ensures gtwin(dv(s).twins, 0).ex,
{
}

/// THEOREM COMMIT-SOUND (INT-HIST-05, no false "not committed"): for a top-level load with no
/// caller transaction that returned, a row reading Committed=0 means the namespace modules are
/// exactly as before the load.
pub proof fn theorem_commit_sound(c: Cfg, s: State)
    requires inv(c, s), !c.caller_tx, s.alive, s.top_pc == 7, kept(c.init, s.ns.mods, 0),
    ensures gtwin(s.ns.twins, 0).com, gtwin(s.ns.twins, 0).ex,
{
}

/// THEOREM TOP-RECORD-COMPLETE (INT-HIST-03, Success = status, TimeEnd set when the action
/// ended normally, Finalized): a top-level load with no caller transaction that returned has a
/// finalized row with TimeEnd set and Success equal to the returned status.
pub proof fn theorem_top_record_complete(c: Cfg, s: State)
    requires inv(c, s), !c.caller_tx, s.alive, s.top_pc == 7,
    ensures
        gtwin(s.ns.twins, 0).ex, gtwin(s.ns.twins, 0).fin, gtwin(s.ns.twins, 0).tend,
        gtwin(s.ns.twins, 0).ok == (if s.top_err { 2u8 } else { 1u8 }),
{
}

/// THEOREM TLEVEL: LoadNewModule returns at the $TLEVEL it was entered at (both activations).
pub proof fn theorem_tlevel(c: Cfg, s: State)
    requires inv(c, s), s.alive,
    ensures
        (s.top_pc == 3 && s.lnm0.pc == 32) ==> s.tx.len() == s.lnm0.itl,
        (dep_running(s) && s.lnm1.pc == 32) ==> s.tx.len() == s.lnm1.itl,
{
}

/// THEOREM PERSIST-TARGET: PersistToTwin always finds its row (the N-P15 branch, where the row
/// was deleted, is unreachable from the load itself).
pub proof fn theorem_persist_target(c: Cfg, s: State)
    requires inv(c, s), s.alive,
    ensures
        s.top_pc == 5 ==> gtwin(s.ns.twins, 0).ex,
        (s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc == 7) ==> gtwin(s.ns.twins, dslot(s)).ex,
{
}

// ------------------------------------------------------------------ R1/R2 per transition

pub open spec fn dv_mono(a: Ns, b: Ns) -> bool { fin_stable(a, b) && rows_kept(a, b) }

pub proof fn lemma_mono_top(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 0,
    ensures dv_mono(dv(s), dv(top_step(c, s))),
{
    if s.top_pc == 0 { assert(dv_mono(dv(s), dv(top_step(c, s)))); }
    else if s.top_pc == 1 { assert(dv_mono(dv(s), dv(top_step(c, s)))); }
    else if s.top_pc == 2 { assert(dv_mono(dv(s), dv(top_step(c, s)))); }
    else if s.top_pc == 3 { assert(dv_mono(dv(s), dv(top_step(c, s)))); }
    else if s.top_pc == 4 { assert(dv_mono(dv(s), dv(top_step(c, s)))); }
    else if s.top_pc == 5 { assert(dv_mono(dv(s), dv(top_step(c, s)))); }
    else { assert(dv_mono(dv(s), dv(top_step(c, s)))); }
}

pub proof fn lemma_mono_lmr(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 2,
    ensures dv_mono(dv(s), dv(lmr_step(c, s))),
{
    if s.lmr_pc <= 4 { assert(dv_mono(dv(s), dv(lmr_step(c, s)))); }
    else if s.lmr_pc == 7 { assert(dv_mono(dv(s), dv(lmr_step(c, s)))); }
    else { assert(dv_mono(dv(s), dv(lmr_step(c, s)))); }
}

pub proof fn lemma_mono_lnm0_a(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1, 0 <= s.lnm0.pc <= 6,
    ensures dv_mono(dv(s), dv(lnm_step(c, s, 0))),
{
    if s.lnm0.pc == 0 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 1 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 2 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 3 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 4 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 5 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 6 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); }
}

pub proof fn lemma_mono_lnm1_a(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3, 0 <= s.lnm1.pc <= 6,
    ensures dv_mono(dv(s), dv(lnm_step(c, s, 1))),
{
    if s.lnm1.pc == 0 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 1 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 2 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 3 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 4 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 5 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 6 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); }
}

pub proof fn lemma_mono_lnm0_b(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1, 7 <= s.lnm0.pc <= 13,
    ensures dv_mono(dv(s), dv(lnm_step(c, s, 0))),
{
    if s.lnm0.pc == 7 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 8 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 9 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 10 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 11 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 12 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 13 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); }
}

pub proof fn lemma_mono_lnm1_b(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3, 7 <= s.lnm1.pc <= 13,
    ensures dv_mono(dv(s), dv(lnm_step(c, s, 1))),
{
    if s.lnm1.pc == 7 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 8 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 9 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 10 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 11 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 12 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 13 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); }
}

pub proof fn lemma_mono_lnm0_c(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1, 14 <= s.lnm0.pc <= 28,
    ensures dv_mono(dv(s), dv(lnm_step(c, s, 0))),
{
    if s.lnm0.pc == 14 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 15 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 16 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 17 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 18 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 19 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 20 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 21 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 22 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 23 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 24 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 25 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 26 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 27 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 28 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); }
}

pub proof fn lemma_mono_lnm1_c(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3, 14 <= s.lnm1.pc <= 28,
    ensures dv_mono(dv(s), dv(lnm_step(c, s, 1))),
{
    if s.lnm1.pc == 14 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 15 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 16 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 17 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 18 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 19 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 20 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 21 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 22 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 23 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 24 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 25 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 26 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 27 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 28 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); }
}

pub proof fn lemma_mono_lnm0_d(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 1, 29 <= s.lnm0.pc <= 32,
    ensures dv_mono(dv(s), dv(lnm_step(c, s, 0))),
{
    if s.lnm0.pc == 29 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 30 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 31 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else if s.lnm0.pc == 32 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); 
    } else { assert(dv_mono(dv(s), dv(lnm_step(c, s, 0)))); }
}

pub proof fn lemma_mono_lnm1_d(c: Cfg, s: State)
    requires inv(c, s), running(s), active(s) == 3, 29 <= s.lnm1.pc <= 32,
    ensures dv_mono(dv(s), dv(lnm_step(c, s, 1))),
{
    if s.lnm1.pc == 29 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 30 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 31 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else if s.lnm1.pc == 32 { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); 
    } else { assert(dv_mono(dv(s), dv(lnm_step(c, s, 1)))); }
}

pub proof fn lemma_mono_step(c: Cfg, s: State)
    requires inv(c, s), running(s),
    ensures dv_mono(dv(s), dv(step(c, s))),
{
    if active(s) == 0 { lemma_mono_top(c, s); }
    else if active(s) == 2 { lemma_mono_lmr(c, s); }
    else if active(s) == 1 {
        if s.lnm0.pc <= 6 { lemma_mono_lnm0_a(c, s); } else if s.lnm0.pc <= 13 { lemma_mono_lnm0_b(c, s); }
        else if s.lnm0.pc <= 28 { lemma_mono_lnm0_c(c, s); } else { lemma_mono_lnm0_d(c, s); }
    } else {
        if s.lnm1.pc <= 6 { lemma_mono_lnm1_a(c, s); } else if s.lnm1.pc <= 13 { lemma_mono_lnm1_b(c, s); }
        else if s.lnm1.pc <= 28 { lemma_mono_lnm1_c(c, s); } else { lemma_mono_lnm1_d(c, s); }
    }
}

pub proof fn lemma_mono_kill(c: Cfg, s: State)
    requires inv(c, s), running(s),
    ensures dv_mono(dv(s), dv(kill(c, s))),
{
}

pub proof fn lemma_mono_terminate(c: Cfg, s: State)
    requires inv(c, s), running(s),
    ensures dv_mono(dv(s), dv(terminate(c, s))),
{
}

pub proof fn lemma_mono_caller(c: Cfg, s: State, commit: bool)
    requires inv(c, s), c.caller_tx, s.alive, s.top_pc == 7,
    ensures fin_stable(dv(s), dv(caller_end(c, s, commit))), commit ==> rows_kept(dv(s), dv(caller_end(c, s, commit))),
{
}

pub proof fn lemma_mono_edit(c: Cfg, s: State, i: usize)
    requires inv(c, s), s.alive, user_edit_enabled(s, i),
    ensures dv_mono(dv(s), dv(user_edit(c, s, i))),
{
}

/// THEOREM R2 (INT-HIST-14): no transition changes a finalized row of the durable store,
/// including another process's edit (the trigger refuses it) and either kind of process end.
pub proof fn theorem_r2(c: Cfg, s: State, s2: State)
    requires inv(c, s), next(c, s, s2),
    ensures fin_stable(dv(s), dv(s2)),
{
    if running(s) && s2 == step(c, s) { lemma_mono_step(c, s); }
    else if running(s) && s2 == kill(c, s) { lemma_mono_kill(c, s); }
    else if running(s) && s2 == terminate(c, s) { lemma_mono_terminate(c, s); }
    else if c.caller_tx && s.alive && s.top_pc == 7 && s2 == caller_end(c, s, true) { lemma_mono_caller(c, s, true); }
    else if c.caller_tx && s.alive && s.top_pc == 7 && s2 == caller_end(c, s, false) { lemma_mono_caller(c, s, false); }
    else {
        let i = choose |i: usize| #[trigger] user_edit_enabled(s, i) && s.alive && s2 == user_edit(c, s, i);
        lemma_mono_edit(c, s, i);
    }
}

/// THEOREM R1: no transition of the load removes a row from the durable store (before the caller
/// of a caller-held transaction rolls it back, which is outside IPM).
pub proof fn theorem_r1(c: Cfg, s: State, s2: State)
    requires inv(c, s), next(c, s, s2), !(c.caller_tx && s2.caller == 2),
    ensures rows_kept(dv(s), dv(s2)),
{
    if running(s) && s2 == step(c, s) { lemma_mono_step(c, s); }
    else if running(s) && s2 == kill(c, s) { lemma_mono_kill(c, s); }
    else if running(s) && s2 == terminate(c, s) { lemma_mono_terminate(c, s); }
    else if c.caller_tx && s.alive && s.top_pc == 7 && s2 == caller_end(c, s, true) { lemma_mono_caller(c, s, true); }
    else if c.caller_tx && s.alive && s.top_pc == 7 && s2 == caller_end(c, s, false) { assert(s2.caller == 2); }
    else {
        let i = choose |i: usize| #[trigger] user_edit_enabled(s, i) && s.alive && s2 == user_edit(c, s, i);
        lemma_mono_edit(c, s, i);
    }
}

/// The invariant is satisfiable (non-vacuity): it holds in the initial state of a concrete load.
pub proof fn lemma_inv_nonvacuous()
    ensures inv(crate::cases::cfg0(), init(crate::cases::cfg0())) || ACTIVE_VARIANT() != 0,
{
    if ACTIVE_VARIANT() == 0 {
        assert(cfg_wf(crate::cases::cfg0())) by (compute_only);
        lemma_init_inv(crate::cases::cfg0());
    }
}

} // verus!
