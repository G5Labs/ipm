use vstd::prelude::*;
verus! {
// Selected ordinary directory workflow: one non-mapped, non-projection
// persistent class, no dependents/locks, successful lifecycle and SDK providers.
// XML/class compilation/storage/SQL semantics remain provider ports qualified
// by the native fixture. Rows are keyed by the fixture's unique Token property,
// never by physical IRIS object IDs. No archive/delivery behavior is modeled.
pub type Text=Seq<char>;
pub type Path=Seq<Text>;
pub type Params=Map<Path,int>;
pub type Rows=Map<Text,int>;
pub enum Case {Plain,Hook}
pub enum Flag {Default,Purge,Configure}
pub struct Callback {pub called:nat,pub purge:int,pub top:int,pub nested:int,pub after:nat}
pub struct World {pub module:Option<Case>,pub class:bool,pub rows:Rows,pub callback:Option<Callback>}
pub struct Read {pub count:int,pub value:int}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub open spec fn token()->Text {seq!['s','e','n','t','i','n','e','l']}
pub open spec fn top_name()->Text {seq!['P','u','r','g','e']}
pub open spec fn configure_name()->Text {seq!['C','o','n','f','i','g','u','r','e']}
pub open spec fn dotted_name()->Text {configure_name()+seq!['.']+top_name()}
pub open spec fn top_key()->Path {seq![top_name()]}
pub open spec fn nested_key()->Path {seq![configure_name(),top_name()]}
// CLI.SetData skips empty dot segments. This models its owned key construction,
// not command tokenization/value parsing or the unrelated custom-prefix branch.
pub open spec fn split_from(s:Text,i:int,part:Text)->Path
 recommends 0<=i<=s.len()
 decreases s.len()-i
{
 if i>=s.len() {if part.len()==0{seq![]}else{seq![part]}}
 else if s[i]=='.' {(if part.len()==0{seq![]}else{seq![part]})+split_from(s,i+1,seq![])}
 else {split_from(s,i+1,part.push(s[i]))}
}
pub open spec fn data_path(s:Text)->Path {split_from(s,0,seq![])}
pub open spec fn set_data(old:Params,name:Text,value:int)->Params {old.insert(data_path(name),value)}
// Modifier dataAlias is the literal "Purge"; explicit custom -DConfigure.Purge=1
// provides the dotted name. The selected lexer/value-provider inputs are native
// qualified. Main/Storage merge this data without rewriting these two keys.
pub open spec fn command_data(f:Flag)->Params {
 match f {Flag::Default=>Map::empty(),Flag::Purge=>set_data(Map::empty(),top_name(),1),Flag::Configure=>set_data(Map::empty(),dotted_name(),1)}
}
pub open spec fn get(p:Params,key:Path)->int {if p.contains_key(key){p[key]}else{0}}
pub open spec fn has_installer(c:Case)->bool {match c {Case::Plain=>false,Case::Hook=>true}}
pub open spec fn seeded_rows()->Rows {map![token()=>271int]}
pub open spec fn seeded(c:Case)->World {World{module:Some(c),class:true,rows:seeded_rows(),callback:None}}
// Successful %DeleteExtent empties this selected extent, leaving the compiled
// class in place. This is an SDK provider relation, not an implemented repair.
pub open spec fn delete_extent(w:World)->World {w}
// Base.%Unconfigure reads Configure.Purge and merges pParams into vars. The
// selected supported fixture callback records both keys and deletes its extent
// iff its actual pPurge argument is nonzero. Other installer bodies are outside.
pub open spec fn prior_called(w:World)->nat {match w.callback{Some(c)=>c.called,None=>0}}
pub open spec fn prior_after(w:World)->nat {match w.callback{Some(c)=>c.after,None=>0}}
pub open spec fn unconfigure(w:World,p:Params)->World
 recommends w.module.is_some()
{
 if !has_installer(w.module.unwrap()){w}else{
  let purge=get(p,nested_key());
  let changed=if purge!=0{delete_extent(w)}else{w};
  World{callback:Some(Callback{called:prior_called(w)+1,purge,top:get(p,top_key()),nested:get(p,nested_key()),after:prior_after(w)+1}),..changed}
 }
}
// Successful OBJ.Delete without the extent qualifier removes the selected
// compiled class/schema while preserving its data. No failure/locking claim.
pub open spec fn delete_class(w:World)->World {World{class:false,..w}}
pub open spec fn uninstall(w:World,f:Flag)->World
 recommends w.module.is_some()
{
 let after_hook=unconfigure(w,command_data(f));
 let after_class=delete_class(after_hook);
 World{module:None,..after_class}
}
// Same stable class/schema, ordinary directory reload and no data-mutating
// install hook. Actual compile/storage association is a qualified provider.
pub open spec fn reload(w:World,c:Case)->World {World{module:Some(c),class:true,..w}}
pub open spec fn removed(c:Case,f:Flag)->World {uninstall(seeded(c),f)}
pub open spec fn after(c:Case,f:Flag)->World {reload(removed(c,f),c)}
// SQL WHERE Token='sentinel' on the unique Token index; absent rows have the
// coalesce(max(Val),0) value0 used by the actual reader. Other rows are framed.
pub open spec fn row_count(rows:Rows)->int {if rows.contains_key(token()){1}else{0}}
pub open spec fn row_value(rows:Rows)->int {if rows.contains_key(token()){rows[token()]}else{0}}
pub open spec fn read(w:World)->Option<Read> {
 if w.class{Some(Read{count:row_count(w.rows),value:row_value(w.rows)})}else{None}
}
pub open spec fn count(w:World)->int recommends w.class {read(w).unwrap().count}
pub open spec fn value(w:World)->int recommends w.class {read(w).unwrap().value}
pub open spec fn observed_callback(w:World)->Callback recommends w.callback.is_some(){w.callback.unwrap()}
pub open spec fn sdk_before()->World {seeded(Case::Plain)}
pub open spec fn sdk_after()->World {delete_extent(sdk_before())}

// All focused controls retain exactly one of these independent witnesses.
// They call definitions only, never another proof lemma.







pub proof fn sdk_neighbor() {
 assert(sdk_after().class);assert(sdk_after().module==sdk_before().module);
 assert(count(sdk_before())==1);assert(value(sdk_before())==271);
 assert(count(sdk_after())==0);assert(value(sdk_after())==0);
}
// Generic selected successful-provider frames, not whole source-body proofs.


}
fn main(){}
