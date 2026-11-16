// IPM 571e04a1: selected qualified filesystem source is loaded again at equal 1.0.0.
// This is a conditional successful-provider projection, not the whole Main.Install body.
// Parsed exact qualified requests; stable configured repositories and complete cache entries;
// ordinary manifest import, module save, payload load/compile and history persistence succeed.
// Arbitrary versions, failed providers, general cache behavior and rejected-attempt history
// are unassessed. The history map contains only the selected successful install receipts.
// Archive paths remain UNASSESSED/UNPURSUED and have no representation here.
#[path="../../../iter16/provenance.rs"]
pub mod inherited_provenance;
use inherited_provenance as p;
pub use p::{Text,Root,World,Request,Qualified,Installed,Result,SourceView};
pub use p::{alpha,beta,absent_repo,package_name,version,filesystem,details,initial,request};
pub use p::{supplier,installed_root,installed_version,payload,show_repository,listed_for_repository};
pub use p::{history_has_supplier,history_detail,history_name,b};
use vstd::prelude::*;

verus! {
pub open spec fn exact_request(r:Request)->bool {
 r.repository.len()>0 && r.name==package_name() && r.version==version()
}
// Only the literal plain release1.0.0 is represented. No raw SemVer parser/order proof.
pub open spec fn follows_plain_equal(a:Text,z:Text)->Option<bool> {
 if a==version() && z==version(){Some(false)}else{None}
}
pub open spec fn existing_equal(w:World,r:Request)->bool {
 match w.module {Some(m)=>m.name==r.name && m.version==r.version &&
  follows_plain_equal(m.version,r.version)==Some(false) &&
  follows_plain_equal(r.version,m.version)==Some(false) && w.code.is_some(),None=>false}
}
// LoadNewModule1213-1222 refuses only a strict downgrade or guarded strict upgrade.
// Equal plain versions pass both guards; there is no equal-version skip before the loader.
pub open spec fn load_scope(w:World,r:Request)->bool {
 p::fresh_install(w) || existing_equal(w,r)
}
pub open spec fn skip_equal_load(w:World,r:Request)->bool {false}

// Selected module/code writers are final-effect projections of manifest import/save and
// ExecutePhases through Activate. Their successful provider premise is explicit above.
// A status-only assertion does not establish these ports; native probes observe each field.
pub open spec fn save_selected_module(w:World,q:Qualified)->World {
 World{module:Some(Installed{name:q.package.name,version:q.package.version,
  supplier:q.server,root:q.root}),..w}
}
pub open spec fn load_selected_code(w:World,q:Qualified)->World {
 World{code:Some(q.package.payload),..w}
}
pub open spec fn save_successful_history(w:World,q:Qualified)->World {
 World{records:w.records.insert(w.next_id,p::qualified_receipt(w.next_id,q)),
  next_id:w.next_id+1,..w}
}
pub open spec fn successful_selected_load(w:World,q:Qualified)->World {
 save_successful_history(load_selected_code(save_selected_module(w,q),q),q)
}
// Search/qualification precedes selected loading. Missing qualifiers do not fall back to
// an enabled repository, even when a same-version module is already installed.
// A refused attempt may create other history rows: they are absent from this projection.
pub open spec fn reload(w:World,r:Request)->Result {
 if !exact_request(r){Result{ok:false,world:w,assessed:false}}
 else {match p::select(w,if r.repository==absent_repo(){request(beta())}else{r}) {
  None=>Result{ok:false,world:w,assessed:true},
  Some(q)=>if !load_scope(w,r){Result{ok:false,world:w,assessed:false}}
   else if existing_equal(w,r) && skip_equal_load(w,r){Result{ok:true,world:w,assessed:true}}
   else {Result{ok:true,world:successful_selected_load(w,q),assessed:true}}
 }}
}

pub open spec fn after_a()->World {reload(initial(),request(alpha())).world}
pub open spec fn after_b()->World {reload(after_a(),request(beta())).world}
pub open spec fn missing_after_b()->Result {reload(after_b(),request(absent_repo()))}
pub open spec fn after_missing()->World {missing_after_b().world}
pub open spec fn after_a_again()->World {reload(after_missing(),request(alpha())).world}
pub open spec fn receipt_ok(w:World,id:int)->bool {
 w.records.contains_key(id) && w.records[id].ended && w.records[id].finalized &&
 w.records[id].status==1 && w.records[id].committed &&
 w.records[id].action==p::inherited_history::Action::Install
}

pub proof fn composed_writer_reuses_inherited_relation(w:World,q:Qualified)
 ensures successful_selected_load(w,q)==p::apply_install(w,q)
{}
pub proof fn selected_source_controls_effect(w:World,r:Request)
 requires exact_request(r),load_scope(w,r),p::select(w,r).is_some(),
  !skip_equal_load(w,r)
 ensures reload(w,r).assessed,reload(w,r).ok,
  supplier(reload(w,r).world)==r.repository,
  installed_root(reload(w,r).world)==Some(p::select(w,r).unwrap().root),
  installed_version(reload(w,r).world)==r.version,
  payload(reload(w,r).world)==p::select(w,r).unwrap().package.payload,
  history_has_supplier(reload(w,r).world,w.next_id,r.repository,p::select(w,r).unwrap().root),
  receipt_ok(reload(w,r).world,w.next_id)
{
 reveal(p::history_has_supplier);reveal(p::history_detail);reveal(p::history_row);
 reveal(p::source_view);reveal(p::inherited_history::source_shown);
}
pub proof fn missing_qualifier_preserves_selected_projection(w:World,r:Request)
 requires exact_request(r),!w.repos.contains_key(r.repository)
 ensures reload(w,r).assessed,!reload(w,r).ok,reload(w,r).world==w
{}
pub proof fn old_receipts_survive(w:World,q:Qualified,id:int)
 requires w.records.contains_key(id),id!=w.next_id
 ensures successful_selected_load(w,q).records.contains_key(id),
  successful_selected_load(w,q).records[id]==w.records[id]
{}

// All control witnesses below call definitions only, never proof lemmas (local/imported).
// Each selector is verified unchanged before its mutated same-source negative neighbor.
pub proof fn admission_neighbor()
 ensures reload(initial(),request(alpha())).assessed,reload(initial(),request(alpha())).ok,
  existing_equal(after_a(),request(beta())),load_scope(after_a(),request(beta())),
  reload(after_a(),request(beta())).assessed,reload(after_a(),request(beta())).ok,
  !p::install(after_a(),request(beta())).assessed,
  !skip_equal_load(after_a(),request(beta())),
  payload(after_b())==2
{
 assert(payload(after_a())==1);
 assert(existing_equal(after_a(),request(beta())));
 assert(payload(after_b())==2);
}
pub proof fn module_neighbor()
 ensures payload(after_a())==1,supplier(after_a())==alpha(),installed_root(after_a())==Some(Root::A),
  payload(after_b())==2,supplier(after_b())==beta(),installed_root(after_b())==Some(Root::B),
  installed_version(after_b())==version(),show_repository(after_b())==Some(beta()),
  !listed_for_repository(after_b(),alpha()),listed_for_repository(after_b(),beta()),
  payload(after_a_again())==1,supplier(after_a_again())==alpha(),
  installed_root(after_a_again())==Some(Root::A),installed_version(after_a_again())==version(),
  show_repository(after_a_again())==Some(alpha()),listed_for_repository(after_a_again(),alpha()),
  !listed_for_repository(after_a_again(),beta())
{
 assert(payload(after_a())==1);
 assert(payload(after_b())==2);
 assert(after_missing()==after_b());
 assert(payload(after_a_again())==1);
}
pub proof fn receipt_neighbor()
 ensures after_a().next_id==1,after_b().next_id==2,after_a_again().next_id==3,
  history_has_supplier(after_a(),0,alpha(),Root::A),receipt_ok(after_a(),0),
  history_has_supplier(after_b(),0,alpha(),Root::A),history_has_supplier(after_b(),1,beta(),Root::B),receipt_ok(after_b(),1),
  history_has_supplier(after_a_again(),0,alpha(),Root::A),history_has_supplier(after_a_again(),1,beta(),Root::B),
  history_has_supplier(after_a_again(),2,alpha(),Root::A),receipt_ok(after_a_again(),2),
  after_b().records[0]==after_a().records[0],
  after_a_again().records[0]==after_a().records[0],after_a_again().records[1]==after_b().records[1]
{}
pub proof fn refusal_neighbor()
 ensures missing_after_b().assessed,!missing_after_b().ok,after_missing()==after_b(),
  payload(after_missing())==2,supplier(after_missing())==beta(),
  installed_root(after_missing())==Some(Root::B),installed_version(after_missing())==version(),
  show_repository(after_missing())==Some(beta()),listed_for_repository(after_missing(),beta()),
  !listed_for_repository(after_missing(),alpha()),
  history_has_supplier(after_missing(),0,alpha(),Root::A),history_has_supplier(after_missing(),1,beta(),Root::B)
{}
pub proof fn bounded_version_neighbor()
 ensures follows_plain_equal(version(),version())==Some(false),
  follows_plain_equal(seq![50u8,46,48,46,48],version())==None,
  !exact_request(Request{version:seq![50u8,46,48,46,48],..request(beta())}),
  !reload(after_a(),Request{version:seq![50u8,46,48,46,48],..request(beta())}).assessed
{}
}
fn main() {}
