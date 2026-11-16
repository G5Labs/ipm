// IPM 571e04a1: qualified filesystem supplier -> installed module -> saved history -> readers.
// Parsed nonempty qualified exact name/version requests, normalized roots, complete cache entries and successful
// load/compile/save providers form the scoped domain. Root A/B/C are distinct normalized paths;
// root_text/details use the actual scratch literals, not a raw-path parser.
// Only successful install/local-load receipts are projected; refused-attempt history is unassessed.
// Archive paths are UNASSESSED/UNPURSUED by user direction and have no representation here.
#[path="../../../verus/history.rs"]
pub mod inherited_history;
use vstd::prelude::*;
use inherited_history::{Row,Action,set_source,finalize,source_shown};

verus! {
pub type Text = Seq<u8>;
#[derive(PartialEq,Eq)]
pub enum Root { A, B, C }
pub struct Repo { pub root:Root, pub enabled:bool }
pub struct Package { pub name:Text, pub version:Text, pub payload:int }
pub struct Request { pub repository:Text, pub name:Text, pub version:Text }
pub struct Qualified { pub server:Text, pub root:Root, pub package:Package }
pub struct Installed { pub name:Text, pub version:Text, pub supplier:Text, pub root:Root }
pub struct World {
 pub repos:Map<Text,Repo>, pub roots:Map<Root,Package>, pub module:Option<Installed>,
 pub code:Option<int>, pub records:Map<int,Row>, pub next_id:int,
}
pub struct Result { pub ok:bool, pub world:World }
pub struct Supplier { pub name:Text, pub moniker:Text, pub details:Text }
pub enum SourceView { NotApplicable, NotSet, Supplier(Supplier) }

pub open spec fn alpha()->Text {seq![114u8,101,112,111,97,108,112,104,97]}
pub open spec fn beta()->Text {seq![114u8,101,112,111,98,101,116,97]}
pub open spec fn absent_repo()->Text {seq![109u8,105,115,115,105,110,103]}
pub open spec fn package_name()->Text {seq![105u8,116,101,114,49,54,45,115,117,112,112,108,105,101,114]}
pub open spec fn version()->Text {seq![49u8,46,48,46,48]}
pub open spec fn filesystem()->Text {seq![102u8,105,108,101,115,121,115,116,101,109]}
pub open spec fn root_text(root:Root)->Text {
 seq![47u8,116,109,112,47,105,116,101,114,49,54,47,114,101,112,111,115,47]
 + match root {Root::A=>seq![97u8,47],Root::B=>seq![98u8,47],Root::C=>seq![99u8,47]}
}
// Filesystem Definition.Details: Root + repeat('*/',Depth) + 'module.xml'.
// This domain has one module folder beneath each root and Depth=1; the native producer qualifies it.
pub open spec fn details(root:Root)->Text {root_text(root)+seq![42u8]}
pub open spec fn fixture(payload:int)->Package {Package{name:package_name(),version:version(),payload}}
pub open spec fn initial()->World {
 World{repos:map![alpha()=>Repo{root:Root::A,enabled:true},beta()=>Repo{root:Root::B,enabled:true}],
 roots:map![Root::A=>fixture(1),Root::B=>fixture(2),Root::C=>fixture(3)],
 module:None,code:None,records:Map::empty(),next_id:0}
}
pub open spec fn request(repository:Text)->Request {Request{repository,name:package_name(),version:version()}}

// Registry qualification restricts the repository query by Name. The single exact version
// below has no cross-repository sort/tie decision; filesystem cache parsing is an explicit port.
pub open spec fn select(w:World,r:Request)->Option<Qualified> {
 if !w.repos.contains_key(r.repository) {None}
 else {let repo=w.repos[r.repository];
  if !repo.enabled || !w.roots.contains_key(repo.root) {None}
  else {let p=w.roots[repo.root];
   if p.name!=r.name || p.version!=r.version {None}
   else {Some(Qualified{server:r.repository,root:repo.root,package:p})}
  }
 }
}
pub open spec fn receipt(id:int,action:Action,p:Package)->Row {
 Row{id,namespace:1,action,package:p.name,version:p.version,source:seq![],moniker:seq![],details:seq![],
 ended:false,status:0,committed:false,finalized:false}
}
// HistoryTemp.PersistToTwin copies these fields and sets Finalized; it does not look up a repo.
pub open spec fn persist(r:Row)->Row {Row{finalized:true,..r}}
pub open spec fn qualified_receipt(id:int,q:Qualified)->Row {
 persist(finalize(set_source(receipt(id,Action::Install,q.package),q.server,filesystem(),details(q.root)),1))
}
pub open spec fn apply_install(w:World,q:Qualified)->World {
 World{module:Some(Installed{name:q.package.name,version:q.package.version,supplier:q.server,root:q.root}),
 code:Some(q.package.payload),records:w.records.insert(w.next_id,qualified_receipt(w.next_id,q)),
 next_id:w.next_id+1,..w}
}
pub open spec fn install(w:World,r:Request)->Result {
 match select(w,r) {None=>Result{ok:false,world:w},Some(q)=>Result{ok:true,world:apply_install(w,q)}}
}
pub open spec fn delete_repo(w:World,name:Text)->World {World{repos:w.repos.remove(name),..w}}
pub open spec fn configure(w:World,name:Text,root:Root,enabled:bool)->World {
 World{repos:w.repos.insert(name,Repo{root,enabled}),..w}
}
// Successful ordinary local load uses the empty pRepository argument. Other load guards and
// failure effects are not generalized here. This also projects its successful load receipt.
pub open spec fn local_load(w:World,root:Root)->Result {
 if !w.roots.contains_key(root) {Result{ok:false,world:w}}
 else {let p=w.roots[root];
  Result{ok:true,world:World{
   module:Some(Installed{name:p.name,version:p.version,supplier:seq![],root}),code:Some(p.payload),
   records:w.records.insert(w.next_id,persist(finalize(receipt(w.next_id,Action::Load,p),1))),
   next_id:w.next_id+1,..w}}
 }
}

pub open spec fn supplier(w:World)->Text {match w.module{Some(m)=>m.supplier,None=>seq![]}}
pub open spec fn installed_root(w:World)->Option<Root> {match w.module{Some(m)=>Some(m.root),None=>None}}
pub open spec fn installed_version(w:World)->Text {match w.module{Some(m)=>m.version,None=>seq![]}}
pub open spec fn payload(w:World)->int {match w.code{Some(v)=>v,None=>0}}
// ModuleSummary.AddField skips an empty value; GetLines reads the saved module's Repository.
pub open spec fn show_repository(w:World)->Option<Text> {
 match w.module{Some(m)=>if m.supplier.len()>0{Some(m.supplier)}else{None},None=>None}
}
// Main.GetListModules uses the saved Repository column, including after its definition is gone.
pub open spec fn listed_for_repository(w:World,name:Text)->bool {
 match w.module{Some(m)=>m.supplier==name,None=>false}
}
pub open spec fn history_row(w:World,id:int)->Option<Row> {
 if w.records.contains_key(id){Some(w.records[id])}else{None}
}
// Both readers use stored row fields. SourceToString gates EVERY action on Install,
// including detail=true; no late lookup of w.repos is part of either reader.
pub open spec fn source_view(r:Row)->SourceView {
 if !source_shown(r){SourceView::NotApplicable}
 else if r.source.len()==0{SourceView::NotSet}
 else {SourceView::Supplier(Supplier{name:r.source,moniker:r.moniker,details:r.details})}
}
pub open spec fn history_detail(w:World,id:int)->Option<SourceView> {
 match history_row(w,id){Some(r)=>Some(source_view(r)),None=>None}
}
pub open spec fn history_name(w:World,id:int)->Option<Text> {
 match history_detail(w,id){Some(SourceView::Supplier(s))=>Some(s.name),_=>None}
}
pub open spec fn history_has_supplier(w:World,id:int,name:Text,root:Root)->bool {
 history_detail(w,id)==Some(SourceView::Supplier(Supplier{name,moniker:filesystem(),details:details(root)}))
}
pub open spec fn after_a()->World {install(initial(),request(alpha())).world}
pub open spec fn after_alpha_delete()->World {delete_repo(after_a(),alpha())}
pub open spec fn after_alpha_recreate()->World {configure(after_alpha_delete(),alpha(),Root::C,true)}
pub open spec fn after_c()->World {install(after_alpha_recreate(),request(alpha())).world}
pub open spec fn after_beta()->World {install(after_c(),request(beta())).world}
pub open spec fn after_local()->World {local_load(after_beta(),Root::A).world}
pub open spec fn missing()->Result {install(initial(),request(absent_repo()))}
pub open spec fn disabled()->Result {install(configure(initial(),alpha(),Root::A,false),request(alpha()))}
pub open spec fn missing_after_a()->Result {install(after_alpha_delete(),request(alpha()))}
pub open spec fn b(v:bool)->int {if v{1}else{0}}

pub proof fn selection_binds_supplier(w:World,r:Request)
 ensures match select(w,r) {Some(q)=>w.repos.contains_key(r.repository) &&
  w.repos[r.repository].enabled && q.server==r.repository && q.root==w.repos[r.repository].root &&
  w.roots.contains_key(q.root) && q.package==w.roots[q.root] && q.package.name==r.name && q.package.version==r.version,
  None=>true}
{}
pub proof fn missing_preserves_installed_state(w:World,r:Request)
 requires !w.repos.contains_key(r.repository)
 ensures !install(w,r).ok,install(w,r).world.module==w.module,install(w,r).world.code==w.code,
  install(w,r).world.records==w.records
{}
pub proof fn config_changes_preserve_receipts(w:World,name:Text,root:Root,enabled:bool)
 ensures delete_repo(w,name).records==w.records,configure(w,name,root,enabled).records==w.records,
 delete_repo(w,name).module==w.module,configure(w,name,root,enabled).module==w.module,
 delete_repo(w,name).code==w.code,configure(w,name,root,enabled).code==w.code
{}
pub proof fn successful_install_binds_receipt(w:World,r:Request)
 requires r.repository.len()>0,select(w,r).is_some()
 ensures install(w,r).ok,
  supplier(install(w,r).world)==r.repository,
  installed_root(install(w,r).world)==Some(select(w,r).unwrap().root),
  payload(install(w,r).world)==select(w,r).unwrap().package.payload,
  history_has_supplier(install(w,r).world,w.next_id,r.repository,select(w,r).unwrap().root),
  install(w,r).world.records[w.next_id].ended,install(w,r).world.records[w.next_id].finalized,
  install(w,r).world.records[w.next_id].status==1,
  install(w,r).world.records[w.next_id].committed
{
 inherited_history::source_and_identity_are_preserved(receipt(w.next_id,Action::Install,select(w,r).unwrap().package),
  r.repository,filesystem(),details(select(w,r).unwrap().root));
 reveal(history_has_supplier);reveal(history_detail);reveal(history_row);
 reveal(source_view);reveal(source_shown);
}
pub proof fn ordinary_neighbors()
 ensures install(initial(),request(alpha())).ok,install(initial(),request(beta())).ok,
 payload(after_a())==1,supplier(after_a())==alpha(),installed_root(after_a())==Some(Root::A),
 show_repository(after_a())==Some(alpha()),listed_for_repository(after_a(),alpha()),!listed_for_repository(after_a(),beta()),
 history_has_supplier(after_a(),0,alpha(),Root::A),
 !after_alpha_delete().repos.contains_key(alpha()),show_repository(after_alpha_delete())==Some(alpha()),
 listed_for_repository(after_alpha_delete(),alpha()),
 history_has_supplier(after_alpha_delete(),0,alpha(),Root::A),
 after_alpha_recreate().repos[alpha()].root==Root::C,payload(after_alpha_recreate())==1,
 history_has_supplier(after_alpha_recreate(),0,alpha(),Root::A),
 payload(after_c())==3,supplier(after_c())==alpha(),installed_root(after_c())==Some(Root::C),
 history_has_supplier(after_c(),0,alpha(),Root::A),history_has_supplier(after_c(),1,alpha(),Root::C),
 payload(after_beta())==2,supplier(after_beta())==beta(),installed_root(after_beta())==Some(Root::B),
 show_repository(after_beta())==Some(beta()),listed_for_repository(after_beta(),beta()),!listed_for_repository(after_beta(),alpha()),
 history_has_supplier(after_beta(),0,alpha(),Root::A),history_has_supplier(after_beta(),1,alpha(),Root::C),history_has_supplier(after_beta(),2,beta(),Root::B),
 installed_version(after_a())==version(),installed_version(after_c())==version(),installed_version(after_beta())==version()
{
 assert(install(initial(),request(alpha())).ok) by (compute);
 assert(install(initial(),request(beta())).ok) by (compute);
}
pub proof fn local_and_refusal_neighbors()
 ensures local_load(after_beta(),Root::A).ok,payload(after_local())==1,supplier(after_local())==seq![],
 installed_root(after_local())==Some(Root::A),show_repository(after_local())==None,
 !listed_for_repository(after_local(),alpha()),!listed_for_repository(after_local(),beta()),
 history_has_supplier(after_local(),1,alpha(),Root::C),history_has_supplier(after_local(),2,beta(),Root::B),
 history_detail(after_local(),3)==Some(SourceView::NotApplicable),
 !missing().ok,missing().world.module==None,payload(missing().world)==0,
 !disabled().ok,disabled().world.module==None,payload(disabled().world)==0,
 !missing_after_a().ok,payload(missing_after_a().world)==1,supplier(missing_after_a().world)==alpha()
{}
pub proof fn reader_neighbors()
 ensures after_a().records[0].action==Action::Install,after_a().records[0].source==alpha(),
 after_a().records[0].moniker==filesystem(),after_a().records[0].details==details(Root::A),
 after_a().records[0].ended,after_a().records[0].finalized,after_a().records[0].status==1,after_a().records[0].committed,
 after_c().records[1].action==Action::Install,after_c().records[1].source==alpha(),
 after_c().records[1].moniker==filesystem(),after_c().records[1].details==details(Root::C),
 after_c().records[1].ended,after_c().records[1].finalized,after_c().records[1].status==1,after_c().records[1].committed,
 after_beta().records[2].action==Action::Install,after_beta().records[2].source==beta(),
 after_beta().records[2].moniker==filesystem(),after_beta().records[2].details==details(Root::B),
 after_beta().records[2].ended,after_beta().records[2].finalized,after_beta().records[2].status==1,after_beta().records[2].committed,
 after_local().records[3].action==Action::Load,after_local().records[3].source==seq![],
 after_local().records[3].moniker==seq![],after_local().records[3].details==seq![],
 history_name(after_a(),0)==Some(alpha()),history_name(after_beta(),2)==Some(beta()),
 history_name(after_local(),3)==None
{}
pub proof fn details_literal_neighbors()
 ensures
 details(Root::A)==seq![47u8,116,109,112,47,105,116,101,114,49,54,47,114,101,112,111,115,47,97,47,42,47,109,111,100,117,108,101,46,120,109,108],
 details(Root::B)==seq![47u8,116,109,112,47,105,116,101,114,49,54,47,114,101,112,111,115,47,98,47,42,47,109,111,100,117,108,101,46,120,109,108],
 details(Root::C)==seq![47u8,116,109,112,47,105,116,101,114,49,54,47,114,101,112,111,115,47,99,47,42,47,109,111,100,117,108,101,46,120,109,108]
{
 assert(details(Root::A)==seq![47u8,116,109,112,47,105,116,101,114,49,54,47,114,101,112,111,115,47,97,47,42,47,109,111,100,117,108,101,46,120,109,108]) by (compute);
 assert(details(Root::B)==seq![47u8,116,109,112,47,105,116,101,114,49,54,47,114,101,112,111,115,47,98,47,42,47,109,111,100,117,108,101,46,120,109,108]) by (compute);
 assert(details(Root::C)==seq![47u8,116,109,112,47,105,116,101,114,49,54,47,114,101,112,111,115,47,99,47,42,47,109,111,100,117,108,101,46,120,109,108]) by (compute);
}
}
fn main() {}
