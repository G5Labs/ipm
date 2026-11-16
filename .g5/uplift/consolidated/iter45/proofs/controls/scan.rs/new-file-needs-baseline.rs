#![allow(non_snake_case)]
// Iteration45: sync file discovery feeding change detection.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter45/bindings.json):
//   Storage/FileHash.cls::WalkAndHashDirs, WalkAndHashFilesPython(Impl)   recursive walk of each scan directory,
//                                                                         skipping .git/__pycache__/node_modules;
//                                                                         an unreadable file hashes to ""
//   Storage/FileHash.cls::NormalizePath, RelPathToDocName, CanonicalDocName   path and document naming, extension
//                                                                         case-insensitive
//   General/Sync/Pipeline.cls::Run                                       a load error fails the sync and leaves the
//                                                                         baseline uncommitted, so the next sync retries
//
// A file is one class-like document in a scanned directory at any depth. Premises (native-qualified):
// Python os.walk and hashlib, the SQL fallback is not exercised, and the loader reads the file.
use vstd::prelude::*;

verus! {

pub struct F { pub depth: nat, pub upper_ext: bool, pub skip_dir: bool, pub readable: bool, pub disk: int, pub base: Option<int>, pub ns: Option<int> }

pub open spec fn walked(f: F) -> bool { !f.skip_dir }
pub open spec fn changed(f: F) -> bool { walked(f) && f.base.is_some() && f.base != Some(f.disk) }

pub struct Out { pub ok: bool, pub files: Seq<F> }

pub open spec fn any_unreadable_change(fs: Seq<F>) -> bool
    decreases fs.len()
{
    if fs.len() == 0 { false } else { (changed(fs.last()) && !fs.last().readable) || any_unreadable_change(fs.drop_last()) }
}

// A changed unreadable file fails the load; nothing is committed. Otherwise every changed file loads.
pub open spec fn sync(fs: Seq<F>) -> Out {
    if any_unreadable_change(fs) { Out { ok: false, files: fs } }
    else { Out { ok: true, files: Seq::new(fs.len(), |i: int| if changed(fs[i]) { F { ns: Some(fs[i].disk), base: Some(fs[i].disk), ..fs[i] } } else { fs[i] }) } }
}

pub proof fn depth_and_case_do_not_matter(f: F, d: nat, u: bool)
    requires walked(f), f.readable, f.base != Some(f.disk)
    ensures sync(seq![F { depth: d, upper_ext: u, ..f }]).ok,
            sync(seq![F { depth: d, upper_ext: u, ..f }]).files[0].ns == Some(f.disk)
{
    reveal_with_fuel(any_unreadable_change, 2);
}

pub proof fn unreadable_fails_then_retries(f: F)
    requires walked(f), !f.readable, f.base != Some(f.disk)
    ensures !sync(seq![f]).ok, sync(seq![f]).files[0] == f,
            sync(seq![F { readable: true, ..sync(seq![f]).files[0] }]).files[0].ns == Some(f.disk)
{
    reveal_with_fuel(any_unreadable_change, 2);
}

pub proof fn skipped_directory_never_synced(f: F)
    requires f.skip_dir
    ensures sync(seq![f]).files[0] == f
{
    reveal_with_fuel(any_unreadable_change, 2);
}

// Native schedule: Inner (depth 2), include, Up.CLS (new, uppercase), Top (unreadable then readable).
pub open spec fn inner(v: int, b: int) -> F { F { depth: 2, upper_ext: false, skip_dir: false, readable: true, disk: v, base: Some(b), ns: Some(b) } }
pub proof fn schedule()
    ensures
        sync(seq![inner(2, 1)]).files[0].ns == Some(2int),
        sync(seq![F { depth: 1, upper_ext: true, skip_dir: false, readable: true, disk: 3, base: None, ns: None }]).files[0].ns == Some(3int),
        !sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: false, disk: 4, base: Some(1), ns: Some(1) }]).ok,
        sync(seq![F { depth: 1, upper_ext: false, skip_dir: false, readable: true, disk: 4, base: Some(1), ns: Some(1) }]).files[0].ns == Some(4int),
{
    reveal_with_fuel(any_unreadable_change, 2);
}

} // verus!

fn main() {}
