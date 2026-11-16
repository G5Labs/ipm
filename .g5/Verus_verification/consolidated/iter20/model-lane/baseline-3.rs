// IPM571e04a1: ordinary two-component, nonstub, unmapped source CLS identifiers.
// Physical inventory is the two-controlled-class projection of the successful provider.
// XML import, relationship persistence, OBJ.Load/compile, returned document names,
// unambiguous SQL index lookup and selected literal case collation are provider ports.
// No whole inventory, raw parser, arbitrary SQL ordering or failed caller proof.
// Archive paths are UNASSESSED/UNPURSUED and have no representation here.
use vstd::prelude::*;
verus! {
pub type Text=Seq<u8>;
#[derive(PartialEq,Eq)]
pub enum Class {Loose,Named}
#[derive(PartialEq,Eq)]
pub enum Extension {Upper,Lower}
pub struct Reference {pub unique_name:Text,pub owner:Option<Text>,pub children:Seq<Text>}
pub struct World {pub code:Map<Class,int>,pub modules:Set<Text>,pub refs:Seq<Reference>}
pub struct ParsedModule {pub name:Text,pub resources:Seq<Text>}
pub struct ClassSource {pub class_id:Class,pub returns:int}
pub struct Home {pub module:Option<Text>,pub resource:Text,pub ref_id:Option<nat>}
pub struct Query {pub rows:Seq<Text>,pub cursor:nat,pub closed:bool}
pub struct Fetch {pub row:Option<Text>,pub next:Query,pub at_end:bool}
pub struct Printed {pub names:Seq<Text>,pub end:Query}
pub open spec fn loose_upper()->Text {seq![73u8,116,101,114,50,48,76,111,111,115,101,46,67,104,101,99,107,115,46,67,76,83]}
pub open spec fn named_upper()->Text {seq![73u8,116,101,114,50,48,78,97,109,101,100,46,76,111,111,115,101,46,67,76,83]}
pub open spec fn loose_lower()->Text {seq![73u8,116,101,114,50,48,76,111,111,115,101,46,67,104,101,99,107,115,46,99,108,115]}
pub open spec fn named_lower()->Text {seq![73u8,116,101,114,50,48,78,97,109,101,100,46,76,111,111,115,101,46,99,108,115]}
pub open spec fn loose_pkg()->Text {seq![73u8,116,101,114,50,48,76,111,111,115,101,46,80,75,71]}
pub open spec fn named_pkg()->Text {seq![73u8,116,101,114,50,48,78,97,109,101,100,46,80,75,71]}
pub open spec fn loose_full_module()->Text {seq![105u8,116,101,114,50,48,108,111,111,115,101,46,99,104,101,99,107,115]}
pub open spec fn named_full_module()->Text {seq![105u8,116,101,114,50,48,110,97,109,101,100,46,108,111,111,115,101]}
pub open spec fn loose_prefix_module()->Text {seq![105u8,116,101,114,50,48,108,111,111,115,101]}
pub open spec fn naming_module()->Text {seq![105u8,116,101,114,50,48,110,97,109,101,100]}
pub open spec fn claim_module()->Text {seq![105u8,116,101,114,50,48,45,99,108,97,105,109]}
pub open spec fn exact_module()->Text {seq![105u8,116,101,114,50,48,45,101,120,97,99,116]}
pub open spec fn other_module()->Text {seq![105u8,116,101,114,50,48,45,111,116,104,101,114]}
pub open spec fn identifier(c:Class,e:Extension)->Text {
 match (c,e){(Class::Loose,Extension::Upper)=>loose_upper(),(Class::Loose,Extension::Lower)=>loose_lower(),
  (Class::Named,Extension::Upper)=>named_upper(),(Class::Named,Extension::Lower)=>named_lower()}
}
pub open spec fn package(c:Class)->Text {match c{Class::Loose=>loose_pkg(),Class::Named=>named_pkg()}}
// Selected SQL literal collation port. It does not rewrite FindHomeModule's
// case-sensitive extension branch or assert generic datatype/parser normalization.
pub open spec fn lookup_key(n:Text)->Text {
 if n==loose_lower(){loose_upper()}else if n==named_lower(){named_upper()}else{n}
}
pub open spec fn candidates(c:Class,e:Extension)->Seq<Text> {
 if e==Extension::Upper{seq![identifier(c,e),package(c)]}
 else{seq![identifier(c,e),identifier(c,e)]}
}
// Two-component class prefixes, longest first; NameExists literal normalization
// maps actual mixed-case class prefixes to these stored lowercase module names.
pub open spec fn module_candidates(c:Class)->Seq<Text> {
 match c{Class::Loose=>seq![loose_full_module(),loose_prefix_module()],
  Class::Named=>seq![named_full_module(),naming_module()]}
}
pub open spec fn empty()->World {World{code:Map::empty(),modules:Set::empty(),refs:seq![]}}
pub open spec fn load_source(w:World,s:ClassSource)->World {World{code:w.code.insert(s.class_id,s.returns),..w}}
pub open spec fn import_refs(w:World,m:ParsedModule,i:nat)->World
 decreases if i<m.resources.len(){(m.resources.len()-i) as nat}else{0nat}
{
 if i>=m.resources.len(){w}else{
  import_refs(World{refs:w.refs+seq![Reference{unique_name:m.resources[i as int],owner:Some(m.name),children:seq![]}],..w},m,i+1)
 }
}
pub open spec fn import_module(w:World,m:ParsedModule)->World {
 import_refs(World{modules:w.modules.insert(m.name),..w},m,0)
}
// Source-compilation and Children replacement are separate successful providers.
// Children names are actual OBJ.Load return values, not declarations inferred by lookup.
pub open spec fn replace_children(w:World,id:nat,names:Seq<Text>)->World {
 if id>=w.refs.len(){w}else{World{refs:w.refs.update(id as int,Reference{children:names,..w.refs[id as int]}),..w}}
}
pub open spec fn document_loaded(w:World,id:nat,s:ClassSource,names:Seq<Text>)->World {
 replace_children(load_source(w,s),id,names)
}
pub open spec fn find_unique(w:World,n:Text,i:nat)->Option<nat>
 decreases if i<w.refs.len(){(w.refs.len()-i) as nat}else{0nat}
{
 if i>=w.refs.len(){None}else if lookup_key(w.refs[i as int].unique_name)==lookup_key(n){Some(i)}else{find_unique(w,n,i+1)}
}
pub open spec fn child_contains(names:Seq<Text>,n:Text,i:nat)->bool
 decreases if i<names.len(){(names.len()-i) as nat}else{0nat}
{i<names.len() && (lookup_key(names[i as int])==lookup_key(n) || child_contains(names,n,i+1))}
pub open spec fn find_child(w:World,n:Text,i:nat)->Option<nat>
 decreases if i<w.refs.len(){(w.refs.len()-i) as nat}else{0nat}
{
 if i>=w.refs.len(){None}else if child_contains(w.refs[i as int].children,n,0){Some(i)}else{find_child(w,n,i+1)}
}
// UniqueName SQL has no Module-is-not-null filter. A null-owner unique row
// shadows the Children query for this candidate; home search may try later candidates.
pub open spec fn lookup_ref(w:World,n:Text)->Option<nat> {
 match find_unique(w,n,0){Some(i)=>Some(i),None=>find_child(w,n,0)}
}
pub open spec fn owner_object(w:World,id:nat)->Option<Text> {
 if id>=w.refs.len(){None}else{match w.refs[id as int].owner{
  Some(n)=>if w.modules.contains(n){Some(n)}else{None},None=>None}}
}
pub open spec fn ref_pass(w:World,cs:Seq<Text>,i:nat,last:Option<nat>)->Home
 decreases if i<cs.len(){(cs.len()-i) as nat}else{0nat}
{
 if i>=cs.len(){Home{module:None,resource:seq![],ref_id:last}}else{
  match lookup_ref(w,cs[i as int]){
   Some(id)=>match owner_object(w,id){Some(n)=>Home{module:Some(n),resource:cs[i as int],ref_id:Some(id)},
    None=>ref_pass(w,cs,i+1,Some(id))},
   None=>ref_pass(w,cs,i+1,last)}
 }
}
pub open spec fn naming_pass(w:World,ms:Seq<Text>,cs:Seq<Text>,i:nat,last:Option<nat>)->Home
 decreases if i<ms.len(){(ms.len()-i) as nat}else{0nat}
{
 if i>=ms.len(){Home{module:None,resource:seq![],ref_id:last}}
 else if w.modules.contains(ms[i as int]){Home{module:Some(ms[i as int]),resource:cs[i as int],ref_id:last}}
 else{naming_pass(w,ms,cs,i+1,last)}
}
pub open spec fn home(w:World,c:Class,e:Extension)->Home {
 let first=ref_pass(w,candidates(c,e),0,None);
 if first.module.is_some(){first}else{naming_pass(w,module_candidates(c),candidates(c,e),0,first.ref_id)}
}
pub open spec fn home_name(w:World,c:Class,e:Extension)->Text {match home(w,c,e).module{Some(n)=>n,None=>seq![]}}
pub open spec fn has_ref(w:World,c:Class,e:Extension)->bool {lookup_ref(w,identifier(c,e)).is_some()}
pub open spec fn exact_ref_owner(w:World,c:Class,e:Extension)->Option<Text> {
 match lookup_ref(w,identifier(c,e)){Some(id)=>owner_object(w,id),None=>None}
}
// Projected complete provider inventory for the two controlled ordinary classes.
// No namespace-wide item count/order or stub/generated/mapped/system inventory proof.
pub open spec fn inventory(w:World)->Seq<Class> {
 (if w.code.contains_key(Class::Loose){seq![Class::Loose]}else{seq![]})+
 (if w.code.contains_key(Class::Named){seq![Class::Named]}else{seq![]})
}
pub open spec fn filter(w:World,items:Seq<Class>,i:nat)->Seq<Text>
 decreases if i<items.len(){(items.len()-i) as nat}else{0nat}
{
 if i>=items.len(){seq![]}else{
  let c=items[i as int];let rest=filter(w,items,i+1);
  if home(w,c,Extension::Lower).module.is_none(){seq![identifier(c,Extension::Lower)]+rest}else{rest}
 }
}
pub open spec fn execute(w:World)->Query {Query{rows:filter(w,inventory(w),0),cursor:0,closed:false}}
pub open spec fn fetch(q:Query)->Fetch {
 if q.cursor>=q.rows.len(){Fetch{row:None,next:q,at_end:true}}
 else{Fetch{row:Some(q.rows[q.cursor as int]),next:Query{cursor:q.cursor+1,..q},at_end:false}}
}
pub open spec fn close(q:Query)->Query {Query{rows:seq![],cursor:0,closed:true}}
// Well-formed fresh normal cursor, consumed to first AtEnd then closed; never
// overlapping, abandoned or fetched again after AtEnd/Close. No stale temp-slot claim.
pub open spec fn drain(q:Query)->Printed
 decreases if q.cursor<q.rows.len(){(q.rows.len()-q.cursor) as nat}else{0nat}
{
 let f=fetch(q);match f.row{None=>Printed{names:seq![],end:close(f.next)},Some(n)=>{
  let rest=drain(f.next);Printed{names:seq![n]+rest.names,end:rest.end}}}
}
pub open spec fn printed(w:World)->Seq<Text> {drain(execute(w)).names}
pub open spec fn visible(w:World,c:Class)->bool {printed(w).contains(identifier(c,Extension::Lower))}
pub open spec fn payload(w:World,c:Class)->int {if w.code.contains_key(c){w.code[c]}else{0}}
pub open spec fn b(v:bool)->int {if v{1}else{0}}

pub open spec fn claim_manifest()->ParsedModule {ParsedModule{name:claim_module(),resources:seq![loose_upper()]}}
pub open spec fn naming_manifest()->ParsedModule {ParsedModule{name:naming_module(),resources:seq![]}}
pub open spec fn exact_manifest()->ParsedModule {ParsedModule{name:exact_module(),resources:seq![named_upper()]}}
pub open spec fn free()->World {load_source(load_source(empty(),ClassSource{class_id:Class::Loose,returns:1}),ClassSource{class_id:Class::Named,returns:1})}
pub open spec fn claimed()->World {document_loaded(import_module(free(),claim_manifest()),0,ClassSource{class_id:Class::Loose,returns:1},seq![loose_upper()])}
pub open spec fn naming()->World {import_module(claimed(),naming_manifest())}
pub open spec fn exact()->World {document_loaded(import_module(naming(),exact_manifest()),1,ClassSource{class_id:Class::Named,returns:1},seq![named_upper()])}

// Scoped pure neighbors for precedence and null owner semantics; no native injection.
pub open spec fn row(n:Text,o:Option<Text>,ch:Seq<Text>)->Reference {Reference{unique_name:n,owner:o,children:ch}}
pub open spec fn with_rows(rs:Seq<Reference>)->World {World{refs:rs,modules:set![claim_module(),naming_module(),exact_module(),other_module()],..free()}}
pub open spec fn null_unique()->World {with_rows(seq![row(named_upper(),None,seq![]),row(loose_pkg(),Some(other_module()),seq![named_upper()])])}
pub open spec fn upper_package()->World {World{modules:set![claim_module()],refs:seq![row(named_pkg(),Some(claim_module()),seq![])],..free()}}

// All focused negative witnesses below unfold definitions only; no proof-lemma calls.
pub proof fn normal_neighbors()
 ensures visible(free(),Class::Loose),visible(free(),Class::Named),
  !visible(claimed(),Class::Loose),visible(claimed(),Class::Named),
  !visible(naming(),Class::Loose),!visible(naming(),Class::Named),
  !visible(exact(),Class::Loose),!visible(exact(),Class::Named),
  home_name(claimed(),Class::Loose,Extension::Lower)==claim_module(),
  home_name(naming(),Class::Named,Extension::Lower)==naming_module(),
  home_name(exact(),Class::Named,Extension::Lower)==exact_module(),
  !has_ref(naming(),Class::Named,Extension::Lower),has_ref(exact(),Class::Named,Extension::Lower),
  naming().code==claimed().code,naming().refs==claimed().refs,
  payload(free(),Class::Loose)==1,payload(free(),Class::Named)==1,
  payload(claimed(),Class::Loose)==1,payload(claimed(),Class::Named)==1,
  payload(naming(),Class::Loose)==1,payload(naming(),Class::Named)==1,
  payload(exact(),Class::Loose)==1,payload(exact(),Class::Named)==1
{reveal_with_fuel(import_refs,3);reveal_with_fuel(find_unique,3);reveal_with_fuel(find_child,3);reveal_with_fuel(child_contains,3);reveal_with_fuel(ref_pass,3);reveal_with_fuel(naming_pass,3);reveal_with_fuel(filter,3);reveal_with_fuel(drain,3);}
pub proof fn extension_neighbor()
 ensures home(naming(),Class::Named,Extension::Lower).resource==named_lower(),
  home(naming(),Class::Named,Extension::Upper).resource==named_pkg(),
  home(naming(),Class::Named,Extension::Lower).ref_id==None,
  home(exact(),Class::Named,Extension::Lower).resource==named_lower(),
  home(exact(),Class::Named,Extension::Upper).resource==named_upper(),
  home(exact(),Class::Named,Extension::Lower).ref_id==Some(1nat),
  home_name(upper_package(),Class::Named,Extension::Upper)==claim_module(),
  home(upper_package(),Class::Named,Extension::Lower).module==None
{
 reveal_with_fuel(import_refs,3);reveal_with_fuel(find_unique,3);reveal_with_fuel(find_child,3);reveal_with_fuel(child_contains,3);reveal_with_fuel(ref_pass,3);reveal_with_fuel(naming_pass,3);
 assert(lookup_ref(naming(),named_upper())==None);
 assert(lookup_ref(naming(),named_pkg())==None);
 assert(ref_pass(naming(),candidates(Class::Named,Extension::Upper),0,None).module==None);
 assert(naming().modules.contains(naming_module()));
 assert(claim_module()!=named_full_module()) by (compute);
 assert(naming_module()!=named_full_module()) by (compute);
 assert(!naming().modules.contains(named_full_module()));
 assert(home(naming(),Class::Named,Extension::Upper).resource==named_pkg());
}
pub proof fn priority_neighbor()
 ensures home_name(with_rows(seq![row(named_pkg(),Some(claim_module()),seq![]),row(named_upper(),Some(exact_module()),seq![])]),Class::Named,Extension::Upper)==exact_module(),
  home_name(with_rows(seq![row(named_pkg(),Some(claim_module()),seq![])]),Class::Named,Extension::Upper)==claim_module(),
  home_name(World{modules:set![naming_module(),named_full_module()],..free()},Class::Named,Extension::Lower)==named_full_module(),
  home_name(with_rows(seq![row(loose_pkg(),Some(other_module()),seq![named_upper()]),row(named_upper(),Some(exact_module()),seq![])]),Class::Named,Extension::Lower)==exact_module()
{reveal_with_fuel(find_unique,3);reveal_with_fuel(find_child,3);reveal_with_fuel(child_contains,3);reveal_with_fuel(ref_pass,3);reveal_with_fuel(naming_pass,3);}
pub proof fn nullable_neighbor()
 ensures lookup_ref(null_unique(),named_lower())==Some(0nat),owner_object(null_unique(),0)==None,
  home_name(null_unique(),Class::Named,Extension::Lower)==naming_module(),
  home(null_unique(),Class::Named,Extension::Lower).ref_id==Some(0nat),
  home(null_unique(),Class::Named,Extension::Upper).ref_id==Some(0nat),
  home(World{modules:Set::empty(),..null_unique()},Class::Named,Extension::Lower).module==None,
  home(World{modules:Set::empty(),refs:seq![row(named_upper(),Some(claim_module()),seq![])],..free()},Class::Named,Extension::Lower).module==None,
  home_name(with_rows(seq![row(loose_pkg(),Some(other_module()),seq![named_upper()])]),Class::Named,Extension::Lower)==other_module()
{reveal_with_fuel(find_unique,3);reveal_with_fuel(find_child,3);reveal_with_fuel(child_contains,3);reveal_with_fuel(ref_pass,3);reveal_with_fuel(naming_pass,3);}
pub proof fn cursor_neighbor()
 ensures execute(free()).rows==seq![loose_lower(),named_lower()],
  printed(free())==seq![loose_lower(),named_lower()],
  fetch(execute(free())).row==Some(loose_lower()),!fetch(execute(free())).at_end,
  fetch(fetch(execute(free())).next).row==Some(named_lower()),
  fetch(fetch(fetch(execute(free())).next).next).at_end,
  execute(claimed()).rows==seq![named_lower()],printed(claimed())==seq![named_lower()],
  printed(naming())==seq![],drain(execute(free())).end.closed,drain(execute(free())).end.rows==seq![]
{reveal_with_fuel(import_refs,3);reveal_with_fuel(find_unique,3);reveal_with_fuel(find_child,3);reveal_with_fuel(child_contains,3);reveal_with_fuel(ref_pass,3);reveal_with_fuel(naming_pass,3);reveal_with_fuel(filter,3);reveal_with_fuel(drain,3);}
pub proof fn children_writer_neighbor()
 ensures replace_children(with_rows(seq![row(loose_upper(),Some(claim_module()),seq![named_upper()])]),0,seq![loose_upper()]).refs[0].children==seq![loose_upper()],
  replace_children(claimed(),0,seq![]).modules==claimed().modules,
  replace_children(claimed(),0,seq![]).code==claimed().code,
  claimed().refs[0].owner==Some(claim_module()),claimed().refs[0].unique_name==loose_upper(),
  exact().refs[1].owner==Some(exact_module()),exact().refs[1].unique_name==named_upper()
{reveal_with_fuel(import_refs,3);}
}
fn main(){}
