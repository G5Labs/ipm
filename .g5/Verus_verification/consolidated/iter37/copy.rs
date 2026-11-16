#![allow(non_snake_case)]
// Iteration37: the Copy resource processor across load and uninstall.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter37/bindings.json):
//   ResourceProcessor/Copy.cls::OnAfterPhase (Reload)   export the package's classes from Source, drop the ones that
//                                                       already exist in the target when Overwrite is false, load the rest
//   ResourceProcessor/Copy.cls::OnResolveChildren       leave out children owned by any module resource in the target
//   ResourceProcessor/Copy.cls::TargetSet, OnPhase      target namespace bookkeeping; Compile in the target namespace
//   Lifecycle/Base.cls::%Clean (via uninstall)          resolve the package's children in the target and delete them
//
// A class is identified by an index into a fixed universe; `pkg` is its package, `src` its value in the
// Source namespace, `owned` whether another installed module lists it as a resource. Premises
// (native-qualified): $system.OBJ.Export/Load copy and compile a class body (the loader's
// /checkuptodate did not skip a newer target in the native run), and package resolution in the target
// namespace returns every class of the package present there.
use vstd::prelude::*;

verus! {

pub struct Cls { pub pkg: int, pub src: Option<int>, pub owned: bool }
pub type Target = Seq<Option<int>>;

pub open spec fn child(c: Cls, pkg: int) -> bool { c.pkg == pkg && !c.owned }

// Load of a module whose Copy resource names package `pkg`.
pub open spec fn copy_one(c: Cls, t: Option<int>, pkg: int, overwrite: bool) -> Option<int> {
    if child(c, pkg) && c.src.is_some() && (overwrite || t.is_none()) { c.src } else { t }
}
pub open spec fn load(u: Seq<Cls>, t: Target, pkg: int, overwrite: bool) -> Target {
    Seq::new(t.len(), |i: int| copy_one(u[i], t[i], pkg, overwrite))
}
// Uninstall: every class of the package present in the target is deleted unless another module owns it.
pub open spec fn uninstall(u: Seq<Cls>, t: Target, pkg: int) -> Target {
    Seq::new(t.len(), |i: int| if child(u[i], pkg) { None } else { t[i] })
}

// Generic neighbours (pointwise over any universe).
pub proof fn overwrite_false_keeps_existing(u: Seq<Cls>, t: Target, pkg: int, i: int)
    requires u.len() == t.len(), 0 <= i < t.len(), t[i].is_some()
    ensures load(u, t, pkg, false)[i] == t[i]
{}
pub proof fn owned_never_copied_or_deleted(u: Seq<Cls>, t: Target, pkg: int, ow: bool, i: int)
    requires u.len() == t.len(), 0 <= i < t.len(), u[i].owned
    ensures load(u, t, pkg, ow)[i] == t[i], uninstall(u, t, pkg)[i] == t[i]
{}
pub proof fn overwrite_true_replaces(u: Seq<Cls>, t: Target, pkg: int, i: int)
    requires u.len() == t.len(), 0 <= i < t.len(), child(u[i], pkg), u[i].src.is_some()
    ensures load(u, t, pkg, true)[i] == u[i].src
{}
// As built, uninstall deletes a class of the package that the load never wrote: it existed in the
// target before the load and Overwrite=false kept it.
pub proof fn uninstall_deletes_kept_item(u: Seq<Cls>, t: Target, pkg: int, i: int)
    requires u.len() == t.len(), 0 <= i < t.len(), child(u[i], pkg), t[i].is_some()
    ensures load(u, t, pkg, false)[i] == t[i], uninstall(u, load(u, t, pkg, false), pkg)[i].is_none()
{}

// Fixture: 0..2 CopyA.C1..C3 (pkg 1), 3..4 CopyB.D1..D2 (pkg 2), 5 CopyC.N1 (pkg 3),
// 6 CopyD.Owned (pkg 4, owned by ownmod), 7 CopyD.Free (pkg 4).
pub open spec fn universe() -> Seq<Cls> {
    seq![Cls { pkg: 1, src: Some(10), owned: false }, Cls { pkg: 1, src: Some(20), owned: false }, Cls { pkg: 1, src: Some(30), owned: false },
         Cls { pkg: 2, src: Some(40), owned: false }, Cls { pkg: 2, src: Some(41), owned: false },
         Cls { pkg: 3, src: Some(50), owned: false },
         Cls { pkg: 4, src: Some(60), owned: true }, Cls { pkg: 4, src: Some(61), owned: false }]
}
pub open spec fn start() -> Target { seq![None, Some(2), None, Some(4), None, Some(5), Some(6), None] }
pub open spec fn o1() -> Target { load(universe(), start(), 1, true) }
pub open spec fn o2() -> Target { load(universe(), o1(), 2, false) }
pub open spec fn o3() -> Target { load(universe(), o2(), 3, true) }
pub open spec fn o4() -> Target { load(universe(), o3(), 4, true) }
pub open spec fn u1() -> Target { uninstall(universe(), o4(), 1) }
pub open spec fn u2() -> Target { uninstall(universe(), u1(), 2) }
pub open spec fn u4() -> Target { uninstall(universe(), u2(), 4) }

pub open spec fn defined(t: Target, i: int) -> int { if t[i].is_some() { 1 } else { 0 } }
pub open spec fn value(t: Target, i: int) -> int { match t[i] { Some(v) => v, None => -1 } }

pub proof fn fixture_schedule()
    ensures
        value(o1(), 1) == 20, value(o2(), 3) == 4, value(o2(), 4) == 41, value(o3(), 5) == 50,
        value(o4(), 6) == 6, value(o4(), 7) == 61,
        defined(u1(), 1) == 0, defined(u2(), 3) == 0, defined(u4(), 6) == 1, defined(u4(), 7) == 0,
{
    assert(value(o1(), 1) == 20) by (compute);
    assert(value(o2(), 3) == 4) by (compute);
    assert(value(o2(), 4) == 41) by (compute);
    assert(value(o3(), 5) == 50) by (compute);
    assert(value(o4(), 6) == 6) by (compute);
    assert(value(o4(), 7) == 61) by (compute);
    assert(defined(u1(), 1) == 0) by (compute);
    assert(defined(u2(), 3) == 0) by (compute);
    assert(defined(u4(), 6) == 1) by (compute);
    assert(defined(u4(), 7) == 0) by (compute);
}

} // verus!

fn main() {}
