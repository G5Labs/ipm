use vstd::prelude::*;
verus! {
// Segment identities are case-folded provider output; extension and prefix parsing
// are checked natively, not proved. A nested package is a proper segment prefix.
pub enum Kind { Class, Routine, Include, Package }
pub struct Name {pub parts:Seq<int>,pub kind:Kind}
pub struct Ref {pub name:Name,pub module:int}
pub open spec fn prefix(a:Seq<int>,b:Seq<int>)->bool {
 a.len()<=b.len() && forall|i:int|0<=i<a.len() ==> a[i]==b[i]
}
pub open spec fn protects(r:Ref,root:Name,doc:Name)->bool {
 r.name!=root && prefix(root.parts,r.name.parts) &&
 (r.name==doc || (r.name.kind==Kind::Package && r.name.parts.len()<doc.parts.len() && prefix(r.name.parts,doc.parts)))
}
pub open spec fn claimed(refs:Seq<Ref>,root:Name,doc:Name)->bool
 decreases refs.len()
{ refs.len()>0 && (protects(refs[0],root,doc) || claimed(refs.skip(1),root,doc)) }
pub open spec fn children(files:Set<Name>,refs:Seq<Ref>,root:Name,check:bool)->Set<Name> {
 files.filter(|d:Name| root.parts.len()<d.parts.len() && prefix(root.parts,d.parts) && (!check || !claimed(refs,root,d)))
}
pub struct World {pub modules:Set<int>,pub files:Set<Name>}
pub open spec fn resource_children(files:Set<Name>,all:Seq<Ref>,r:Name)->Set<Name> {
 if r.kind==Kind::Package {children(files,all,r,true)}else{files.filter(|d:Name|d==r)}
}
pub open spec fn module_children(files:Set<Name>,all:Seq<Ref>,todo:Seq<Ref>,module:int)->Set<Name>
 decreases todo.len()
{ if todo.len()==0 {Set::empty()}else{
 let tail=module_children(files,all,todo.skip(1),module);
 if todo[0].module==module{tail.union(resource_children(files,all,todo[0].name))}else{tail}
} }
pub open spec fn clean(s:World,refs:Seq<Ref>,module:int)->World {
 World{modules:s.modules.remove(module),files:s.files.difference(module_children(s.files,refs,refs,module))}
}
pub open spec fn root()->Name {Name{parts:seq![1int,2],kind:Kind::Package}}
pub open spec fn own()->Name {Name{parts:seq![1int,2,3],kind:Kind::Class}}
pub open spec fn extra()->Name {Name{parts:seq![1int,2,4],kind:Kind::Class}}
pub open spec fn nested()->Name {Name{parts:seq![1int,2,5],kind:Kind::Package}}
pub open spec fn inner()->Name {Name{parts:seq![1int,2,5,6],kind:Kind::Class}}
pub open spec fn refs()->Seq<Ref> {seq![Ref{name:root(),module:1},Ref{name:extra(),module:2},Ref{name:nested(),module:3}]}
pub open spec fn world()->World {World{modules:set![1int,2,3],files:set![own(),extra(),inner()]}}
pub proof fn unselected_child_survives(s:World,rs:Seq<Ref>,r:Name,d:Name,m:int)
 requires s.files.contains(d),!module_children(s.files,rs,rs,m).contains(d)
 ensures clean(s,rs,m).files.contains(d)
{}
pub proof fn selected_child_removed(s:World,rs:Seq<Ref>,r:Name,d:Name,m:int)
 requires module_children(s.files,rs,rs,m).contains(d)
 ensures !clean(s,rs,m).files.contains(d)
{}
pub proof fn package_neighbors() {
 reveal_with_fuel(claimed,5);reveal_with_fuel(module_children,6);
 assert(!claimed(seq![],root(),extra()));
 assert(claimed(refs(),root(),extra()));
 assert(claimed(refs(),root(),inner()));
 assert(!claimed(refs(),root(),own()));
 let a=clean(world(),refs(),1);
 assert(!a.files.contains(own()));assert(a.files.contains(extra()));assert(a.files.contains(inner()));
 assert(!a.modules.contains(1));assert(a.modules.contains(2));
 assert(children(world().files,refs(),root(),false).contains(extra()));
 // The cache also excludes a separately declared child of the same module.
 assert(protects(Ref{name:extra(),module:1},root(),extra()));
 let same=seq![Ref{name:root(),module:1},Ref{name:extra(),module:1}];
 assert(!children(world().files,same,root(),true).contains(extra()));
 assert(!clean(world(),same,1).files.contains(extra()));
}
// Step keys are (class, method, kind), not module. IRIS generated persistence
// supplies uniqueness and FK cascade; its two real storage classes are probed.
pub enum StepKind {Any,Primary}
pub struct Key {pub class:int,pub method:int,pub kind:StepKind}
pub struct Step {pub home:int,pub completed:bool,pub ok:bool}
pub struct Store {pub modules:Set<int>,pub steps:Map<Key,Step>}
pub open spec fn get_add(s:Store,k:Key,home:int)->Store
 recommends s.modules.contains(home)
{
 let prior=if s.steps.contains_key(k){s.steps[k]}else{Step{home,completed:false,ok:true}};
 Store{modules:s.modules,steps:s.steps.insert(k,Step{home:prior.home,completed:prior.completed,ok:prior.ok})}
}
pub open spec fn seed(s:Store,k:Key,home:int)->Store
 recommends s.modules.contains(home)
{
 let a=get_add(s,k,home);let row=a.steps[k];
 Store{modules:a.modules,steps:a.steps.insert(k,if !row.completed || !row.ok {Step{home:row.home,completed:true,ok:true}}else{row})}
}
pub open spec fn delete_module(s:Store,home:int,deleted:bool)->Store {
 if !deleted{s}else{Store{modules:s.modules.remove(home),steps:s.steps.restrict(s.steps.dom().filter(|k:Key|s.steps[k].home!=home))}}
}
pub open spec fn key(kind:StepKind)->Key {Key{class:7,method:8,kind}}
pub open spec fn seeded()->Store {
 seed(seed(Store{modules:set![1int,2],steps:Map::empty()},key(StepKind::Any),1),key(StepKind::Primary),1)
}
pub proof fn cascade_preserves_other_home(s:Store,k:Key,m:int)
 requires s.steps.contains_key(k),s.steps[k].home!=m
 ensures delete_module(s,m,true).steps.contains_key(k),delete_module(s,m,true).steps[k]==s.steps[k]
{}
pub open spec fn both_moved()->Store {seed(seed(seeded(),key(StepKind::Any),2),key(StepKind::Primary),2)}
pub proof fn cascade_neighbors() {
 let a=seeded();let b=delete_module(a,1,true);
 assert(a.steps.contains_key(key(StepKind::Any)));assert(a.steps[key(StepKind::Any)].completed);
 assert(a.steps.contains_key(key(StepKind::Primary)));
 assert(!b.steps.contains_key(key(StepKind::Any)));assert(!b.steps.contains_key(key(StepKind::Primary)));
 assert(b.modules.contains(2));assert(!b.modules.contains(1));
 assert(delete_module(a,1,false)==a);
 let moved=seed(a,key(StepKind::Any),2);
 assert(moved.steps[key(StepKind::Any)].home==2);assert(moved.steps[key(StepKind::Any)].completed);
 assert(delete_module(moved,1,true).steps.contains_key(key(StepKind::Any)));
 assert(!delete_module(moved,1,true).steps.contains_key(key(StepKind::Primary)));
 assert(!delete_module(delete_module(moved,1,true),2,true).steps.contains_key(key(StepKind::Any)));
 assert(delete_module(both_moved(),1,true).steps.contains_key(key(StepKind::Primary)));
 assert(!delete_module(delete_module(both_moved(),1,true),2,true).steps.contains_key(key(StepKind::Primary)));
}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
}
fn main() {}
