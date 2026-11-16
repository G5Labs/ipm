use vstd::prelude::*;
verus! {
// Code package/routine pair, after both successful Config creates. No repository
// loop, local installation, config failure or concurrent writer is represented.
pub struct S {pub cfg:Set<int>,pub active:Set<int>}
pub struct Scan {pub state:S,pub ok:bool,pub marked:Set<int>}
pub struct Out {pub state:S,pub ok:bool,pub activated:bool}
pub open spec fn empty()->S {S{cfg:Set::empty(),active:Set::empty()}}
pub open spec fn visit(s:S,targets:Seq<int>,existing:Set<int>,preview:bool)->Scan
 decreases targets.len()
{
 if targets.len()==0 {Scan{state:s,ok:true,marked:Set::empty()}}
 else if false {Scan{state:s,ok:false,marked:Set::empty()}}
 else {
  let next=if preview{s}else{S{cfg:s.cfg.insert(targets[0]),..s}};
  let rest=visit(next,targets.drop_first(),existing,preview);
  Scan{marked:if preview{rest.marked}else{rest.marked.insert(targets[0])},..rest}
 }
}
// Effective visibility requires loading each marked namespace after MoveToActive.
// A different namespace activation does not refresh a pending target.
pub open spec fn publish(s:S,marked:Set<int>)->S {S{active:s.active.union(marked),..s}}
pub open spec fn run(s:S,targets:Seq<int>,existing:Set<int>,preview:bool)->Out {
 let scan=visit(s,targets,existing,preview);
 let activate=scan.ok && !preview && scan.marked.len()>0;
 Out{state:if activate{publish(scan.state,scan.marked)}else{scan.state},ok:scan.ok,activated:activate}
}
pub open spec fn known()->Set<int> {set![1int,2int,3int,4int,5int]}
pub open spec fn late()->Out {run(empty(),seq![1int,9int],known(),false)}
pub open spec fn first_bad()->Out {run(empty(),seq![9int,2int],known(),false)}
pub open spec fn preview_bad()->Out {run(empty(),seq![3int,9int],known(),true)}
pub open spec fn retry()->Out {run(late().state,seq![1int],known(),false)}
pub open spec fn other()->Out {run(late().state,seq![4int],known(),false)}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub proof fn selected_neighbors()
 ensures !late().ok,late().state.cfg.contains(1),!late().state.active.contains(1),!late().activated,
 !first_bad().ok,!first_bad().state.cfg.contains(2),!first_bad().state.active.contains(2),
 !preview_bad().ok,!preview_bad().state.cfg.contains(3),!preview_bad().state.active.contains(3),
 retry().ok,retry().state.cfg.contains(1),retry().state.active.contains(1),retry().activated,
 other().ok,!other().state.active.contains(1),other().state.active.contains(4),
 other().state.cfg.contains(1),other().state.cfg.contains(4)
{
 reveal_with_fuel(visit,4);
 assert(!late().ok);assert(late().state.cfg.contains(1));assert(!late().state.active.contains(1));assert(!late().activated);
 assert(!first_bad().ok);assert(!first_bad().state.cfg.contains(2));assert(!first_bad().state.active.contains(2));
 assert(!preview_bad().ok);assert(!preview_bad().state.cfg.contains(3));assert(!preview_bad().state.active.contains(3));
 assert(retry().ok);assert(retry().state.cfg.contains(1));assert(retry().state.active.contains(1));assert(retry().activated);
 assert(other().ok);assert(!other().state.active.contains(1));assert(other().state.active.contains(4));
 assert(other().state.cfg.contains(1));assert(other().state.cfg.contains(4));
}
}
fn main(){}
