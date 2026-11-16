use vstd::prelude::*;
verus! {
// One class/method key shared by two installed modules. Class-list discovery,
// IRIS save atomicity and successful callbacks are provider boundaries. The
// observable counter instruments entries; the sample payload assignment is
// idempotent. No at-most-once business requirement is inferred.
pub struct Row {pub home:int,pub complete:bool,pub ok:bool}
pub struct Run {pub module:int,pub lock:int,pub due:bool,pub entered:bool,pub snapshot:Row}
pub struct State {pub locks:Set<int>,pub row:Option<Row>,pub runs:Map<int,Run>,pub count:nat,pub value:int,pub results:Map<int,bool>}
pub struct Attempt {pub state:State,pub started:bool,pub ok:bool}
pub enum Save {Snapshot,Delta}
pub open spec fn initial()->State {State{locks:Set::empty(),row:None,runs:Map::empty(),count:0,value:0,results:Map::empty()}}
pub open spec fn lock_key(module:int,shared:bool)->int {if shared{0}else{module}}
pub open spec fn begin(s:State,job:int,module:int,shared:bool)->Attempt
 recommends !s.runs.contains_key(job)
{
 let lock=lock_key(module,shared);
 if s.locks.contains(lock){Attempt{state:State{results:s.results.insert(job,true),..s},started:false,ok:true}}else{
  let prior=match s.row{Some(r)=>r,None=>Row{home:module,complete:false,ok:true}};
  let row=Row{home:module,..prior};
  let run=Run{module,lock,due:!row.complete,entered:false,snapshot:row};
  Attempt{state:State{locks:s.locks.insert(lock),row:Some(row),runs:s.runs.insert(job,run),count:s.count,value:s.value,results:s.results},started:true,ok:true}
 }
}
pub open spec fn enter(s:State,job:int)->State {
 if s.runs.contains_key(job) && s.runs[job].due && !s.runs[job].entered {
  State{count:s.count+1,value:1,runs:s.runs.insert(job,Run{entered:true,..s.runs[job]}),..s}
 }else{s}
}
pub open spec fn finish(s:State,job:int,save:Save)->State {
 if s.runs.contains_key(job) && (s.runs[job].entered || !s.runs[job].due) {
  let r=s.runs[job];
  let row=if r.entered {Some(Row{home:match save{Save::Snapshot=>r.snapshot.home,Save::Delta=>match s.row{Some(x)=>x.home,None=>r.module}},complete:true,ok:true})}else{s.row};
  State{locks:s.locks.remove(r.lock),runs:s.runs.remove(job),row,count:s.count,value:s.value,results:s.results.insert(job,true)}
 }else{s}
}
pub open spec fn a()->State {enter(begin(initial(),10,1,false).state,10)}
pub open spec fn both()->State {enter(begin(a(),20,2,false).state,20)}
pub open spec fn after_b()->State {finish(both(),20,Save::Snapshot)}
pub open spec fn after_a()->State {finish(after_b(),10,Save::Snapshot)}
pub open spec fn same_attempt()->Attempt {begin(a(),20,1,false)}
pub open spec fn serial()->State {
 let first=finish(a(),10,Save::Snapshot);
 finish(enter(begin(first,20,2,false).state,20),20,Save::Snapshot)
}
pub open spec fn repaired_attempt()->Attempt {
 let first=enter(begin(initial(),10,1,true).state,10);
 begin(first,20,2,true)
}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub proof fn busy_preserves_state(s:State,j:int,m:int,shared:bool)
 requires s.locks.contains(lock_key(m,shared))
 ensures begin(s,j,m,shared).state.row==s.row,
 begin(s,j,m,shared).state.locks==s.locks,begin(s,j,m,shared).state.runs==s.runs,
 begin(s,j,m,shared).state.count==s.count,begin(s,j,m,shared).state.value==s.value,
 begin(s,j,m,shared).state.results[j],!begin(s,j,m,shared).started,begin(s,j,m,shared).ok
{}
pub proof fn enter_once(s:State,j:int)
 ensures enter(enter(s,j),j)==enter(s,j),enter(s,j).count<=s.count+1
{}
pub proof fn completed_success_is_not_due(s:State,j:int,m:int)
 requires s.row.is_some(),s.row.unwrap().complete,s.row.unwrap().ok,!s.locks.contains(m)
 ensures !begin(s,j,m,false).state.runs[j].due
{}
pub proof fn shared_key_neighbors() {
 assert(a().locks.contains(1));assert(!a().locks.contains(2));
 assert(a().row.unwrap().home==1);assert(!a().row.unwrap().complete);
 assert(both().count==2);assert(both().value==1);assert(both().locks.contains(2));
 assert(after_b().row.unwrap().home==2);assert(after_b().row.unwrap().complete);
 assert(after_a().row.unwrap().home==1);assert(after_a().row.unwrap().complete);
 assert(after_a().count==2);assert(after_a().value==1);
 assert(after_a().results[10]);assert(after_a().results[20]);
 assert(finish(after_b(),10,Save::Delta).row.unwrap().home==2);
 assert(!same_attempt().started);assert(same_attempt().ok);assert(same_attempt().state.results[20]);assert(same_attempt().state.count==1);
 assert(serial().count==1);assert(serial().value==1);assert(serial().results[10]);assert(serial().results[20]);assert(serial().row.unwrap().home==2);
 assert(!repaired_attempt().started);assert(repaired_attempt().state.count==1);
 let failed=State{row:Some(Row{home:1,complete:true,ok:false}),..initial()};
 assert(begin(failed,10,1,false).state.runs[10].due);
 assert(enter(begin(failed,10,1,false).state,10).count==1);
}
}
fn main(){}
