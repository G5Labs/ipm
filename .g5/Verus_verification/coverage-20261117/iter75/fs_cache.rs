//! Iteration-75 neighbours for the filesystem repository cache refresh (repo.filesystem#4 and #8).
//!
//! The deciding computation is the inherited `owners.rs::build_cache` with `scan`, `add_cache` and
//! `scanned_error`, which follow Repo/Filesystem/Definition.cls BuildCache (lines 117-182):
//! - :151 and ScanDirectory :329 skip a manifest deeper than a nonzero Depth;
//! - AddCacheItem :196-200 skips a file $system.OBJ.Load cannot load, and :225-227 returns the failed
//!   %Save of a cache row whose Name fails %IPM.DataType.ModuleName validation;
//! - :170-175 aggregates every item status and throws after the loop, so :179 rolls back the purge and
//!   the saves of the whole refresh.
//! Native case TR7 (.g5/Verus_verification/consolidated/native/results/20261114T025102Z-r1-temp-repo)
//! observed a valid module beside an invalid-name one in .modules: no repository was created, while TR7b
//! with a valid unused module succeeded.
#[path = "../../consolidated/verus/owners.rs"]
mod owners;

use vstd::prelude::*;
use owners::*;

verus! {

pub open spec fn valid1() -> Entry { Entry { key: 1, depth: 1, manifest: Manifest::Valid } }
pub open spec fn invalid2() -> Entry { Entry { key: 2, depth: 2, manifest: Manifest::InvalidName } }
pub open spec fn unreadable3() -> Entry { Entry { key: 3, depth: 1, manifest: Manifest::Unreadable } }
pub open spec fn old_repo() -> Repository { Repository { exists: true, cache: set![9nat] } }

/// repo.filesystem#4: an invalid name within scan depth fails the refresh and leaves the old rows; the
/// same tree with the invalid manifest beyond the depth succeeds and replaces the cache.
pub proof fn invalid_name_fails_whole_refresh() {
    reveal_with_fuel(scanned_error, 3);
    reveal_with_fuel(scanned_keys, 3);
    let es = seq![valid1(), invalid2()];
    assert(es[0] == valid1() && es[1] == invalid2() && es.len() == 2);
    let failed = build_cache(old_repo(), es, 0, true, false, true, 0);
    assert(!failed.ok);
    assert(failed.repo == old_repo());
    let beyond = build_cache(old_repo(), es, 1, true, false, true, 0);
    assert(beyond.ok);
    assert(beyond.repo.cache =~= set![1nat]);
    // An unreadable manifest is skipped instead.
    let es3 = seq![valid1(), unreadable3()];
    assert(es3[0] == valid1() && es3[1] == unreadable3() && es3.len() == 2);
    let skipped = build_cache(old_repo(), es3, 0, true, false, true, 0);
    assert(skipped.ok);
    assert(skipped.repo.cache =~= set![1nat]);
}

/// The intended skip of the working assumption: an invalid name is skipped like an unreadable manifest.
pub open spec fn intended_add(e: Entry) -> CacheAction {
    match e.manifest { Manifest::Valid => CacheAction::Cached, _ => CacheAction::Skipped }
}

/// repo.filesystem#8 is refuted as built: with a valid module beside an invalid-name one, the refresh of a
/// new repository fails and the valid module is not offered, where the intended skip would offer it.
pub proof fn intended_skip_refuted() {
    reveal_with_fuel(scanned_error, 3);
    let es = seq![valid1(), invalid2()];
    assert(es[0] == valid1() && es[1] == invalid2() && es.len() == 2);
    let fresh = Repository { exists: false, cache: Set::empty() };
    let out = build_cache(fresh, es, 0, true, false, true, 0);
    assert(!out.ok);
    assert(!out.repo.exists);
    assert(!out.repo.cache.contains(1));
    assert(intended_add(valid1()) is Cached && intended_add(invalid2()) is Skipped);
    assert(add_cache(invalid2()) is Error);
}

fn main() {}

} // verus!
