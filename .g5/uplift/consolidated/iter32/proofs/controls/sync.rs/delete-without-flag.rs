#![allow(non_snake_case)]
// Iteration32: developer-mode `sync [-delete]` change detection, routing and deletion, the
// manifest-change path, and whole-namespace order.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter32/bindings.json):
//   ResourceProcessor/Default/Document.cls::GetSyncDirectory, OnItemRelativePath   tracked paths = {SourcesRoot}/{Directory}/...
//   ResourceProcessor/Default/Document.cls::OnPhase (Reload)                        load also tries {SourcesRoot}/ when Directory is not declared
//   Storage/FileHash.cls::CollectScanDirs, ComputeChanges, CommitChanges             modified / deleted against the baseline
//   General/Sync/Pipeline.cls::Run, SyncBuildReverseIndex, SyncRoutePathSet,
//     SyncApplyDeletes, SyncCheckModuleXml, SyncCommitModuleXml                      routing, -delete, manifest reload
//   Main.cls::Sync, Utils/Module.cls::GetDevModeModulesInDependencyOrder             whole-namespace order
//
// A file abstracts one class document: `tracked` means its path lies under the directory sync scans
// and a declared resource routes it (conventional layout); a flat-layout file loads through the
// load fallback but is never tracked. Hash equality is abstracted as value equality. Premises
// (native-qualified): the SHA-1 provider distinguishes the edited files, $system.OBJ.Delete removes
// a class definition, and the loader compiles a loaded class.
use vstd::prelude::*;

verus! {

pub struct File { pub owner: int, pub routed: bool, pub compilable: bool, pub disk: Option<int>, pub base: Option<int>, pub ns: Option<int> }

pub open spec fn modified(f: File) -> bool { f.routed && f.disk.is_some() && f.disk != f.base }
pub open spec fn deleted(f: File) -> bool { f.routed && f.base.is_some() && f.disk.is_none() }

pub open spec fn count_mod(fs: Seq<File>, m: int) -> int
    decreases fs.len()
{
    if fs.len() == 0 { 0 } else { count_mod(fs.drop_last(), m) + if fs.last().owner == m && modified(fs.last()) { 1int } else { 0int } }
}
pub open spec fn count_del(fs: Seq<File>, m: int) -> int
    decreases fs.len()
{
    if fs.len() == 0 { 0 } else { count_del(fs.drop_last(), m) + if fs.last().owner == m && deleted(fs.last()) { 1int } else { 0int } }
}

// Pipeline.Run's early exit: nothing modified, and either nothing deleted or -delete absent.
pub open spec fn nothing(fs: Seq<File>, m: int, del: bool) -> bool { count_mod(fs, m) == 0 && (count_del(fs, m) == 0 || !del) }

// Per-file effect of one module sync. Modified files load and their hash commits. With -delete, a
// deleted file routed to a compilable resource loses its document; its baseline row is removed.
pub open spec fn sync_file(f: File, m: int, del: bool, skip: bool) -> File {
    if f.owner != m || skip { f }
    else if modified(f) { File { ns: f.disk, base: f.disk, ..f } }
    else if deleted(f) { File { ns: if f.compilable { None } else { f.ns }, base: None, ..f } }
    else { f }
}

pub struct Outcome { pub files: Seq<File>, pub nothing: bool, pub not_applied: int, pub deleted: int, pub updated: int }

pub open spec fn sync(fs: Seq<File>, m: int, del: bool) -> Outcome {
    let skip = nothing(fs, m, del);
    Outcome {
        files: Seq::new(fs.len(), |i: int| sync_file(fs[i], m, del, skip)),
        nothing: skip,
        not_applied: if skip && count_del(fs, m) > 0 && !del { count_del(fs, m) } else { 0 },
        deleted: if !skip && del { count_del(fs, m) } else { 0 },
        updated: if skip { 0 } else { count_mod(fs, m) },
    }
}

// Generic neighbours (pointwise, any file set).
pub proof fn no_delete_without_flag(fs: Seq<File>, m: int, i: int)
    requires 0 <= i < fs.len(), fs[i].ns.is_some()
    ensures sync(fs, m, false).files[i].ns.is_some()
{}

pub proof fn untracked_never_synced(fs: Seq<File>, m: int, del: bool, i: int)
    requires 0 <= i < fs.len(), !fs[i].routed
    ensures sync(fs, m, del).files[i] == fs[i]
{}

pub proof fn delete_flag_removes_compilable(fs: Seq<File>, m: int, i: int)
    requires 0 <= i < fs.len(), fs[i].owner == m, deleted(fs[i]), fs[i].compilable, !nothing(fs, m, true)
    ensures sync(fs, m, true).files[i].ns.is_none()
{}

// Load: a file loads when its layout is conventional, or flat with no declared Directory.
pub open spec fn loads(conventional: bool, directory_declared: bool) -> bool { conventional || !directory_declared }
// Sync scans {SourcesRoot}/{Directory} only.
pub open spec fn scanned(conventional: bool) -> bool { conventional }

pub proof fn flat_layout_loads_but_is_not_synced()
    ensures loads(false, false), !scanned(false), loads(true, false), scanned(true)
{}

// Fixture: files 0..3 SyncC.A,B,C and SyncD.D (module 0, conventional), 4 SyncE.E (module 0, routed
// only after the manifest adds SyncE.PKG), 5..6 SyncF.A,B (module 1, flat layout).
pub open spec fn f(owner: int, routed: bool, v: Option<int>) -> File { File { owner, routed, compilable: true, disk: v, base: if routed { v } else { None }, ns: v } }
pub open spec fn start() -> Seq<File> {
    seq![f(0, true, Some(1)), f(0, true, Some(1)), f(0, true, Some(1)), f(0, true, Some(1)),
         File { owner: 0, routed: false, compilable: true, disk: None, base: None, ns: None },
         f(1, scanned(false), Some(1)), f(1, scanned(false), Some(1))]
}
pub open spec fn set_disk(fs: Seq<File>, i: int, v: Option<int>) -> Seq<File> { fs.update(i, File { disk: v, ..fs[i] }) }
// SyncCheckModuleXml reloads the manifest: SyncD.D.CLS is replaced by SyncE.PKG and E's file exists.
pub open spec fn manifest_change(fs: Seq<File>) -> Seq<File> {
    fs.update(3, File { routed: false, ..fs[3] }).update(4, File { routed: true, disk: Some(5), ..fs[4] })
}
// load -dev of the flat module: loadable files take their disk value (no deletion).
pub open spec fn load_module(fs: Seq<File>, m: int) -> Seq<File> {
    Seq::new(fs.len(), |i: int| if fs[i].owner == m && fs[i].disk.is_some() && loads(fs[i].routed, false) { File { ns: fs[i].disk, ..fs[i] } } else { fs[i] })
}

pub open spec fn s0() -> Outcome { sync(start(), 0, false) }
pub open spec fn s1() -> Outcome { sync(set_disk(s0().files, 1, None), 0, false) }
pub open spec fn s2() -> Outcome { sync(s1().files, 0, true) }
pub open spec fn s3() -> Outcome { sync(set_disk(set_disk(s2().files, 0, Some(2)), 2, None), 0, true) }
pub open spec fn s4() -> Outcome { sync(set_disk(s3().files, 3, None), 0, true) }
pub open spec fn s5() -> Outcome { sync(manifest_change(s4().files), 0, false) }
pub open spec fn f1() -> Outcome { sync(set_disk(s5().files, 5, Some(2)), 1, false) }
pub open spec fn f2() -> Outcome { sync(set_disk(f1().files, 6, None), 1, true) }
pub open spec fn f3_files() -> Seq<File> { load_module(f2().files, 1) }

pub open spec fn present(fs: Seq<File>, i: int) -> int { if fs[i].ns.is_some() { 1 } else { 0 } }
pub open spec fn value(fs: Seq<File>, i: int) -> int { match fs[i].ns { Some(v) => v, None => -1 } }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub proof fn delete_schedule()
    ensures
        s1().not_applied == 1, present(s1().files, 1) == 1,
        s2().deleted == 1, present(s2().files, 1) == 0, present(s2().files, 0) == 1,
        value(s3().files, 0) == 2, present(s3().files, 2) == 0, s3().updated == 1,
        present(s4().files, 3) == 0,
        value(s5().files, 4) == 5, s5().updated == 1,
{
    reveal_with_fuel(count_mod, 8);
    reveal_with_fuel(count_del, 8);
    assert(s1().not_applied == 1) by (compute);
    assert(present(s1().files, 1) == 1) by (compute);
    assert(s2().deleted == 1) by (compute);
    assert(present(s2().files, 1) == 0) by (compute);
    assert(present(s2().files, 0) == 1) by (compute);
    assert(value(s3().files, 0) == 2) by (compute);
    assert(present(s3().files, 2) == 0) by (compute);
    assert(s3().updated == 1) by (compute);
    assert(present(s4().files, 3) == 0) by (compute);
    assert(value(s5().files, 4) == 5) by (compute);
    assert(s5().updated == 1) by (compute);
}

// As built, a flat-layout module's edit and deletion are both "nothing to sync"; a load picks the edit up.
pub proof fn flat_schedule()
    ensures
        f1().nothing, value(f1().files, 5) == 1,
        f2().nothing, present(f2().files, 6) == 1,
        value(f3_files(), 5) == 2,
{
    reveal_with_fuel(count_mod, 8);
    reveal_with_fuel(count_del, 8);
    assert(f1().nothing) by (compute);
    assert(value(f1().files, 5) == 1) by (compute);
    assert(f2().nothing) by (compute);
    assert(present(f2().files, 6) == 1) by (compute);
    assert(value(f3_files(), 5) == 2) by (compute);
}

// Whole-namespace order. Modules: 0 syncc, 1 syncf, 2 ox, 3 tx, 4 oy, 5 ty, 6 tz (not developer mode).
pub open spec fn dev(m: int) -> bool { m != 6 }
pub open spec fn deps(m: int) -> Seq<int> { if m == 2 { seq![4] } else if m == 3 { seq![6] } else if m == 6 { seq![5] } else { seq![] } }
// Developer-mode rows in row (ID) order, as observed.
pub open spec fn rows() -> Seq<int> { seq![0, 1, 2, 3, 4, 5] }

pub open spec fn mem(s: Seq<int>, x: int) -> bool
    decreases s.len()
{
    if s.len() == 0 { false } else { s.last() == x || mem(s.drop_last(), x) }
}
// Edges restricted to the developer-mode set.
pub open spec fn ready_deps(ds: Seq<int>, emitted: Seq<int>) -> bool
    decreases ds.len()
{
    if ds.len() == 0 { true } else { (!dev(ds.last()) || mem(emitted, ds.last())) && ready_deps(ds.drop_last(), emitted) }
}
// One scan over rows; a module emitted earlier in the same pass counts as emitted.
pub open spec fn pass(rs: Seq<int>, emitted: Seq<int>) -> Seq<int>
    decreases rs.len()
{
    if rs.len() == 0 { emitted }
    else {
        let e = pass(rs.drop_last(), emitted);
        let m = rs.last();
        if !mem(e, m) && ready_deps(deps(m), e) { e.push(m) } else { e }
    }
}
pub open spec fn rest(rs: Seq<int>, emitted: Seq<int>) -> Seq<int>
    decreases rs.len()
{
    if rs.len() == 0 { emitted } else { let e = rest(rs.drop_last(), emitted); if mem(e, rs.last()) { e } else { e.push(rs.last()) } }
}
pub open spec fn order_from(rs: Seq<int>, emitted: Seq<int>, fuel: nat) -> Seq<int>
    decreases fuel
{
    if fuel == 0 || emitted.len() >= rs.len() { emitted }
    else {
        let e = pass(rs, emitted);
        if e.len() == emitted.len() { rest(rs, emitted) } else { order_from(rs, e, (fuel - 1) as nat) }
    }
}
pub open spec fn order() -> Seq<int> { order_from(rows(), seq![], 6) }

pub open spec fn position(s: Seq<int>, x: int) -> int
    decreases s.len()
{
    if s.len() == 0 { 0 } else if position(s.drop_last(), x) != 0 { position(s.drop_last(), x) } else if s.last() == x { s.len() as int } else { 0 }
}

// Direct developer-mode dependency is respected (oy before ox); the transitive one through a
// non-developer module is not (tx before ty), contrary to "dependency order".
pub proof fn order_neighbors()
    ensures
        position(order(), 4) < position(order(), 2),
        position(order(), 3) < position(order(), 5),
        order() == seq![0int, 1, 3, 4, 5, 2],
{
    assert(order() =~= seq![0int, 1, 3, 4, 5, 2]) by (compute);
    assert(position(order(), 4) < position(order(), 2)) by (compute);
    assert(position(order(), 3) < position(order(), 5)) by (compute);
}

} // verus!

fn main() {}
