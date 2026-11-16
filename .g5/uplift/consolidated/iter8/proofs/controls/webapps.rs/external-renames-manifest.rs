use vstd::prelude::*;
verus! {
pub enum Key { Authentication, Dispatch, Namespace, Name, Path, NamespaceDefault, AccessTimeout, RefreshTimeout, JwtEnabled, Description }
pub type Props=Map<Key,int>;
pub open spec fn value(p:Props,k:Key)->int {if p.contains_key(k){p[k]}else{0}}
pub open spec fn skipped(k:Key)->bool {k==Key::Namespace || k==Key::Name || k==Key::Path || k==Key::NamespaceDefault}
pub open spec fn generated_props(old:Props,defaults:Props,prompt:Props)->Props {
 let keys=old.dom().filter(|k:Key| !skipped(k) && value(old,k)!=value(defaults,k)).union(prompt.dom()).insert(Key::Authentication);
 Map::new(keys,|k:Key| if prompt.contains_key(k){prompt[k]}else if k==Key::Authentication {if old.contains_key(k){old[k]}else{value(defaults,k)}}else{old[k]})
}
pub struct Jwt {pub enable:int,pub access:int,pub refresh:int}
pub open spec fn jwt_prompt(raw:Jwt)->Props {map![Key::JwtEnabled=>raw.enable,Key::AccessTimeout=>raw.access,Key::RefreshTimeout=>raw.refresh]}
pub open spec fn prefix(a:Seq<int>,b:Seq<int>)->bool {a.len()<=b.len() && (forall|i:int|0<=i<a.len() ==> a[i]==b[i])}
pub enum PathExpr { Literal(Seq<int>), Csp(Seq<int>) }
pub open spec fn manifest_path(raw:Seq<int>,csp_root:Seq<int>)->PathExpr {
 if prefix(csp_root,raw) {PathExpr::Csp(raw.skip(csp_root.len() as int))} else {PathExpr::Literal(raw)}
}
pub open spec fn evaluate_path(expr:PathExpr,csp_root:Seq<int>)->Seq<int> {
 match expr {PathExpr::Literal(p)=>p,PathExpr::Csp(s)=>csp_root+s}
}
pub open spec fn generated_path(dispatch:int,path:Option<Seq<int>>,csp_root:Seq<int>)->Option<PathExpr> {
 if dispatch!=0 {None} else {match path {Some(p)=>Some(manifest_path(p,csp_root)),None=>None}}
}
pub open spec fn filecopy_target(generated:Option<PathExpr>)->Option<PathExpr> {generated}
pub open spec fn copy_one(before:Map<Seq<int>,int>,source:Map<int,int>,name:int,target:Option<PathExpr>,csp_root:Seq<int>)->Map<Seq<int>,int> {
 match target {Some(t)=>if source.contains_key(name){before.insert(evaluate_path(t,csp_root)+seq![name],source[name])}else{before},None=>before}
}
pub open spec fn webapp_export_handled()->bool {true}
pub open spec fn package_app(has_source_document:bool)->bool {webapp_export_handled() || has_source_document}
pub open spec fn uninstall(apps:Map<int,Seq<int>>,name:int)->Map<int,Seq<int>> {apps.remove(name)}
pub open spec fn reinstall(apps:Map<int,Seq<int>>,name:int,archived:PathExpr,csp_root:Seq<int>)->Map<int,Seq<int>> {apps.insert(name,evaluate_path(archived,csp_root))}
pub struct Archive {pub path:PathExpr}
pub open spec fn package_webapp(declared:PathExpr,has_source_document:bool)->Option<Archive> {
 if package_app(has_source_document){Some(Archive{path:declared})}else{None}
}
pub open spec fn publish_archive(registry:Map<int,Archive>,key:int,archive:Archive,provider_ok:bool)->Map<int,Archive> {
 if provider_ok {registry.insert(key,archive)}else{registry}
}
pub open spec fn install_registry(apps:Map<int,Seq<int>>,registry:Map<int,Archive>,key:int,name:int,csp_root:Seq<int>)->Map<int,Seq<int>> {
 if registry.contains_key(key){reinstall(apps,name,registry[key].path,csp_root)}else{apps}
}
pub open spec fn package_roundtrip(before:Map<int,Seq<int>>,name:int,declared:PathExpr,csp_root:Seq<int>)->Map<int,Seq<int>> {
 let packaged=package_webapp(declared,false);
 match packaged {Some(archive)=>install_registry(uninstall(before,name),publish_archive(Map::empty(),name,archive,true),name,name,csp_root),None=>before}
}
pub proof fn package_protocol() {
 let before=map![7int=>seq![1int,2,3]];
 let after=package_roundtrip(before,7,PathExpr::Csp(seq![3int]),seq![1int,2]);
 assert(after.contains_key(7));assert(after[7] =~= seq![1int,2,3]);
 let moved=package_roundtrip(before,7,PathExpr::Csp(seq![3int]),seq![8int,9]);
 assert(moved[7] =~= seq![8int,9,3]);
}
pub open spec fn published_key(internal:int,external:int,use_external:bool)->int {if use_external{external}else{internal}}
pub open spec fn archive_manifest_name(internal:int,external:int,use_external:bool)->int {external}
pub open spec fn b(v:bool)->int {if v{1}else{0}}
pub proof fn generated_neighbors() {
 let defaults=map![Key::Authentication=>1int,Key::AccessTimeout=>0int,Key::RefreshTimeout=>0int,Key::JwtEnabled=>0int];
 let props=map![Key::Authentication=>1int,Key::Namespace=>99int,Key::Dispatch=>7int,Key::AccessTimeout=>600int];
 let p=generated_props(props,defaults,jwt_prompt(Jwt{enable:1,access:120,refresh:1800}));
 assert(p.contains_key(Key::Authentication) && p[Key::Authentication]==1);
 assert(!p.contains_key(Key::Namespace));assert(p[Key::Dispatch]==7);
 assert(p[Key::AccessTimeout]==120 && p[Key::RefreshTimeout]==1800 && p[Key::JwtEnabled]==1);
 assert(generated_path(7,Some(seq![1int,2,3]),seq![1int,2])==None);
 let path=generated_path(0,Some(seq![1int,2,3]),seq![1int,2]);
 assert(path.is_some());assert(matches!(path.unwrap(),PathExpr::Csp(_)));
 assert(evaluate_path(filecopy_target(path).unwrap(),seq![8int,9]) =~= seq![8int,9,3]);
}
pub proof fn package_roundtrip_neighbors() {
 assert(package_app(false));
 let files=copy_one(Map::empty(),map![5int=>77int],5,Some(PathExpr::Csp(seq![3int])),seq![1int,2]);
 assert(evaluate_path(PathExpr::Csp(seq![3int]),seq![1int,2])+seq![5int] =~= seq![1int,2,3,5]);
 assert(files[seq![1int,2,3,5]]==77);
 assert(!copy_one(Map::empty(),map![5int=>77int],5,None,seq![1int,2]).contains_key(seq![1int,2,3,5]));
 let name=7int;let expr=PathExpr::Csp(seq![3int]);let root=seq![1int,2];
 let installed=map![name=>evaluate_path(expr,root)];
 let removed=uninstall(installed,name);assert(!removed.contains_key(name));
 let after=reinstall(removed,name,expr,root);assert(after[name] =~= seq![1int,2,3]);
 let relocated=reinstall(removed,name,expr,seq![8int,9]);assert(relocated[name] =~= seq![8int,9,3]);
 assert(published_key(1,2,true)==2 && published_key(1,2,false)==1);
 assert(archive_manifest_name(1,2,true)==1);
}
}
fn main() {}
