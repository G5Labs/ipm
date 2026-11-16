// Finite IRIS boundary contract, not an ObjectScript implementation proof.
// Provider premises and native table: TRUST.md. No axiomatic proof assumptions.
use vstd::prelude::*;
verus! {
pub struct History { pub temp:bool, pub row:bool, pub final_row:bool, pub buffered:bool }
pub enum Loss { Process, Instance }
pub open spec fn initialized() -> History { History{temp:true,row:true,final_row:false,buffered:false} }
pub open spec fn lose(h:History,kind:Loss,durable_database:bool) -> History {
    match kind {
        Loss::Process=>h,
        Loss::Instance=>History{temp:false,row:h.row && durable_database,final_row:h.final_row && durable_database,buffered:false}
    }
}
// save_after_delete and replay_on_close describe distinct observed IRIS provider schedules.
pub open spec fn delete(h:History) -> History { History{row:false,final_row:false,..h} }
pub open spec fn persist(h:History,save_after_delete:bool) -> History {
    History{row:h.row || save_after_delete,final_row:h.row || save_after_delete,buffered:h.row || save_after_delete,..h}
}
pub open spec fn close(h:History,replay_on_close:bool) -> History {
    History{temp:false,row:h.row || (h.buffered && replay_on_close),final_row:h.final_row || (h.buffered && replay_on_close),buffered:false}
}
pub open spec fn accessible_schedule() -> History { close(persist(delete(initialized()),false),true) }
pub open spec fn locked_schedule() -> History { close(delete(persist(initialized(),false)),true) }
pub struct Lock { pub depth:nat, pub held:bool, pub deferred:bool, pub immediate:bool }
pub open spec fn new_lock(depth:nat,immediate:bool) -> Lock { Lock{depth,held:true,deferred:false,immediate} }
pub open spec fn close_lock(l:Lock) -> Lock {
    Lock{held:l.held && !l.immediate && l.depth>0,deferred:l.held && !l.immediate && l.depth>0,..l}
}
pub open spec fn commit(l:Lock) -> Lock {
    Lock{depth:if l.depth>0 {(l.depth-1) as nat} else {0},held:l.held && !(l.deferred && l.depth<=1),deferred:l.deferred && l.depth>1,..l}
}
// BuildCache's local manager is constructed with the default immediate=true.
pub open spec fn cache_return(outer:nat) -> Lock { close_lock(Lock{depth:outer,..new_lock(outer+1,true)}) }
pub proof fn process_loss_preserves_temporary_history(h:History)
    ensures lose(h,Loss::Process,true)==h
{}
pub proof fn instance_restart_drops_temp_only_under_durable_database(h:History)
    ensures !lose(h,Loss::Instance,true).temp,
        lose(h,Loss::Instance,true).row==h.row,
        !lose(h,Loss::Instance,false).row
{}
pub proof fn delete_finalize_distinguishes_schedules() {
    assert(!accessible_schedule().row) by (compute);
    assert(locked_schedule().row && locked_schedule().final_row) by (compute);
    assert(!accessible_schedule().temp && !locked_schedule().temp) by (compute);
}
pub proof fn constructor_at_actual_transaction_boundary(depth:nat)
    requires depth>0
    ensures close_lock(new_lock(depth,false)).held,
        !close_lock(new_lock(depth,true)).held,
        !commit(close_lock(new_lock(1,false))).held,
        commit(close_lock(new_lock(depth+1,false))).held
{}
pub proof fn default_cache_return_does_not_hold_outer_lock(outer:nat)
    ensures !cache_return(outer).held
{}
pub proof fn reachable_old_outer_lock_counterexample() {
    assert(!cache_return(1).held) by (compute);
}
pub struct CPF { pub database_sentinel:bool, pub config_role:bool, pub ok:bool }
// The child process controls configuration. These branches preserve success and refusal;
// they do not prove every possible configuration merge or compensate partial effects.
pub open spec fn cpf_do_merge(file_exists:bool,streams_ok:bool,child_exit:int,provider_role:bool) -> CPF {
    CPF{database_sentinel:true,config_role:provider_role,ok:file_exists && streams_ok && child_exit==0}
}
pub open spec fn rollback_cpf(c:CPF) -> CPF { CPF{database_sentinel:false,..c} }
pub proof fn cpf_status_does_not_establish_actual_configuration(role:bool)
    ensures cpf_do_merge(true,true,0,role).ok,
        !cpf_do_merge(true,true,7,role).ok,
        rollback_cpf(cpf_do_merge(true,true,0,role)).config_role==role
{}
pub proof fn cpf_native_valid_invalid_witnesses() {
    assert(rollback_cpf(cpf_do_merge(true,true,0,true)).config_role) by (compute);
    assert(!rollback_cpf(cpf_do_merge(true,true,7,false)).config_role) by (compute);
    assert(!rollback_cpf(cpf_do_merge(true,true,0,true)).database_sentinel) by (compute);
    assert(!cpf_do_merge(false,true,0,false).ok) by (compute);
}
}
verus! { proof fn old_claim() {assert(cache_return(1).held);} }
fn main() {}
