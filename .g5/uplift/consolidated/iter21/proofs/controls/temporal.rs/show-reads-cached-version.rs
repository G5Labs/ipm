// IPM571e04a1: cached exact-version selection vs current physical directory load.
// Normalized one-directory filesystem repo; valid parsed plain1/2 manifests;
// no dependencies/dependent constraints, noUpdatePackage, no dev/force, and
// successful copy/configuration/manifest import/save/load/compile providers.
// This extends the stale-source gap without changing19's stable-source contract.
// Other names/versions/errors, removal, concurrency and general cache lifetimes
// are unassessed. Archive paths remain UNASSESSED/UNPURSUED with no representation.
#[path="../../../../../../Verus_verification/consolidated/iter19/duplicates.rs"]
pub mod inherited_duplicates;
use inherited_duplicates as p;
use vstd::prelude::*;
verus! {
pub type Text=Seq<u8>;
#[derive(PartialEq,Eq)]
pub enum Version {One,Two}
pub struct World {pub current:p::Scenario,pub cache:p::Build,pub module:Option<p::Installed>}
pub struct Outcome {pub world:World,pub selected:Option<Version>,pub ok:bool,pub assessed:bool}
pub open spec fn package_name()->Text {seq![105u8,116,101,114,50,49,45,99,97,99,104,101]}
pub open spec fn root()->Text {seq![47u8,116,109,112,47,105,116,101,114,50,49,47,114,101,112,111,47]}
pub open spec fn active()->Text {seq![97u8,99,116,105,118,101]}
pub open spec fn repository()->Text {seq![116u8,101,109,112,111,114,97,108]}
pub open spec fn version(v:Version)->Text {match v{Version::One=>seq![49u8,46,48,46,48],Version::Two=>seq![50u8,46,48,46,48]}}
pub open spec fn decode(t:Text)->Option<Version> {if t==version(Version::One){Some(Version::One)}else if t==version(Version::Two){Some(Version::Two)}else{None}}
pub open spec fn rank(v:Version)->int {match v{Version::One=>1,Version::Two=>2}}
pub open spec fn rank_text(t:Text)->int {match decode(t){Some(v)=>rank(v),None=>0}}
pub open spec fn disk(v:Version)->p::Disk {p::Disk{manifest:p::Manifest{name:package_name(),version:version(v),
 marker:match v{Version::One=>p::source_a(),Version::Two=>p::source_b()}},payload:match v{Version::One=>101,Version::Two=>202}}}
pub open spec fn source(v:Version)->p::Scenario {p::Scenario{root:root(),repository:repository(),directories:map![active()=>disk(v)],order:seq![active()]}}
pub open spec fn initial()->World {let s=source(Version::One);World{current:s,cache:p::build(s),module:None}}
pub open spec fn cache_key(w:World,v:Version)->p::Key {p::Key{root:w.current.root,name:package_name(),version:version(v)}}
pub open spec fn selected_row(w:World,v:Version)->Option<p::Row> {p::lookup(w.cache,cache_key(w,v))}
// One cached row in this domain; this helper is not a general sorting/search policy.
pub open spec fn offered(w:World)->Option<Version> {
 if selected_row(w,Version::One).is_some(){Some(Version::One)}
 else if selected_row(w,Version::Two).is_some(){Some(Version::Two)}else{None}
}
pub open spec fn cache_row(w:World)->Option<p::Row> {match offered(w){Some(v)=>selected_row(w,v),None=>None}}
pub open spec fn cache_version(w:World)->int {match cache_row(w){Some(r)=>rank_text(r.key.version),None=>0}}
pub open spec fn cache_manifest_version(w:World)->int {match cache_row(w){Some(r)=>rank_text(r.manifest.version),None=>0}}
pub open spec fn cache_marker(w:World)->int {match cache_row(w){Some(r)=>p::marker_code(r.manifest.marker),None=>0}}
pub open spec fn service_manifest(w:World)->Option<p::Manifest> {
 match offered(w){Some(v)=>p::service_manifest(w.cache,cache_key(w,v)),None=>None}
}
pub open spec fn service_version(w:World)->int {match service_manifest(w){Some(m)=>rank_text(m.version),None=>0}}
pub open spec fn service_marker(w:World)->int {match service_manifest(w){Some(m)=>p::marker_code(m.marker),None=>0}}
pub open spec fn service_directory(w:World)->Option<Text> {
 match offered(w){Some(v)=>p::service_directory(w.cache,cache_key(w,v)),None=>None}
}
pub open spec fn physical_version(w:World)->int {if w.current.directories.contains_key(active()){rank_text(w.current.directories[active()].manifest.version)}else{0}}
pub open spec fn physical_marker(w:World)->int {if w.current.directories.contains_key(active()){p::marker_code(w.current.directories[active()].manifest.marker)}else{0}}
// Maintainer's completed ordinary file replacements change both current source
// files; no cache rebuild or runtime compile is invoked by this writer.
pub open spec fn physical_replace(w:World,v:Version)->World {
 World{current:p::Scenario{directories:w.current.directories.insert(active(),disk(v)),..w.current},..w}
}
// Normal same-definition repo configuration calls BuildCache(1,1,1).
// Fresh purge/build uses current files; module/code projection is preserved.
pub open spec fn configure_same_repo(w:World)->World {World{cache:p::build(w.current),..w}}
pub open spec fn eligible_actual(w:World,r:p::Row)->bool {
 p::indexed(w.cache.cache) && r.key.root==w.current.root && r.key.name==package_name() &&
 w.current.directories.contains_key(r.subdir) &&
 w.current.directories[r.subdir].manifest.name==package_name() &&
 decode(w.current.directories[r.subdir].manifest.version).is_some() &&
 match w.module{None=>true,Some(m)=>decode(m.version).is_some() &&
  rank_text(m.version)<=rank_text(w.current.directories[r.subdir].manifest.version)}
}
// Conditional successful directory effect. Cached selection controls WHERE,
// while actual current module.xml/code controls WHAT gets imported/compiled.
// No request-version revalidation occurs in this as-built slice.
pub open spec fn actual_directory_load(w:World,r:p::Row)->World {
 let actual=w.current.directories[r.subdir];
 World{module:Some(p::Installed{name:actual.manifest.name,version:actual.manifest.version,
  description:actual.manifest.marker,root:p::directory_text(r.key,r.subdir),
  repository:w.current.repository,payload:actual.payload}),..w}
}
pub open spec fn install(w:World,requested:Version)->Outcome {
 if !w.cache.assessed || !w.cache.ok{Outcome{world:w,selected:None,ok:false,assessed:false}}
 else{match selected_row(w,requested){
  None=>Outcome{world:w,selected:None,ok:false,assessed:true},
  Some(r)=>if !eligible_actual(w,r){Outcome{world:w,selected:decode(r.key.version),ok:false,assessed:false}}
   else{Outcome{world:actual_directory_load(w,r),selected:decode(r.key.version),ok:true,assessed:true}}
 }}
}
pub open spec fn installed_version(w:World)->int {match w.module{Some(m)=>rank_text(m.version),None=>0}}
pub open spec fn installed_marker(w:World)->int {match w.module{Some(m)=>p::marker_code(m.description),None=>0}}
pub open spec fn payload(w:World)->int {match w.module{Some(m)=>m.payload,None=>0}}
pub open spec fn installed_root(w:World)->Option<Text> {match w.module{Some(m)=>Some(m.root),None=>None}}
pub open spec fn supplier(w:World)->Text {match w.module{Some(m)=>m.repository,None=>seq![]}}
pub open spec fn requested_version_preserved(o:Outcome,v:Version)->bool {match o.world.module{Some(m)=>m.version==version(v),None=>false}}
pub open spec fn listed_version(w:World,v:Version)->bool {selected_row(w,v).is_some()}
pub open spec fn shown_version(w:World,v:Version)->bool {match w.module{Some(m)=>m.name==package_name() && service_version(w)==rank(v),None=>false}}
pub open spec fn b(v:bool)->int {if v{1}else{0}}
pub open spec fn stable_one()->Outcome {install(initial(),Version::One)}
pub open spec fn after_edit()->World {physical_replace(stable_one().world,Version::Two)}
pub open spec fn stale_one()->Outcome {install(after_edit(),Version::One)}
pub open spec fn after_refresh()->World {configure_same_repo(stale_one().world)}
pub open spec fn refused_one()->Outcome {install(after_refresh(),Version::One)}
pub open spec fn new_two()->Outcome {install(refused_one().world,Version::Two)}

pub proof fn physical_writer_frames_cache_and_runtime(w:World,v:Version)
 ensures physical_replace(w,v).cache==w.cache,physical_replace(w,v).module==w.module,
  physical_replace(w,v).current.root==w.current.root,physical_replace(w,v).current.repository==w.current.repository
{}
pub proof fn refusal_frames_selected_state(w:World,v:Version)
 requires w.cache.assessed,w.cache.ok,selected_row(w,v)==None
 ensures install(w,v).assessed,!install(w,v).ok,install(w,v).world==w
{}
// Focused control witnesses call definitions only, no proof lemmas/broadcasts.
pub proof fn snapshot_neighbor()
 ensures cache_version(initial())==1,cache_manifest_version(initial())==1,cache_marker(initial())==1,
  service_version(initial())==1,service_marker(initial())==1,physical_version(initial())==1,
  cache_version(after_edit())==1,cache_manifest_version(after_edit())==1,service_version(after_edit())==1,
  cache_marker(after_edit())==1,service_marker(after_edit())==1,physical_version(after_edit())==2,physical_marker(after_edit())==2,
  after_edit().cache==stable_one().world.cache,payload(after_edit())==101,installed_version(after_edit())==1,
  service_directory(initial())==Some(root()+active()+p::slash()),service_directory(after_edit())==Some(root()+active()+p::slash())
{reveal_with_fuel(p::materialize,2);reveal_with_fuel(p::fold,2);assert(version(Version::One)!=version(Version::Two)) by(compute);}
pub proof fn stable_neighbor()
 ensures stable_one().assessed,stable_one().ok,stable_one().selected==Some(Version::One),
  installed_version(stable_one().world)==1,installed_marker(stable_one().world)==1,payload(stable_one().world)==101,
  requested_version_preserved(stable_one(),Version::One),supplier(stable_one().world)==repository(),
  installed_root(stable_one().world)==Some(root()+active()+p::slash())
{reveal_with_fuel(p::materialize,2);reveal_with_fuel(p::fold,2);assert(version(Version::One)!=version(Version::Two)) by(compute);}
pub proof fn stale_neighbor()
 ensures stale_one().assessed,stale_one().ok,stale_one().selected==Some(Version::One),
  installed_version(stale_one().world)==2,installed_marker(stale_one().world)==2,payload(stale_one().world)==202,
  !requested_version_preserved(stale_one(),Version::One),supplier(stale_one().world)==repository(),
  installed_root(stale_one().world)==Some(root()+active()+p::slash()),
  !p::stable_selected_source(after_edit().current,after_edit().cache,cache_key(after_edit(),Version::One)),
  !p::install_directory(after_edit().current,after_edit().cache,cache_key(after_edit(),Version::One)).assessed
{reveal_with_fuel(p::materialize,2);reveal_with_fuel(p::fold,2);assert(version(Version::One)!=version(Version::Two)) by(compute);}
pub proof fn refresh_neighbor()
 ensures cache_version(after_refresh())==2,cache_manifest_version(after_refresh())==2,cache_marker(after_refresh())==2,
  service_version(after_refresh())==2,service_marker(after_refresh())==2,physical_version(after_refresh())==2,physical_marker(after_refresh())==2,
  service_directory(after_refresh())==Some(root()+active()+p::slash()),
  !listed_version(after_refresh(),Version::One),listed_version(after_refresh(),Version::Two),
  installed_version(after_refresh())==2,payload(after_refresh())==202,
  refused_one().assessed,!refused_one().ok,refused_one().world==after_refresh(),refused_one().selected==None,
  payload(refused_one().world)==202,installed_version(refused_one().world)==2,
  new_two().assessed,new_two().ok,new_two().selected==Some(Version::Two),
  requested_version_preserved(new_two(),Version::Two),payload(new_two().world)==202,
  installed_version(new_two().world)==2,installed_marker(new_two().world)==2,supplier(new_two().world)==repository(),
  installed_root(new_two().world)==Some(root()+active()+p::slash())
{reveal_with_fuel(p::materialize,2);reveal_with_fuel(p::fold,2);assert(version(Version::One)!=version(Version::Two)) by(compute);}
pub proof fn readers_neighbor()
 ensures listed_version(initial(),Version::One),!listed_version(initial(),Version::Two),
  listed_version(after_edit(),Version::One),!listed_version(after_edit(),Version::Two),
  shown_version(stable_one().world,Version::One),!shown_version(stable_one().world,Version::Two),
  !shown_version(stale_one().world,Version::One),shown_version(stale_one().world,Version::Two),
  !shown_version(refused_one().world,Version::One),shown_version(refused_one().world,Version::Two),
  !shown_version(new_two().world,Version::One),shown_version(new_two().world,Version::Two)
{reveal_with_fuel(p::materialize,2);reveal_with_fuel(p::fold,2);assert(version(Version::One)!=version(Version::Two)) by(compute);}
}
fn main(){}
