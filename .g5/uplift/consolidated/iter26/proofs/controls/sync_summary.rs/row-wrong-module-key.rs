// IPM571e04a1. Disclosed repair: actual AddSkipped decision -> job-local
// per-module numeric increment/class slots -> selected Report projection.
// No persistent SQL/opaque IDs, full sync/status/test dispatch or generic IO proof.
// Other Summary fields, callback effects and resource assignment remain outside.
// Excluded boundaries UNASSESSED; no private evaluator inputs accessed.
use vstd::prelude::*;
verus! {
pub type Text=Seq<u8>;
pub struct Store {pub active:bool,pub counts:Map<Text,nat>,pub rows:Map<(Text,nat),Text>}
pub struct Call {pub module:Text,pub class:Text}
pub struct Row {pub module:Text,pub sequence:nat,pub class:Text}
pub struct SelectedResource {pub owner_module:Text,pub phases:Seq<Text>}
pub open spec fn zeta()->Text {seq![105u8,116,101,114,50,54,45,122,101,116,97]}
pub open spec fn alpha()->Text {seq![105u8,116,101,114,50,54,45,97,108,112,104,97]}
pub open spec fn verify_z()->Text {seq![73u8,116,101,114,50,54,46,86,101,114,105,102,121,90]}
pub open spec fn verify_a()->Text {seq![73u8,116,101,114,50,54,46,86,101,114,105,102,121,65]}
pub open spec fn alpha_class()->Text {seq![73u8,116,101,114,50,54,46,65,108,112,104,97]}
pub open spec fn test_phase()->Text {seq![116u8,101,115,116]}
pub open spec fn verify_phase()->Text {seq![118u8,101,114,105,102,121]}
pub open spec fn skip_label()->Text {seq![32u8,32,32,32,115,107,105,112,112,101,100,32,40,118,101,114,105,102,121,45,115,99,111,112,101,100,41,58,32]}
pub open spec fn empty()->Store {Store{active:false,counts:Map::empty(),rows:Map::empty()}}
pub open spec fn begin(_old:Store)->Store {Store{active:true,counts:Map::empty(),rows:Map::empty()}}
pub open spec fn end(_old:Store)->Store {empty()}
pub open spec fn count(s:Store,module:Text)->nat {if s.counts.contains_key(module){s.counts[module]}else{0}}
pub open spec fn has_row(s:Store,module:Text,n:nat)->bool {s.rows.contains_key((module,n))}
pub open spec fn row_value(s:Store,module:Text,n:nat)->Option<Text> {if has_row(s,module,n){Some(s.rows[(module,n)])}else{None}}
pub open spec fn add_skipped(s:Store,module:Text,class:Text)->Store {
 if !s.active{s}else{let sequence=count(s,module)+1;Store{counts:s.counts.insert(module,sequence),rows:s.rows.insert((alpha(),sequence),class),..s}}}
pub open spec fn calls(s:Store,input:Seq<Call>,i:nat)->Store
 decreases if i<input.len(){(input.len()-i) as nat}else{0nat}
{if i>=input.len(){s}else{let c=input[i as int];calls(add_skipped(s,c.module,c.class),input,i+1)}}
// Tracking producer only. Nonempty module argument and no-found-modules paths
// do not Begin or forcibly deactivate an already active outer context.
pub open spec fn begins(module_argument:Text,ordered:Seq<Text>)->bool {module_argument.len()==0 && ordered.len()>0}
pub open spec fn start(s:Store,module_argument:Text,ordered:Seq<Text>)->Store {if begins(module_argument,ordered){begin(s)}else{s}}
pub open spec fn finish(s:Store,started:bool)->Store {if started{end(s)}else{s}}
// Callsite projection after actual byName/OwnsClass assignment. classes are the
// already selected unique, ordered class keys for this resource. Selection,
// Processor %IsA/availability and callback effects remain source/provider seams.
pub open spec fn selected_calls(r:SelectedResource,classes:Seq<Text>,i:nat)->Seq<Call>
 decreases if i<classes.len(){(classes.len()-i) as nat}else{0nat}
{if r.phases.contains(test_phase()) || i>=classes.len(){seq![]}else{seq![Call{module:r.owner_module,class:classes[i as int]}]+selected_calls(r,classes,i+1)}}
pub open spec fn record_selected(s:Store,r:SelectedResource,classes:Seq<Text>)->Store {calls(s,selected_calls(r,classes,0),0)}
// Counter bounds the Report walk only under the generated-prefix invariant:
// Begin and AddSkipped produce exactly numeric keys1..SkippedCount. Arbitrary
// manual/corrupt stores and other-field section gates are not a Report contract.
pub open spec fn well_formed(s:Store)->bool {
  (forall|m:Text| #[trigger]s.counts.contains_key(m) ==> s.counts[m]>0) &&
 (forall|m:Text,n:nat| #[trigger]s.rows.contains_key((m,n)) == (s.counts.contains_key(m) && 1<=n<=s.counts[m]))}
pub open spec fn rows_from(s:Store,module:Text,n:nat)->Seq<Row>
 decreases if n<=count(s,module){(count(s,module)-n+1) as nat}else{0nat}
{if n>count(s,module){seq![]}else{(if has_row(s,module,n){seq![Row{module,sequence:n,class:s.rows[(module,n)]}]}else{seq![]})+rows_from(s,module,n+1)}}
pub open spec fn report_from(s:Store,names:Seq<Text>,i:nat)->Seq<Row>
 decreases if i<names.len(){(names.len()-i) as nat}else{0nat}
{if i>=names.len(){seq![]}else{let m=names[i as int];(if count(s,m)>0{rows_from(s,m,1)}else{seq![]})+report_from(s,names,i+1)}}
pub open spec fn report(s:Store,names:Seq<Text>)->Seq<Row> {if !s.active{seq![]}else{report_from(s,names,0)}}
pub open spec fn printed_class(r:Row)->Text {skip_label()+r.class}
// Skipped-only DIRECT API store projection: other Summary fields can create
// module buckets without SkippedCount. Never used as real CLI total module count.
pub open spec fn module_count(s:Store)->nat {s.counts.dom().len()}
pub open spec fn matching(s:Store,module:Text,n:nat,class:Text)->bool {row_value(s,module,n)==Some(class)}
pub open spec fn b(v:bool)->int {if v{1}else{0}}

pub open spec fn inactive()->Store {add_skipped(end(empty()),zeta(),verify_z())}
pub open spec fn begun()->Store {begin(inactive())}
pub open spec fn a1()->Store {add_skipped(begun(),zeta(),verify_z())}
pub open spec fn b1()->Store {add_skipped(a1(),alpha(),alpha_class())}
pub open spec fn a2()->Store {add_skipped(b1(),zeta(),verify_a())}
pub open spec fn a3()->Store {add_skipped(a2(),zeta(),verify_z())}
pub open spec fn after_report()->Store {a3()}
pub open spec fn after_end()->Store {end(after_report())}
pub open spec fn after_inactive()->Store {add_skipped(after_end(),alpha(),alpha_class())}
pub open spec fn reset_empty()->Store {begin(after_inactive())}
pub open spec fn reset()->Store {add_skipped(reset_empty(),zeta(),verify_z())}
pub open spec fn cp_begun()->Store {Store{active:true,..empty()}}
pub open spec fn cp_a1()->Store {Store{active:true,counts:map![zeta()=>1nat],rows:map![(zeta(),1nat)=>verify_z()]}}
pub open spec fn cp_b1()->Store {Store{active:true,counts:map![zeta()=>1nat,alpha()=>1nat],rows:map![(zeta(),1nat)=>verify_z(),(alpha(),1nat)=>alpha_class()]}}
pub open spec fn cp_a2()->Store {Store{active:true,counts:map![zeta()=>2nat,alpha()=>1nat],rows:map![(zeta(),1nat)=>verify_z(),(zeta(),2nat)=>verify_a(),(alpha(),1nat)=>alpha_class()]}}
pub open spec fn cp_a3()->Store {Store{active:true,counts:map![zeta()=>3nat,alpha()=>1nat],rows:map![(zeta(),1nat)=>verify_z(),(zeta(),2nat)=>verify_a(),(zeta(),3nat)=>verify_z(),(alpha(),1nat)=>alpha_class()]}}
pub open spec fn forward()->Seq<Row> {report(a3(),seq![zeta(),alpha()])}
pub open spec fn reverse()->Seq<Row> {report(a3(),seq![alpha(),zeta()])}
pub open spec fn zeta_rows()->Seq<Row> {seq![Row{module:zeta(),sequence:1,class:verify_z()},Row{module:zeta(),sequence:2,class:verify_a()},Row{module:zeta(),sequence:3,class:verify_z()}]}
pub open spec fn alpha_rows()->Seq<Row> {seq![Row{module:alpha(),sequence:1,class:alpha_class()}]}

pub open spec fn cli_module()->Text {seq![105u8,116,101,114,50,54,45,99,108,105]}
pub open spec fn cli_class()->Text {seq![73u8,116,101,114,50,54,46,86,101,114,105,102,121,46,67,97,115,101]}
pub open spec fn cli_resource()->SelectedResource {SelectedResource{owner_module:cli_module(),phases:seq![verify_phase()]}}
pub open spec fn verbose_skip_count(r:SelectedResource,classes:Seq<Text>,verbose:bool)->nat {if verbose{selected_calls(r,classes,0).len()}else{0}}
pub open spec fn callback_selected(r:SelectedResource,classes:Seq<Text>)->bool {r.phases.contains(test_phase()) && classes.len()>0}
pub open spec fn cli_single_store()->Store {record_selected(start(empty(),cli_module(),seq![]),cli_resource(),seq![cli_class()])}
pub open spec fn cli_all_before_end()->Store {record_selected(start(empty(),seq![],seq![cli_module()]),cli_resource(),seq![cli_class()])}
pub open spec fn cli_all_report()->Seq<Row> {report(cli_all_before_end(),seq![cli_module()])}
pub open spec fn cli_all_finished()->Store {finish(cli_all_before_end(),true)}
pub open spec fn cp_cli()->Store {Store{active:true,counts:map![cli_module()=>1nat],rows:map![(cli_module(),1nat)=>cli_class()]}}
// Independent definition-only selectors. No local/imported proof lemma calls.

pub proof fn append_neighbor(s:Store,m:Text,c:Text,other:Text)
 requires s.active,other!=m
 ensures add_skipped(s,m,c).active,count(add_skipped(s,m,c),m)==count(s,m)+1,
 row_value(add_skipped(s,m,c),m,count(s,m)+1)==Some(c),count(add_skipped(s,m,c),other)==count(s,other)
{}










}
fn main(){}
