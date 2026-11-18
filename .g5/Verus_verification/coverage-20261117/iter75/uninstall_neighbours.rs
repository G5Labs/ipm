//! Iteration 75: uninstall data-loss neighbours for IPM 571e04a1. Hand translation, not ObjectScript verification.
//!
//! The deciding computation is inherited unchanged from playbook-uplift/models/verus/uninstall.rs:
//! - `code_gone` transcribes Lifecycle/Base.cls %Clean lines 314, 375-378 and 414-417: collected code whose
//!   `%Library.RoutineMgr.IsMapped` is true is skipped with a WARNING unless `Clean.GlobalScope` is set, which only
//!   the `-global` modifier sets (Main.cls line 500, dataAlias="Clean.GlobalScope").
//! - `clean_step` on `Gbl` transcribes ResourceProcessor/Default/Global.cls lines 76-80: the Clean phase kills
//!   `@..Global` only when the Global attribute is non-empty and Preserve is off.
//! The neighbours below run whole `uninstall <module>` commands (`main_uninstall`) on one-module worlds and read
//! the artifact set (`present`) afterwards, the observation of uninstall.cleanup#1 and #2.
#[path = "../../../../playbook-uplift/models/verus/uninstall.rs"]
mod u;

use vstd::prelude::*;

verus! {

use u::*;

proof fn lookup_only(w: W, k: nat, m: nat)
    requires
        w.mods.contains_key(m),
        w.mods[m].key == k,
        forall|x: nat| #![trigger w.mods[x]] w.mods.contains_key(x) && w.mods[x].key == k ==> x == m,
    ensures lookup(w, k) == Some(m)
{
}

/// Module 2 owns one code resource 13, compiled; `mapped` says whether IsMapped reports it as borrowed from a
/// shared database.
pub open spec fn w_code(mapped: bool) -> W {
    let g = mk(2, 20, seq![], seq![13nat], Set::empty(), Some(Set::empty()));
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, g),
         Map::empty().insert(0nat, code(0)).insert(13nat, Res { kind: RKind::Code { mapped: mapped }, owner: 2 }),
         set![0nat, 13nat])
}

/// Module 2 owns one global resource 13 whose Global attribute is set; `preserve` is its Preserve attribute.
pub open spec fn w_gbl(preserve: bool) -> W {
    let g = mk(2, 20, seq![], seq![13nat], Set::empty(), Some(Set::empty()));
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, g),
         Map::empty().insert(0nat, code(0)).insert(13nat, Res { kind: RKind::Gbl { named: true, preserve: preserve }, owner: 2 }),
         set![0nat, 13nat])
}

/// Walks `uninstall <2>` on a one-resource world down to its Clean result.
proof fn walk(w: W, global: bool)
    requires
        w.mods.contains_key(2) && w.mods.contains_key(0),
        w.mods[2] == mk(2, 20, seq![], seq![13nat], Set::empty(), Some(Set::empty())),
        w.mods[0] == ipm_mod(),
        w.mods.dom() == set![0nat, 2nat],
        w.res.contains_key(13) && !(w.res[13].kind is Web),
        w.res.contains_key(0) && w.res[0] == code(0),
        w.present == set![0nat, 13nat],
    ensures ({
        let o = main_uninstall(w, plain(2), false, false, global, false, 3);
        let w3 = clean_code(w, seq![13nat], global);
        &&& o.ok
        &&& !o.w.mods.contains_key(2)
        &&& o.w.present == w3.present
    })
{
    lookup_only(w, 2, 2);
    assert(w.mods[2].res[0] == 13);
    let p = top_p(false, false, global, false);
    reveal_with_fuel(before_clean, 2);
    reveal_with_fuel(clean_res, 2);
    assert(before_clean(w, seq![13nat]) == w);
    assert(!exists|m: nat| code_dependent(w, 2, p.cycle, m));
    let p1 = P { cycle: p.cycle.insert(2), ..p };
    assert(!exists|m: nat| code_dependent(w, 2, p1.cycle, m));
    let m = w.mods[2];
    let w3 = clean_code(w, seq![13nat], global);
    assert(after_code(w, m, p) == w3);
    assert(after_deps(w3, m, p1, 3) == Out { w: w3, ok: true, skip: false });
    assert(w3.res == w.res);
    assert(w3.present.contains(sd()));
    let e = exec_clean(w, 2, p, 3);
    assert(e == record_delete(Out { w: w3, ok: true, skip: false }, 2));
}

/// uninstall.cleanup#1: without -global a mapped class survives uninstall; with -global it is deleted. Unmapped
/// code is deleted either way.
pub proof fn mapped_code_needs_global()
    ensures
        main_uninstall(w_code(true), plain(2), false, false, false, false, 3).ok,
        main_uninstall(w_code(true), plain(2), false, false, false, false, 3).w.present.contains(13),
        !main_uninstall(w_code(true), plain(2), false, false, true, false, 3).w.present.contains(13),
        !main_uninstall(w_code(false), plain(2), false, false, false, false, 3).w.present.contains(13),
        !main_uninstall(w_code(false), plain(2), false, false, true, false, 3).w.present.contains(13),
{
    reveal_with_fuel(clean_res, 2);
    walk(w_code(true), false);
    walk(w_code(true), true);
    walk(w_code(false), false);
    walk(w_code(false), true);
    let a = clean_res(w_code(true), seq![13nat]);
    assert(a.del.contains(13));
    assert(code_gone(a.w, a.del, true).contains(13));
    assert(!code_gone(a.w, a.del, false).contains(13));
    let b = clean_res(w_code(false), seq![13nat]);
    assert(b.del.contains(13));
    assert(code_gone(b.w, b.del, false).contains(13));
    assert(code_gone(b.w, b.del, true).contains(13));
}

/// uninstall.cleanup#2: a global named by the Global attribute is killed unless Preserve is set; -global does not
/// change that.
pub proof fn preserve_keeps_global_data()
    ensures
        main_uninstall(w_gbl(true), plain(2), false, false, false, false, 3).ok,
        main_uninstall(w_gbl(true), plain(2), false, false, false, false, 3).w.present.contains(13),
        main_uninstall(w_gbl(true), plain(2), false, false, true, false, 3).w.present.contains(13),
        !main_uninstall(w_gbl(false), plain(2), false, false, false, false, 3).w.present.contains(13),
{
    reveal_with_fuel(clean_res, 2);
    walk(w_gbl(true), false);
    walk(w_gbl(true), true);
    walk(w_gbl(false), false);
    let a = clean_res(w_gbl(true), seq![13nat]);
    assert(a.w.present.contains(13));
    assert(!a.del.contains(13));
    let b = clean_res(w_gbl(false), seq![13nat]);
    assert(!b.w.present.contains(13));
}

fn main() {}

} // verus!
