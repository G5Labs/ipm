use vstd::prelude::*;
verus! {
pub enum Mode { Transaction, NoTransaction, Developer }
pub struct CompilerOutcome { pub descriptor: bool, pub callable: bool }
pub struct Resource { pub definition: bool, pub descriptor: bool, pub callable: bool }
pub struct Outcome { pub resources: Seq<Resource>, pub installed: bool, pub status_ok: bool, pub transaction_level: nat }
// IRIS premise: multicompile attempts every input, can retain a descriptor for a failed class.
// It is deliberately an input: descriptor existence never proves callability.
pub open spec fn compile(mode: Mode, results: Seq<CompilerOutcome>, all_ok: bool, entry: nat) -> Outcome {
    let keep = all_ok || mode != Mode::Transaction;
    Outcome { resources: Seq::new(results.len(), |i:int| Resource {
        definition:keep,descriptor:keep && results[i].descriptor,callable:keep && results[i].callable}),
        installed:keep,status_ok:all_ok,transaction_level:entry }
}
pub proof fn rollback_removes_every_resource(results: Seq<CompilerOutcome>, entry: nat)
    ensures forall|i:int| 0 <= i < results.len() ==> !compile(Mode::Transaction,results,false,entry).resources[i].definition
        && !compile(Mode::Transaction,results,false,entry).resources[i].descriptor
        && !compile(Mode::Transaction,results,false,entry).resources[i].callable
{}
pub proof fn compiler_distinction_is_preserved(mode: Mode, results: Seq<CompilerOutcome>, entry: nat, i:int)
    requires mode != Mode::Transaction, 0 <= i < results.len(), results[i].descriptor, !results[i].callable
    ensures compile(mode,results,false,entry).resources[i].descriptor,
        !compile(mode,results,false,entry).resources[i].callable,
        !compile(mode,results,false,entry).status_ok
{}
pub proof fn two_resource_witness()
    ensures compile(Mode::NoTransaction,seq![CompilerOutcome{descriptor:true,callable:true},CompilerOutcome{descriptor:true,callable:false}],false,0).resources[0].callable,
        !compile(Mode::NoTransaction,seq![CompilerOutcome{descriptor:true,callable:true},CompilerOutcome{descriptor:true,callable:false}],false,0).resources[1].callable,
        compile(Mode::Developer,seq![CompilerOutcome{descriptor:true,callable:true},CompilerOutcome{descriptor:true,callable:false}],false,0).resources[1].descriptor
{}
pub proof fn successful_neighbor(results: Seq<CompilerOutcome>, mode:Mode, entry:nat)
    requires forall|i:int| 0 <= i < results.len() ==> results[i].descriptor && results[i].callable
    ensures compile(mode,results,true,entry).status_ok,
        forall|i:int| 0 <= i < results.len() ==> compile(mode,results,true,entry).resources[i].callable
{}
pub proof fn returns_to_entry(mode:Mode, results:Seq<CompilerOutcome>, all_ok:bool, entry:nat)
    ensures compile(mode,results,all_ok,entry).transaction_level == entry
{}
}
fn main() {}
