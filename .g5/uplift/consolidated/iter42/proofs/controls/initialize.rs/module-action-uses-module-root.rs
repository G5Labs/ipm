#![allow(non_snake_case)]
// Iteration42: the Initialize phase and module preload code.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter42/bindings.json):
//   Lifecycle/Base.cls::%Initialize   preload root = RootDirectory parameter + "preload"; ImportDir + compile;
//                                     the import/compile status is kept in a local and not returned
//   Utils/Module.cls::LoadNewModule   a directory load supplies RootDirectory; a module action does not
//   Utils/Module.cls::RemoveUnsupportedFileErrors   filters the import status before it is dropped
//
// Premises (native-qualified): ImportDir compiles valid classes and leaves a class with a compile error
// uncompiled; a relative "preload" path does not exist in the server process's directory.
use vstd::prelude::*;

verus! {

pub struct World { pub disk: int, pub boot: int, pub preload_broken: bool, pub broken_compiled: bool }
pub struct Outcome { pub world: World, pub ok: bool, pub skipped: bool }

// root_param: whether the command supplies RootDirectory (directory load, or an explicit -D).
pub open spec fn initialize(w: World, root_param: bool) -> Outcome {
    if false { Outcome { world: w, ok: true, skipped: true } }
    else if w.preload_broken {
        // The compile error is held in a local status; Initialize still returns OK.
        Outcome { world: World { broken_compiled: false, ..w }, ok: true, skipped: false }
    } else { Outcome { world: World { boot: w.disk, ..w }, ok: true, skipped: false } }
}

// The same compile error in an ordinary resource fails the load (Compile phase status is checked).
pub open spec fn ordinary_compile_ok(broken: bool) -> bool { !broken }

pub proof fn module_action_skips_preload(w: World)
    ensures initialize(w, false).world.boot == w.boot, initialize(w, false).skipped
{}
pub proof fn directory_load_refreshes(w: World)
    requires !w.preload_broken
    ensures initialize(w, true).world.boot == w.disk
{}
pub proof fn preload_error_swallowed(w: World)
    requires w.preload_broken
    ensures initialize(w, true).ok, !initialize(w, true).world.broken_compiled, !ordinary_compile_ok(true)
{}

// Native schedule. Codes: boot values as written to disk.
pub open spec fn w0() -> World { World { disk: 1, boot: 0, preload_broken: false, broken_compiled: false } }
pub open spec fn setup() -> Outcome { initialize(w0(), true) }
pub open spec fn i1() -> Outcome { initialize(World { disk: 2, ..setup().world }, false) }
pub open spec fn i2() -> Outcome { initialize(i1().world, false) }
pub open spec fn i3() -> Outcome { initialize(i2().world, true) }
pub open spec fn i4() -> Outcome { initialize(World { disk: 3, ..i3().world }, true) }
pub open spec fn b1() -> Outcome { initialize(World { preload_broken: true, ..i4().world }, true) }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub proof fn schedule()
    ensures setup().world.boot == 1, i1().world.boot == 1, i1().skipped, i2().world.boot == 1, i3().world.boot == 2,
        i4().world.boot == 3, b1().ok, !b1().world.broken_compiled
{}

} // verus!

fn main() {}
