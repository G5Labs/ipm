// L1/L2 models of IPM developer sync, file baselines and retry marks (ipm 571e04a1), with refinement proofs.
//
// L2 transcribes, over an unbounded set of module files (a Map keyed by path id):
//   - `sync`                 General/Sync/Pipeline.cls Run #L12-L226: dev check #L29-L31, no-baseline stamp #L43-L48,
//                            ComputeChanges (Storage/FileHash.cls #L228-L336: passes 1-2 hash compare, pass 3 retry
//                            marks, pass 4 test marks), unrouted files dropped (#L120-L123), load + compile
//                            (#L133-L138), CommitChanges (FileHash.cls#L366-L405, Pipeline.cls#L149), MarkTestPending
//                            (#L158-L163), MarkFailed on error before commit (#L210-L222, FileHash.cls#L420-L460);
//   - dev-mode `compile` and `load -dev` again (Lifecycle/Base.cls %Reload #L615-L650, %Compile #L1052-L1246 with the
//                            baseline stamp at #L1235-L1244 unless -skip-sync, StampModule FileHash.cls#L103-L150,
//                            StampOneFileWithHash #L154-L170 sets only the hash);
//   - `reload`               Base.cls %Reload (no compile, no stamp; no-op without a root, #L634);
//   - an edit on disk.
// A file's bytes are an int; a baseline stores the bytes' hash, modelled as the bytes themselves (SHA-1 is trusted
// to have no collision on the files involved: sync_baseline.TRUST.md). Files are never removed from disk (-delete and
// module.xml changes are not modelled). The compile outcome and, on failure, the set of files already loaded are
// environment choices (step arguments).
//
// L1 (SYNC-B8, B15, B16, B18): sync, compile and reload only ever put a file's current disk bytes on the server;
// a sync that reports success leaves the server copy of every file a resource owns equal to its disk bytes.
// L1 outcome (SYNC-B1, B16, B31; both directions): a sync reports success exactly when the module is in developer
// mode and every step it ran worked (no change to apply, or the loads and compile worked and, with -test, the tests
// passed); a refused sync, a failed load or compile, or a failed test reports failure.
//
// Proved (sync_lemma_*): every L2 step refines its L1 step and keeps the invariant "an unmarked baseline equals the
// server copy", under named premises. Proved defect witnesses (sync_witness_*): F-SYNC-1, F-SYNC-2.
// Named premises:
//   OUT  (defect, F-SYNC-1) a load, compile or reload outside sync that records no baseline (it fails, it is a
//        reload, or it runs with -skip-sync) loads no file whose disk bytes differ from its unmarked baseline;
//   BASE (defect, F-SYNC-2) a sync that finds no baseline runs only when every owned file's server copy equals its
//        disk bytes.
// Premise declarations read by scripts/ontology_index.py (a P tag must name each label its lemma requires):
// PREMISE premise_out = OUT
// PREMISE premise_out_reload = OUT
// PREMISE premise_base = BASE
// Trust and scope: models/verus/sync_baseline.TRUST.md.
use vstd::prelude::*;

verus! {

pub struct F {
    pub disk: int,              // bytes on disk
    pub server: Option<int>,    // the server copy (source loaded in the namespace), None if never loaded
    pub hash: Option<int>,      // %IPM.Storage.FileHash row: Hash (None: no row)
    pub failed: bool,           // SyncFailed
    pub pending: bool,          // TestPending
    pub owned: bool,            // a resource of the module owns the file (it is in the reverse index)
    pub test: bool,             // the owning resource is a test resource
}

pub struct M { pub files: Map<int, F>, pub dev: bool }

// ---------------------------------------------------------------- L2

// HasBaseline (FileHash.cls#L501-L510): any row for the module.
pub open spec fn has_baseline(m: M) -> bool { exists |p: int| m.files.dom().contains(p) && (#[trigger] m.files[p]).hash is Some }

// ComputeChanges passes 1-2 (CompareOneFile #L341-L362: no row or a different hash), pass 3 (SyncFailed), pass 4
// (TestPending, only with -test); then SyncReportUnroutedPaths drops a file no resource owns (Pipeline.cls#L123).
pub open spec fn detected(f: F, run_tests: bool) -> bool {
    f.owned && (f.hash != Some(f.disk) || f.failed || (run_tests && f.pending))
}

pub open spec fn any_detected(m: M, run_tests: bool) -> bool {
    exists |p: int| m.files.dom().contains(p) && detected(#[trigger] m.files[p], run_tests)
}

// StampModule: every file in a tracked folder gets Hash = its disk hash; marks are kept (#L154-L170).
pub open spec fn stamp_all(m: M) -> M {
    M { files: Map::new(m.files.dom(), |p: int| F { hash: Some(m.files[p].disk), ..m.files[p] }), ..m }
}

// Success: OnSync/load of each changed file, SyncCompile, CommitChanges (Hash, SyncFailed = 0, TestPending = 0 when
// tests run), then MarkTestPending for changed test files when tests do not run.
pub open spec fn commit_one(f: F, run_tests: bool) -> F {
    if detected(f, run_tests) {
        F { server: Some(f.disk), hash: Some(f.disk), failed: false,
            pending: if run_tests { false } else { f.pending || f.test }, ..f }
    } else { f }
}

// Failure before the commit: the files loaded so far stay on the server; MarkFailed flags every changed file that
// has a row (a file with no row already reads as changed, #L434-L460).
pub open spec fn fail_one(f: F, run_tests: bool, was_loaded: bool) -> F {
    if detected(f, run_tests) {
        F { server: if was_loaded { Some(f.disk) } else { f.server }, failed: f.failed || f.hash is Some, ..f }
    } else { f }
}

pub open spec fn sync_state(m: M, run_tests: bool, ok: bool, loaded: Set<int>) -> M {
    if !m.dev { m }                                                    // "Sync requires dev mode." (#L29-L31)
    else if !has_baseline(m) { stamp_all(m) }                          // "Baseline established." (#L43-L48)
    else if !any_detected(m, run_tests) { m }                          // "Nothing to sync." (#L92-L102)
    else if ok { M { files: Map::new(m.files.dom(), |p: int| commit_one(m.files[p], run_tests)), ..m } }
    else { M { files: Map::new(m.files.dom(), |p: int| fail_one(m.files[p], run_tests, loaded.contains(p))), ..m } }
}

// The command's result. A failing test after the commit fails the command but changes nothing more (#L205-L208).
pub open spec fn sync_ok(m: M, run_tests: bool, ok: bool, tests_ok: bool) -> bool {
    if !m.dev { false }
    else if !has_baseline(m) || !any_detected(m, run_tests) { true }
    else { ok && (!run_tests || tests_ok) }
}

// `compile <module>` / `load -dev` again in developer mode: %Reload loads every owned file from disk (on failure, the
// ones in `loaded`), %Compile; the stamp at Base.cls#L1238 runs only after a successful compile without -skip-sync.
pub open spec fn compile_dev(m: M, ok: bool, skip: bool, loaded: Set<int>) -> M {
    let m1 = M { files: Map::new(m.files.dom(), |p: int|
                     if m.files[p].owned && (ok || loaded.contains(p)) { F { server: Some(m.files[p].disk), ..m.files[p] } }
                     else { m.files[p] }), ..m };
    if ok && !skip { stamp_all(m1) } else { m1 }
}

// `reload <module>`: loads files from disk, no compile, no stamp; a module not in developer mode has no root: no-op.
pub open spec fn reload(m: M) -> M {
    if !m.dev { m }
    else { M { files: Map::new(m.files.dom(), |p: int|
                   if m.files[p].owned { F { server: Some(m.files[p].disk), ..m.files[p] } } else { m.files[p] }), ..m } }
}

pub open spec fn edit(m: M, p: int, b: int) -> M {
    if m.files.dom().contains(p) { M { files: m.files.insert(p, F { disk: b, ..m.files[p] }), ..m } } else { m }
}

// ---------------------------------------------------------------- invariant

// An unmarked baseline of an owned file equals its server copy.
pub open spec fn inv_f(f: F) -> bool { f.owned && f.hash is Some && !f.failed ==> f.server == f.hash }

pub open spec fn inv(m: M) -> bool { forall |p: int| #[trigger] m.files.dom().contains(p) ==> inv_f(m.files[p]) }

// ---------------------------------------------------------------- premises

pub open spec fn premise_out(m: M, ok: bool, skip: bool, loaded: Set<int>) -> bool {
    !(ok && !skip) ==> forall |p: int| #[trigger] m.files.dom().contains(p) && m.files[p].owned && (ok || loaded.contains(p))
        && m.files[p].hash is Some && !m.files[p].failed ==> m.files[p].hash == Some(m.files[p].disk)
}

pub open spec fn premise_out_reload(m: M) -> bool {
    m.dev ==> forall |p: int| #[trigger] m.files.dom().contains(p) && m.files[p].owned
        && m.files[p].hash is Some && !m.files[p].failed ==> m.files[p].hash == Some(m.files[p].disk)
}

pub open spec fn premise_base(m: M) -> bool {
    m.dev && !has_baseline(m) ==> forall |p: int| #[trigger] m.files.dom().contains(p) && m.files[p].owned
        ==> m.files[p].server == Some(m.files[p].disk)
}

// ---------------------------------------------------------------- L1

pub struct V { pub disk: int, pub server: Option<int>, pub owned: bool }

pub open spec fn abs(m: M) -> Map<int, V> {
    Map::new(m.files.dom(), |p: int| V { disk: m.files[p].disk, server: m.files[p].server, owned: m.files[p].owned })
}

// Loading puts only the current disk bytes on the server; disk and ownership do not change.
pub open spec fn l1_load(a: Map<int, V>, b: Map<int, V>) -> bool {
    &&& b.dom() == a.dom()
    &&& forall |p: int| #[trigger] a.dom().contains(p) ==> b[p].disk == a[p].disk && b[p].owned == a[p].owned
            && (b[p].server == a[p].server || b[p].server == Some(a[p].disk))
}

pub open spec fn l1_sync(a: Map<int, V>, b: Map<int, V>, ok: bool) -> bool {
    &&& l1_load(a, b)
    &&& ok ==> forall |p: int| #[trigger] b.dom().contains(p) && b[p].owned ==> b[p].server == Some(b[p].disk)
}

// `ran`: the sync applied changes (it found a baseline and a changed or marked file); `ok`: its loads and compile
// worked; `tests_ok`: the tests it ran passed. These are the environment's outcomes for the step.
pub open spec fn l1_outcome(dev: bool, ran: bool, ok: bool, run_tests: bool, tests_ok: bool, reported: bool) -> bool {
    &&& !dev ==> !reported
    &&& dev && ran && !ok ==> !reported
    &&& dev && ran && run_tests && !tests_ok ==> !reported
    &&& dev && (!ran || (ok && (!run_tests || tests_ok))) ==> reported
}

pub open spec fn sync_ran(m: M, run_tests: bool) -> bool { has_baseline(m) && any_detected(m, run_tests) }

pub open spec fn l1_edit(a: Map<int, V>, b: Map<int, V>, p: int, x: int) -> bool {
    b == (if a.dom().contains(p) { a.insert(p, (V { disk: x, ..a[p] })) } else { a })
}

// ---------------------------------------------------------------- refinement

pub proof fn sync_lemma_stamp_frame(m: M)
    ensures stamp_all(m).files.dom() == m.files.dom(),
            forall |p: int| #[trigger] m.files.dom().contains(p) ==>
                stamp_all(m).files[p] == (F { hash: Some(m.files[p].disk), ..m.files[p] }),
{}

pub proof fn sync_lemma_sync(m: M, run_tests: bool, ok: bool, loaded: Set<int>, tests_ok: bool)
    requires inv(m), premise_base(m),
    ensures l1_sync(abs(m), abs(sync_state(m, run_tests, ok, loaded)), sync_ok(m, run_tests, ok, tests_ok)),
            l1_outcome(m.dev, sync_ran(m, run_tests), ok, run_tests, tests_ok, sync_ok(m, run_tests, ok, tests_ok)),
            inv(sync_state(m, run_tests, ok, loaded)),
{
    let post = sync_state(m, run_tests, ok, loaded);
    sync_lemma_stamp_frame(m);
    assert(post.files.dom() == m.files.dom());
    assert forall |p: int| #[trigger] m.files.dom().contains(p) implies inv_f(post.files[p])
        && post.files[p].disk == m.files[p].disk && post.files[p].owned == m.files[p].owned
        && (post.files[p].server == m.files[p].server || post.files[p].server == Some(m.files[p].disk))
        && (m.dev && has_baseline(m) && ok && any_detected(m, run_tests) ==> post.files[p].owned ==> post.files[p].server == Some(post.files[p].disk))
        && (m.dev && has_baseline(m) && !any_detected(m, run_tests) ==> post.files[p].owned ==> post.files[p].server == Some(post.files[p].disk))
        && (m.dev && !has_baseline(m) ==> post.files[p].owned ==> post.files[p].server == Some(post.files[p].disk))
    by {
        assert(inv_f(m.files[p]));
        if m.dev && has_baseline(m) && !any_detected(m, run_tests) && m.files[p].owned {
            assert(!detected(m.files[p], run_tests));
        }
    }
    assert(abs(post).dom() == abs(m).dom());
}

pub proof fn sync_lemma_compile(m: M, ok: bool, skip: bool, loaded: Set<int>)
    requires inv(m), premise_out(m, ok, skip, loaded),
    ensures l1_load(abs(m), abs(compile_dev(m, ok, skip, loaded))), inv(compile_dev(m, ok, skip, loaded)),
{
    let m1 = M { files: Map::new(m.files.dom(), |p: int|
                     if m.files[p].owned && (ok || loaded.contains(p)) { F { server: Some(m.files[p].disk), ..m.files[p] } }
                     else { m.files[p] }), ..m };
    sync_lemma_stamp_frame(m1);
    let post = compile_dev(m, ok, skip, loaded);
    assert(post.files.dom() == m.files.dom());
    assert forall |p: int| #[trigger] m.files.dom().contains(p) implies inv_f(post.files[p])
        && post.files[p].disk == m.files[p].disk && post.files[p].owned == m.files[p].owned
        && (post.files[p].server == m.files[p].server || post.files[p].server == Some(m.files[p].disk)) by {
        assert(inv_f(m.files[p]));
    }
    assert(abs(post).dom() == abs(m).dom());
}

pub proof fn sync_lemma_reload(m: M)
    requires inv(m), premise_out_reload(m),
    ensures l1_load(abs(m), abs(reload(m))), inv(reload(m)),
{
    let post = reload(m);
    assert(post.files.dom() == m.files.dom());
    assert forall |p: int| #[trigger] m.files.dom().contains(p) implies inv_f(post.files[p])
        && post.files[p].disk == m.files[p].disk && post.files[p].owned == m.files[p].owned
        && (post.files[p].server == m.files[p].server || post.files[p].server == Some(m.files[p].disk)) by {
        assert(inv_f(m.files[p]));
    }
    assert(abs(post).dom() == abs(m).dom());
}

pub proof fn sync_lemma_edit(m: M, p: int, b: int)
    requires inv(m),
    ensures l1_edit(abs(m), abs(edit(m, p, b)), p, b), inv(edit(m, p, b)),
{
    let post = edit(m, p, b);
    if m.files.dom().contains(p) {
        assert(post.files.dom() == m.files.dom());
        assert forall |q: int| #[trigger] post.files.dom().contains(q) implies inv_f(post.files[q]) by { assert(inv_f(m.files[q])); }
        assert(abs(post) =~= abs(m).insert(p, V { disk: b, ..abs(m)[p] }));
    }
}

// ---------------------------------------------------------------- per-file effects (statement-level lemmas)

// SYNC-B8, B11, B13, B14, B18, B19: on a sync that works, every changed, new or retry-marked owned file is loaded,
// gets its new baseline and loses its retry mark; a file no resource owns is left alone (and found again).
pub proof fn sync_lemma_success_effects(m: M, run_tests: bool, loaded: Set<int>, p: int)
    requires m.dev, has_baseline(m), m.files.dom().contains(p),
    ensures
        detected(m.files[p], run_tests) ==> any_detected(m, run_tests),
        detected(m.files[p], run_tests) ==> sync_state(m, run_tests, true, loaded).files[p].server == Some(m.files[p].disk)
            && sync_state(m, run_tests, true, loaded).files[p].hash == Some(m.files[p].disk)
            && !sync_state(m, run_tests, true, loaded).files[p].failed,
        m.files[p].owned && (m.files[p].hash != Some(m.files[p].disk) || m.files[p].failed) ==> detected(m.files[p], run_tests),
        !m.files[p].owned ==> sync_state(m, run_tests, true, loaded).files[p] == m.files[p],
{
    if detected(m.files[p], run_tests) { assert(m.files.dom().contains(p) && detected(m.files[p], run_tests)); }
}

// SYNC-B16, B17: a failed sync saves no baseline, puts a retry mark on each changed file that has a baseline, and
// every file it was applying is found again by the next sync.
pub proof fn sync_lemma_failed_sync_effects(m: M, run_tests: bool, loaded: Set<int>, p: int)
    requires m.dev, has_baseline(m), m.files.dom().contains(p),
    ensures
        sync_state(m, run_tests, false, loaded).files[p].hash == m.files[p].hash,
        sync_state(m, run_tests, false, loaded).files[p].disk == m.files[p].disk,
        detected(m.files[p], run_tests) ==> detected(sync_state(m, run_tests, false, loaded).files[p], run_tests),
        detected(m.files[p], run_tests) && m.files[p].hash is Some ==> sync_state(m, run_tests, false, loaded).files[p].failed,
{}

// SYNC-B3, B4, B6, B20, B22, B48: a dev-mode load or compile records every file's baseline only when it compiles and
// runs without -skip-sync; it never sets or clears a retry or test mark; on failure the loaded files stay loaded.
pub proof fn sync_lemma_compile_effects(m: M, ok: bool, skip: bool, loaded: Set<int>, p: int)
    requires m.files.dom().contains(p),
    ensures
        compile_dev(m, ok, skip, loaded).files[p].failed == m.files[p].failed,
        compile_dev(m, ok, skip, loaded).files[p].pending == m.files[p].pending,
        ok && !skip ==> compile_dev(m, ok, skip, loaded).files[p].hash == Some(m.files[p].disk),
        !(ok && !skip) ==> compile_dev(m, ok, skip, loaded).files[p].hash == m.files[p].hash,
        m.files[p].owned && (ok || loaded.contains(p)) ==> compile_dev(m, ok, skip, loaded).files[p].server == Some(m.files[p].disk),
{
    let m1 = M { files: Map::new(m.files.dom(), |q: int|
                     if m.files[q].owned && (ok || loaded.contains(q)) { F { server: Some(m.files[q].disk), ..m.files[q] } }
                     else { m.files[q] }), ..m };
    sync_lemma_stamp_frame(m1);
}

// SYNC-B40, B41: with no baseline at all, sync records the disk bytes as the baseline, loads nothing and succeeds.
pub proof fn sync_lemma_first_sync(m: M, run_tests: bool, ok: bool, loaded: Set<int>, tests_ok: bool, p: int)
    requires m.dev, !has_baseline(m), m.files.dom().contains(p),
    ensures sync_state(m, run_tests, ok, loaded).files[p] == (F { hash: Some(m.files[p].disk), ..m.files[p] }),
            sync_ok(m, run_tests, ok, tests_ok),
{
    sync_lemma_stamp_frame(m);
}

// SYNC-B50, B51: reload saves no baseline; for a module not in developer mode it changes nothing.
pub proof fn sync_lemma_reload_effects(m: M, p: int)
    requires m.files.dom().contains(p),
    ensures reload(m).files[p].hash == m.files[p].hash, reload(m).files[p].failed == m.files[p].failed,
            !m.dev ==> reload(m) == m,
{}

// SYNC-B24, B25: a sync without -test that works keeps every test mark and marks each changed test file.
pub proof fn sync_lemma_test_marks(m: M, loaded: Set<int>, p: int)
    requires m.dev, has_baseline(m), m.files.dom().contains(p),
    ensures
        m.files[p].pending ==> sync_state(m, false, true, loaded).files[p].pending,
        detected(m.files[p], false) && m.files[p].test ==> sync_state(m, false, true, loaded).files[p].pending,
{
    if detected(m.files[p], false) { assert(m.files.dom().contains(p) && detected(m.files[p], false)); }
}

// ---------------------------------------------------------------- defect witnesses (proved)

// One owned class file, loaded with -dev: disk = server = baseline = 1.
pub open spec fn f0(hash: Option<int>) -> F { F { disk: 1, server: Some(1), hash: hash, failed: false, pending: false, owned: true, test: false } }
pub open spec fn m0(hash: Option<int>) -> M { M { files: Map::empty().insert(0int, f0(hash)), dev: true } }

// F-SYNC-1 (O:nc-sync-N18-compile-fail-revert): edit to broken bytes 2, `compile` fails after loading them (no mark,
// baseline still 1), exact revert to 1, then `sync` says nothing to sync and succeeds while the server keeps 2.
pub proof fn sync_witness_failed_compile_then_revert()
    ensures ({
        let a = m0(Some(1));
        let b = edit(a, 0, 2);
        let c = compile_dev(b, false, false, Set::empty().insert(0int));
        let d = edit(c, 0, 1);
        let e = sync_state(d, false, true, Set::empty());
        &&& inv(a) && inv(b) && !premise_out(b, false, false, Set::empty().insert(0int)) && !inv(c)
        &&& c.files[0].server == Some(2int) && c.files[0].hash == Some(1int) && !c.files[0].failed
        &&& !any_detected(d, false) && sync_ok(d, false, true, true)
        &&& e.files[0].server == Some(2int) && e.files[0].disk == 1
        &&& !l1_sync(abs(d), abs(e), sync_ok(d, false, true, true))
    })
{
    let a = m0(Some(1));
    assert(a.files.dom() =~= Set::empty().insert(0int));
    assert(inv(a)) by { assert forall |p: int| #[trigger] a.files.dom().contains(p) implies inv_f(a.files[p]) by {} }
    let b = edit(a, 0, 2);
    sync_lemma_edit(a, 0, 2);
    assert(b.files[0] == F { disk: 2, ..f0(Some(1)) });
    let c = compile_dev(b, false, false, Set::empty().insert(0int));
    assert(c.files[0] == F { server: Some(2int), ..b.files[0] });
    let d = edit(c, 0, 1);
    assert(d.files.dom() =~= Set::empty().insert(0int));
    assert(d.files[0] == F { disk: 1, server: Some(2int), ..f0(Some(1)) });
    assert(has_baseline(d)) by { assert(d.files.dom().contains(0int) && d.files[0].hash is Some); }
    assert(!any_detected(d, false)) by {
        assert forall |p: int| d.files.dom().contains(p) implies !detected(#[trigger] d.files[p], false) by {}
    }
    let e = sync_state(d, false, true, Set::empty());
    assert(e == d);
    assert(abs(e)[0] == V { disk: 1, server: Some(2int), owned: true });
    assert(abs(e).dom().contains(0int));
}

// F-SYNC-2 (O:nc-sync-N19-skip-sync-first-sync): `load -dev -skip-sync` leaves no baseline; the file is edited to 2;
// the first sync only records the baseline (2) and succeeds while the server keeps 1; the next sync finds nothing.
pub proof fn sync_witness_first_sync_absorbs_edit()
    ensures ({
        let a = m0(None);
        let b = edit(a, 0, 2);
        let c = sync_state(b, false, true, Set::empty());
        &&& inv(a) && inv(b) && !premise_base(b)
        &&& sync_ok(b, false, true, true) && c.files[0].hash == Some(2int) && c.files[0].server == Some(1int)
        &&& !l1_sync(abs(b), abs(c), sync_ok(b, false, true, true))
        &&& !any_detected(c, false)
    })
{
    let a = m0(None);
    assert(a.files.dom() =~= Set::empty().insert(0int));
    assert(inv(a)) by { assert forall |p: int| #[trigger] a.files.dom().contains(p) implies inv_f(a.files[p]) by {} }
    let b = edit(a, 0, 2);
    sync_lemma_edit(a, 0, 2);
    assert(b.files.dom() =~= Set::empty().insert(0int));
    assert(b.files[0] == F { disk: 2, ..f0(None) });
    assert(!has_baseline(b)) by {
        assert forall |p: int| b.files.dom().contains(p) implies (#[trigger] b.files[p]).hash is None by {}
    }
    assert(!premise_base(b)) by { assert(b.files.dom().contains(0int)); }
    let c = sync_state(b, false, true, Set::empty());
    sync_lemma_stamp_frame(b);
    assert(c.files[0] == F { hash: Some(2int), ..b.files[0] });
    assert(abs(c)[0] == V { disk: 2, server: Some(1int), owned: true });
    assert(abs(c).dom().contains(0int));
    assert(!any_detected(c, false)) by {
        assert forall |p: int| c.files.dom().contains(p) implies !detected(#[trigger] c.files[p], false) by {}
    }
}

// The same edit-then-revert after a `reload` (F-SYNC-1, reload case; O:nc-nsp-model-sync-reload-revert).
pub proof fn sync_witness_reload_then_revert()
    ensures ({
        let a = m0(Some(1));
        let b = edit(a, 0, 2);
        let c = reload(b);
        let d = edit(c, 0, 1);
        let e = sync_state(d, false, true, Set::empty());
        &&& !premise_out_reload(b) && !inv(c)
        &&& sync_ok(d, false, true, true) && e.files[0].server == Some(2int)
        &&& !l1_sync(abs(d), abs(e), sync_ok(d, false, true, true))
    })
{
    let a = m0(Some(1));
    assert(a.files.dom() =~= Set::empty().insert(0int));
    let b = edit(a, 0, 2);
    assert(b.files.dom() =~= Set::empty().insert(0int));
    assert(b.files[0] == F { disk: 2, ..f0(Some(1)) });
    let c = reload(b);
    assert(c.files[0] == F { server: Some(2int), ..b.files[0] });
    let d = edit(c, 0, 1);
    assert(d.files.dom() =~= Set::empty().insert(0int));
    assert(d.files[0] == F { disk: 1, server: Some(2int), ..f0(Some(1)) });
    assert(has_baseline(d)) by { assert(d.files.dom().contains(0int) && d.files[0].hash is Some); }
    assert(!any_detected(d, false)) by {
        assert forall |p: int| d.files.dom().contains(p) implies !detected(#[trigger] d.files[p], false) by {}
    }
    let e = sync_state(d, false, true, Set::empty());
    assert(e == d);
    assert(abs(e)[0] == V { disk: 1, server: Some(2int), owned: true });
    assert(abs(e).dom().contains(0int));
}

// Positive (O:nc-sync-N22-retry-mark-after-compile; issue #1224): a failed sync marks the file; after an exact
// revert the next sync retries it and the server again holds the disk bytes.
pub proof fn sync_witness_failed_sync_is_retried()
    ensures ({
        let a = m0(Some(1));
        let b = edit(a, 0, 2);
        let c = sync_state(b, false, false, Set::empty().insert(0int));
        let d = edit(c, 0, 1);
        let e = sync_state(d, false, true, Set::empty());
        &&& !sync_ok(b, false, false, true) && c.files[0].failed && c.files[0].server == Some(2int)
        &&& inv(c) && premise_base(d) && any_detected(d, false)
        &&& sync_ok(d, false, true, true) && e.files[0].server == Some(1int) && !e.files[0].failed
        &&& l1_sync(abs(d), abs(e), true)
    })
{
    let a = m0(Some(1));
    assert(a.files.dom() =~= Set::empty().insert(0int));
    assert(inv(a)) by { assert forall |p: int| #[trigger] a.files.dom().contains(p) implies inv_f(a.files[p]) by {} }
    let b = edit(a, 0, 2);
    sync_lemma_edit(a, 0, 2);
    assert(b.files.dom() =~= Set::empty().insert(0int));
    assert(b.files[0] == F { disk: 2, ..f0(Some(1)) });
    assert(has_baseline(b)) by { assert(b.files.dom().contains(0int) && b.files[0].hash is Some); }
    assert(any_detected(b, false)) by { assert(b.files.dom().contains(0int) && detected(b.files[0], false)); }
    let c = sync_state(b, false, false, Set::empty().insert(0int));
    sync_lemma_sync(b, false, false, Set::empty().insert(0int), true);
    assert(c.files[0] == F { server: Some(2int), failed: true, ..b.files[0] });
    let d = edit(c, 0, 1);
    sync_lemma_edit(c, 0, 1);
    assert(d.files.dom() =~= Set::empty().insert(0int));
    assert(d.files[0] == F { disk: 1, server: Some(2int), failed: true, ..f0(Some(1)) });
    assert(has_baseline(d)) by { assert(d.files.dom().contains(0int) && d.files[0].hash is Some); }
    assert(any_detected(d, false)) by { assert(d.files.dom().contains(0int) && detected(d.files[0], false)); }
    sync_lemma_sync(d, false, true, Set::empty(), true);
    let e = sync_state(d, false, true, Set::empty());
    assert(e.files[0] == commit_one(d.files[0], false));
}

// OUT, -skip-sync case (O:nc-nsp-model-sync-skipsync-revert): `load -dev -skip-sync` over an existing baseline
// compiles the edited bytes and records nothing; after an exact revert sync finds nothing and the server keeps 2.
pub proof fn sync_witness_skip_sync_load_then_revert()
    ensures ({
        let a = m0(Some(1));
        let b = edit(a, 0, 2);
        let c = compile_dev(b, true, true, Set::empty());
        let d = edit(c, 0, 1);
        let e = sync_state(d, false, true, Set::empty());
        &&& !premise_out(b, true, true, Set::empty()) && !inv(c)
        &&& sync_ok(d, false, true, true) && e.files[0].server == Some(2int)
        &&& !l1_sync(abs(d), abs(e), sync_ok(d, false, true, true))
    })
{
    let a = m0(Some(1));
    assert(a.files.dom() =~= Set::empty().insert(0int));
    let b = edit(a, 0, 2);
    assert(b.files.dom() =~= Set::empty().insert(0int));
    assert(b.files[0] == F { disk: 2, ..f0(Some(1)) });
    assert(!premise_out(b, true, true, Set::empty())) by { assert(b.files.dom().contains(0int)); }
    let c = compile_dev(b, true, true, Set::empty());
    assert(c.files[0] == F { server: Some(2int), ..b.files[0] });
    let d = edit(c, 0, 1);
    assert(d.files.dom() =~= Set::empty().insert(0int));
    assert(d.files[0] == F { disk: 1, server: Some(2int), ..f0(Some(1)) });
    assert(has_baseline(d)) by { assert(d.files.dom().contains(0int) && d.files[0].hash is Some); }
    assert(!any_detected(d, false)) by {
        assert forall |p: int| d.files.dom().contains(p) implies !detected(#[trigger] d.files[p], false) by {}
    }
    let e = sync_state(d, false, true, Set::empty());
    assert(e == d);
    assert(abs(e)[0] == V { disk: 1, server: Some(2int), owned: true });
    assert(abs(e).dom().contains(0int));
}

fn main() {}

} // verus!
