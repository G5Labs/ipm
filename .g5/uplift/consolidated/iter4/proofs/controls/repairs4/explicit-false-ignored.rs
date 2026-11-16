//! Repairs to now-public partial answers. The producer of each distinguishing input is modeled
//! rather than passed in: the lifecycle class decides the deploy default, the declared phase list is
//! admitted before selection, and the keyword string is tokenized before the filesystem gate.
//! These supersede the corresponding iteration 3 functions for the cases named; iteration 3 files are unchanged.
use vstd::prelude::*;
verus! {
pub type Text = Seq<int>;

// --- Deployment default (I2E14). Source: Lifecycle/Base.cls DEPLOYBYDEFAULT=0 and
// GetDeployDecision-style resolution at Base.cls:1945-1963; Lifecycle/DeployedModule.cls DEPLOYBYDEFAULT=1.
#[derive(PartialEq, Eq)]
pub enum Lifecycle { Module, DeployedModule, Subclass(bool) }

pub open spec fn deploy_by_default(l: Lifecycle) -> bool {
    match l { Lifecycle::Module => false, Lifecycle::DeployedModule => true, Lifecycle::Subclass(b) => b }
}

#[derive(PartialEq, Eq)]
pub enum Explicit { Missing, Value(bool) }

/// class_param: the named Deploy.Parameter on a defined .cls resource, if the class defines it.
pub open spec fn deploy(l: Lifecycle, class_param: Option<int>, explicit: Explicit) -> bool {
    match class_param {
        Some(v) => v == 1,
        None => match explicit { Explicit::Missing => deploy_by_default(l), Explicit::Value(b) => b || deploy_by_default(l) },
    }
}

pub proof fn deploy_neighbors() {
    assert(!deploy(Lifecycle::Module, None, Explicit::Missing));
    assert(deploy(Lifecycle::DeployedModule, None, Explicit::Missing));
    assert(!deploy(Lifecycle::DeployedModule, None, Explicit::Value(false)));
    assert(deploy(Lifecycle::Module, None, Explicit::Value(true)));
    assert(!deploy(Lifecycle::DeployedModule, Some(0), Explicit::Value(true)));
    assert(deploy(Lifecycle::Module, Some(1), Explicit::Value(false)));
}

// --- Test phase membership (I2E07 domain). Source: ResourceProcessor/Test.cls Phase is
// ListOfOptions(VALUELIST=",test,verify"); TestsShouldRun lowercases only the requested phase.
pub open spec fn lower(s: Text) -> Text
    decreases s.len()
{
    if s.len() == 0 { s } else {
        seq![if 65 <= s[0] && s[0] <= 90 { s[0] + 32 } else { s[0] }] + lower(s.drop_first())
    }
}
pub open spec fn test_word() -> Text { seq![116int, 101, 115, 116] }
pub open spec fn verify_word() -> Text { seq![118int, 101, 114, 105, 102, 121] }

pub open spec fn admitted(declared: Seq<Text>) -> bool {
    forall|i: int| 0 <= i < declared.len() ==> (declared[i] == test_word() || declared[i] == verify_word())
}
/// As built: stored phases are compared exactly with the lowercased request.
pub open spec fn phase_enabled(phase: Text, declared: Seq<Text>) -> bool {
    exists|i: int| 0 <= i < declared.len() && declared[i] == lower(phase)
}
/// Iteration 3's reading, which also folded the stored value.
pub open spec fn phase_enabled_iter3(phase: Text, declared: Seq<Text>) -> bool {
    exists|i: int| 0 <= i < declared.len() && lower(declared[i]) == lower(phase)
}

pub proof fn lemma_lower_fixes_admitted(w: Text)
    requires w == test_word() || w == verify_word()
    ensures lower(w) == w
{
    reveal_with_fuel(lower, 7);
    if w == test_word() { assert(lower(w) =~= w); } else { assert(lower(w) =~= w); }
}

/// On admitted declarations both readings agree, so iteration 3's answer holds on that domain.
pub proof fn lemma_admitted_agree(phase: Text, declared: Seq<Text>)
    requires admitted(declared)
    ensures phase_enabled(phase, declared) == phase_enabled_iter3(phase, declared)
{
    if phase_enabled_iter3(phase, declared) {
        let i = choose|i: int| 0 <= i < declared.len() && lower(declared[i]) == lower(phase);
        lemma_lower_fixes_admitted(declared[i]);
    }
    if phase_enabled(phase, declared) {
        let i = choose|i: int| 0 <= i < declared.len() && declared[i] == lower(phase);
        lemma_lower_fixes_admitted(declared[i]);
    }
}

pub proof fn phase_neighbors() {
    reveal_with_fuel(lower, 5);
    let upper_test = seq![84int, 69, 83, 84];
    assert(lower(upper_test) =~= test_word());
    // Admitted lowercase declaration with an uppercase request: selected.
    assert(phase_enabled(upper_test, seq![test_word()])) by { assert(seq![test_word()][0] == lower(upper_test)); }
    // Unadmitted uppercase declaration: as built false, iteration 3 reading true.
    assert(!admitted(seq![upper_test])) by {
        let d = seq![upper_test];
        assert(d[0] == upper_test);
        assert(upper_test[0] != test_word()[0]);
        assert(upper_test != test_word());
        assert(upper_test[0] != verify_word()[0]);
        assert(upper_test != verify_word());
    }
    assert(!phase_enabled(test_word(), seq![upper_test])) by {
        assert(lower(test_word()) =~= test_word());
        assert(upper_test[0] != test_word()[0]);
    }
    assert(phase_enabled_iter3(test_word(), seq![upper_test])) by {
        assert(lower(test_word()) =~= test_word());
        assert(lower(seq![upper_test][0]) == lower(test_word()));
    }
}

// --- Test path to class name (I2C04). Source: ResourceProcessor/Test.cls::RelPathToClassName.
// The resource directory is removed only as a raw text prefix, then one leading slash.
pub open spec fn has_prefix(s: Text, p: Text) -> bool {
    p.len() <= s.len() && s.take(p.len() as int) == p
}
pub open spec fn package_rel_path(rel: Text, dir: Text) -> Text {
    let r = if has_prefix(rel, dir) { rel.skip(dir.len() as int) } else { rel };
    if r.len() > 0 && r[0] == 47 { r.skip(1) } else { r }
}

pub proof fn rel_path_neighbors() {
    let dir = seq![117int];
    let inside = seq![117int, 47, 65];
    assert(inside.take(1) =~= dir);
    assert(package_rel_path(inside, dir) =~= seq![65int]);
    // Outside the directory: the whole relative path is kept, not rejected.
    let outside = seq![111int, 47, 65];
    assert(!has_prefix(outside, dir)) by { assert(outside.take(1)[0] != dir[0]); }
    assert(package_rel_path(outside, dir) =~= outside);
    // A sibling folder that merely starts with the same text is stripped as if inside.
    let sibling = seq![117int, 88, 47, 65];
    assert(sibling.take(1) =~= dir);
    assert(package_rel_path(sibling, dir) =~= seq![88int, 47, 65]);
}

// --- Keyword search tokens (I2E21). Source: Repo/SearchCriteria.cls::KeywordsSet and
// Repo/Manager.cls::SearchRepositoriesForModule.
pub open spec fn split_acc(s: Text, cur: Text) -> Seq<Text>
    decreases s.len()
{
    if s.len() == 0 { seq![cur] }
    else if s[0] == 32 { seq![cur] + split_acc(s.drop_first(), seq![]) }
    else { split_acc(s.drop_first(), cur.push(s[0])) }
}
pub open spec fn nonempty(ts: Seq<Text>) -> Seq<Text>
    decreases ts.len()
{
    if ts.len() == 0 { ts } else {
        (if ts[0].len() > 0 { seq![ts[0]] } else { Seq::<Text>::empty() }) + nonempty(ts.drop_first())
    }
}

/// A plain string is split on single spaces and empty pieces are dropped. A $list input skips the
/// split but still drops empty elements. A list object is assigned as given.
pub enum KeywordInput { Plain(Text), DollarList(Seq<Text>), Object(Seq<Text>) }

pub open spec fn keywords(input: KeywordInput) -> Seq<Text> {
    match input {
        KeywordInput::Plain(s) => nonempty(split_acc(s, seq![])),
        KeywordInput::DollarList(l) => nonempty(l),
        KeywordInput::Object(l) => l,
    }
}

/// The filesystem gate reads only the token count; the criteria name is lowercased first.
pub open spec fn visits_filesystem(input: KeywordInput, enabled: bool, registry_match: bool) -> bool {
    enabled && registry_match && keywords(input).len() == 0
}
pub open spec fn criteria_name(name: Text) -> Text { lower(name) }

pub proof fn keyword_neighbors() {
    reveal_with_fuel(split_acc, 4);
    reveal_with_fuel(nonempty, 4);
    let spaces = seq![32int, 32];
    assert(split_acc(spaces, seq![]) =~= seq![seq![], seq![], seq![]]);
    assert(keywords(KeywordInput::Plain(spaces)).len() == 0);
    assert(visits_filesystem(KeywordInput::Plain(spaces), true, true));
    let padded = seq![32int, 97, 32];
    assert(split_acc(padded, seq![]) =~= seq![seq![], seq![97int], seq![]]);
    assert(keywords(KeywordInput::Plain(padded)) =~= seq![seq![97int]]);
    assert(!visits_filesystem(KeywordInput::Plain(padded), true, true));
    assert(keywords(KeywordInput::DollarList(seq![seq![]])).len() == 0);
    assert(keywords(KeywordInput::Object(seq![seq![]])).len() == 1);
    assert(!visits_filesystem(KeywordInput::Object(seq![seq![]]), true, true));
    reveal_with_fuel(lower, 3);
    assert(criteria_name(seq![65int, 98]) =~= seq![97int, 98]);
}

}
fn main() {}
