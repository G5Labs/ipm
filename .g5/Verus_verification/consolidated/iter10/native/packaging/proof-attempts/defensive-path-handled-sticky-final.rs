use vstd::prelude::*;
verus! {
// UnitTest packaging at product 571e04a1. XML/list decoding and path normalization
// supply Phase and Text values. Namespace document enumeration supplies docs;
// source file maps and faithful directory-copy/tar providers are explicit ports.
// No claim about XML parsing, gzip/tar implementation or filesystem atomicity.
pub enum Phase {Test,Verify,Package}
pub type Text=Seq<int>;
pub struct Resource {pub name:Text,pub declared:Seq<Phase>,pub phase:Seq<Phase>}
pub struct Children {pub paths:Set<Text>,pub metadata:bool,pub returned_scope:Seq<Phase>}
pub struct References {pub paths:Set<Text>,pub metadata:bool}
pub open spec fn prefix(a:Text,b:Text)->bool {
 a.len()<=b.len() && forall|i:int|0<=i<a.len() ==> a[i]==b[i]
}
pub open spec fn path_name(n:Text)->bool {n.len()>0 && n[0]==47}
// The fallback normalizes a directory before CopyDir, so /tests cannot select
// /testsSibling. The filesystem port supplies already-normalized file keys.
pub open spec fn directory_prefix(n:Text)->Text {
 if n.len()>0 && n[n.len()-1]==47 {n}else{n+seq![47int]}
}
pub open spec fn has_scope(phases:Seq<Phase>,scope:Seq<Phase>)->bool {
 scope.len()==0 || exists|i:int|0<=i<scope.len() && phases.contains(scope[i])
}
// Test.OnResolveChildren stamps Scope from its own Phase property. The direct
// ResourceReference result has documents and metadata, but no directory key.
pub open spec fn unit_children(r:Resource)->Children {
 Children{paths:Set::empty(),metadata:true,returned_scope:r.phase}
}
// Only path entries are represented here; namespace document enumeration stays
// outside this claim. A UnitTest path is absent from direct ResolveChildren.
pub open spec fn references(r:Resource,phases:Seq<Phase>)->References {
 if !has_scope(phases,r.declared) {References{paths:Set::empty(),metadata:false}} else {
  let children=unit_children(r);
  if !has_scope(phases,children.returned_scope) {References{paths:Set::empty(),metadata:false}} else {
   let base=children.paths;
   References{paths:if path_name(r.name) {base.insert(r.name)} else {base},metadata:false}
  }
 }
}
// Re-added paths carry the module scalar, not Processor metadata. The actual
// export therefore falls through to directory copy without a processor call.
pub open spec fn path_has_processor()->bool {false}
// The defensive Test.OnExportItem path API also returns OK and unhandled.
pub open spec fn test_path_export_handled()->bool {true}
pub open spec fn path_export_handled()->bool {
 path_has_processor() && test_path_export_handled()
}
// Same explicit-parameter precedence as inherited scopes.rs::export_phases.
pub open spec fn export_phases(base:Seq<Phase>,param:Option<bool>,manifest:bool)->Seq<Phase> {
 let include=match param {Some(b)=>b,None=>manifest};
 if include {base.push(Phase::Test).push(Phase::Verify)}else{base}
}
pub open spec fn copied(r:Resource,phases:Seq<Phase>,source:Map<Text,Text>)->Map<Text,Text> {
 if references(r,phases).paths.contains(r.name) && !path_export_handled() {
  Map::new(source.dom().filter(|p:Text|prefix(directory_prefix(r.name),p)),|p:Text|source[p])
 }else{Map::empty()}
}
pub open spec fn manifest_path()->Text {seq![0int]}
pub open spec fn manifest_bytes()->Text {seq![90int]}
// Ordinary package exports into a fresh temporary folder. Compression/reading
// is related to this file map by the native membership + byte-equality check.
pub open spec fn archive(files:Map<Text,Text>)->Map<Text,Text> {
 files.insert(manifest_path(),manifest_bytes())
}
pub open spec fn package(r:Resource,param:Option<bool>,manifest:bool,source:Map<Text,Text>)->Map<Text,Text> {
 archive(copied(r,export_phases(seq![Phase::Package],param,manifest),source))
}
// UpdatePersistentFromStream imports into the existing module via XMLNew.
// It clears collections, but an omitted scalar IncludeTests retains old value.
// Some(false) is an explicit manifest reset; None is an absent XML element.
pub open spec fn refresh_include(old:bool,incoming:Option<bool>)->bool {
 match incoming {Some(value)=>value,None=>old}
}
pub struct PackageState {pub stored:bool,pub contents:Map<Text,Text>}
pub open spec fn refresh_and_package(r:Resource,old:bool,incoming:Option<bool>,command:Option<bool>,files:Map<Text,Text>)->PackageState {
 let stored=refresh_include(old,incoming);
 PackageState{stored,contents:package(r,command,stored,files)}
}
pub open spec fn fixture(phase:Phase)->Resource {
 Resource{name:seq![47int,116,101,115,116,115,47],declared:seq![],phase:seq![phase]}
}
pub open spec fn filename()->Text {fixture(Phase::Test).name+seq![65int]}
pub open spec fn source()->Map<Text,Text> {map![filename()=>seq![7int,8,9]]}
pub open spec fn archive_has(phase:Phase,param:Option<bool>,manifest:bool)->bool {
 package(fixture(phase),param,manifest,source()).contains_key(filename())
}
pub open spec fn archive_matches(phase:Phase,param:Option<bool>,manifest:bool)->bool {
 let a=package(fixture(phase),param,manifest,source());
 a.contains_key(filename()) && a[filename()]==source()[filename()]
}
pub open spec fn split_scope()->Resource {Resource{declared:seq![Phase::Verify],..fixture(Phase::Test)}}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub proof fn scope_metadata_is_not_a_resource(r:Resource,phases:Seq<Phase>)
 ensures !references(r,phases).metadata
{}
pub proof fn selected_file_keeps_payload(r:Resource,phases:Seq<Phase>,s:Map<Text,Text>,f:Text)
 requires path_name(r.name),has_scope(phases,r.declared),has_scope(phases,r.phase),s.contains_key(f),prefix(directory_prefix(r.name),f)
 ensures copied(r,phases,s).contains_key(f),copied(r,phases,s)[f]==s[f]
{}
pub proof fn copied_file_requires_path(r:Resource,phases:Seq<Phase>,s:Map<Text,Text>,f:Text)
 requires copied(r,phases,s).contains_key(f)
 ensures references(r,phases).paths.contains(r.name),prefix(directory_prefix(r.name),f),s.contains_key(f)
{}
pub proof fn fixture_selected(phase:Phase,phases:Seq<Phase>)
 requires phases.contains(phase)
 ensures copied(fixture(phase),phases,source()).contains_key(filename()),copied(fixture(phase),phases,source())[filename()]==source()[filename()]
{
 assert(has_scope(phases,fixture(phase).phase)) by {assert(fixture(phase).phase[0]==phase);}
 assert(prefix(directory_prefix(fixture(phase).name),filename())) by {
  assert forall|i:int|0<=i<directory_prefix(fixture(phase).name).len() implies directory_prefix(fixture(phase).name)[i]==filename()[i] by {};
 }
 selected_file_keeps_payload(fixture(phase),phases,source(),filename());
}
// The seven native requests are consecutive refreshes of the same module.
pub open spec fn matrix_incoming(n:nat)->Option<bool> {
 if n==2 || n==3 {Some(true)}else if n>=5 {Some(false)}else{None}
}
pub open spec fn matrix_command(n:nat)->Option<bool> {
 if n==1 || n==5 {Some(true)}else if n==3 {Some(false)}else{None}
}
pub open spec fn matrix_prior(n:nat)->bool
 decreases n
{ if n==0 {false}else{refresh_include(matrix_prior((n-1) as nat),matrix_incoming((n-1) as nat))} }
pub open spec fn matrix(n:nat)->PackageState {
 refresh_and_package(fixture(if n>=5 {Phase::Verify}else{Phase::Test}),matrix_prior(n),matrix_incoming(n),matrix_command(n),source())
}
pub proof fn matrix_neighbors() {
 reveal_with_fuel(matrix_prior,10);
 let p=export_phases(seq![Phase::Package],Some(true),false);
 assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
 fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
 let q=export_phases(seq![Phase::Package],None,true);
 assert(q[1]==Phase::Test);fixture_selected(Phase::Test,q);
 assert(!matrix(0).contents.contains_key(filename()));
 assert(matrix(1).contents.contains_key(filename()));assert(!matrix(1).stored);
 assert(matrix(2).contents.contains_key(filename()));assert(matrix(2).stored);
 assert(!matrix(3).contents.contains_key(filename()));assert(matrix(3).stored);
 assert(matrix(4).contents.contains_key(filename()));assert(matrix(4).stored);
 assert(matrix(4).contents[filename()]==source()[filename()]);
 assert(matrix(5).contents.contains_key(filename()));assert(!matrix(5).stored);
 assert(!matrix(6).contents.contains_key(filename()));assert(!matrix(6).stored);
}
pub open spec fn sticky()->PackageState {
 refresh_and_package(fixture(Phase::Test),refresh_include(false,Some(true)),None,None,source())
}
pub open spec fn reset()->PackageState {
 refresh_and_package(fixture(Phase::Test),true,Some(false),None,source())
}
pub proof fn refresh_neighbors() {
 assert(refresh_include(false,None)==false);
 assert(refresh_include(true,None)==true);
 assert(!refresh_include(true,Some(false)));
 let zero=refresh_and_package(fixture(Phase::Test),true,None,Some(false),source());
 assert(zero.stored);
 assert(zero.contents==package(fixture(Phase::Test),Some(false),true,source()));
 assert(!has_scope(seq![Phase::Package],seq![Phase::Test]));
 assert(references(fixture(Phase::Test),seq![Phase::Package]).paths==Set::<Text>::empty());
 assert(copied(fixture(Phase::Test),seq![Phase::Package],source())==Map::<Text,Text>::empty());
 assert(filename()!=manifest_path()) by (compute);
 assert(!archive_has(Phase::Test,Some(false),true));
 assert(!zero.contents.contains_key(filename()));
 let after=refresh_and_package(fixture(Phase::Test),zero.stored,None,None,source());
 let p=export_phases(seq![Phase::Package],None,true);
 assert(p[1]==Phase::Test);fixture_selected(Phase::Test,p);
 assert(after.stored);assert(after.contents.contains_key(filename()));
 assert(after.contents[filename()]==source()[filename()]);
 assert(sticky().stored);assert(sticky().contents.contains_key(filename()));
 assert(sticky().contents[filename()]==source()[filename()]);
 assert(!reset().stored);assert(!reset().contents.contains_key(filename()));
}
pub proof fn directory_boundary() {
 let r=Resource{name:seq![47int,116,101,115,116,115],declared:seq![],phase:seq![Phase::Test]};
 let f=seq![47int,116,101,115,116,115,88,47,65];
 let s=map![f=>seq![1int]];
 assert(!copied(r,seq![Phase::Test],s).contains_key(f));
}
pub proof fn packaging_neighbors() {
 let p=export_phases(seq![Phase::Package],Some(true),false);
 assert(p[1]==Phase::Test);assert(p[2]==Phase::Verify);
 fixture_selected(Phase::Test,p);fixture_selected(Phase::Verify,p);
 let q=export_phases(seq![Phase::Package],None,true);
 assert(q[1]==Phase::Test);assert(q[2]==Phase::Verify);
 fixture_selected(Phase::Test,q);fixture_selected(Phase::Verify,q);
 assert(!archive_has(Phase::Test,None,false));
 assert(archive_has(Phase::Test,Some(true),false));
 assert(archive_has(Phase::Test,None,true));
 assert(!archive_has(Phase::Test,Some(false),true));
 assert(archive_matches(Phase::Test,Some(true),false));
 assert(archive_matches(Phase::Test,None,true));
 assert(archive_matches(Phase::Verify,Some(true),false));
 assert(archive_matches(Phase::Verify,None,true));
 assert(!archive_has(Phase::Verify,None,false));
 assert(package(fixture(Phase::Test),None,false,source()).contains_key(manifest_path()));
 assert(package(fixture(Phase::Test),Some(false),true,source()).contains_key(manifest_path()));
 assert(!unit_children(fixture(Phase::Test)).paths.contains(fixture(Phase::Test).name));
 assert(unit_children(fixture(Phase::Test)).metadata);
 assert(!path_has_processor());assert(!test_path_export_handled());
 assert(!references(split_scope(),seq![Phase::Test]).paths.contains(split_scope().name));
 assert(!references(split_scope(),seq![Phase::Verify]).paths.contains(split_scope().name));
 assert(references(split_scope(),seq![Phase::Test,Phase::Verify]).paths.contains(split_scope().name));
}
}
fn main() {}
