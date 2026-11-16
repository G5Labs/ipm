//! Test-run outcome: failure counting, the per-process result accumulator, per-resource
//! outcome, lifecycle versus sync aggregation and the process exit code a CI job observes.
//! Owners: build.test.outcome.* in features/build-and-publish.g5.md and devtools.sync.test-stop.
use vstd::prelude::*;
verus! {

/// One stored %UnitTest result row as BuildResultTree classifies it.
#[derive(PartialEq, Eq)]
pub enum Row { CaseError, CaseOk, MethodError, MethodOk, AssertFailed, AssertPassed }

pub open spec fn is_failure(r: Row) -> bool {
    r == Row::CaseError || r == Row::AssertFailed
}

/// GetAllTestsStatus: one failure per case error, per method error and per failed assertion.
pub open spec fn failures(log: Seq<Row>) -> nat
    decreases log.len()
{
    if log.len() == 0 { 0 } else {
        (if is_failure(log.last()) { 1nat } else { 0nat }) + failures(log.drop_last())
    }
}

/// Accumulator slots 1..n; None is an empty slot.
pub type Slots = Seq<Option<Seq<Row>>>;

pub open spec fn slot_failures(s: Option<Seq<Row>>) -> nat {
    match s { Some(l) => failures(l), None => 0 }
}

/// Failures in slots after `start` (slots are 1-based in the product; index i here is slot i+1).
pub open spec fn suffix_failures(slots: Slots, start: nat) -> nat
    decreases slots.len()
{
    if slots.len() <= start { 0 } else {
        slot_failures(slots.last()) + suffix_failures(slots.drop_last(), start)
    }
}

pub struct StatusResult { pub ok: bool, pub count: nat, pub after: Option<Slots> }

/// As built: count the suffix, fail iff nonzero, then consume it. A top-level reader (start 0)
/// removes the accumulator; a nested reader truncates back to its start.
pub open spec fn get_all_status(slots: Slots, start: nat) -> StatusResult {
    let c = suffix_failures(slots, start);
    StatusResult {
        ok: c == 0,
        count: c,
        after: if start == 0 { None } else { Some(slots.take(start as int)) },
    }
}

/// OutputFailures(start): the same rows, printed only from nonempty slots after start.
pub open spec fn displayed_lines(slots: Slots, start: nat) -> nat {
    suffix_failures(slots, start)
}

/// One UnitTest resource in one test command.
pub struct TestRes {
    pub selected: bool,
    pub suite_named: bool,
    pub suite_dir_exists: bool,
    pub has_classes: bool,
    pub run_ok: bool,
    pub log: Seq<Row>,
}

pub struct Opts { pub fatal: bool, pub report_requested: bool, pub report_ext_ok: bool }

/// A named suite with no directory fails while loading; an unselected empty package is skipped.
pub open spec fn load_fails(r: TestRes) -> bool {
    r.selected && r.suite_named && !r.suite_dir_exists
}

pub open spec fn ran(r: TestRes) -> bool {
    r.selected && !load_fails(r) && r.has_classes
}

pub open spec fn resource_ok(r: TestRes, o: Opts) -> bool {
    if !r.selected { true }
    else if load_fails(r) { false }
    else if !r.has_classes { true }
    else { r.run_ok && (!o.report_requested || o.report_ext_ok) && (!o.fatal || failures(r.log) == 0) }
}

/// The report file is written before the failure check, and never for an unsupported extension.
pub open spec fn report_written(r: TestRes, o: Opts) -> bool {
    ran(r) && r.run_ok && o.report_requested && o.report_ext_ok
}

/// %Test calls every in-scope resource and accumulates the statuses.
pub open spec fn lifecycle_invoked(rs: Seq<TestRes>, o: Opts, i: int) -> bool {
    0 <= i < rs.len()
}

pub open spec fn all_ok(rs: Seq<TestRes>, o: Opts) -> bool {
    forall|i: int| 0 <= i < rs.len() ==> resource_ok(rs[i], o)
}

/// sync -test throws on the first failing resource, so later resources are never called.
pub open spec fn sync_invoked(rs: Seq<TestRes>, o: Opts, i: int) -> bool {
    0 <= i < rs.len() && forall|j: int| 0 <= j < i ==> resource_ok(rs[j], o)
}

/// Shell(command, terminateOnError): exit 1 only for a failed command with the flag set.
pub open spec fn exit_code(ok: bool, terminate: bool) -> int {
    if !ok && terminate { 1 } else { 0 }
}

pub proof fn lemma_nested_keeps_parent(slots: Slots, start: nat)
    requires 0 < start <= slots.len()
    ensures get_all_status(slots, start).after == Some(slots.take(start as int)),
            get_all_status(slots, start).after.unwrap().len() == start,
{
}

pub proof fn lemma_suffix_ignores_prefix(slots: Slots, start: nat, extra: Option<Seq<Row>>)
    requires start <= slots.len()
    ensures suffix_failures(slots.push(extra), start) == slot_failures(extra) + suffix_failures(slots, start)
{
    assert(slots.push(extra).drop_last() =~= slots);
}

/// The fixture classes used natively.
pub open spec fn fail_log() -> Seq<Row> {
    // Asserts: TestThrows has a method error; TestTwoFailedAsserts fails only through its
    // assertions (no method error row). SetupErr: OnBeforeAllTests error is a case error.
    seq![Row::CaseOk, Row::MethodError, Row::MethodOk, Row::AssertFailed, Row::AssertPassed, Row::AssertFailed, Row::CaseError]
}
pub open spec fn pass_log() -> Seq<Row> { seq![Row::CaseOk, Row::MethodOk, Row::AssertPassed] }

pub proof fn fixture_counts()
    ensures failures(fail_log()) == 4, failures(pass_log()) == 0
{
    reveal_with_fuel(failures, 8);
}

pub open spec fn res(selected: bool, suite_named: bool, dir: bool, classes: bool, log: Seq<Row>) -> TestRes {
    TestRes { selected, suite_named, suite_dir_exists: dir, has_classes: classes, run_ok: true, log }
}

pub proof fn outcome_neighbors() {
    fixture_counts();
    let fatal = Opts { fatal: true, report_requested: false, report_ext_ok: true };
    let lenient = Opts { fatal: false, report_requested: false, report_ext_ok: true };
    let fail = res(true, false, true, true, fail_log());
    let pass = res(true, false, true, true, pass_log());
    let empty = res(true, false, false, false, seq![]);
    let rs = seq![fail, pass, empty];
    // Default: the failing resource fails the command, yet the later two are still called.
    assert(!all_ok(rs, fatal)) by { assert(!resource_ok(rs[0], fatal)); }
    assert(lifecycle_invoked(rs, fatal, 1) && lifecycle_invoked(rs, fatal, 2));
    assert(!sync_invoked(rs, fatal, 1)) by { assert(!resource_ok(rs[0], fatal)); }
    // FailuresAreFatal=0 keeps the same failing tests green.
    assert(all_ok(rs, lenient));
    // An empty, unselected-by-suite package is skipped green; a named suite without a directory fails.
    assert(resource_ok(empty, fatal) && !ran(empty));
    assert(!resource_ok(res(true, true, false, false, seq![]), fatal));
    // Passing tests plus an unsupported report extension still fail, and no file is written.
    let bad_ext = Opts { fatal: true, report_requested: true, report_ext_ok: false };
    assert(!resource_ok(pass, bad_ext) && !report_written(pass, bad_ext));
    let json = Opts { fatal: true, report_requested: true, report_ext_ok: true };
    assert(report_written(fail, json) && !resource_ok(fail, json));
    // Only the terminate flag turns a failure into a nonzero process exit.
    assert(exit_code(false, true) == 1 && exit_code(false, false) == 0 && exit_code(true, true) == 0);
}

pub proof fn accumulator_neighbors() {
    fixture_counts();
    let p = Some(pass_log());
    let f = Some(fail_log());
    reveal_with_fuel(suffix_failures, 5);
    // Nested reader after a passing parent slot sees only the failing child and keeps the parent.
    let nested = get_all_status(seq![p, f], 1);
    assert(!nested.ok && nested.count == 4);
    assert(nested.after == Some(seq![p])) by { assert(seq![p, f].take(1) =~= seq![p]); }
    // The parent then reads from zero, passes and clears the accumulator.
    assert(get_all_status(seq![p], 0).ok && get_all_status(seq![p], 0).after.is_none());
    // A top-level reader over both slots fails.
    assert(!get_all_status(seq![p, f], 0).ok);
    // Reversed: nested reader after a failing parent slot passes.
    assert(get_all_status(seq![f, p], 1).ok);
    // Display from slot 1 prints slots 2 and 3; an empty slot 2 prints nothing.
    assert(displayed_lines(seq![f, f, f], 1) == 8);
    assert(displayed_lines(seq![f, None, f], 1) == 4);
    assert(displayed_lines(seq![f, f, f], 2) == 4);
}

}
fn main() {}
