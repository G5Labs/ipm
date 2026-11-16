// IPM571e04a1: Reinstall caller pins installed plain version then searches afresh.
// Ordinary one-module, no-dependency, nondev successful installs at current1.0.0;
// alpha offers1/2 and beta1 from stable valid filesystem sources. Default/dev
// no-repository failures and missing-name early refusal are separately represented.
// Generic parsers/ranges, snapshots/dependencies, successful dev/current2 loads,
// other errors, concurrency and general cache lifetimes are unassessed.
// Archive paths remain UNASSESSED/UNPURSUED with no investigation or representation.
#[path="../../../iter18/reloading.rs"]
pub mod inherited_reloading;
#[path="../../../verus/temp_repo.rs"]
pub mod inherited_search;
use inherited_reloading as r;
use r::inherited_provenance as p;
use vstd::prelude::*;
verus! {
pub type Text=Seq<u8>;
#[derive(PartialEq,Eq)]
pub enum Place {AlphaOne,AlphaTwo,BetaOne}
pub struct Entry {pub repository:Text,pub place:Place}
pub struct Choice {pub entry:Entry,pub package:p::Package,pub root:p::Root,pub age:nat}
pub struct Command {pub name:Text,pub dev:bool,pub snapshots:Option<bool>,pub original:Text}
#[derive(PartialEq,Eq)]
pub enum HistoryEffect {NoNewInstallRecord,SuccessfulInstallRecord,FailedInstallFieldsUnknown,Unassessed}
pub struct World {
 pub base:p::World,pub files:Map<Place,p::Package>,pub catalog:Seq<Entry>,pub created:Map<Text,nat>,
 pub directory:Option<Place>,pub commands:Map<int,Text>,pub install_records:nat,
}
pub struct Outcome {pub world:World,pub requested:Option<Text>,pub selected:Option<Text>,
 pub delegated:bool,pub update_snapshots:bool,pub history:HistoryEffect,pub ok:bool,pub assessed:bool}
pub open spec fn reinstall_command()->Text {seq![114u8,101,105,110,115,116,97,108,108,32,105,116,101,114,49,54,45,111,114,105,103,105,110]}
pub open spec fn beta_command()->Text {seq![105u8,110,115,116,97,108,108,32,98,101,116,97,47,105,116,101,114,49,54,45,111,114,105,103,105,110,32,49,46,48,46,48]}
pub open spec fn dev_command()->Text {seq![114u8,101,105,110,115,116,97,108,108,32,105,116,101,114,49,54,45,111,114,105,103,105,110,32,45,100,101,118]}
pub open spec fn missing_name()->Text {seq![105u8,116,101,114,50,50,45,109,105,115,115,105,110,103]}
pub open spec fn missing_command()->Text {seq![114u8,101,105,110,115,116,97,108,108,32,105,116,101,114,50,50,45,109,105,115,115,105,110,103]}
pub open spec fn new_subdir()->Text {seq![105u8,116,101,114,49,54,45,111,114,105,103,105,110,45,110,101,119,47]}
pub open spec fn alpha()->Text {p::alpha()}
pub open spec fn beta()->Text {p::beta()}
pub open spec fn package_name()->Text {p::package_name()}
pub open spec fn one()->Text {p::version()}
pub open spec fn two()->Text {seq![50u8,46,48,46,48]}
pub open spec fn version_rank(t:Text)->nat {if t==one(){1}else if t==two(){2}else{0}}
pub open spec fn root_of(place:Place)->p::Root {match place{Place::AlphaOne|Place::AlphaTwo=>p::Root::A,Place::BetaOne=>p::Root::B}}
pub open spec fn directory_text(place:Place)->Text {p::root_text(root_of(place))+
 match place{Place::AlphaTwo=>new_subdir(),_=>package_name()+seq![47u8]}}
pub open spec fn initial()->World {
 let b=p::initial();World{base:b,files:map![Place::AlphaOne=>b.roots[p::Root::A],Place::BetaOne=>b.roots[p::Root::B],
 Place::AlphaTwo=>p::Package{name:package_name(),version:two(),payload:4}],
 catalog:seq![Entry{repository:beta(),place:Place::BetaOne},Entry{repository:alpha(),place:Place::AlphaTwo},Entry{repository:alpha(),place:Place::AlphaOne}],
 created:map![alpha()=>0nat,beta()=>1nat],directory:None,commands:Map::empty(),install_records:0}
}
pub open spec fn command()->Command {Command{name:package_name(),dev:false,snapshots:None,original:reinstall_command()}}
pub open spec fn dev_args()->Command {Command{dev:true,original:dev_command(),..command()}}
pub open spec fn missing_args()->Command {Command{name:missing_name(),original:missing_command(),..command()}}
pub open spec fn update_snapshots(a:Command)->bool {true}
// Plain1/2 Version.ToString calibration only. No VersionString/build-label claim.
pub open spec fn installed_request(w:World,a:Command)->Option<Text> {
 match w.base.module{Some(m)=>if m.name==a.name && (m.version==one() || m.version==two()){Some(m.version)}else{None},None=>None}
}
pub open spec fn installed_name(w:World,a:Command)->bool {match w.base.module{Some(m)=>m.name==a.name,None=>false}}
pub open spec fn eligible(w:World,e:Entry,name:Text,v:Text)->bool {
 w.base.repos.contains_key(e.repository) && w.created.contains_key(e.repository) &&
 w.base.repos[e.repository].enabled && w.files.contains_key(e.place) &&
 w.base.repos[e.repository].root==root_of(e.place) &&
 w.files[e.place].name==name && w.files[e.place].version==v
}
pub open spec fn choice(w:World,e:Entry)->Choice {Choice{entry:e,package:w.files[e.place],root:root_of(e.place),age:w.created[e.repository]}}
pub open spec fn rank(c:Choice)->inherited_search::Candidate {inherited_search::Candidate{
 version:version_rank(c.package.version),repository_id:c.age,local:false,sort:0}}
pub open spec fn prefer(a:Choice,b:Choice)->Choice {if inherited_search::select(rank(a),rank(b))==rank(a){a}else{b}}
pub open spec fn search_from(w:World,name:Text,v:Text,i:nat,best:Option<Choice>)->Option<Choice>
 decreases if i<w.catalog.len(){(w.catalog.len()-i) as nat}else{0nat}
{
 if i>=w.catalog.len(){best}else{
  let e=w.catalog[i as int];let next=if eligible(w,e,name,v){let c=choice(w,e);match best{Some(old)=>Some(prefer(old,c)),None=>Some(c)}}else{best};
  search_from(w,name,v,i+1,next)
 }
}
pub open spec fn search(w:World,name:Text,v:Text)->Option<Choice> {search_from(w,name,v,0,None)}
pub open spec fn qualified(c:Choice)->p::Qualified {p::Qualified{server:c.entry.repository,root:c.root,package:c.package}}
pub open spec fn begin_install(w:World)->World {World{install_records:w.install_records+1,..w}}
pub open spec fn successful_selected(w:World,c:Choice,raw:Text)->World {
 World{base:r::successful_selected_load(w.base,qualified(c)),directory:Some(c.entry.place),
 commands:w.commands.insert(w.base.next_id,raw),..w}
}
pub open spec fn reinstall(w:World,a:Command)->Outcome {
 if !installed_name(w,a){Outcome{world:w,requested:None,selected:None,delegated:false,
  update_snapshots:update_snapshots(a),history:HistoryEffect::NoNewInstallRecord,ok:false,assessed:true}}
 else{match installed_request(w,a){None=>Outcome{world:w,requested:None,selected:None,delegated:true,
  update_snapshots:update_snapshots(a),history:HistoryEffect::Unassessed,ok:false,assessed:false},Some(v)=>{
  let entered=begin_install(w);
  match search(entered,a.name,v){
   None=>Outcome{world:entered,requested:Some(v),selected:None,delegated:true,
    update_snapshots:update_snapshots(a),history:HistoryEffect::FailedInstallFieldsUnknown,ok:false,assessed:true},
   Some(c)=>if a.dev || v!=one() || !r::load_scope(w.base,p::request(c.entry.repository)){
    Outcome{world:entered,requested:Some(v),selected:Some(c.entry.repository),delegated:true,
     update_snapshots:update_snapshots(a),history:HistoryEffect::Unassessed,ok:false,assessed:false}
   }else{Outcome{world:successful_selected(entered,c,a.original),requested:Some(v),selected:Some(c.entry.repository),delegated:true,
    update_snapshots:update_snapshots(a),history:HistoryEffect::SuccessfulInstallRecord,ok:true,assessed:true}}
  }
 }}}
}
// Applicable unchanged18 qualified-v1 setup/reset; stable beta source exists.
pub open spec fn qualified_beta(w:World)->World {World{base:r::reload(w.base,p::request(beta())).world,
 directory:Some(Place::BetaOne),commands:w.commands.insert(w.base.next_id,beta_command()),install_records:w.install_records+1,..w}}
pub open spec fn disable_alpha(w:World)->World {World{base:p::configure(w.base,alpha(),p::Root::A,false),..w}}
pub open spec fn delete_definition(w:World,n:Text)->World {World{base:p::delete_repo(w.base,n),created:w.created.remove(n),..w}}
pub open spec fn beta_initial()->World {qualified_beta(initial())}
pub open spec fn default_result()->Outcome {reinstall(beta_initial(),command())}
pub open spec fn reset_beta()->World {qualified_beta(default_result().world)}
pub open spec fn disabled()->World {disable_alpha(reset_beta())}
pub open spec fn beta_result()->Outcome {reinstall(disabled(),command())}
pub open spec fn deleted()->World {delete_definition(delete_definition(beta_result().world,alpha()),beta())}
pub open spec fn failed_default()->Outcome {reinstall(deleted(),command())}
pub open spec fn failed_dev()->Outcome {reinstall(failed_default().world,dev_args())}
pub open spec fn missing()->Outcome {reinstall(failed_dev().world,missing_args())}
pub open spec fn payload(w:World)->int {p::payload(w.base)}
pub open spec fn supplier(w:World)->Text {p::supplier(w.base)}
pub open spec fn current_version(w:World)->Text {p::installed_version(w.base)}
// The imported Module.root is a logical repository root. Read that stored field
// plus the selected subdirectory, rather than returning the sidecar path alone.
pub open spec fn module_directory(w:World)->Option<Text> {match (w.base.module,w.directory){
 (Some(m),Some(d))=>Some(p::root_text(m.root)+match d{Place::AlphaTwo=>new_subdir(),_=>package_name()+seq![47u8]}),_=>None}}
// Existence of the actual stored path in the controlled physical-files projection.
// This is not a general path parser or a complete arbitrary-filesystem inventory.
pub open spec fn root_exists(w:World)->bool {match module_directory(w){None=>false,Some(path)=>
 (w.files.contains_key(Place::AlphaOne) && path==directory_text(Place::AlphaOne)) ||
 (w.files.contains_key(Place::AlphaTwo) && path==directory_text(Place::AlphaTwo)) ||
 (w.files.contains_key(Place::BetaOne) && path==directory_text(Place::BetaOne))}}
pub open spec fn shown_supplier(w:World)->Option<Text> {p::show_repository(w.base)}
pub open spec fn listed_for(w:World,n:Text)->bool {p::listed_for_repository(w.base,n)}
pub open spec fn has_version(w:World,v:Text)->bool {search(w,package_name(),v).is_some()}
pub open spec fn history_supplier(w:World,id:int,n:Text,d:Place)->bool {p::history_has_supplier(w.base,id,n,root_of(d))}
pub open spec fn history_flags(w:World,id:int)->bool {w.base.records.contains_key(id) && r::receipt_ok(w.base,id)}
pub open spec fn history_version(w:World,id:int)->Text {if w.base.records.contains_key(id){w.base.records[id].version}else{seq![]}}
pub open spec fn history_command(w:World,id:int)->Text {if w.commands.contains_key(id){w.commands[id]}else{seq![]}}
pub open spec fn record_delta(before:World,after:World)->int {after.install_records-before.install_records}
pub open spec fn b(v:bool)->int {if v{1}else{0}}

// All control witnesses below call definitions only, no proof lemmas/broadcasts.
pub proof fn caller_neighbor()
 ensures installed_request(beta_initial(),command())==Some(one()),has_version(beta_initial(),two()),
  default_result().requested==Some(one()),default_result().delegated,default_result().assessed,default_result().ok,
  default_result().selected==Some(alpha()),current_version(default_result().world)==one(),payload(default_result().world)==1,
  installed_request(World{base:p::World{module:Some(p::Installed{name:package_name(),version:two(),supplier:alpha(),root:p::Root::A}),..beta_initial().base},..beta_initial()},command())==Some(two())
{reveal_with_fuel(search_from,4);assert(one()!=two()) by(compute);}
pub proof fn catalog_neighbor()
 ensures search(beta_initial(),package_name(),one()).unwrap().entry.repository==alpha(),
  search(disabled(),package_name(),one()).unwrap().entry.repository==beta(),
  disabled().base.repos.contains_key(alpha()),!disabled().base.repos[alpha()].enabled,
  has_version(beta_initial(),two()),!has_version(disabled(),two()),
  search(deleted(),package_name(),one())==None,search(deleted(),package_name(),two())==None,
  !deleted().base.repos.contains_key(alpha()),!deleted().base.repos.contains_key(beta())
{reveal_with_fuel(search_from,4);assert(one()!=two()) by(compute);}
pub proof fn reinstall_neighbor()
 ensures payload(beta_initial())==2,supplier(beta_initial())==beta(),current_version(beta_initial())==one(),
  supplier(default_result().world)==alpha(),payload(default_result().world)==1,
  module_directory(default_result().world)==Some(directory_text(Place::AlphaOne)),
  shown_supplier(default_result().world)==Some(alpha()),listed_for(default_result().world,alpha()),!listed_for(default_result().world,beta()),
  supplier(reset_beta())==beta(),payload(reset_beta())==2,
  beta_result().ok,beta_result().assessed,beta_result().selected==Some(beta()),payload(beta_result().world)==2,supplier(beta_result().world)==beta(),
  module_directory(beta_result().world)==Some(directory_text(Place::BetaOne)),current_version(beta_result().world)==one(),
  shown_supplier(beta_result().world)==Some(beta()),listed_for(beta_result().world,beta()),!listed_for(beta_result().world,alpha())
{reveal_with_fuel(search_from,4);assert(one()!=two()) by(compute);}
pub proof fn deletion_neighbor()
 ensures root_exists(deleted()),deleted().files==beta_result().world.files,deleted().base.records==beta_result().world.base.records,
  failed_default().assessed,!failed_default().ok,failed_default().delegated,
  failed_dev().assessed,!failed_dev().ok,failed_dev().delegated,
  payload(failed_default().world)==2,payload(failed_dev().world)==2,
  supplier(failed_default().world)==beta(),supplier(failed_dev().world)==beta(),
  current_version(failed_default().world)==one(),current_version(failed_dev().world)==one(),
  module_directory(failed_dev().world)==Some(directory_text(Place::BetaOne)),
  failed_default().world.base.module==deleted().base.module,failed_dev().world.base.code==deleted().base.code,
  shown_supplier(failed_dev().world)==Some(beta()),listed_for(failed_dev().world,beta()),!listed_for(failed_dev().world,alpha())
{reveal_with_fuel(search_from,4);assert(one()!=two()) by(compute);}
pub proof fn history_neighbor()
 ensures history_flags(beta_initial(),0),history_flags(default_result().world,1),history_flags(reset_beta(),2),history_flags(beta_result().world,3),
  history_supplier(deleted(),0,beta(),Place::BetaOne),history_supplier(deleted(),1,alpha(),Place::AlphaOne),
  history_supplier(deleted(),2,beta(),Place::BetaOne),history_supplier(deleted(),3,beta(),Place::BetaOne),
  history_version(beta_initial(),0)==one(),history_version(default_result().world,1)==one(),history_version(beta_result().world,3)==one(),
  history_command(beta_initial(),0)==beta_command(),history_command(default_result().world,1)==reinstall_command(),history_command(beta_result().world,3)==reinstall_command(),
  record_delta(beta_initial(),default_result().world)==1,record_delta(deleted(),failed_default().world)==1,
  record_delta(failed_default().world,failed_dev().world)==1,
  failed_default().history==HistoryEffect::FailedInstallFieldsUnknown,failed_dev().history==HistoryEffect::FailedInstallFieldsUnknown,
  !missing().ok,missing().assessed,!missing().delegated,missing().history==HistoryEffect::NoNewInstallRecord,
  record_delta(failed_dev().world,missing().world)==0,
  missing().world.base.records==failed_dev().world.base.records
{reveal_with_fuel(search_from,4);assert(one()!=two()) by(compute);}
pub proof fn flags_neighbor()
 ensures update_snapshots(command()),!update_snapshots(Command{snapshots:Some(false),..command()}),
  update_snapshots(Command{snapshots:Some(true),..command()}),default_result().update_snapshots,
  !reinstall(beta_initial(),Command{snapshots:Some(false),..command()}).update_snapshots,
  !failed_dev().ok,failed_dev().update_snapshots
{reveal_with_fuel(search_from,4);assert(one()!=two()) by(compute);}
pub proof fn stored_root_neighbor()
 ensures !root_exists(World{base:p::World{module:Some(p::Installed{root:p::Root::C,..beta_initial().base.module.unwrap()}),..beta_initial().base},..beta_initial()}),
  root_exists(World{base:p::World{module:Some(p::Installed{root:p::Root::A,..beta_initial().base.module.unwrap()}),..beta_initial().base},files:beta_initial().files.remove(Place::BetaOne),..beta_initial()})
{
 assert(p::root_text(p::Root::C)+(package_name()+seq![47u8])!=directory_text(Place::AlphaOne)) by(compute);
 assert(p::root_text(p::Root::C)+(package_name()+seq![47u8])!=directory_text(Place::AlphaTwo)) by(compute);
 assert(p::root_text(p::Root::C)+(package_name()+seq![47u8])!=directory_text(Place::BetaOne)) by(compute);
 assert(beta_initial().directory==Some(Place::BetaOne));
 let changed=World{base:p::World{module:Some(p::Installed{root:p::Root::C,..beta_initial().base.module.unwrap()}),..beta_initial().base},..beta_initial()};
 assert(module_directory(changed)==Some(p::root_text(p::Root::C)+(package_name()+seq![47u8])));
 assert(!root_exists(changed));
}
}
fn main(){}
