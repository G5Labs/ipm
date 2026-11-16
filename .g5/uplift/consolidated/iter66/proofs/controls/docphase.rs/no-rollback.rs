#![allow(non_snake_case)]
// Iteration66: the `document` lifecycle phase.
// Source role at pin 571e04a1: Lifecycle/Base.cls::%Document. With Document.Recurse (default 1) it builds
// the dependency graph, refuses when a dependency is not installed (or fails resolution when no repository
// offers it), runs the Document phase on each dependency with Recurse=0, all inside one transaction. It
// generates nothing itself; only Document-phase hooks (Invoke, processors) act. Storage/Module.cls::
// ExecutePhases prints START before the lifecycle body and runs After-Document Invokes after it.
// Premises (native-qualified): dependency resolution against the local filesystem repository and
// database rollback of hook writes. Fixture: 1 d66, 2 m66 (needs 1), 3 e66 (hook fails), 4 f66 (needs 3),
// 5 n66 (no hook, no classes).
use vstd::prelude::*;

verus! {

pub struct W { pub d_installed: bool, pub d_offered: bool }
pub struct R { pub ok: bool, pub not_installed: bool, pub marks: Seq<int>, pub starts: int }

pub open spec fn dep(m: int) -> int { if m == 2 { 1 } else if m == 4 { 3 } else { 0 } }
pub open spec fn installed(w: W, x: int) -> bool { x != 1 || w.d_installed }
pub open spec fn has_hook(x: int) -> bool { x != 5 }
pub open spec fn hook_fails(x: int) -> bool { x == 3 }
pub open spec fn own(x: int) -> Seq<int> { if has_hook(x) { seq![x] } else { seq![] } }

pub open spec fn document(w: W, m: int, recurse: bool) -> R {
    let d = dep(m);
    if recurse && d != 0 && !installed(w, d) {
        R { ok: false, not_installed: w.d_offered, marks: seq![], starts: 1 }
    } else {
        let deps_run = recurse && d != 0;
        let marks = (if deps_run { own(d) } else { seq![] }) + own(m);
        let fails = (deps_run && hook_fails(d)) || hook_fails(m);
        R { ok: !fails, not_installed: false, marks: marks, starts: if deps_run { 2 } else { 1 } }
    }
}

pub open spec fn pos(s: Seq<int>, x: int) -> int { if s.len() > 0 && s[0] == x { 1 } else if s.len() > 1 && s[1] == x { 2 } else { 0 } }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
pub open spec fn full() -> W { W { d_installed: true, d_offered: false } }

pub proof fn neighbors()
    ensures
        document(full(), 2, true).ok, pos(document(full(), 2, true).marks, 1) == 1, pos(document(full(), 2, true).marks, 2) == 2,
        pos(document(full(), 2, false).marks, 1) == 0, pos(document(full(), 2, false).marks, 2) == 1,
        document(full(), 5, true).ok, document(full(), 5, true).marks.len() == 0,
        !document(full(), 4, true).ok, pos(document(full(), 4, true).marks, 3) == 0,
        !document(W { d_installed: false, d_offered: false }, 2, true).ok,
        !document(W { d_installed: false, d_offered: false }, 2, true).not_installed,
        document(W { d_installed: false, d_offered: true }, 2, true).not_installed,
{}

// The phase itself writes nothing: with no hooks anywhere, nothing is marked.
pub proof fn phase_generates_nothing(w: W)
    ensures document(w, 5, true).marks == Seq::<int>::empty()
{
    assert(document(w, 5, true).marks =~= Seq::<int>::empty());
}

} // verus!

fn main() {}
