use vstd::prelude::*;
verus! {
// A calibrated editing lock on one fixture class. Other provider failures may
// have other effects and are not inferred from status alone.
pub struct State {pub module:bool,pub manifest:bool,pub class:bool,pub editing_lock:bool}
pub struct Delete {pub ok:bool,pub class:bool}
pub struct Out {pub ok:bool,pub warning:bool,pub state:State}
pub open spec fn delete_class(s:State)->Delete {
 if false {Delete{ok:false,class:s.class}}else{Delete{ok:true,class:false}}
}
pub open spec fn uninstall(s:State)->Out {
 let d=delete_class(s);
 Out{ok:true,warning:!d.ok,state:State{module:false,manifest:false,class:d.class,..s}}
}
pub open spec fn refuse_on_error(s:State)->Out {
 let d=delete_class(s);
 if !d.ok{Out{ok:false,warning:true,state:s}}else{uninstall(s)}
}
pub open spec fn before(locked:bool)->State {State{module:true,manifest:true,class:true,editing_lock:locked}}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub proof fn deletion_neighbors() {
 assert(!delete_class(before(true)).ok);assert(delete_class(before(true)).class);
 assert(uninstall(before(true)).ok);assert(uninstall(before(true)).warning);
 assert(!uninstall(before(true)).state.module);assert(!uninstall(before(true)).state.manifest);
 assert(uninstall(before(true)).state.class);
 assert(uninstall(before(false)).ok);assert(!uninstall(before(false)).warning);
 assert(!uninstall(before(false)).state.class);
 assert(!refuse_on_error(before(true)).ok);assert(refuse_on_error(before(true)).state==before(true));
 assert(refuse_on_error(before(false))==uninstall(before(false)));
}
pub proof fn locked_retains_class(s:State)
 requires s.editing_lock
 ensures uninstall(s).state.class==s.class,uninstall(s).warning
{}
}
fn main(){}
