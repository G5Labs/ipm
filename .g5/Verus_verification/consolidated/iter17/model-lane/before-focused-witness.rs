// Qualified report identity at IPM 571e04a1. One suite/case/method per completed run.
// Raw list/JSON primitive decoding, %UnitTest execution/allocation, serializer/filesystem success
// are provider ports. Native ordinary Assert/Shell calls qualify the selected raw rows and IDs.
// Every numeric result child is counted, including provider LogMessage duration rows.
// Archive paths remain UNASSESSED/UNPURSUED and have no representation here.
#[path="../iter2/readers.rs"]
pub mod inherited_readers;
#[path="../iter4/test_outcome.rs"]
pub mod inherited_outcome;
use vstd::prelude::*;
verus! {
pub type Text=Seq<u8>;
#[derive(PartialEq,Eq)]
pub enum Action {AssertTrue,AssertEquals,AssertStatusOK,LogMessage,Other}
pub struct Header {pub status:int,pub category:Text,pub message:Text}
pub struct RawEntry {pub status:int,pub action:Action,pub description:Text,pub location:Text}
pub struct RawRun {pub case_name:Text,pub method_name:Text,pub case_header:Header,pub method_header:Header,pub entries:Seq<RawEntry>}
pub struct Entry {pub passed:bool,pub action:Action,pub description:Text,pub location:Text}
pub struct Tree {pub case_name:Text,pub method_name:Text,pub case_passed:bool,pub method_passed:bool,pub case_error:Text,pub method_error:Text,pub entries:Seq<Entry>}
pub struct Stored {pub id:int,pub run:RawRun}
pub struct State {pub db:Seq<Stored>,pub tracked_ids:Seq<int>}
pub struct Snapshot {pub id:int,pub tree:Tree}
pub struct Report {pub file:Option<Snapshot>,pub device:Option<Snapshot>,pub failures:nat,pub ok:bool,pub after:State}

pub open spec fn outer()->Text {seq![73u8,116,101,114,49,55,46,79,117,116,101,114,46,67,104,101,99,107,115]}
pub open spec fn child()->Text {seq![73u8,116,101,114,49,55,46,67,104,105,108,100,46,67,104,101,99,107,115]}
pub open spec fn test_pass()->Text {seq![84u8,101,115,116,80,97,115,115]}
pub open spec fn test_fail()->Text {seq![84u8,101,115,116,70,97,105,108]}
pub open spec fn test_nested()->Text {seq![84u8,101,115,116,78,101,115,116,101,100,70,97,105,108]}
pub open spec fn outer_true()->Text {seq![111u8,117,116,101,114,32,116,114,117,101]}
pub open spec fn outer_equals()->Text {seq![111u8,117,116,101,114,32,101,113,117,97,108,115]}
pub open spec fn precondition()->Text {seq![112u8,114,101,99,111,110,100,105,116,105,111,110,32,115,117,99,99,101,115,115]}
pub open spec fn outer_false()->Text {seq![111u8,117,116,101,114,32,100,101,108,105,98,101,114,97,116,101,32,102,97,108,115,101]}
pub open spec fn outer_mismatch()->Text {seq![111u8,117,116,101,114,32,100,101,108,105,98,101,114,97,116,101,32,109,105,115,109,97,116,99,104]}
pub open spec fn child_true()->Text {seq![99u8,104,105,108,100,32,116,114,117,101]}
pub open spec fn child_equals()->Text {seq![99u8,104,105,108,100,32,101,113,117,97,108,115]}
pub open spec fn aggregate_case()->Text {seq![84u8,104,101,114,101,32,97,114,101,32,102,97,105,108,101,100,32,84,101,115,116,77,101,116,104,111,100,115]}
pub open spec fn aggregate_method()->Text {seq![84u8,104,101,114,101,32,97,114,101,32,102,97,105,108,101,100,32,84,101,115,116,65,115,115,101,114,116,115]}
pub open spec fn passed(status:int)->bool {status!=0}
pub open spec fn error(h:Header)->Text {
 if inherited_readers::error_present(passed(h.status),h.category.len()>0){h.category+seq![58u8,32]+h.message}else{seq![]}
}
pub open spec fn decode_entries(xs:Seq<RawEntry>)->Seq<Entry>
 decreases xs.len()
{if xs.len()==0{seq![]}else{seq![Entry{passed:passed(xs[0].status),action:xs[0].action,description:xs[0].description,location:xs[0].location}]+decode_entries(xs.skip(1))}}
pub open spec fn decode(r:RawRun)->Tree {
 Tree{case_name:r.case_name,method_name:r.method_name,case_passed:passed(r.case_header.status),method_passed:passed(r.method_header.status),
 case_error:error(r.case_header),method_error:error(r.method_header),entries:decode_entries(r.entries)}
}
pub open spec fn failed_entries(xs:Seq<Entry>)->nat
 decreases xs.len()
{if xs.len()==0{0}else{(if xs[0].passed{0nat}else{1nat})+failed_entries(xs.skip(1))}}
pub open spec fn summary(t:Tree)->inherited_readers::Count {
 inherited_readers::count_methods(seq![inherited_readers::Method{passed:t.method_passed,
 passed_assertions:(t.entries.len()-failed_entries(t.entries)) as nat,failed_assertions:failed_entries(t.entries)}],t.case_error.len()>0)
}
pub open spec fn entry_log(xs:Seq<Entry>)->Seq<inherited_outcome::Row>
 decreases xs.len()
{if xs.len()==0{seq![]}else{seq![if xs[0].passed{inherited_outcome::Row::AssertPassed}else{inherited_outcome::Row::AssertFailed}]+entry_log(xs.skip(1))}}
pub open spec fn classified(t:Tree)->Seq<inherited_outcome::Row> {
 seq![if t.case_error.len()>0{inherited_outcome::Row::CaseError}else{inherited_outcome::Row::CaseOk},
 if t.method_error.len()>0{inherited_outcome::Row::MethodError}else{inherited_outcome::Row::MethodOk}]+entry_log(t.entries)
}
pub open spec fn failure_descriptions(xs:Seq<Entry>)->Seq<Text>
 decreases xs.len()
{if xs.len()==0{seq![]}else{(if xs[0].passed{seq![]}else{seq![xs[0].description]})+failure_descriptions(xs.skip(1))}}
pub open spec fn device_failures(t:Tree)->Seq<Text> {
 (if t.case_error.len()>0{seq![t.case_error]}else{seq![]})+
 if !t.method_passed {
  (if t.method_error.len()>0{seq![t.method_error]}else{seq![]})+failure_descriptions(t.entries)
 }else{seq![]}
}
pub open spec fn lookup(db:Seq<Stored>,id:int)->Option<RawRun>
 decreases db.len()
{if db.len()==0{None}else if db[0].id==id{Some(db[0].run)}else{lookup(db.skip(1),id)}}
pub open spec fn latest(db:Seq<Stored>)->Option<int>
 decreases db.len()
{if db.len()==0{None}else{match latest(db.skip(1)){None=>Some(db[0].id),Some(other)=>Some(if db[0].id>other{db[0].id}else{other})}}}
pub open spec fn empty()->State {State{db:seq![],tracked_ids:seq![]}}
pub open spec fn complete(w:State,id:int,r:RawRun)->State {
 State{db:w.db.push(Stored{id,run:r}),tracked_ids:w.tracked_ids.push(id)}
}
pub open spec fn consume(w:State,start:nat)->State {
 State{tracked_ids:w.tracked_ids.take(start as int),..w}
}
pub open spec fn device_index(w:State)->Option<int> {
 if w.tracked_ids.len()>0{Some(w.tracked_ids.last())}else{latest(w.db)}
}
pub open spec fn file_index(w:State,explicit:Option<int>)->Option<int> {
 match explicit {Some(id)=>Some(id),None=>latest(w.db)}
}
pub open spec fn snapshot(w:State,id:Option<int>)->Option<Snapshot> {
 match id {None=>None,Some(i)=>match lookup(w.db,i){None=>None,Some(r)=>Some(Snapshot{id:i,tree:decode(r)})}}
}
pub open spec fn export_file(w:State,explicit:Option<int>)->Option<Snapshot> {snapshot(w,file_index(w,explicit))}
pub open spec fn slots(w:State,ids:Seq<int>)->inherited_outcome::Slots
 decreases ids.len()
{if ids.len()==0{seq![]}else{seq![match lookup(w.db,ids[0]){Some(r)=>Some(classified(decode(r))),None=>None}]+slots(w,ids.skip(1))}}
pub open spec fn report(w:State,start:nat,explicit_file:Option<int>)->Report {
 let status=inherited_outcome::get_all_status(slots(w,w.tracked_ids),start);
 Report{file:export_file(w,explicit_file),device:snapshot(w,device_index(w)),
 failures:status.count,ok:status.ok,after:consume(w,start)}
}
pub open spec fn header(status:int)->Header {Header{status,category:seq![],message:seq![]}}
pub open spec fn row(status:int,action:Action,description:Text)->RawEntry {RawEntry{status,action,description,location:seq![]}}
pub open spec fn duration()->RawEntry {row(1,Action::LogMessage,seq![])}
// Native provider adds a passed duration LogMessage; its variable text and locations are not literal claims.
pub open spec fn pass_run(child_run:bool)->RawRun {
 RawRun{case_name:if child_run{child()}else{outer()},method_name:test_pass(),case_header:header(1),method_header:header(1),
 entries:seq![row(1,Action::AssertTrue,if child_run{child_true()}else{outer_true()}),
 row(1,Action::AssertEquals,if child_run{child_equals()}else{outer_equals()}),duration()]}
}
pub open spec fn fail_run(nested_run:bool)->RawRun {
 RawRun{case_name:outer(),method_name:if nested_run{test_nested()}else{test_fail()},
 case_header:Header{status:0,category:seq![],message:aggregate_case()},method_header:Header{status:0,category:seq![],message:aggregate_method()},
 entries:seq![row(1,Action::AssertStatusOK,precondition()),row(0,Action::AssertTrue,outer_false()),
 row(0,Action::AssertEquals,outer_mismatch()),duration()]}
}
pub open spec fn single(id:int,r:RawRun)->State {complete(empty(),id,r)}
// IDs are provider-allocated inputs; this completion schedule alone does not decide their numeric order.
// Child SaveResult appends, its status reader consumes its process slot, then outer SaveResult appends.
// Persistent global results remain present. The natural parent native run qualified outer3 < child4.
pub open spec fn nested_state(outer_id:int,child_id:int)->State {
 complete(consume(complete(empty(),child_id,pass_run(true)),0),outer_id,fail_run(true))
}
// Named witnesses retain the earlier completed runs observed in the isolated native session.
// The child's intermediate view omits the still-running outer3; its ID is below selected max4.
// No arbitrary concurrent writer or arbitrary whole-database enumeration is claimed.
pub open spec fn pass_state()->State {single(1,pass_run(false))}
pub open spec fn pass()->Report {report(pass_state(),0,None)}
pub open spec fn fail_state()->State {complete(pass().after,2,fail_run(false))}
pub open spec fn fail()->Report {report(fail_state(),0,None)}
pub open spec fn child_state()->State {complete(fail().after,4,pass_run(true))}
pub open spec fn child_report()->Report {report(child_state(),0,None)}
pub open spec fn nested_full_state()->State {complete(child_report().after,3,fail_run(true))}
pub open spec fn nested()->Report {report(nested_full_state(),0,None)}
pub open spec fn forwarded()->Report {report(nested_full_state(),0,Some(3))}
pub open spec fn emitted(r:Report)->Seq<Snapshot> {match r.device{Some(s)=>seq![s],None=>seq![]}}
pub open spec fn nested_devices()->Seq<Snapshot> {emitted(child_report())+emitted(nested())}
pub open spec fn duration_rows(xs:Seq<RawEntry>)->nat
 decreases xs.len()
{if xs.len()==0{0}else{(if xs[0].action==Action::LogMessage{1nat}else{0nat})+duration_rows(xs.skip(1))}}
pub open spec fn file_id(r:Report)->int {match r.file{Some(s)=>s.id,None=>0}}
pub open spec fn device_id(r:Report)->int {match r.device{Some(s)=>s.id,None=>0}}
pub open spec fn file_failed_methods(r:Report)->nat {match r.file{Some(s)=>summary(s.tree).failed_methods,None=>0}}
pub open spec fn device_failed_methods(r:Report)->nat {match r.device{Some(s)=>summary(s.tree).failed_methods,None=>0}}
pub open spec fn file_assertions(r:Report)->nat {match r.file{Some(s)=>summary(s.tree).assertions,None=>0}}
pub open spec fn device_assertions(r:Report)->nat {match r.device{Some(s)=>summary(s.tree).assertions,None=>0}}
pub open spec fn file_failed_assertions(r:Report)->nat {match r.file{Some(s)=>summary(s.tree).failed_assertions,None=>0}}
pub open spec fn device_failed_assertions(r:Report)->nat {match r.device{Some(s)=>summary(s.tree).failed_assertions,None=>0}}
pub open spec fn file_case(r:Report)->Text {match r.file{Some(s)=>s.tree.case_name,None=>seq![]}}
pub open spec fn device_case(r:Report)->Text {match r.device{Some(s)=>s.tree.case_name,None=>seq![]}}
pub open spec fn file_method(r:Report)->Text {match r.file{Some(s)=>s.tree.method_name,None=>seq![]}}
pub open spec fn device_method(r:Report)->Text {match r.device{Some(s)=>s.tree.method_name,None=>seq![]}}
pub open spec fn b(v:bool)->int {if v{1}else{0}}

pub proof fn raw_neighbors()
 ensures passed(2),passed(-1),!passed(0),error(Header{status:0,category:seq![],message:aggregate_method()}).len()==0,
 error(Header{status:0,category:seq![67u8],message:seq![77u8]})==seq![67u8,58,32,77],
 error(Header{status:2,category:seq![67u8],message:seq![77u8]}).len()==0,
 decode(pass_run(false)).entries.len()==3,decode(fail_run(false)).entries.len()==4,
 summary(decode(pass_run(false))).methods==1,summary(decode(pass_run(false))).failed_methods==0,
 summary(decode(pass_run(false))).assertions==3,summary(decode(pass_run(false))).failed_assertions==0,
 summary(decode(fail_run(false))).methods==1,summary(decode(fail_run(false))).failed_methods==1,
 summary(decode(fail_run(false))).assertions==4,summary(decode(fail_run(false))).failed_assertions==2,
 inherited_outcome::failures(classified(decode(fail_run(false))))==2,
 device_failures(decode(fail_run(false)))==seq![outer_false(),outer_mismatch()]
{
 reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(entry_log,5);
 reveal_with_fuel(inherited_readers::count_methods,3);reveal_with_fuel(inherited_outcome::failures,8);
 reveal_with_fuel(failure_descriptions,5);
}
pub proof fn latest_older_frame(old:Seq<Stored>,db:Seq<Stored>,selected:int)
 requires latest(db)==Some(selected),forall|i:int|0<=i<old.len() ==> old[i].id<selected
 ensures latest(old+db)==Some(selected)
 decreases old.len()
{
 if old.len()>0 {
  latest_older_frame(old.skip(1),db,selected);
  assert((old+db).skip(1)==old.skip(1)+db);
 }else{assert(old+db==db);}
}
pub proof fn symbolic_order_neighbors(a:int,b:int)
 requires a!=b
 ensures device_index(nested_state(a,b))==Some(a),
 file_index(nested_state(a,b),None)==Some(if a>b{a}else{b}),
 file_index(nested_state(a,b),Some(a))==Some(a),
 nested_state(a,b).db.len()==2,nested_state(a,b).tracked_ids==seq![a]
{
 assert(nested_state(a,b).db==seq![Stored{id:b,run:pass_run(true)},Stored{id:a,run:fail_run(true)}]);
 reveal_with_fuel(latest,3);
}
pub proof fn tracked_last_neighbor(a:int,b:int)
 ensures device_index(State{db:seq![],tracked_ids:seq![a,b]})==Some(b)
{}
pub proof fn explicit_forwarding_preserves_reader(w:State)
 ensures export_file(w,device_index(w))==snapshot(w,device_index(w))
{match device_index(w){Some(i)=>{},None=>{}}}
pub proof fn nested_keeps_persistent_child(a:int,b:int)
 requires a!=b
 ensures lookup(nested_state(a,b).db,a)==Some(fail_run(true)),
 lookup(nested_state(a,b).db,b)==Some(pass_run(true))
{reveal_with_fuel(lookup,3);}
pub proof fn named_state_frames()
 ensures pass_state().db==seq![Stored{id:1,run:pass_run(false)}],pass_state().tracked_ids==seq![1int],
 fail_state().db==seq![Stored{id:1,run:pass_run(false)},Stored{id:2,run:fail_run(false)}],fail_state().tracked_ids==seq![2int],
 child_state().db==seq![Stored{id:1,run:pass_run(false)},Stored{id:2,run:fail_run(false)},Stored{id:4,run:pass_run(true)}],child_state().tracked_ids==seq![4int],
 nested_full_state().db==seq![Stored{id:1,run:pass_run(false)},Stored{id:2,run:fail_run(false)},Stored{id:4,run:pass_run(true)},Stored{id:3,run:fail_run(true)}],nested_full_state().tracked_ids==seq![3int]
{}
pub proof fn single_report_neighbors()
 ensures pass().ok,!fail().ok,pass().failures==0,fail().failures==2,
 file_id(pass())==1,device_id(pass())==1,file_id(fail())==2,device_id(fail())==2,
 file_assertions(pass())==3,device_assertions(pass())==3,file_assertions(fail())==4,device_assertions(fail())==4,
 pass().after.tracked_ids.len()==0,fail().after.tracked_ids.len()==0,
 pass_state().db.len()==1,fail_state().db.len()==2,
 duration_rows(pass_run(false).entries)==1,duration_rows(fail_run(false).entries)==1
{named_state_frames();
 raw_neighbors();
 reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(slots,3);
 reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(entry_log,5);
 reveal_with_fuel(inherited_readers::count_methods,3);reveal_with_fuel(inherited_outcome::failures,8);
 reveal_with_fuel(inherited_outcome::suffix_failures,3);reveal_with_fuel(duration_rows,5);

 assert(pass().ok);
 assert(!fail().ok);
 assert(pass().failures==0);
 assert(fail().failures==2);
 assert(file_id(pass())==1);
 assert(device_id(pass())==1);
 assert(file_id(fail())==2);
 assert(device_id(fail())==2);
 assert(file_assertions(pass())==3);
 assert(device_assertions(pass())==3);
 assert(file_assertions(fail())==4);
 assert(device_assertions(fail())==4);
}
pub proof fn nested_identity_neighbors()
 ensures nested().file==Some(Snapshot{id:4,tree:decode(pass_run(true))}),
 nested().device==Some(Snapshot{id:3,tree:decode(fail_run(true))}),forwarded().file==nested().device,
 file_id(nested())==4,device_id(nested())==3,file_id(forwarded())==3,device_id(forwarded())==3,
 file_case(nested())==child(),device_case(nested())==outer(),
 file_method(nested())==test_pass(),device_method(nested())==test_nested(),
 nested().file==snapshot(nested_full_state(),Some(4)),nested().file==child_report().file,
 nested_devices().len()==2,nested_full_state().db.len()==4
{named_state_frames();
 raw_neighbors();
 reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(slots,3);
 reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(entry_log,5);
 reveal_with_fuel(inherited_readers::count_methods,3);reveal_with_fuel(inherited_outcome::failures,8);
 reveal_with_fuel(inherited_outcome::suffix_failures,3);reveal_with_fuel(duration_rows,5);

 assert(nested().file==Some(Snapshot{id:4,tree:decode(pass_run(true))}));
 assert(nested().device==Some(Snapshot{id:3,tree:decode(fail_run(true))}));
 assert(forwarded().file==nested().device);
 assert(file_id(nested())==4);
 assert(device_id(nested())==3);
 assert(file_id(forwarded())==3);
 assert(device_id(forwarded())==3);
 assert(file_case(nested())==child());
 assert(device_case(nested())==outer());
 assert(file_method(nested())==test_pass());
 assert(device_method(nested())==test_nested());
 assert(nested().file==snapshot(nested_full_state(),Some(4)));
 assert(nested().file==child_report().file);
 assert(nested_devices().len()==2);
 assert(nested_full_state().db.len()==4);
}
pub proof fn nested_count_neighbors()
 ensures !nested().ok,!forwarded().ok,nested().failures==2,forwarded().failures==2,
 file_failed_methods(nested())==0,device_failed_methods(nested())==1,
 file_assertions(nested())==3,device_assertions(nested())==4,
 file_failed_assertions(nested())==0,device_failed_assertions(nested())==2,
 file_failed_methods(forwarded())==1,file_assertions(forwarded())==4,file_failed_assertions(forwarded())==2,
 nested().after.tracked_ids.len()==0
{named_state_frames();nested_identity_neighbors();
 raw_neighbors();
 reveal_with_fuel(lookup,6);reveal_with_fuel(latest,6);reveal_with_fuel(slots,3);
 reveal_with_fuel(decode_entries,5);reveal_with_fuel(failed_entries,5);reveal_with_fuel(entry_log,5);
 reveal_with_fuel(inherited_readers::count_methods,3);reveal_with_fuel(inherited_outcome::failures,8);
 reveal_with_fuel(inherited_outcome::suffix_failures,3);reveal_with_fuel(duration_rows,5);

 assert(!nested().ok);
 assert(!forwarded().ok);
 assert(nested().failures==2);
 assert(forwarded().failures==2);
 assert(file_failed_methods(nested())==0);
 assert(device_failed_methods(nested())==1);
 assert(file_assertions(nested())==3);
 assert(device_assertions(nested())==4);
 assert(file_failed_assertions(nested())==0);
 assert(device_failed_assertions(nested())==2);
 assert(file_failed_methods(forwarded())==1);
 assert(file_assertions(forwarded())==4);
 assert(file_failed_assertions(forwarded())==2);
 assert(nested().after.tracked_ids.len()==0);
}
pub proof fn fixture_neighbors()
 ensures pass().ok,!fail().ok,!nested().ok,!forwarded().ok,
 pass().failures==0,fail().failures==2,nested().failures==2,forwarded().failures==2,
 file_id(pass())==1,device_id(pass())==1,file_id(fail())==2,device_id(fail())==2,
 file_id(nested())==4,device_id(nested())==3,file_id(forwarded())==3,device_id(forwarded())==3,
 file_case(nested())==child(),device_case(nested())==outer(),
 file_method(nested())==test_pass(),device_method(nested())==test_nested(),
 file_failed_methods(nested())==0,device_failed_methods(nested())==1,
 file_assertions(pass())==3,device_assertions(pass())==3,
 file_assertions(fail())==4,device_assertions(fail())==4,
 file_assertions(nested())==3,device_assertions(nested())==4,
 file_failed_assertions(nested())==0,device_failed_assertions(nested())==2,
 file_failed_methods(forwarded())==1,file_assertions(forwarded())==4,file_failed_assertions(forwarded())==2,
 nested().file==snapshot(nested_full_state(),Some(4)),forwarded().file==nested().device,
 nested().file==child_report().file,nested_devices().len()==2,
 pass_state().db.len()==1,fail_state().db.len()==2,nested_full_state().db.len()==4,
 duration_rows(pass_run(false).entries)==1,duration_rows(fail_run(false).entries)==1,
 pass().after.tracked_ids.len()==0,fail().after.tracked_ids.len()==0,nested().after.tracked_ids.len()==0
{raw_neighbors();single_report_neighbors();nested_identity_neighbors();nested_count_neighbors();}
}
fn main() {}
