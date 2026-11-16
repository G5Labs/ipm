use vstd::prelude::*;
verus! {
pub struct Resolution {pub ok:bool,pub children:Set<int>}
// The extension is allowed to return either status; successful enumeration is
// local until its hook succeeds. The native injection returns an error; a throw
// caught by ResolveChildren reaches the same returned-status boundary. This
// model permits no hook mutation of code/module/manifest before that return.
pub open spec fn resolve(enumerated:Set<int>,hook_ok:bool)->Resolution {
 if hook_ok{Resolution{ok:true,children:enumerated}}else{Resolution{ok:false,children:enumerated}}
}
pub struct State {pub module:bool,pub manifest:bool,pub code:Set<int>}
pub struct Outcome {pub ok:bool,pub state:State}
pub open spec fn package(s:State,enumerated:Set<int>,hook_ok:bool)->Outcome {
 let r=resolve(enumerated,hook_ok);Outcome{ok:r.ok,state:s}
}
pub open spec fn uninstall(s:State,enumerated:Set<int>,hook_ok:bool)->Outcome {
 let r=resolve(enumerated,hook_ok);
 // As built Clean uses DO and discards r.ok. Final metadata deletion succeeds
 // in this domain, even if the child list is empty after a returned error.
 Outcome{ok:true,state:State{module:false,manifest:false,code:s.code.difference(r.children)}}
}
pub open spec fn repaired_uninstall(s:State,enumerated:Set<int>,hook_ok:bool)->Outcome {
 let r=resolve(enumerated,hook_ok);
 if !r.ok{Outcome{ok:false,state:s}}else{uninstall(s,enumerated,hook_ok)}
}
pub open spec fn before()->State {State{module:true,manifest:true,code:set![71int]}}
pub open spec fn b(v:bool)->int {if v{1}else{0}}
pub proof fn returned_error_neighbors() {
 assert(resolve(set![71int],true).children.contains(71));
 assert(!resolve(set![71int],false).ok);assert(resolve(set![71int],false).children.is_empty());
 assert(!package(before(),set![71int],false).ok);
 assert(uninstall(before(),set![71int],false).ok);
 assert(!uninstall(before(),set![71int],false).state.module);
 assert(!uninstall(before(),set![71int],false).state.manifest);
 assert(uninstall(before(),set![71int],false).state.code.contains(71));
 assert(uninstall(before(),set![71int],true).ok);
 assert(!uninstall(before(),set![71int],true).state.code.contains(71));
 assert(!repaired_uninstall(before(),set![71int],false).ok);
 assert(repaired_uninstall(before(),set![71int],false).state==before());
 assert(repaired_uninstall(before(),set![71int],true)==uninstall(before(),set![71int],true));
}
pub proof fn failed_hook_leaves_code(s:State,e:Set<int>)
 ensures uninstall(s,e,false).state.code==s.code,!package(s,e,false).ok
{}
}
fn main(){}
