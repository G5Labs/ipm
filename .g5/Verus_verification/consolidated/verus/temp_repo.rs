use vstd::prelude::*;
verus! {
pub struct Manager { pub created: bool, pub id: int, pub object: bool }
pub struct Created { pub manager: Manager, pub rows: Set<int>, pub warning: bool, pub ok: bool }
pub open spec fn skip_create(directory_exists: bool, root_registered: bool) -> bool {
    !directory_exists || root_registered
}
// Unique Root/Name and BuildCache rollback are environment obligations, not consequences of this spec.
pub open spec fn on_new(rows: Set<int>, id: int, skip: bool, cache_ok: bool) -> Created {
    if skip { Created {manager: Manager {created:false,id,object:true},rows,warning:false,ok:true} }
    else if cache_ok { Created {manager: Manager {created:true,id,object:true},rows:rows.insert(id),warning:false,ok:true} }
    else { Created {manager: Manager {created:true,id,object:false},rows,warning:true,ok:false} }
}
pub open spec fn cleanup(rows: Set<int>, m: Manager) -> (Set<int>,bool) {
    if !m.created { (rows,false) } else { (rows.remove(m.id), !rows.contains(m.id)) }
}
pub proof fn foreign_repository_preserved(rows: Set<int>, m: Manager, foreign: int)
    requires foreign != m.id, rows.contains(foreign)
    ensures cleanup(rows,m).0.contains(foreign)
{}
pub proof fn reused_root_is_not_owned(rows: Set<int>, id: int)
    ensures !on_new(rows,id,true,true).manager.created,
            cleanup(rows,on_new(rows,id,true,true).manager).0 == rows
{}
pub proof fn failed_creation_can_warn_without_row(rows: Set<int>, id: int)
    requires !rows.contains(id)
    ensures !on_new(rows,id,false,false).rows.contains(id),
            on_new(rows,id,false,false).warning, !on_new(rows,id,false,false).manager.object
{}
pub struct Candidate { pub version: nat, pub repository_id: nat, pub local: bool, pub sort: int }
// Version is the result of the native comparator. No assertion of semver totality outside exact release domain.
pub open spec fn better(a: Candidate,b: Candidate) -> bool {
    a.version > b.version || (a.version == b.version && a.repository_id < b.repository_id)
}
pub open spec fn select(a: Candidate,b: Candidate) -> Candidate { if better(a,b) { a } else { b } }
pub proof fn equal_version_older_repository_wins(a: Candidate,b: Candidate)
    requires a.version == b.version, a.repository_id < b.repository_id
    ensures select(a,b) == a
{}
pub proof fn newer_version_beats_repository_priority(a: Candidate,b: Candidate)
    requires a.version > b.version
    ensures select(a,b) == a
{}
pub struct Loaded { pub parent: bool, pub child: bool, pub status_ok: bool, pub history_ok: bool, pub committed: bool }
pub open spec fn dependency_result(dev: bool, failed_manager: bool) -> Loaded {
    Loaded {parent: !failed_manager || dev, child: !failed_manager,
        status_ok: !failed_manager,history_ok: !failed_manager,committed: !failed_manager || dev}
}
pub proof fn dependency_failure_distinguishes_first_commit()
    ensures !dependency_result(false,true).parent, dependency_result(true,true).parent,
        !dependency_result(true,true).child, !dependency_result(true,true).status_ok,
        dependency_result(true,true).committed,dependency_result(false,false).child
{}
pub open spec fn postcommit_cleanup(module: bool, cleanup_ok: bool) -> Loaded {
    Loaded {parent: module,child:false,status_ok:cleanup_ok,history_ok:cleanup_ok,committed:true}
}
pub proof fn cleanup_error_does_not_undo_commit()
    ensures postcommit_cleanup(true,false).parent, !postcommit_cleanup(true,false).status_ok,
        postcommit_cleanup(true,false).committed
{}
}
fn main() {}
