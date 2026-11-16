use vstd::prelude::*;
verus! {
pub enum Token { Literal(int), Parameter(int) }
pub open spec fn replace(tokens: Seq<Token>, key: int, value: Seq<Token>) -> Seq<Token>
 decreases tokens.len()
{
 if tokens.len()==0 { seq![] } else {
  (match tokens[0] { Token::Parameter(k) => if k==key { value } else { seq![tokens[0]] }, _=>seq![tokens[0]] })
  + replace(tokens.skip(1),key,value)
 }
}
// ICU replacement subset for ordinary text and $0. Other dollar markers fail in this no-group pattern.
pub open spec fn replacement(raw: Seq<int>, matched: Seq<int>) -> Option<Seq<int>>
 decreases raw.len()
{
 if raw.len()==0 {Some(seq![])} else if raw[0]==36 {
  if raw.len()>=2 && raw[1]==48 {match replacement(raw.skip(2),matched) {Some(t)=>Some(matched+t),None=>None}} else {None}
 } else {match replacement(raw.skip(1),matched) {Some(t)=>Some(seq![raw[0]]+t),None=>None}}
}
pub open spec fn sweep(tokens: Seq<Token>, keys: Seq<int>, values: Map<int,Seq<Token>>) -> Seq<Token>
 decreases keys.len()
{
 if keys.len()==0 { tokens } else { sweep(replace(tokens,keys[0],values[keys[0]]),keys.skip(1),values) }
}
pub open spec fn split(raw: Seq<int>, delimiter: int) -> Seq<Seq<int>>
 decreases raw.len()
{
 if raw.len()==0 { seq![Seq::<int>::empty()] } else {
  let rest=split(raw.skip(1),delimiter);
  if raw[0]==delimiter { seq![Seq::<int>::empty()]+rest }
  else { seq![seq![raw[0]]+rest[0]]+rest.skip(1) }
 }
}
pub open spec fn delimiter(configured: Option<int>) -> int { match configured { Some(c)=>c, None=>44 } }
pub open spec fn csp_directory(explicit: Option<int>, dispatch: Option<int>, root_plus_resource: int) -> int {
 match explicit { Some(d)=>d, None=>match dispatch {Some(_)=>-1, None=>root_plus_resource} }
}
// Root and resource are separate normalized path segments, not a precomputed answer.
pub open spec fn csp_path(explicit: Option<Seq<int>>, dispatch: bool, root: Seq<int>, resource: Seq<int>) -> Option<Seq<int>> {
 match explicit {Some(d)=>Some(d),None=>if dispatch {None} else {Some(root+resource)}}
}
pub open spec fn csp_after_before(inferred: int) -> Option<int> { if inferred == -1 { None } else {Some(inferred)} }
pub enum Detail { Dispatch(int), Wsgi(int), Files(int), Empty }
pub open spec fn detail(dispatch: Option<int>, wsgi: Option<int>, path: Option<int>, directory: Option<int>) -> Detail {
 match dispatch {Some(d)=>Detail::Dispatch(d),None=>match wsgi {Some(w)=>Detail::Wsgi(w),None=>match directory {Some(p)=>Detail::Files(p),None=>match directory {Some(d)=>Detail::Files(d),None=>Detail::Empty}}}}
}
// Empty values and absent old properties are both the empty string, encoded as 0.
pub open spec fn differs(old: Map<int,int>, declared: Map<int,int>) -> bool {
 exists|k:int| declared.contains_key(k) && declared[k] != (if old.contains_key(k) {old[k]} else {0})
}
pub proof fn change_witness(old:Map<int,int>,declared:Map<int,int>,k:int)
 requires declared.contains_key(k),declared[k]!=(if old.contains_key(k){old[k]}else{0})
 ensures differs(old,declared)
{}
pub open spec fn app_status(old: Map<int,int>, declared: Map<int,int>, modify_ok: bool) -> bool {
 if differs(old,declared) { modify_ok } else {true}
}
pub open spec fn cors_calls(origins: Seq<Seq<int>>, provider: spec_fn(Seq<int>)->bool) -> Seq<Seq<int>>
 decreases origins.len()
{
 if origins.len()==0 {seq![]} else if !provider(origins[0]) {seq![origins[0]]}
 else {seq![origins[0]]+cors_calls(origins.skip(1),provider)}
}
pub open spec fn cors_ok(origins: Seq<Seq<int>>, provider: spec_fn(Seq<int>)->bool) -> bool
 decreases origins.len()
{
 origins.len()==0 || (provider(origins[0]) && cors_ok(origins.skip(1),provider))
}
pub struct Stamp { pub saved: bool, pub value: int, pub ok: bool }
pub open spec fn stamp(old: int, hash: Seq<int>, open_ok: bool, save_ok: bool) -> Stamp {
 if hash.len()==0 { Stamp{saved:false,value:old,ok:false} }
 else if !open_ok {Stamp{saved:false,value:old,ok:false}}
 else {Stamp{saved:save_ok,value:if save_ok {hash[0]} else {old},ok:save_ok}}
}
pub struct Reload { pub root: int, pub env_called: bool, pub reads_manifest: bool }
pub open spec fn reload(explicit: int, developer: bool, stored_root: int, env_ok: bool) -> Reload {
 let root=if explicit==0 && developer {stored_root} else {explicit};
 Reload{root,env_called:true,reads_manifest:env_ok && root!=0}
}
pub open spec fn reload_status(explicit:int,developer:bool,stored_root:int,env_ok:bool,manifest_ok:bool)->bool {
 env_ok && (!reload(explicit,developer,stored_root,env_ok).reads_manifest || manifest_ok)
}
pub open spec fn config_get_called(configurable: Set<int>, key: int) -> bool {configurable.contains(key)}
pub enum ConfigLine { NotFound(int), Value(int,int) }
pub open spec fn config_line(configurable:Set<int>,key:int,effective:int)->ConfigLine {
 if config_get_called(configurable,key) {ConfigLine::Value(key,effective)} else {ConfigLine::NotFound(key)}
}
pub proof fn config_output_neighbors() {
 assert(config_line(set![1int],1,7)==ConfigLine::Value(1,7));
 assert(config_line(set![1int],2,7)==ConfigLine::NotFound(2));
}
pub open spec fn b(x: bool)->int {if x {1}else{0}}
pub open spec fn system_target(raw: Seq<int>, phase: int) -> Option<Seq<int>> {
 let parts=split(raw,46);
 if phase==0 && parts.len()>1 && parts[0].len()==3 && parts[0][0]==67 && parts[0][1]==83 && parts[0][2]==80 && parts[1].len()>0 {Some(parts[1])} else {None}
}
pub open spec fn system_value(old: int, raw: Seq<int>, phase: int, value: int) -> int {
 if system_target(raw,phase).is_some() {value} else {old}
}
pub proof fn replacement_neighbors() {
 reveal_with_fuel(replacement,5);
 assert(replacement(seq![123int,36,66,125],seq![123int,36,65,125])==None);
 assert(replacement(seq![120int],seq![]).is_some());
}
pub proof fn system_neighbors() {
 reveal_with_fuel(split,6);
 let raw=seq![67int,83,80,46,84];
 let parts=split(raw,46);
 assert(parts.len()==2);
 assert(parts[0] =~= seq![67int,83,80]);
 assert(parts[1] =~= seq![84int]);
 assert(system_target(raw,0).is_some());assert(!system_target(raw,1).is_some());
 assert(system_value(9,raw,0,123)==123);assert(system_value(123,raw,1,0)==123);
}
pub proof fn parameter_neighbors() {
 reveal_with_fuel(replace,10); reveal_with_fuel(sweep,10); reveal_with_fuel(split,10); reveal_with_fuel(cors_calls,10); reveal_with_fuel(cors_ok,10);
 let forward=sweep(seq![Token::Parameter(1)],seq![1int,2],map![1int=>seq![Token::Parameter(2)],2int=>seq![Token::Literal(10)]]);
 assert(forward =~= seq![Token::Literal(10)]);
 let backward=sweep(seq![Token::Parameter(2)],seq![1int,2],map![1int=>seq![Token::Literal(10)],2int=>seq![Token::Parameter(1)]]);
 assert(backward =~= seq![Token::Parameter(1)]);
}
pub proof fn list_neighbors() {
 reveal_with_fuel(replace,10); reveal_with_fuel(sweep,10); reveal_with_fuel(split,10); reveal_with_fuel(cors_calls,10); reveal_with_fuel(cors_ok,10);
 let raw=seq![97int,44,98,124,99];
 let comma=split(raw,delimiter(None));let pipe=split(raw,delimiter(Some(124)));
 assert(comma.len()==2 && comma[0].len()==1);
 assert(pipe.len()==2 && pipe[0].len()==3);
 assert(pipe[0] =~= seq![97int,44,98]);
}
pub proof fn directory_and_reader_neighbors() {
 assert(csp_directory(None,None,10)==10);assert(csp_directory(None,Some(1),10)==-1);
 assert(csp_directory(Some(20),Some(1),10)==20);
 assert(csp_after_before(-1)==None);
 assert(csp_path(None,false,seq![1int,2],seq![3int]).unwrap() =~= seq![1int,2,3]);
 assert(csp_path(None,true,seq![1int,2],seq![3int])==None);
 assert(detail(Some(1),Some(2),Some(3),Some(4))==Detail::Dispatch(1));
 assert(detail(None,Some(2),Some(3),Some(4))==Detail::Wsgi(2));
 assert(detail(None,None,Some(3),Some(4))==Detail::Files(3));
 assert(detail(None,None,None,Some(4))==Detail::Files(4));
 assert(detail(None,None,None,None)==Detail::Empty);
}
pub proof fn status_neighbors() {
 let old=map![1int=>10int];let equal=map![1int=>10int];let changed=map![1int=>20int];
 assert(!differs(old,equal));assert(app_status(old,equal,false));
 assert(differs(old,changed)) by {assert(changed.contains_key(1) && changed[1] != (if old.contains_key(1) {old[1]} else {0}));}
 assert(!app_status(old,changed,false));assert(app_status(old,changed,true));
 assert(!differs(old,map![2int=>0int]));assert(app_status(old,map![2int=>0int],false));
}
pub proof fn cors_neighbors() {
 reveal_with_fuel(replace,10); reveal_with_fuel(sweep,10); reveal_with_fuel(split,10); reveal_with_fuel(cors_calls,10); reveal_with_fuel(cors_ok,10);
 let origins=seq![seq![1int],seq![2int],seq![3int]];
 let all=|o:Seq<int>| true;
 let bad=|o:Seq<int>| o.len()!=1 || o[0]!=2;
 assert(cors_calls(origins,all).len()==3);assert(cors_ok(origins,all));
 assert(cors_calls(origins,bad).len()==2);assert(!cors_ok(origins,bad));
}
pub proof fn hash_reload_config_neighbors() {
 assert(!stamp(42,seq![],true,true).saved);assert(!stamp(42,seq![],true,true).ok);assert(stamp(42,seq![],true,true).value==42);
 assert(stamp(42,seq![99int],true,true).saved && stamp(42,seq![99int],true,true).value==99);
 assert(!stamp(42,seq![99int],true,false).ok);
 assert(reload(0,false,10,true).env_called && !reload(0,false,10,true).reads_manifest);
 assert(reload(0,true,10,true).root==10 && reload(0,true,10,true).reads_manifest);
 assert(!reload(0,true,10,false).reads_manifest);
 assert(reload(20,true,10,true).root==20);
 assert(reload_status(0,false,10,true,false));assert(!reload_status(0,true,10,true,false));assert(!reload_status(0,false,10,false,true));
 assert(config_get_called(set![1int],1));assert(!config_get_called(set![1int],2));
}
}
fn main() {}
