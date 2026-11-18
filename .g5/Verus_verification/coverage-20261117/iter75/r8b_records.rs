// Iteration75 (repair group R8B): update-step record registration and the temporary local repository.
// IPM 571e04a1.
//
// ownership.rs::get_add is inherited unchanged and faithful: Storage/Module.cls:259-272 GetAddUpdateStep opens the
// row by its unique (ClassName, MethodName) index (UpdateStep/AnyMember.cls:8, :18-30; PrimaryOnly.cls likewise)
// or makes a new one, sets HomeModule and saves. TimeStampEnd (completed) and Status (ok) are not touched.
//
// temp_repo.rs::cleanup is inherited unchanged and faithful: General/TempLocalRepoManager.cls:73-86 CleanUp does
// nothing without a repository object id, otherwise %DeleteId and, when that fails, prints the
// "Failed to clean up repository" warning. %DeleteId of a missing row returns an error (IRIS; observed in the C:D2
// receipt below).
//
// temp_repo.rs::on_new writes the failed creation's outcome down (rows unchanged, warning true). The successor
// `on_new75` derives it instead: Create (TempLocalRepoManager.cls:13-31) calls BuildCache
// (Repo/Filesystem/Definition.cls:117-180), which saves the definition inside its own TSTART (:132, :145), so the
// object keeps the id it was given, and rolls back to the entry level on any failure (:179-180). %OnNew
// (TempLocalRepoManager.cls:63-68) then calls CleanUp on the failure path. Native receipt:
// uplift/consolidated/findings-audit-iter28/runs/20261116T031715006352Z-early (TR7 failed creation prints the
// warning and leaves no row; TR2/TR7b successful creation).
#[path="../../consolidated/iter9/ownership.rs"]
pub mod own;
#[path="../../consolidated/verus/temp_repo.rs"]
pub mod tr;
use vstd::prelude::*;

verus! {

use own::{Store, Step, Key, StepKind, get_add, seed, key};
use tr::{Manager, Created, cleanup};

// ---------------------------------------------------------------- update.steps.records#1

pub open spec fn with_row(home: int, completed: bool, ok: bool) -> Store {
    Store { modules: set![1int, 2], steps: Map::empty().insert(key(StepKind::Any), Step { home, completed, ok }) }
}

// Registering module 1's key for module 2 moves the home and keeps completion and status, whatever they were.
pub proof fn get_add_keeps_completion(s: Store, k: Key, home: int)
    requires s.steps.contains_key(k)
    ensures get_add(s, k, home).steps[k].home == home,
        get_add(s, k, home).steps[k].completed == s.steps[k].completed,
        get_add(s, k, home).steps[k].ok == s.steps[k].ok
{}

pub proof fn registration_neighbors() {
    let k = key(StepKind::Any);
    // Done and ok: stays done, so `update` does not run it again for module 2.
    let done = get_add(with_row(1, true, true), k, 2);
    assert(done.steps[k] == Step { home: 2, completed: true, ok: true });
    // Never run: stays due.
    let due = get_add(with_row(1, false, true), k, 2);
    assert(due.steps[k] == Step { home: 2, completed: false, ok: true });
    // Failed last run: the error status is kept.
    let failed = get_add(with_row(1, true, false), k, 2);
    assert(failed.steps[k] == Step { home: 2, completed: true, ok: false });
    // A new key starts due.
    let fresh = get_add(Store { modules: set![1int, 2], steps: Map::empty() }, k, 2);
    assert(fresh.steps[k] == Step { home: 2, completed: false, ok: true });
    // Seeding (not registration) is what marks a row done.
    assert(seed(with_row(1, false, true), k, 2).steps[k].completed);
}

// ---------------------------------------------------------------- load.dependencies.local-repo#6

// BuildCache: TSTART, %Save inserts the row and gives the object its id, then the scan and AddCacheItem calls;
// TCOMMIT when they succeed, TROLLBACK to the entry level when any fails. The object id survives the rollback.
pub struct Build { pub rows: Set<int>, pub object_id: bool }
pub open spec fn build_cache(rows: Set<int>, id: int, items_ok: bool) -> Build {
    let at_tstart = rows;
    let saved = rows.insert(id);
    Build { rows: if items_ok { saved } else { at_tstart }, object_id: true }
}

// %OnNew: SkipCreate, else Create; on failure return the error joined with CleanUp's status.
pub open spec fn on_new75(rows: Set<int>, id: int, skip: bool, items_ok: bool) -> Created {
    if skip { Created { manager: Manager { created: false, id, object: true }, rows, warning: false, ok: true } }
    else {
        let b = build_cache(rows, id, items_ok);
        let m = Manager { created: b.object_id, id, object: true };
        if items_ok { Created { manager: m, rows: b.rows, warning: false, ok: true } }
        else {
            let c = cleanup(b.rows, m);
            Created { manager: Manager { object: false, ..m }, rows: c.0, warning: c.1, ok: false }
        }
    }
}

pub proof fn temp_repo_neighbors(rows: Set<int>, id: int)
    requires !rows.contains(id)
    ensures
        // Failed creation: no row, the manager kept its id, and CleanUp warns about the missing row.
        !on_new75(rows, id, false, false).rows.contains(id),
        on_new75(rows, id, false, false).manager.created,
        on_new75(rows, id, false, false).warning,
        !on_new75(rows, id, false, false).ok,
        // Successful creation: the row exists and the later cleanup removes it without a warning.
        on_new75(rows, id, false, true).rows.contains(id),
        !on_new75(rows, id, false, true).warning,
        !cleanup(on_new75(rows, id, false, true).rows, on_new75(rows, id, false, true).manager).0.contains(id),
        !cleanup(on_new75(rows, id, false, true).rows, on_new75(rows, id, false, true).manager).1,
        // Skipped creation owns nothing and its cleanup is silent.
        !cleanup(rows, on_new75(rows, id, true, true).manager).1,
{}

}
fn main() {}
