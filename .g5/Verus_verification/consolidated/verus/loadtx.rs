use vstd::prelude::*;
verus! {
pub struct Flags {pub no_journal:Option<bool>,pub no_transaction:Option<bool>}
pub open spec fn merge_key(parent:Option<bool>,own:Option<bool>) -> Option<bool> { if parent.is_some() {parent} else {own} }
pub open spec fn effective(parent:Flags,own:Flags) -> Flags {
    Flags{no_journal:merge_key(parent.no_journal,own.no_journal),no_transaction:merge_key(parent.no_transaction,own.no_transaction)}
}
pub open spec fn enabled(v:Option<bool>) -> bool { v == Some(true) }
pub open spec fn own_transaction(f:Flags) -> bool { !enabled(f.no_journal) && !enabled(f.no_transaction) }
pub open spec fn entry_depth(parent_depth:nat,f:Flags) -> nat {parent_depth + if own_transaction(f) {1nat} else {0nat}}
pub proof fn explicit_false_parent_overrides_child()
    ensures merge_key(Some(false),Some(true)) == Some(false), merge_key(None,Some(true)) == Some(true)
{}
pub proof fn no_transaction_child_does_not_cancel_parent(parent_depth:nat,f:Flags)
    requires parent_depth > 0,enabled(f.no_transaction)
    ensures entry_depth(parent_depth,f) == parent_depth,entry_depth(parent_depth,f)>0
{}
// Disjoint write keys are the supported prefix domain. Same-key compare-and-restore is left to the
// inherited loadtx.Store probes; it is not a snapshot rollback law for arbitrary interleavings.
pub struct Write {pub owner:int,pub key:int,pub value:int,pub at:nat,pub journal:bool,pub depth:nat,pub commit_at:Option<nat>}
pub open spec fn survived(w:Write,cut:nat) -> bool {
    w.at < cut && (!w.journal || w.depth == 0 || (w.commit_at.is_some() && w.commit_at.unwrap() < cut))
}
pub open spec fn projected(ws:Seq<Write>,cut:nat) -> Set<int> {
    ws.to_set().filter(|w:Write| survived(w,cut)).map(|w:Write|w.key)
}
pub proof fn cut_before_write_cannot_keep_it(w:Write,cut:nat)
    requires cut <= w.at
    ensures !survived(w,cut)
{}
pub proof fn nojournal_survives_parent_rollback(w:Write,cut:nat)
    requires !w.journal,w.at < cut,w.depth > 0,w.commit_at.is_none()
    ensures survived(w,cut)
{}
pub proof fn journaled_child_cannot_survive_uncommitted_parent(w:Write,cut:nat)
    requires w.journal,w.depth>0,w.commit_at.is_none()
    ensures !survived(w,cut)
{}
pub proof fn homogeneous_transactional_domain(w:Write,cut:nat,outer_commit:nat)
    requires w.journal,w.depth>0,w.commit_at == Some(outer_commit),w.at < outer_commit
    ensures survived(w,cut) == (outer_commit < cut)
{}
pub proof fn prefix_is_monotone(w:Write,a:nat,b:nat)
    requires a<=b,survived(w,a)
    ensures survived(w,b)
{}
pub proof fn impossible_partial_survival(a:Write,b:Write,cut:nat,commit:nat)
    requires a.journal,b.journal,a.depth>0,b.depth>0,a.commit_at==Some(commit),b.commit_at==Some(commit),
        a.at<commit,b.at<commit
    ensures survived(a,cut) == survived(b,cut)
{homogeneous_transactional_domain(a,cut,commit);homogeneous_transactional_domain(b,cut,commit);}
pub enum End { Return, Terminate, Kill, InstanceRestart }
pub struct EndState {pub row:bool,pub temp:bool,pub ended:bool,pub finalized:bool,pub transaction_depth:nat}
// Boundary premises: Terminate calls destructors; Kill does not. Restart loses IRISTEMP.
// A permanent row may already have been deleted by rollback; finalized is not invented for a missing row.
pub open spec fn settle(kind:End,row_durable:bool,temp_exists:bool,temp_ended:bool) -> EndState {
    let destructor=kind==End::Return || kind==End::Terminate;
    EndState {row:row_durable,temp:temp_exists && !destructor && kind!=End::InstanceRestart,
        ended:row_durable && destructor && temp_ended,finalized:row_durable && destructor,transaction_depth:0}
}
pub proof fn process_kill_is_not_instance_restart()
    ensures settle(End::Kill,true,true,true).temp,!settle(End::InstanceRestart,true,true,true).temp,
        !settle(End::Kill,true,true,true).finalized,settle(End::Terminate,true,true,true).finalized
{}
pub open spec fn retry(pre:Map<int,int>,keys:Seq<int>,values:Seq<int>) -> Map<int,int>
    recommends keys.len()==values.len()
    decreases keys.len()
{
    if keys.len()==0 {pre} else {retry(pre.insert(keys[0],values[0]),keys.drop_first(),values.drop_first())}
}
pub proof fn retry_empty_preserves_foreign(pre:Map<int,int>) ensures retry(pre,seq![],seq![])==pre {}
pub proof fn retry_installs_single_after_any_settled_cut(pre:Map<int,int>,key:int,value:int)
    ensures retry(pre,seq![key],seq![value])[key]==value
{reveal_with_fuel(retry,3);assert(seq![key].drop_first() =~= Seq::<int>::empty());}
pub proof fn retry_preserves_foreign(pre:Map<int,int>,keys:Seq<int>,values:Seq<int>,foreign:int)
    requires keys.len()==values.len(),pre.contains_key(foreign),!keys.contains(foreign)
    ensures retry(pre,keys,values).contains_key(foreign),retry(pre,keys,values)[foreign]==pre[foreign]
    decreases keys.len()
{
    if keys.len()>0 { retry_preserves_foreign(pre.insert(keys[0],values[0]),keys.drop_first(),values.drop_first(),foreign); }
}
}
fn main() {}
