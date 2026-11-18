// Iteration75 (repair group R8B): sync -delete baselines and the compile set after resolution. IPM 571e04a1.
//
// sync.rs::sync_file is inherited unchanged and faithful for the baseline: with -delete, Pipeline.cls:140-149 runs
// SyncApplyDeletes (deletes the server document of a compilable resource only, :535-578) and then
// FileHash.cls:366-411 CommitChanges, which deletes the baseline row of every routed deleted path
// (processDeletes, :394-405). Without -delete the baseline row stays. Unrouted paths were dropped before
// (SyncReportUnroutedPaths). A SyncCompile failure throws before CommitChanges; that case is not represented.
//
// sync_compile.rs::eligible / ::contributed are inherited unchanged and faithful: Pipeline.cls:690-749 SyncCompile
// skips a resource whose processor has CompileFromProject (:710-712) before it resolves the children, so such a
// resource neither contributes to the compile set nor warns.
#[path="../../consolidated/iter32/sync.rs"]
pub mod sy;
#[path="../../consolidated/iter4/sync_compile.rs"]
pub mod sc;
use vstd::prelude::*;

verus! {

use sy::{File, sync, deleted, nothing};
use sc::{SyncRes, Child, sync_compile, pkg, compile_set_size, warnings, kept};

// devtools.sync#5: -delete removes the baseline row of a deleted file (compilable or not); without -delete the
// row stays.
pub proof fn delete_flag_removes_baseline(fs: Seq<File>, m: int, i: int)
    requires 0 <= i < fs.len(), fs[i].owner == m, deleted(fs[i]), !nothing(fs, m, true)
    ensures sync(fs, m, true).files[i].base.is_none()
{}

pub proof fn no_flag_keeps_baseline(fs: Seq<File>, m: int, i: int)
    requires 0 <= i < fs.len(), deleted(fs[i])
    ensures sync(fs, m, false).files[i].base == fs[i].base, sync(fs, m, false).files[i].base.is_some()
{}

pub open spec fn gone(compilable: bool) -> File {
    File { owner: 1, routed: true, compilable, disk: None, base: Some(7), ns: Some(7) }
}

pub proof fn baseline_neighbors() {
    let with_flag = sync(seq![gone(true), gone(false)], 1, true);
    let without = sync(seq![gone(true), gone(false)], 1, false);
    reveal_with_fuel(sy::count_mod, 3);
    reveal_with_fuel(sy::count_del, 3);
    assert(!nothing(seq![gone(true), gone(false)], 1, true));
    assert(with_flag.files[0].base.is_none() && with_flag.files[0].ns.is_none());
    assert(with_flag.files[1].base.is_none() && with_flag.files[1].ns.is_some());
    assert(without.files[0].base == Some(7int) && without.files[0].ns == Some(7int));
}

// devtools.sync.compile-after-resolve#5: a resource compiled from a deployed project is skipped even when its
// children resolve, and is not warned about; the same resource without CompileFromProject is compiled.
pub proof fn project_resource_neighbors() {
    reveal_with_fuel(compile_set_size, 2);
    reveal_with_fuel(warnings, 2);
    reveal_with_fuel(kept, 2);
    let project = SyncRes { from_project: true, ..pkg(true) };
    let r = sync_compile(seq![project], true, true);
    assert(!r.compile_called && r.ok && r.printed == 0);
    let plain = sync_compile(seq![pkg(true)], true, true);
    assert(plain.compile_called);
}

}
fn main() {}
