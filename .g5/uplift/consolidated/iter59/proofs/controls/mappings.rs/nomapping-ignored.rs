#![allow(non_snake_case)]
// Iteration59: automatic resource mappings and where module code lives across load and uninstall.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter59/bindings.json):
//   ResourceProcessor/Default/{Package,Class,Routine,LocalizedMessages}.cls::OnConfigureMappings
//       if the module is not GlobalScope and the resource does not resolve to the namespace's routine
//       database, add a package/routine mapping to that database
//   ResourceProcessor/Default/Document.cls::ResourceIsMappedToDefaultDB (the test), ::OnPhase (Reload calls the
//       hook unless NoMapping)
//   Utils/Module.cls::AddPackageMapping (create + activate); no uninstall path removes these mappings
//   Lifecycle/Base.cls::%Clean  skips a GlobalScope module without -global; deletes classes unless they are
//       mapped (resolve outside the namespace database), which only -global (Clean.GlobalScope) removes
// Premises (native-qualified): %Z packages resolve to IRISSYS by default; Config.MapPackages and
// %SYS.Namespace.GetPackageDest report mappings and destinations; the compiler stores a class where its
// package resolves.
use vstd::prelude::*;

verus! {

pub struct Pkg { pub default_dest: bool, pub sys: bool }          // resolves to the namespace DB; is a %Z package
pub struct St { pub map: bool, pub code: int, pub module: bool }  // code: 0 none, 1 namespace DB, 2 elsewhere

pub open spec fn needs_map(p: Pkg, s: St, nomapping: bool, global: bool) -> bool {
    !global && !p.default_dest && !s.map
}
pub open spec fn load(p: Pkg, s: St, nomapping: bool, global: bool) -> St {
    let map = s.map || needs_map(p, s, nomapping, global);
    St { map: map, code: if map || p.default_dest { 1 } else { 2 }, module: true }
}
pub open spec fn uninstall(s: St, module_global: bool, flag_global: bool) -> St {
    if module_global && !flag_global { s }
    else { St { map: s.map, code: if s.code == 2 && !flag_global { 2 } else { 0 }, module: false } }
}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
pub open spec fn visible_user(s: St) -> int { b(s.code != 0) }
pub open spec fn visible_sys(p: Pkg, s: St) -> int { b(s.code == 2 && p.sys) }

pub proof fn mapping_outlives_uninstall(p: Pkg, s: St)
    requires !p.default_dest, !s.map, s.code == 0
    ensures load(p, s, false, false).map, uninstall(load(p, s, false, false), false, false).map,
            uninstall(load(p, s, false, false), false, false).code == 0
{}
pub proof fn nomapping_code_orphaned(p: Pkg, s: St)
    requires !p.default_dest, !s.map
    ensures load(p, s, true, false).code == 2, !uninstall(load(p, s, true, false), false, false).module,
            uninstall(load(p, s, true, false), false, false).code == 2,
            uninstall(load(p, s, true, false), false, true).code == 0
{}
pub proof fn ordinary_package_unmapped(p: Pkg, s: St)
    requires p.default_dest, !s.map
    ensures !load(p, s, false, false).map, load(p, s, false, false).code == 1
{}
pub proof fn global_module_skipped(p: Pkg, s: St)
    requires !p.default_dest, !s.map
    ensures uninstall(load(p, s, false, true), true, false) == load(p, s, false, true),
            uninstall(load(p, s, false, true), true, true).code == 0
{}

// Native schedule.
pub open spec fn A() -> Pkg { Pkg { default_dest: false, sys: true } }   // %ZI59
pub open spec fn B() -> Pkg { Pkg { default_dest: false, sys: false } }  // P59.Sub after the admin maps P59 -> DB59
pub open spec fn C() -> Pkg { Pkg { default_dest: false, sys: true } }   // %ZI59C with -DNoMapping=1
pub open spec fn D() -> Pkg { Pkg { default_dest: true, sys: false } }   // N59
pub open spec fn E() -> Pkg { Pkg { default_dest: false, sys: true } }   // %ZI59G, GlobalScope
pub open spec fn none() -> St { St { map: false, code: 0, module: false } }
pub open spec fn a1() -> St { load(A(), none(), false, false) }
pub open spec fn a2() -> St { uninstall(a1(), false, false) }
pub open spec fn b1() -> St { load(B(), none(), false, false) }
pub open spec fn b2() -> St { uninstall(b1(), false, false) }
pub open spec fn c1() -> St { load(C(), none(), true, false) }
pub open spec fn c2() -> St { uninstall(c1(), false, false) }
pub open spec fn c3() -> St { load(C(), c2(), true, false) }
pub open spec fn c4() -> St { uninstall(c3(), false, true) }
pub open spec fn d1() -> St { load(D(), none(), false, false) }
pub open spec fn e1() -> St { load(E(), none(), false, true) }
pub open spec fn e2() -> St { uninstall(e1(), true, false) }
pub open spec fn e3() -> St { uninstall(e2(), true, true) }

pub proof fn schedule()
    ensures a2().map, visible_user(a2()) == 0, b2().map, visible_sys(C(), c2()) == 1, !c2().module,
            visible_sys(C(), c4()) == 0, !d1().map, visible_sys(E(), e2()) == 1, e2().module, !e3().module
{}

} // verus!

fn main() {}
