//! Resource rules, source-derived precedence and error boundaries. No ObjectScript implementation proof.
use vstd::prelude::*;
verus! {
pub type Text=Seq<int>;
#[derive(PartialEq, Eq)]
pub enum Tri { Unset, Off, On }
pub open spec fn selected(t:Tri,default:bool) -> bool {
    match t {Tri::Unset=>default,Tri::Off=>false,Tri::On=>true}
}
pub open spec fn deploy(class_type:bool,class_exists:bool,parameter:Option<int>,resource:Tri,default:bool) -> bool {
    if class_type && class_exists && parameter.is_some() {parameter.unwrap()==1}
    else {selected(resource,default)}
}
pub struct LoadProject {pub from_project:bool,pub marked_deployed:bool,pub provider_called:bool,pub marker:bool,pub ok:bool}
pub open spec fn load_project(deploy:bool,deployable_type:bool,path_name:bool,generated:bool,preload:bool,
    project_exists:bool,already_marked:bool,installed_marker:bool,save_ok:bool,install_ok:bool) -> LoadProject {
    let eligible=deploy && (deployable_type || path_name) && !generated && project_exists;
    let can_install=eligible && (already_marked || save_ok);
    LoadProject{from_project:eligible,marked_deployed:already_marked || eligible,
        provider_called:can_install && !installed_marker,
        marker:installed_marker || (can_install && install_ok),
        ok:!eligible || (can_install && (installed_marker || install_ok))}
}
pub open spec fn resource_save_allowed(new_object:bool,unique:Text,existing:Set<Text>) -> bool {
    !new_object || unique.len()==0 || !existing.contains(unique)
}
pub open spec fn project_database(before:bool,eligible:bool,already_marked:bool,save_ok:bool) -> bool {
    if eligible && !already_marked && save_ok {true} else {before}
}
pub proof fn resource_identity_and_database_reader() {
    let name=seq![65int];let store=Set::empty().insert(name);
    assert(resource_save_allowed(false,name,store));
    assert(!resource_save_allowed(true,name,store));
    assert(resource_save_allowed(true,seq![],store));
    let duplicate=resource_save_allowed(true,name,store);
    let reused=resource_save_allowed(false,name,store);
    assert(!project_database(false,true,false,duplicate));
    assert(project_database(false,true,false,reused));
    assert(load_project(true,true,false,false,false,true,false,false,duplicate,false).marked_deployed);
    assert(!load_project(true,true,false,false,false,true,false,false,duplicate,false).provider_called);
}
pub struct ExportResult {pub ok:bool,pub handled:bool,pub source_called:bool}
// Deployed-item error is overwritten by OK in Document.OnExportItem before the fallback.
pub open spec fn export_item(deployed_ok:bool,deployed_handled:bool,source_format:bool,source_ok:bool) -> ExportResult {
    if deployed_handled {ExportResult{ok:true,handled:true,source_called:false}}
    else {ExportResult{ok:!source_format || source_ok,handled:source_format,source_called:source_format}}
}
// Proposed status propagation, not a product edit.
pub open spec fn repaired_export_item(deployed_ok:bool,deployed_handled:bool,source_format:bool,source_ok:bool) -> ExportResult {
    if !deployed_ok {ExportResult{ok:false,handled:deployed_handled,source_called:false}}
    else {export_item(deployed_ok,deployed_handled,source_format,source_ok)}
}
pub proof fn export_repair_preserves_success(h:bool,f:bool,s:bool)
    ensures repaired_export_item(true,h,f,s)==export_item(true,h,f,s)
{}
pub proof fn export_repair_stops_failed_helper() {
    assert(!repaired_export_item(false,false,true,true).ok);
    assert(!repaired_export_item(false,false,true,true).source_called);
}
pub open spec fn final_export(item:ExportResult,project_check_ok:bool) -> bool {item.ok && project_check_ok}
pub open spec fn check_project(items:Seq<int>,exists:Set<int>) -> Seq<int>
    decreases items.len()
{
    if items.len()==0 {items} else {
        (if exists.contains(items[0]) {seq![items[0]]} else {seq![]}) + check_project(items.drop_first(),exists)
    }
}
pub proof fn project_check_prunes_missing() {
    reveal_with_fuel(check_project,4);
    assert(check_project(seq![1int,2],Set::empty().insert(2)) =~= seq![2int]);
    assert(check_project(seq![1int],Set::empty()).len()==0);
}
pub struct Resource {pub id:int,pub document:bool,pub path_name:bool,pub stem:Text}
pub open spec fn before(a:Text,b:Text) -> bool
    decreases a.len()+b.len()
{
    if a.len()==0 {b.len()>0} else if b.len()==0 {false}
    else if a[0]!=b[0] {a[0]<b[0]} else {before(a.drop_first(),b.drop_first())}
}
pub open spec fn ordered_before(a:Resource,b:Resource) -> bool {
    let ad=a.document && !a.path_name;let bd=b.document && !b.path_name;
    if ad!=bd {ad} else if ad {before(a.stem,b.stem)} else {false}
}
pub struct DefaultRule {pub class_name:Text,pub conditions:Seq<(int,int)>,pub values:Map<int,int>}
pub open spec fn satisfies(props:Map<int,int>,conditions:Seq<(int,int)>) -> bool {
    forall|i:int| 0<=i<conditions.len() ==> props.contains_key(conditions[i].0) && props[conditions[i].0]==conditions[i].1
}
pub open spec fn processor_class_matches(target:Text,declared:Text) -> bool {
    target==declared || target==seq![37int,73,80,77,46,82,101,115,111,117,114,99,101,80,114,111,99,101,115,115,111,114,46]+declared
}
pub open spec fn defaults_step(props:Map<int,int>,explicit:Set<int>,rule:DefaultRule,class_name:Text) -> Map<int,int> {
    if processor_class_matches(class_name,rule.class_name) && satisfies(props,rule.conditions) {
        Map::new(props.dom().union(rule.values.dom().difference(explicit)),
            |k:int| if rule.values.contains_key(k) && !explicit.contains(k) {rule.values[k]} else {props[k]})
    } else {props}
}
pub open spec fn apply_defaults(props:Map<int,int>,explicit:Set<int>,rules:Seq<DefaultRule>,class_name:Text) -> Map<int,int>
    decreases rules.len()
{
    if rules.len()==0 {props} else {apply_defaults(defaults_step(props,explicit,rules[0],class_name),explicit,rules.drop_first(),class_name)}
}
pub struct Requirements {pub health:Tri,pub interoperability:int,pub sys_namespace:bool}
pub struct Environment {pub health:bool,pub interoperability:bool,pub namespace:Text}
#[derive(PartialEq, Eq)]
pub enum Failure { Interoperability, NeedHealth, ExcludeHealth, SYSNamespace }
pub open spec fn requirement_failures(r:Requirements,e:Environment) -> Seq<Failure> {
    let interop=(r.interoperability==1 || r.health==Tri::On) && !e.interoperability;
    let health=if r.health==Tri::On && !e.health {seq![Failure::NeedHealth]}
        else if r.health==Tri::Off && e.health {seq![Failure::ExcludeHealth]} else {seq![]};
    let sys=r.sys_namespace && e.namespace!=seq![37int,83,89,83];
    (if interop {seq![Failure::Interoperability]} else {seq![]}) + health + (if sys {seq![Failure::SYSNamespace]} else {seq![]})
}
pub proof fn deployment_precedence() {
    assert(!deploy(true,true,Some(0),Tri::On,true));
    assert(deploy(true,true,Some(1),Tri::Off,false));
    assert(!deploy(true,true,Some(2),Tri::On,true));
    assert(deploy(false,true,Some(0),Tri::On,false));
    assert(deploy(true,false,Some(0),Tri::On,false));
    assert(deploy(true,true,None,Tri::Unset,true));
    assert(!deploy(true,true,None,Tri::Unset,false));
}
pub proof fn load_project_boundaries() {
    let a=load_project(true,true,false,false,false,true,false,false,true,false);
    assert(a.marked_deployed && !a.ok && !a.marker && a.provider_called);
    let b=load_project(true,true,false,false,false,true,false,false,true,true);
    assert(b.ok && b.marker && b.provider_called);
    assert(!load_project(true,true,false,false,false,true,true,true,true,false).provider_called);
    assert(!load_project(true,false,false,false,false,true,false,false,true,true).from_project);
    assert(load_project(true,false,true,false,false,true,false,false,true,true).from_project);
    assert(!load_project(true,true,false,true,false,true,false,false,true,true).from_project);
    assert(!load_project(true,true,false,false,true,true,false,false,true,true).from_project);
    assert(!load_project(true,true,false,false,false,false,false,false,true,true).from_project);
}
pub proof fn overwritten_error_and_final_reader() {
    let a=export_item(false,false,true,true);
    assert(a.ok && a.handled && a.source_called);
    assert(!final_export(a,false));
    assert(final_export(a,true));
    assert(!export_item(true,false,true,false).ok);
    assert(!export_item(true,true,true,false).source_called);
    assert(export_item(false,true,true,false).ok);
}
pub proof fn resource_order() {
    reveal_with_fuel(before,5);
    let parent=Resource{id:1,document:true,path_name:false,stem:seq![65int]};
    let child=Resource{id:2,stem:seq![65int,46,66],..parent};
    let path=Resource{id:3,path_name:true,stem:seq![48int],..parent};
    assert(ordered_before(parent,child));
    assert(ordered_before(child,path));
    assert(!ordered_before(path,parent));
}
pub proof fn explicit_default_is_protected(props:Map<int,int>,explicit:Set<int>,r:DefaultRule,full:Text,k:int)
    requires props.contains_key(k),explicit.contains(k)
    ensures defaults_step(props,explicit,r,full)[k]==props[k]
{}
pub proof fn chained_defaults_and_last_writer() {
    reveal_with_fuel(apply_defaults,4);
    let c=seq![65int];let initial=Map::empty().insert(1,0).insert(2,0);
    let one=DefaultRule{class_name:c,conditions:seq![],values:Map::empty().insert(1,1)};
    let two=DefaultRule{class_name:c,conditions:seq![(1int,1int)],values:Map::empty().insert(1,2).insert(2,3)};
    assert(satisfies(initial,seq![]));
    let step=defaults_step(initial,Set::empty(),one,c);
    assert(step[1]==1);
    assert(satisfies(step,two.conditions));
    assert(apply_defaults(initial,Set::empty(),seq![one,two],c)[1]==2);
    assert(apply_defaults(initial,Set::empty(),seq![one,two],c)[2]==3);
    assert(apply_defaults(initial,Set::empty().insert(1),seq![one,two],c)[1]==0);
    let protected=defaults_step(initial,Set::empty().insert(1),one,c);
    assert(protected[1]==0);
    assert(two.conditions[0].0==1 && two.conditions[0].1==1);
    assert(!satisfies(protected,two.conditions));
    assert(apply_defaults(initial,Set::empty().insert(1),seq![one,two],c)[2]==0);
}
pub proof fn requirement_neighbors() {
    let user=Environment{health:true,interoperability:true,namespace:seq![85int,83,69,82]};
    let unset=Requirements{health:Tri::Unset,interoperability:0,sys_namespace:false};
    assert(requirement_failures(unset,user).len()==0);
    assert(requirement_failures(Requirements{health:Tri::Off,..unset},user)==seq![Failure::ExcludeHealth]);
    let disabled=Environment{health:true,interoperability:false,..user};
    assert(requirement_failures(Requirements{health:Tri::On,interoperability:2,..unset},disabled)==seq![Failure::Interoperability]);
    assert(requirement_failures(Requirements{interoperability:2,..unset},user).len()==0);
    assert(requirement_failures(Requirements{sys_namespace:true,..unset},user)==seq![Failure::SYSNamespace]);
    let sys=Environment{namespace:seq![37int,83,89,83],..user};
    assert(requirement_failures(Requirements{sys_namespace:true,..unset},sys).len()==0);
    let plain=Environment{health:false,interoperability:false,..user};
    assert(requirement_failures(Requirements{health:Tri::On,sys_namespace:true,..unset},plain)==seq![Failure::Interoperability,Failure::NeedHealth,Failure::SYSNamespace]);
}
}
fn main() {}
