use vstd::prelude::*;
verus! {
// Names are integer ranks of distinct normalized ASCII fixture names in IRIS
// $order order. BuildDependencyGraph resolution, name/version parsing, collection
// JSON encoding and filesystem writes are provider boundaries, checked natively.
pub struct RawNode {pub name:int,pub parents:Seq<int>}
pub struct Edge {pub dependency:int,pub dependent:int}
pub open spec fn node_names(rows:Seq<RawNode>)->Seq<int>
 decreases rows.len()
{if rows.len()==0{seq![]}else{seq![rows[0].name]+node_names(rows.skip(1))}}
pub open spec fn edges_for(name:int,parents:Seq<int>)->Seq<Edge>
 decreases parents.len()
{if parents.len()==0{seq![]}else{seq![Edge{dependency:name,dependent:parents[0]}]+edges_for(name,parents.skip(1))}}
pub open spec fn invert(rows:Seq<RawNode>)->Seq<Edge>
 decreases rows.len()
{if rows.len()==0{seq![]}else{edges_for(rows[0].name,rows[0].parents)+invert(rows.skip(1))}}
pub open spec fn remove(names:Seq<int>,name:int)->Seq<int>
 decreases names.len()
{if names.len()==0{seq![]}else{(if names[0]==name{seq![]}else{seq![names[0]]})+remove(names.skip(1),name)}}
pub open spec fn after(names:Seq<int>,cursor:int)->Option<int>
 decreases names.len()
{if names.len()==0{None}else if names[0]>cursor{Some(names[0])}else{after(names.skip(1),cursor)}}
pub open spec fn next(names:Seq<int>,cursor:int)->int {
 match after(names,cursor){Some(n)=>n,None=>if names.len()>0{names[0]}else{0}}
}
pub open spec fn has(names:Seq<int>,name:int)->bool
 decreases names.len()
{names.len()>0 && (names[0]==name || has(names.skip(1),name))}
pub open spec fn blocked(edges:Seq<Edge>,name:int,remaining:Seq<int>)->bool
 decreases edges.len()
{edges.len()>0 && ((edges[0].dependent==name && has(remaining,edges[0].dependency)) || blocked(edges.skip(1),name,remaining))}
pub struct Flat {pub names:Seq<int>,pub remaining:Seq<int>}
// Preserve the production cursor after each visit, including skipped nodes.
// Fuel records a finite audit bound; exhaustion is incomplete, never success.
pub open spec fn walk(remaining:Seq<int>,edges:Seq<Edge>,cursor:int,fuel:nat)->Flat
 decreases fuel
{if remaining.len()==0 || fuel==0{Flat{names:seq![],remaining}}else{
 let n=next(remaining,cursor);
 if blocked(edges,n,remaining){walk(remaining,edges,n,(fuel-1) as nat)}else{
  let rest=walk(remove(remaining,n),edges,0,(fuel-1) as nat);
  Flat{names:seq![n]+rest.names,remaining:rest.remaining}
 }
}}
pub open spec fn flatten(rows:Seq<RawNode>,fuel:nat)->Flat {walk(node_names(rows),invert(rows),0,fuel)}
pub struct Reference {pub name:int,pub version:int,pub repository:int}
pub struct Module {pub version:int,pub repository:int,pub requirements:Map<int,int>}
pub enum Field {Type,SortOrder,ReadOnly,Root,Depth,Url,Namespace,Username,Password,Token}
pub enum RepoKind {Filesystem,Remote,Oras}
pub struct Repo {pub kind:RepoKind,pub fields:Map<Field,int>}
pub open spec fn allowed(kind:RepoKind)->Set<Field> {
 let common=set![Field::Type,Field::SortOrder,Field::ReadOnly];
 match kind{RepoKind::Filesystem=>common.union(set![Field::Root,Field::Depth]),RepoKind::Remote=>common.insert(Field::Url),RepoKind::Oras=>common.union(set![Field::Url,Field::Namespace])}
}
pub open spec fn public_repo(repo:Repo)->Map<Field,int> {repo.fields.restrict(allowed(repo.kind))}
pub struct Document {pub dependencies:Map<int,Module>,pub repositories:Map<int,Map<Field,int>>,pub ok:bool}
pub open spec fn empty()->Document {Document{dependencies:Map::empty(),repositories:Map::empty(),ok:true}}
pub open spec fn add_repo(doc:Document,name:int,repos:Map<int,Repo>)->Document {
 if !repos.contains_key(name){Document{ok:false,..doc}}else if doc.repositories.contains_key(name){doc}else{
 Document{repositories:doc.repositories.insert(name,public_repo(repos[name])),..doc}}
}
pub open spec fn add_dependency(doc:Document,r:Reference,installed:Map<int,Module>,repos:Map<int,Repo>)->Document {
 if !doc.ok || !installed.contains_key(r.name){Document{ok:false,..doc}}else{
  let m=installed[r.name];
  add_repo(Document{dependencies:doc.dependencies.insert(r.name,m),..doc},m.repository,repos)
 }
}
pub open spec fn write_dependencies(doc:Document,flat:Seq<Reference>,installed:Map<int,Module>,repos:Map<int,Repo>)->Document
 decreases flat.len()
{if flat.len()==0{doc}else{write_dependencies(add_dependency(doc,flat[0],installed,repos),flat.skip(1),installed,repos)}}
pub open spec fn build(flat:Seq<Reference>,installed:Map<int,Module>,repos:Map<int,Repo>,base_repo:int)->Document {
 let doc=write_dependencies(empty(),flat,installed,repos);
 if base_repo==0{doc}else{add_repo(doc,base_repo,repos)}
}
pub open spec fn refs(names:Seq<int>)->Seq<Reference>
 decreases names.len()
{if names.len()==0{seq![]}else{seq![Reference{name:names[0],version:999,repository:999}]+refs(names.skip(1))}}
// IRIS array-of-objects is keyed by Name. Its natural-key serialization order is
// a provider contract, separate from the graph's cursor order. Native reverse-
// name JSON checks must confirm this before correspondence is credited.
pub open spec fn json_keys(natural_keys:Seq<int>,doc:Document)->Seq<int>
 decreases natural_keys.len()
{if natural_keys.len()==0{seq![]}else{let tail=json_keys(natural_keys.skip(1),doc);if doc.dependencies.contains_key(natural_keys[0]){seq![natural_keys[0]]+tail}else{tail}}}
pub open spec fn k_rows()->Seq<RawNode> {seq![RawNode{name:1,parents:seq![3int,5]},RawNode{name:2,parents:seq![3int]},RawNode{name:3,parents:seq![]},RawNode{name:5,parents:seq![]}]}
pub open spec fn l_rows()->Seq<RawNode> {seq![RawNode{name:1,parents:seq![3int,5]},RawNode{name:2,parents:seq![3int,6]},RawNode{name:3,parents:seq![4int]},RawNode{name:4,parents:seq![]},RawNode{name:5,parents:seq![6int]},RawNode{name:6,parents:seq![]}]}
pub open spec fn reverse_rows()->Seq<RawNode> {seq![RawNode{name:1,parents:seq![]},RawNode{name:2,parents:seq![1int]}]}
pub open spec fn cursor_rows()->Seq<RawNode> {seq![RawNode{name:1,parents:seq![]},RawNode{name:2,parents:seq![]},RawNode{name:3,parents:seq![1int]},RawNode{name:4,parents:seq![]}]}
pub open spec fn installed()->Map<int,Module> {map![1int=>Module{version:1,repository:10,requirements:Map::empty()},2int=>Module{version:1,repository:10,requirements:Map::empty()},3int=>Module{version:1,repository:10,requirements:map![1int=>11int,2int=>11int]},4int=>Module{version:1,repository:10,requirements:map![3int=>11int]},5int=>Module{version:1,repository:10,requirements:map![1int=>11int]},6int=>Module{version:1,repository:10,requirements:map![2int=>11int,5int=>11int]}]}
pub open spec fn repos()->Map<int,Repo> {map![10int=>Repo{kind:RepoKind::Filesystem,fields:map![Field::Type=>1int,Field::Root=>100int,Field::Depth=>0int,Field::ReadOnly=>0int]},20int=>Repo{kind:RepoKind::Filesystem,fields:map![Field::Type=>1int,Field::Root=>200int,Field::Depth=>0int,Field::ReadOnly=>0int]}]}
pub open spec fn k_doc()->Document {build(refs(flatten(k_rows(),32).names),installed(),repos(),20)}
pub open spec fn l_doc()->Document {build(refs(flatten(l_rows(),64).names),installed(),repos(),20)}
pub open spec fn duplicate_doc()->Document {build(refs(seq![1int,2,3,5,1]),installed(),repos(),20)}
pub open spec fn reverse_store()->Map<int,Module> {map![1int=>Module{version:1,repository:30,requirements:map![2int=>11int]},2int=>Module{version:1,repository:30,requirements:Map::empty()}]}
pub open spec fn reverse_repos()->Map<int,Repo> {map![30int=>Repo{kind:RepoKind::Filesystem,fields:map![Field::Type=>1int,Field::Root=>300int,Field::Depth=>0int,Field::ReadOnly=>0int]}]}
pub open spec fn reverse_doc()->Document {build(refs(flatten(reverse_rows(),8).names),reverse_store(),reverse_repos(),30)}
pub open spec fn credentials()->Repo {Repo{kind:RepoKind::Remote,fields:map![Field::Type=>2int,Field::Url=>80int,Field::ReadOnly=>0int,Field::Username=>91int,Field::Password=>92int,Field::Token=>93int]}}
pub open spec fn credential_doc()->Document {build(seq![],Map::empty(),map![40int=>credentials()],40)}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub proof fn projection_drops_credentials(r:Repo)
 ensures !public_repo(r).contains_key(Field::Username),!public_repo(r).contains_key(Field::Password),!public_repo(r).contains_key(Field::Token)
{}
pub proof fn installed_identity_drives_entry(d:Document,r:Reference,store:Map<int,Module>,rs:Map<int,Repo>)
 requires d.ok,store.contains_key(r.name)
 ensures add_dependency(d,r,store,rs).dependencies.contains_key(r.name),add_dependency(d,r,store,rs).dependencies[r.name]==store[r.name]
{}
pub proof fn graph_neighbors() {
 assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
 assert(flatten(k_rows(),32).remaining.len()==0) by (compute);
 assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
 assert(flatten(l_rows(),64).remaining.len()==0) by (compute);
 assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
 assert(flatten(cursor_rows(),24).names==seq![2int,3,4,1]) by (compute);
 assert(flatten(reverse_rows(),1).remaining.len()>0) by (compute);
}
pub proof fn writer_neighbors()
 ensures json_keys(seq![1int,2,3,5],k_doc())==seq![1int,2,3,5],
 json_keys(seq![1int,2,3,5],duplicate_doc())==seq![1int,2,3,5],
 json_keys(seq![1int,2,3,4,5,6],l_doc())==seq![1int,2,3,4,5,6],
 json_keys(seq![1int,2],reverse_doc())==seq![1int,2]
{
 reveal_with_fuel(write_dependencies,8);reveal_with_fuel(refs,8);reveal_with_fuel(json_keys,8);
 assert(flatten(k_rows(),32).names==seq![1int,2,3,5]) by (compute);
 assert(flatten(l_rows(),64).names==seq![1int,2,3,4,5,6]) by (compute);
 assert(flatten(reverse_rows(),8).names==seq![2int,1]) by (compute);
 let k=k_doc();let l=l_doc();let dup=duplicate_doc();let rev=reverse_doc();
 assert(k.ok);assert(l.ok);assert(dup.ok);assert(rev.ok);
 assert(k.dependencies.dom()==set![1int,2,3,5]);assert(l.dependencies.dom()==set![1int,2,3,4,5,6]);
 assert(dup.dependencies.dom()==k.dependencies.dom());assert(dup.dependencies[1].version==1);
 assert(k.dependencies[1].version==1);assert(k.dependencies[3].requirements[1]==11);
 assert(k.repositories.dom()==set![10int,20]);
 assert(json_keys(seq![1int,2],rev)==seq![1int,2]);assert(rev.repositories.dom()==set![30int]);
 assert(!public_repo(credentials()).contains_key(Field::Token));assert(public_repo(credentials()).contains_key(Field::Url));assert(public_repo(credentials())[Field::Url]==80);
 assert(!build(refs(seq![99int]),installed(),repos(),20).ok);
 assert(!add_repo(empty(),99,repos()).ok);
 assert(json_keys(seq![1int,2,3,5],k)==seq![1int,2,3,5]);
 assert(json_keys(seq![1int,2,3,5],dup)==seq![1int,2,3,5]);
 assert(json_keys(seq![1int,2,3,4,5,6],l)==seq![1int,2,3,4,5,6]);
}
}
fn main(){}
