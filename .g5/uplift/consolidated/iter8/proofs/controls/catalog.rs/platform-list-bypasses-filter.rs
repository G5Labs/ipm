use vstd::prelude::*;
verus! {
// Major-only release versions are the declared scope; raw range is Any or exclusive upper bound.
#[verifier::opaque]
pub open spec fn insert_desc(v:int,sorted:Seq<int>)->Seq<int>
 decreases sorted.len()
{if sorted.len()==0 {seq![v]} else if v>=sorted[0] {seq![v]+sorted} else {seq![sorted[0]]+insert_desc(v,sorted.skip(1))}}
#[verifier::opaque]
pub open spec fn ordered(raw:Seq<int>)->Seq<int>
 decreases raw.len()
{if raw.len()==0{seq![]}else{insert_desc(raw[0],ordered(raw.skip(1)))}}
#[verifier::opaque]
pub open spec fn eligible(raw:Seq<int>,upper:Option<int>)->Seq<int>
 decreases raw.len()
{if raw.len()==0{seq![]}else{let rest=eligible(raw.skip(1),upper);if match upper{None=>true,Some(b)=>raw[0]<b}{seq![raw[0]]+rest}else{rest}}}
pub struct Group {pub rows:nat,pub selected:Option<int>,pub versions:Seq<int>}
pub open spec fn grouped(raw:Seq<int>,upper:Option<int>,all_versions:bool)->Group {
 let list=ordered(eligible(raw,upper));
 Group{rows:if list.len()>0{1}else{0},selected:if list.len()>0{Some(list[0])}else{None},versions:if all_versions{list}else{seq![]}}
}
pub struct Artifact {pub version:int,pub deployed:bool,pub platforms:Set<int>}
pub open spec fn compatible(a:Artifact,current:int)->bool {!a.deployed || a.platforms.contains(current)}
pub open spec fn catalog_visible(a:Artifact,current:int,show_all:bool)->bool {true}
pub open spec fn install_candidate(a:Artifact,current:int)->bool {compatible(a,current)}
pub open spec fn reported_platform(a:Artifact,current:int)->Option<int> {if a.deployed && a.platforms.contains(current){Some(current)}else{None}}
pub enum Tag {Source(int),Deployed(int,int)}
pub open spec fn tag_for(a:Artifact,selected_platform:Option<int>)->Option<Tag> {
 if a.deployed {match selected_platform{Some(p)=>Some(Tag::Deployed(a.version,p)),None=>None}}else{Some(Tag::Source(a.version))}
}
pub proof fn sort_three()
 ensures ordered(seq![1int,3,2])==seq![3int,2,1]
{
 reveal_with_fuel(ordered,4);reveal_with_fuel(insert_desc,4);
 assert(ordered(seq![1int,3,2]) =~= seq![3int,2,1]);
}
pub proof fn sort_two()
 ensures ordered(seq![1int,2])==seq![2int,1],ordered(Seq::<int>::empty())==seq![]
{
 reveal_with_fuel(ordered,3);reveal_with_fuel(insert_desc,3);
 assert(ordered(seq![1int,2]) =~= seq![2int,1]);
}
pub proof fn filter_three()
 ensures eligible(seq![1int,3,2],None)==seq![1int,3,2],eligible(seq![1int,3,2],Some(3))==seq![1int,2],eligible(seq![1int,3,2],Some(1))==seq![]
{
 reveal_with_fuel(eligible,4);
 assert(eligible(seq![1int,3,2],None) =~= seq![1int,3,2]);
 assert(eligible(seq![1int,3,2],Some(3)) =~= seq![1int,2]);
 assert(eligible(seq![1int,3,2],Some(1)) =~= Seq::<int>::empty());
}
pub proof fn list_neighbors() {
 sort_three();sort_two();filter_three();
 let any=grouped(seq![1int,3,2],None,true);let limited=grouped(seq![1int,3,2],Some(3),true);
 assert(any.rows==1 && any.selected==Some(3));assert(any.versions =~= seq![3int,2,1]);
 assert(limited.rows==1 && limited.selected==Some(2));assert(limited.versions =~= seq![2int,1]);
 assert(grouped(seq![1int,3,2],Some(1),true).rows==0);
}
pub proof fn platform_neighbors() {
 let good=Artifact{version:1,deployed:true,platforms:set![26int]};let bad=Artifact{version:1,deployed:true,platforms:set![9999int]};
 assert(catalog_visible(good,26,false));assert(install_candidate(good,26));assert(reported_platform(good,26)==Some(26));
 assert(!catalog_visible(bad,26,false));assert(!install_candidate(bad,26));assert(catalog_visible(bad,26,true));
 assert(tag_for(good,reported_platform(good,26))==Some(Tag::Deployed(1,26)));
 assert(tag_for(good,None)==None);
 assert(compatible(Artifact{version:1,deployed:false,platforms:Set::empty()},26));
}
}
fn main() {}
