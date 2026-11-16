#![allow(non_snake_case)]
// Iteration52: the stored module record across loads of a changed manifest, and lifecycle class choice.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter52/bindings.json):
//   Storage/Module.cls::LifecycleGet               first compiled class among OverrideLifecycleClass, LifecycleClass
//                                                  (short names under %IPM.Lifecycle); otherwise Module, and
//                                                  LifecycleClass is overwritten with "Module"
//   Storage/Module.cls::OverrideLifecycleClassSet   resets the cached lifecycle when the override changes
//   Storage/Module.cls::%OnAddToSaveSet, %OnBeforeSave   hash of the exported definition (which includes LastUpdated)
//                                                  sets LastModifiedTimestamp; LastUpdated set on every save
//   Storage/Module.cls::%OnNew, %OnOpen            Version object from VersionString; Root from source control
//   Utils/Module.cls::LoadNewModule (load path)     the manifest replaces resources, dependencies and defaults
// Premises (native-qualified): ExecutePhases resolves the lifecycle before Initialize and again after the
// "*" refresh that follows Reload; preload code is compiled by then, ordinary resources only in Compile.
use vstd::prelude::*;

verus! {

// Lifecycle choice. `compiled` says whether the named class exists when the lifecycle is resolved.
pub enum Life { Module, Named(int) }
pub open spec fn resolve(name: Option<int>, compiled: bool) -> Life {
    match name { Some(n) => if compiled { Life::Named(n) } else { Life::Module }, None => Life::Module }
}
// First install: an ordinary-resource class is not yet compiled at either resolution; a preload class is
// compiled by the refresh after Initialize; an earlier install leaves the class compiled.
pub open spec fn compiled_at_refresh(preload: bool, installed_before: bool, exists: bool) -> bool {
    exists && (preload || installed_before)
}
pub open spec fn first_install(name: int, preload: bool) -> Life { resolve(Some(name), compiled_at_refresh(preload, false, true)) }
pub open spec fn reinstall(name: int) -> Life { resolve(Some(name), compiled_at_refresh(false, true, true)) }
pub open spec fn unknown(name: int) -> Life { resolve(Some(name), compiled_at_refresh(false, true, false)) }

// Record and namespace after loading a changed manifest: the record takes the new lists; code and
// dependency modules from the old manifest stay.
pub struct State { pub record_resources: nat, pub record_deps: nat, pub old_class_defined: bool, pub dep_installed: bool, pub default_value: int }
pub open spec fn load_changed(s: State, new_resources: nat, new_deps: nat, new_default: int) -> State {
    State { record_resources: new_resources, record_deps: new_deps, default_value: new_default, ..s }
}

// LastModifiedTimestamp: the hash covers LastUpdated, which every save changes.
pub open spec fn modified_changes(definition_changed: bool) -> bool { definition_changed }

pub proof fn lifecycle_neighbors()
    ensures first_install(2, false) == Life::Module, reinstall(2) == Life::Named(2),
            first_install(5, true) == Life::Named(5), unknown(3) == Life::Module
{}

pub proof fn changed_manifest_keeps_old_code(s: State)
    requires s.old_class_defined, s.dep_installed
    ensures load_changed(s, 1, 0, 2).record_resources == 1, load_changed(s, 1, 0, 2).record_deps == 0,
            load_changed(s, 1, 0, 2).old_class_defined, load_changed(s, 1, 0, 2).dep_installed,
            load_changed(s, 1, 0, 2).default_value == 2
{}

pub proof fn identical_reload_still_modified()
    ensures modified_changes(false)
{}

pub open spec fn s0() -> State { State { record_resources: 2, record_deps: 1, old_class_defined: true, dep_installed: true, default_value: 1 } }
pub open spec fn code(l: Life) -> int { match l { Life::Module => 1, Life::Named(n) => n } }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

} // verus!

fn main() {}
