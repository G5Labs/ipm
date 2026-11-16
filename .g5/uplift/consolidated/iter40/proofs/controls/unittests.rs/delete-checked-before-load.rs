#![allow(non_snake_case)]
// Iteration40: UnitTest resource processor, from load to the test phase to uninstall.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter40/bindings.json):
//   ResourceProcessor/Test.cls::OnAfterPhase   at Reload, load the test directory only in developer mode
//   ResourceProcessor/Test.cls::OnPhase        when the phase matches: load the whole test directory, compile the
//                                              resource's package, decide deletion by checking for package classes
//                                              AFTER that load, run with /nodelete when any exist
//   ResourceProcessor/Test.cls::%OnNew          embedded Package processor over the resource directory
//   Test/Manager.cls::LoadTestDirectory         every subdirectory of the test root, uncompiled
//   uninstall (Clean)                           resolved test children deleted with the module
//
// Two UnitTest resources share one directory: package T (Phase test) and package V (Phase verify).
// Premises (native-qualified): %UnitTest.Manager runs the selected package and honours /nodelete;
// selection by phase is the already-owned build.test.selection rule.
use vstd::prelude::*;

verus! {

pub struct S { pub record: bool, pub main: bool, pub t_def: bool, pub v_def: bool, pub t_compiled: bool, pub ran: int, pub vran: int }

pub open spec fn empty() -> S { S { record: false, main: false, t_def: false, v_def: false, t_compiled: false, ran: 0, vran: 0 } }

// load: Reload loads the test directory only in developer mode, uncompiled.
pub open spec fn load(s: S, dev: bool) -> S {
    S { record: true, main: true, t_def: s.t_def || dev, v_def: s.v_def || dev, ..s }
}

// Directory load inside the test phase defines every package found under the test root.
pub open spec fn load_dir(s: S) -> S { S { t_def: true, v_def: true, ..s } }

// Deletion is decided after load_dir, so the package always has classes and deletion never happens.
pub open spec fn do_delete(after_load: S) -> bool { true }

// `test <module>`: only the Phase="test" resource runs.
pub open spec fn test(s: S) -> S {
    let l = load_dir(s);
    let run = S { t_compiled: true, ran: l.ran + 1, ..l };
    if do_delete(l) { S { t_def: false, t_compiled: false, ..run } } else { run }
}

pub open spec fn uninstall(s: S) -> S { S { record: false, main: false, t_def: false, v_def: false, t_compiled: false, ..s } }

pub proof fn test_leaves_test_code(s: S)
    ensures test(s).t_def, test(s).v_def, test(s).t_compiled, test(s).ran == s.ran + 1, test(s).vran == s.vran
{}
pub proof fn non_dev_load_has_no_tests()
    ensures !load(empty(), false).t_def, load(empty(), true).t_def, !load(empty(), true).t_compiled
{}
pub proof fn uninstall_clears(s: S)
    ensures !uninstall(s).t_def, !uninstall(s).v_def, !uninstall(s).record
{}

pub open spec fn n0() -> S { load(empty(), false) }
pub open spec fn n1() -> S { test(n0()) }
pub open spec fn n2() -> S { uninstall(n1()) }
pub open spec fn d0() -> S { load(n2(), true) }
pub open spec fn d1() -> S { test(d0()) }
pub open spec fn d2() -> S { uninstall(d1()) }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub proof fn schedule()
    ensures n1().t_def, n1().v_def, !n2().t_def, d0().t_def, !d0().t_compiled, d1().ran == 2, d1().vran == 0
{}

} // verus!

fn main() {}
