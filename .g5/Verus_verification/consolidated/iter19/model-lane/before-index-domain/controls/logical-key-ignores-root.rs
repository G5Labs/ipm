// IPM571e04a1: fresh filesystem cache admits first logical-key row, preserving
// its parsed manifest and physical directory for the ordinary directory install reader.
// Provider domain: normalized roots/subdirectories, distinct scanned directories,
// valid parsed manifests, successful fresh purge/new-row save or recognized
// RootNameVersion duplicate handling, stable physical files and successful load/compile.
// No portable scan sorting, generic XML/status parser, failed refresh, concurrency,
// source mutation, removed directories or other package-delivery branch is claimed.
// Archive paths remain UNASSESSED/UNPURSUED and have no representation here.
#[path="../../../verus/owners.rs"]
pub mod inherited_owners;
use vstd::prelude::*;

verus! {
pub type Text=Seq<u8>;
pub struct Manifest {pub name:Text,pub version:Text,pub marker:Text}
pub struct Disk {pub manifest:Manifest,pub payload:int}
pub struct Key {pub root:Text,pub name:Text,pub version:Text}
pub struct Entry {pub key:Key,pub subdir:Text,pub manifest:Manifest}
pub struct Row {pub key:Key,pub subdir:Text,pub manifest:Manifest}
pub struct Warning {pub key:Key,pub retained:Text,pub duplicate:Text}
pub struct Cache {pub rows:Map<Key,Row>,pub warnings:Seq<Warning>}
pub struct Admission {pub cache:Cache,pub ok:bool}
pub struct Build {pub cache:Cache,pub ok:bool,pub assessed:bool}
pub struct Scenario {
 pub root:Text,pub repository:Text,pub directories:Map<Text,Disk>,pub order:Seq<Text>,
}
pub struct Installed {
 pub name:Text,pub version:Text,pub description:Text,pub root:Text,pub repository:Text,pub payload:int,
}
pub struct Installation {pub module:Option<Installed>,pub ok:bool,pub assessed:bool}

pub open spec fn name()->Text {seq![105u8,116,101,114,49,57,45,100,117,112,101]}
pub open spec fn version()->Text {seq![49u8,46,48,46,48]}
pub open spec fn source_a()->Text {seq![115u8,111,117,114,99,101,45,65]}
pub open spec fn source_b()->Text {seq![115u8,111,117,114,99,101,45,66]}
pub open spec fn disk_a()->Disk {Disk{manifest:Manifest{name:name(),version:version(),marker:source_a()},payload:101}}
pub open spec fn disk_b()->Disk {Disk{manifest:Manifest{name:name(),version:version(),marker:source_b()},payload:202}}
pub open spec fn low_a()->Text {seq![48u8,48,45,97]}
pub open spec fn high_b()->Text {seq![57u8,57,45,98]}
pub open spec fn low_b()->Text {seq![48u8,48,45,98]}
pub open spec fn high_a()->Text {seq![57u8,57,45,97]}
pub open spec fn slash()->Text {seq![47u8]}
pub open spec fn empty()->Cache {Cache{rows:Map::empty(),warnings:seq![]}}
pub open spec fn key(root:Text,m:Manifest)->Key {Key{root:root_ab(),name:m.name,version:m.version}}
pub open spec fn target(s:Scenario)->Key {Key{root:s.root,name:name(),version:version()}}
pub open spec fn valid_order(s:Scenario)->bool {
 (forall|i:int| 0<=i<s.order.len() ==> s.directories.contains_key(s.order[i])) &&
 (forall|i:int,j:int| 0<=i<j<s.order.len() ==> s.order[i]!=s.order[j])
}
// ScanDirectory supplies this order; materialize preserves it and derives key+manifest
// from the selected physical source. Every disk manifest is already parsed and valid.
pub open spec fn materialize(s:Scenario,i:nat)->Seq<Entry>
 decreases if i<s.order.len(){(s.order.len()-i) as nat}else{0nat}
{
 if i>=s.order.len(){seq![]}else{
  let dir=s.order[i as int]; let m=s.directories[dir].manifest;
  seq![Entry{key:key(s.root,m),subdir:dir,manifest:m}]+materialize(s,i+1)
 }
}
pub open spec fn add_valid(c:Cache,e:Entry)->Admission {
 if c.rows.contains_key(e.key) {
  Admission{cache:Cache{rows:c.rows,warnings:c.warnings+seq![Warning{
   key:e.key,retained:c.rows[e.key].subdir,duplicate:e.subdir}]},ok:true}
 }else{
  Admission{cache:Cache{rows:c.rows.insert(e.key,Row{key:e.key,subdir:e.subdir,manifest:e.manifest}),
   warnings:c.warnings},ok:true}
 }
}
pub open spec fn fold(es:Seq<Entry>,i:nat,c:Cache)->Admission
 decreases if i<es.len(){(es.len()-i) as nat}else{0nat}
{
 if i>=es.len(){Admission{cache:c,ok:true}}else{
  let a=add_valid(c,es[i as int]);let rest=fold(es,i+1,a.cache);
  Admission{cache:rest.cache,ok:a.ok && rest.ok}
 }
}
// Fresh purged cache only. Other save failures/cache refresh interactions are unassessed.
pub open spec fn build(s:Scenario)->Build {
 if !valid_order(s){Build{cache:empty(),ok:false,assessed:false}}else{
  let a=fold(materialize(s,0),0,empty());Build{cache:a.cache,ok:a.ok,assessed:true}
 }
}
pub open spec fn lookup(b:Build,k:Key)->Option<Row> {
 if b.cache.rows.contains_key(k){Some(b.cache.rows[k])}else{None}
}
pub open spec fn service_manifest(b:Build,k:Key)->Option<Manifest> {
 match lookup(b,k){Some(r)=>Some(r.manifest),None=>None}
}
pub open spec fn directory_text(k:Key,subdir:Text)->Text {k.root+subdir+slash()}
pub open spec fn service_directory(b:Build,k:Key)->Option<Text> {
 match lookup(b,k){Some(r)=>Some(directory_text(k,r.subdir)),None=>None}
}
pub open spec fn stable_selected_source(s:Scenario,b:Build,k:Key)->bool {
 match lookup(b,k){Some(r)=>s.root==k.root && s.directories.contains_key(r.subdir) &&
  s.directories[r.subdir].manifest==r.manifest && key(s.root,s.directories[r.subdir].manifest)==k,
  None=>false}
}
// Actual ordinary consumer uses the directory reader then reads module.xml/code from
// that directory. Cached manifest and live source agree only under the stable-source port.
pub open spec fn install_directory(s:Scenario,b:Build,k:Key)->Installation {
 if !b.assessed || !b.ok || !stable_selected_source(s,b,k){
  Installation{module:None,ok:false,assessed:false}
 }else{
  let row=lookup(b,k).unwrap();let disk=s.directories[row.subdir];
  Installation{module:Some(Installed{name:disk.manifest.name,version:disk.manifest.version,
   description:disk.manifest.marker,root:directory_text(k,row.subdir),repository:s.repository,
   payload:disk.payload}),ok:true,assessed:true}
 }
}
pub open spec fn loaded(s:Scenario)->Installation {install_directory(s,build(s),target(s))}
pub open spec fn payload(i:Installation)->int {match i.module{Some(m)=>m.payload,None=>0}}
pub open spec fn description(i:Installation)->Text {match i.module{Some(m)=>m.description,None=>seq![]}}
pub open spec fn installed_root(i:Installation)->Option<Text> {match i.module{Some(m)=>Some(m.root),None=>None}}
pub open spec fn supplier(i:Installation)->Text {match i.module{Some(m)=>m.repository,None=>seq![]}}
pub open spec fn installed_version(i:Installation)->Text {match i.module{Some(m)=>m.version,None=>seq![]}}
pub open spec fn shown_supplier(i:Installation)->Option<Text> {
 match i.module{Some(m)=>if m.repository.len()>0{Some(m.repository)}else{None},None=>None}
}
pub open spec fn listed_for_repository(i:Installation,r:Text)->bool {
 match i.module{Some(m)=>m.name==name() && m.repository==r,None=>false}
}
pub open spec fn marker_code(t:Text)->int {if t==source_a(){1}else if t==source_b(){2}else{0}}
pub open spec fn retained_dir(s:Scenario)->Text {match lookup(build(s),target(s)){Some(r)=>r.subdir,None=>seq![]}}
pub open spec fn cached_marker(s:Scenario)->Text {match service_manifest(build(s),target(s)){Some(m)=>m.marker,None=>seq![]}}
pub open spec fn b(v:bool)->int {if v{1}else{0}}

// These are provider-order witnesses. Native qualifies single_a, ab_a_first and
// ba_b_first only. Opposite-order neighbors are conditional, not asserted schedules;
// none asserts that the filesystem enumerator portably sorts these names.
pub open spec fn root_single()->Text {seq![47u8,116,109,112,47,105,116,101,114,49,57,47,114,101,112,111,115,47,115,105,110,103,108,101,47]}
pub open spec fn root_ab()->Text {seq![47u8,116,109,112,47,105,116,101,114,49,57,47,114,101,112,111,115,47,97,98,47]}
pub open spec fn root_ba()->Text {seq![47u8,116,109,112,47,105,116,101,114,49,57,47,114,101,112,111,115,47,98,97,47]}
pub open spec fn single(order:Seq<Text>)->Scenario {Scenario{root:root_single(),repository:seq![115u8,105,110,103,108,101],directories:map![low_a()=>disk_a()],order}}
pub open spec fn ab(order:Seq<Text>)->Scenario {Scenario{root:root_ab(),repository:seq![97u8,98],directories:map![low_a()=>disk_a(),high_b()=>disk_b()],order}}
pub open spec fn ba(order:Seq<Text>)->Scenario {Scenario{root:root_ba(),repository:seq![98u8,97],directories:map![low_b()=>disk_b(),high_a()=>disk_a()],order}}
pub open spec fn single_a()->Scenario {single(seq![low_a()])}
pub open spec fn ab_a_first()->Scenario {ab(seq![low_a(),high_b()])}
pub open spec fn ab_b_first()->Scenario {ab(seq![high_b(),low_a()])}
pub open spec fn ba_b_first()->Scenario {ba(seq![low_b(),high_a()])}
pub open spec fn ba_a_first()->Scenario {ba(seq![high_a(),low_b()])}
pub open spec fn rows(s:Scenario)->nat {build(s).cache.rows.dom().len()}
pub open spec fn warnings(s:Scenario)->nat {build(s).cache.warnings.len()}

pub proof fn same_key_preserves_pair(c:Cache,e:Entry)
 requires c.rows.contains_key(e.key)
 ensures add_valid(c,e).ok,add_valid(c,e).cache.rows==c.rows,
  add_valid(c,e).cache.warnings.len()==c.warnings.len()+1,
  add_valid(c,e).cache.warnings.last().retained==c.rows[e.key].subdir,
  add_valid(c,e).cache.warnings.last().duplicate==e.subdir
{}
pub proof fn distinct_key_inserts_pair(c:Cache,e:Entry)
 requires !c.rows.contains_key(e.key)
 ensures add_valid(c,e).ok,add_valid(c,e).cache.rows[e.key].subdir==e.subdir,
  add_valid(c,e).cache.rows[e.key].manifest==e.manifest,
  add_valid(c,e).cache.warnings==c.warnings
{}
pub proof fn first_row_survives(es:Seq<Entry>,i:nat,c:Cache,k:Key)
 requires c.rows.contains_key(k)
 ensures fold(es,i,c).cache.rows.contains_key(k),fold(es,i,c).cache.rows[k]==c.rows[k]
 decreases if i<es.len(){(es.len()-i) as nat}else{0nat}
{
 if i<es.len(){
  let a=add_valid(c,es[i as int]);
  first_row_survives(es,i+1,a.cache,k);
 }
}
pub proof fn two_readers_open_same_row(b:Build,k:Key)
 requires b.cache.rows.contains_key(k)
 ensures service_manifest(b,k)==Some(b.cache.rows[k].manifest),
  service_directory(b,k)==Some(directory_text(k,b.cache.rows[k].subdir))
{}

// Focused control witnesses call definitions only and no proof lemmas/broadcasts.
pub proof fn admission_neighbor()
 ensures build(single_a()).assessed,build(single_a()).ok,rows(single_a())==1,warnings(single_a())==0,
  build(ab_a_first()).assessed,build(ab_a_first()).ok,rows(ab_a_first())==1,warnings(ab_a_first())==1,
  retained_dir(ab_a_first())==low_a(),cached_marker(ab_a_first())==source_a(),
  retained_dir(ab_b_first())==high_b(),cached_marker(ab_b_first())==source_b(),
  retained_dir(ba_b_first())==low_b(),cached_marker(ba_b_first())==source_b(),
  build(ba_b_first()).assessed,build(ba_b_first()).ok,rows(ba_b_first())==1,warnings(ba_b_first())==1,
  retained_dir(single_a())==low_a(),cached_marker(single_a())==source_a()
{
 reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);
 assert(build(single_a()).cache.rows==map![target(single_a())=>Row{key:target(single_a()),subdir:low_a(),manifest:disk_a().manifest}]);
 assert(build(ab_a_first()).cache.rows==map![target(ab_a_first())=>Row{key:target(ab_a_first()),subdir:low_a(),manifest:disk_a().manifest}]);
}
pub proof fn warning_neighbor()
 ensures build(ab_a_first()).cache.warnings==seq![Warning{key:target(ab_a_first()),retained:low_a(),duplicate:high_b()}],
  build(ab_b_first()).cache.warnings==seq![Warning{key:target(ab_b_first()),retained:high_b(),duplicate:low_a()}],
  build(ba_b_first()).cache.warnings==seq![Warning{key:target(ba_b_first()),retained:low_b(),duplicate:high_a()}],
  build(ab_b_first()).ok,build(ba_b_first()).ok
{reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);}
pub proof fn service_neighbor()
 ensures service_manifest(build(single_a()),target(single_a()))==Some(disk_a().manifest),
  service_directory(build(single_a()),target(single_a()))==Some(root_single()+low_a()+slash()),
  service_manifest(build(ab_a_first()),target(ab_a_first()))==Some(disk_a().manifest),
  service_directory(build(ab_a_first()),target(ab_a_first()))==Some(root_ab()+low_a()+slash()),
  service_manifest(build(ab_b_first()),target(ab_b_first()))==Some(disk_b().manifest),
  service_directory(build(ab_b_first()),target(ab_b_first()))==Some(directory_text(target(ab_b_first()),high_b())),
  service_manifest(build(ba_b_first()),target(ba_b_first()))==Some(disk_b().manifest),
  service_directory(build(ba_b_first()),target(ba_b_first()))==Some(root_ba()+low_b()+slash())
{reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);}
pub proof fn installation_neighbor()
 ensures loaded(single_a()).assessed,loaded(single_a()).ok,payload(loaded(single_a()))==101,
  description(loaded(single_a()))==source_a(),supplier(loaded(single_a()))==single_a().repository,
  installed_version(loaded(single_a()))==version(),
  installed_root(loaded(single_a()))==Some(root_single()+low_a()+slash()),
  shown_supplier(loaded(single_a()))==Some(single_a().repository),listed_for_repository(loaded(single_a()),single_a().repository),
  loaded(ab_a_first()).assessed,loaded(ab_a_first()).ok,payload(loaded(ab_a_first()))==101,
  description(loaded(ab_a_first()))==source_a(),supplier(loaded(ab_a_first()))==ab_a_first().repository,
  installed_version(loaded(ab_a_first()))==version(),
  shown_supplier(loaded(ab_a_first()))==Some(ab_a_first().repository),listed_for_repository(loaded(ab_a_first()),ab_a_first().repository),
  installed_root(loaded(ab_a_first()))==Some(root_ab()+low_a()+slash()),
  loaded(ab_b_first()).ok,payload(loaded(ab_b_first()))==202,description(loaded(ab_b_first()))==source_b(),
  loaded(ba_b_first()).assessed,loaded(ba_b_first()).ok,payload(loaded(ba_b_first()))==202,
  description(loaded(ba_b_first()))==source_b(),supplier(loaded(ba_b_first()))==ba_b_first().repository,
  installed_version(loaded(ba_b_first()))==version(),
  shown_supplier(loaded(ba_b_first()))==Some(ba_b_first().repository),listed_for_repository(loaded(ba_b_first()),ba_b_first().repository),
  installed_root(loaded(ba_b_first()))==Some(root_ba()+low_b()+slash())
{reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);}
pub proof fn logical_key_neighbor()
 ensures key(root_single(),disk_a().manifest)!=key(root_ab(),disk_a().manifest),
  key(root_ab(),disk_a().manifest)!=key(root_ab(),Manifest{name:seq![120u8],..disk_a().manifest}),
  key(root_ab(),disk_a().manifest)!=key(root_ab(),Manifest{version:seq![50u8,46,48,46,48],..disk_a().manifest})
{}
pub proof fn coarse_inherited_neighbor()
 ensures inherited_owners::scanned_keys(seq![inherited_owners::Entry{key:0,depth:1,manifest:inherited_owners::Manifest::Valid}],1,0)==set![0nat],
  inherited_owners::scanned_keys(seq![inherited_owners::Entry{key:0,depth:1,manifest:inherited_owners::Manifest::Valid},inherited_owners::Entry{key:0,depth:1,manifest:inherited_owners::Manifest::Valid}],1,0)==set![0nat],
  rows(ab_a_first())==rows(ab_b_first()),cached_marker(ab_a_first())!=cached_marker(ab_b_first())
{reveal_with_fuel(materialize,3);reveal_with_fuel(fold,3);reveal_with_fuel(inherited_owners::scanned_keys,3);}
}
fn main(){}
