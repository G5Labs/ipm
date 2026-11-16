// Reuse the inherited IPM-specific provider/state contract without changing it.
#[path="../../../../../../../playbook-uplift/models/verus/ns_mapping.rs"] pub mod inherited;
pub use inherited::*;
use vstd::prelude::*;
verus! {
// This command boundary covers map/repo requests with one existing empty target
// and successful configuration/activation providers. Installation, arbitrary
// provider errors and batches with local installs are not newly claimed.
pub struct Out {pub state:S,pub ok:bool}
pub open spec fn request(s:S,map:bool,repos:bool,preview:bool)->Out
 recommends map || repos
{
 if false {Out{state:s,ok:false}}
 else {Out{state:if preview{s}else{enable_map(s,1,set![2int],repos)},ok:true}}
}
pub open spec fn unmap_request(s:S,repos_only:bool)->Out {Out{state:unmap(s,set![2int],repos_only),ok:true}}
pub open spec fn first()->S {request(w4(),true,true,false).state}
pub open spec fn own_target()->S {S{ns:first().ns.insert(2,Ns{rows:true,..first().ns[2]})}}
pub open spec fn own_skip()->S {enable_map(own_target(),1,set![2int],true)}
pub open spec fn own_cleared()->S {S{ns:own_skip().ns.insert(2,Ns{rows:false,..own_skip().ns[2]})}}
pub open spec fn second()->S {request(first(),true,true,false).state}
pub open spec fn repos_only()->S {unmap_request(second(),true).state}
pub open spec fn third()->S {enable_map(repos_only(),1,set![2int],true)}
pub open spec fn unmapped()->S {unmap_request(third(),false).state}
// Preview and argument refusal are caller behavior absent from the inherited
// state-only model. Native commands and unchanged maps exercise both neighbors.
pub proof fn preview_and_refusal(s:S) {
 assert(request(s,false,true,false).state==s);assert(!request(s,false,true,false).ok);
 assert(request(s,true,true,true).state==s);assert(request(s,true,true,true).ok);
}
// Version units are fixture ranks after the inherited empty-string and version
// parsers, not a new general SemVer implementation: current10 >= required1.
pub open spec fn requirement(s:S,n:int,minimum:int)->bool {
 let actual=match check_ipm_version(s,n){Some(v)=>v,None=>0};
 actual>=minimum
}
pub struct Load {pub ok:bool,pub row:bool}
pub open spec fn new_normal_load(s:S,n:int,minimum:int)->Load {
 let ok=requirement(s,n,minimum);Load{ok,row:ok}
}
pub open spec fn repo_catalog(s:S)->Map<int,Set<int>> {map![1int=>set![7int],2int=>if s.ns[2].rows {set![8int]}else{Set::empty()}]}
pub open spec fn repo_visible(s:S,n:int)->bool {
 let db=glo_dest(s.ns[n].act,n);
 repo_catalog(s).contains_key(db) && repo_catalog(s)[db].contains(7)
}
pub open spec fn own_visible(s:S,n:int)->bool {
 let db=glo_dest(s.ns[n].act,n);
 repo_catalog(s).contains_key(db) && repo_catalog(s)[db].contains(8)
}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub proof fn known_mapping(s:S)
 requires s.ns.dom() =~= set![1int,2int],s.ns[1]==ns_local(1,10),s.ns[2].rec==None,pkg_dest(s.ns[2].act)==1
 ensures listed(s,2)
{
 assert(is_source(s,1));assert(!is_source(s,2));
 assert forall|y:int| #[trigger] is_source(s,y) && pkg_dest(s.ns[y].act)==pkg_dest(s.ns[2].act) implies y<=1 by {
  assert(s.ns.dom().contains(y));assert(y==1 || y==2);
 }
 assert(mapped_from(s,2,1));
}
pub proof fn selected_namespace_neighbors()
 ensures repos_only().ns[2].cfg.repo==None,repos_only().ns[2].cfg.pkg==Some(1int),
 unmapped().ns[2].cfg.pkg==None,unmapped().ns[2].cfg.rtn==Some(1int),unmapped().ns[2].cfg.repo==None,
 has_ipm(unmapped(),2),!repo_visible(first(),2),repo_visible(second(),2),!repo_visible(repos_only(),2),!repo_visible(unmapped(),2)
{
 nsp_witness_repos_second_run();
 assert(first().ns[2].cfg.pkg==Some(1int));
 assert(first().ns[2].cfg.rtn==Some(1int));
 assert(first().ns[2].cfg.repo==None);
 assert(second().ns[2].cfg.repo==Some(1int));
 assert(own_skip().ns[2].cfg.repo==None);assert(!repo_visible(own_skip(),2));assert(own_visible(own_skip(),2));
 assert(!own_visible(own_cleared(),2));
 assert(!repo_visible(first(),2));assert(repo_visible(second(),2));
 known_mapping(second());
 assert(repos_only().ns[2].cfg.repo==None);
 assert(repos_only().ns[2].cfg.pkg==Some(1int));
 known_mapping(third());
 assert(unmapped().ns[2].cfg.pkg==None);
 assert(unmapped().ns[2].cfg.rtn==Some(1int));
 assert(unmapped().ns[2].cfg.repo==None);
 assert(!repo_visible(repos_only(),2));assert(!repo_visible(unmapped(),2));
 assert(has_ipm(unmapped(),2));
 assert(unmap_request(third(),false).ok);
 assert(requirement(first(),1,1));assert(new_normal_load(first(),1,1).ok);assert(new_normal_load(first(),1,1).row);assert(!requirement(first(),2,1));
 assert(!new_normal_load(first(),2,1).ok);assert(!new_normal_load(first(),2,1).row);
 assert(!new_normal_load(unmapped(),2,1).ok);assert(!new_normal_load(unmapped(),2,1).row);
}
}
fn main(){}
