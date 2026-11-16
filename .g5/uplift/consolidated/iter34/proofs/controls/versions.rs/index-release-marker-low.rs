#![allow(non_snake_case)]
// Iteration34: filesystem repository version index and match order, and the install choice.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter34/bindings.json):
//   General/SemanticVersion/Index.cls::GetIndexGlobalNode, Insert       index subscripts per version
//   General/SemanticVersion/Find.cls::PreviousItem, ContainsItem,
//     MatchesExecute, MatchesFetch                                       reverse $Query walk, prerelease/snapshot flags
//   Repo/Filesystem/Cache.cls::OrderedMatchesExecute/Fetch               the cache query rows
//   Repo/Utils.cls::moduleSqlToList                                      exact-match first, then ORDER BY version
//                                                                        columns, first row per name kept
//   Repo/Manager.cls::SearchRepositoriesForModule, Main.cls::Install     one result per module, GetAt(1) installed
//
// Domain: versions with major/minor/patch and an optional prerelease of numeric or word identifiers;
// no build metadata. Words are encoded as a single character code above '9' (premise: the fixture
// words "beta" and "rc" compare by their first character against digits). IRIS subscript collation
// (canonical numbers before strings, numbers numerically) and SQL ordering of the version columns
// (major/minor/patch numerically; prerelease as text; a release's empty prerelease last in DESC) are
// native-qualified premises, each fixed by observed rows.
use vstd::prelude::*;

verus! {

pub enum Id { Num(nat), Word(nat) }
pub struct V { pub major: nat, pub minor: nat, pub patch: nat, pub pre: Seq<Id> }

// version.order: semantic comparison. Returns -1, 0, 1.
pub open spec fn cmp_nat(a: nat, b: nat) -> int { if a < b { -1 } else if a > b { 1 } else { 0 } }
pub open spec fn cmp_id(a: Id, b: Id) -> int {
    match (a, b) {
        (Id::Num(x), Id::Num(y)) => cmp_nat(x, y),
        (Id::Num(_), Id::Word(_)) => -1,
        (Id::Word(_), Id::Num(_)) => 1,
        (Id::Word(x), Id::Word(y)) => cmp_nat(x, y),
    }
}
pub open spec fn cmp_ids(a: Seq<Id>, b: Seq<Id>) -> int
    decreases a.len()
{
    if a.len() == 0 && b.len() == 0 { 0 } else if a.len() == 0 { -1 } else if b.len() == 0 { 1 }
    else if cmp_id(a[0], b[0]) != 0 { cmp_id(a[0], b[0]) } else { cmp_ids(a.drop_first(), b.drop_first()) }
}
pub open spec fn semver_cmp(a: V, b: V) -> int {
    if a.major != b.major { cmp_nat(a.major, b.major) }
    else if a.minor != b.minor { cmp_nat(a.minor, b.minor) }
    else if a.patch != b.patch { cmp_nat(a.patch, b.patch) }
    else if a.pre.len() == 0 && b.pre.len() == 0 { 0 }
    else if a.pre.len() == 0 { 1 } else if b.pre.len() == 0 { -1 }
    else { cmp_ids(a.pre, b.pre) }
}

// Index subscripts: (major, minor, patch, 2) for a release; (major, minor, patch, 0, ids..., -1)
// for a prerelease. Subscript collation: numbers ascending, then strings ascending.
pub enum Sub { N(int), S(nat) }
pub open spec fn sub_of(i: Id) -> Sub { match i { Id::Num(n) => Sub::N(n as int), Id::Word(w) => Sub::S(w) } }
pub open spec fn pre_subs(ids: Seq<Id>) -> Seq<Sub>
    decreases ids.len()
{
    if ids.len() == 0 { seq![Sub::N(-1)] } else { seq![sub_of(ids[0])] + pre_subs(ids.drop_first()) }
}
pub open spec fn key(v: V) -> Seq<Sub> {
    seq![Sub::N(v.major as int), Sub::N(v.minor as int), Sub::N(v.patch as int)]
        + if v.pre.len() == 0 { seq![Sub::N(-2)] } else { seq![Sub::N(0)] + pre_subs(v.pre) }
}
pub open spec fn cmp_int(a: int, b: int) -> int { if a < b { -1 } else if a > b { 1 } else { 0 } }
pub open spec fn cmp_sub(a: Sub, b: Sub) -> int {
    match (a, b) {
        (Sub::N(x), Sub::N(y)) => cmp_int(x, y),
        (Sub::N(_), Sub::S(_)) => -1,
        (Sub::S(_), Sub::N(_)) => 1,
        (Sub::S(x), Sub::S(y)) => cmp_nat(x, y),
    }
}
pub open spec fn cmp_key(a: Seq<Sub>, b: Seq<Sub>) -> int
    decreases a.len()
{
    if a.len() == 0 && b.len() == 0 { 0 } else if a.len() == 0 { -1 } else if b.len() == 0 { 1 }
    else if cmp_sub(a[0], b[0]) != 0 { cmp_sub(a[0], b[0]) } else { cmp_key(a.drop_first(), b.drop_first()) }
}

// SQL order used by moduleSqlToList: numeric triple DESC, then prerelease text DESC with an empty
// prerelease last. Text of a prerelease: identifiers joined by '.' (code 46); a number is its decimal
// digits (codes 48..57); a word is its character code.
pub open spec fn digits(n: nat) -> Seq<nat>
    decreases n
{
    if n < 10 { seq![48 + n] } else { digits(n / 10) + seq![48 + n % 10] }
}
pub open spec fn id_text(i: Id) -> Seq<nat> { match i { Id::Num(n) => digits(n), Id::Word(w) => seq![w] } }
pub open spec fn pre_text(ids: Seq<Id>) -> Seq<nat>
    decreases ids.len()
{
    if ids.len() == 0 { seq![] } else if ids.len() == 1 { id_text(ids[0]) } else { id_text(ids[0]) + seq![46nat] + pre_text(ids.drop_first()) }
}
pub open spec fn cmp_text(a: Seq<nat>, b: Seq<nat>) -> int
    decreases a.len()
{
    if a.len() == 0 && b.len() == 0 { 0 } else if a.len() == 0 { -1 } else if b.len() == 0 { 1 }
    else if a[0] != b[0] { cmp_nat(a[0], b[0]) } else { cmp_text(a.drop_first(), b.drop_first()) }
}
// sql_before(a, b): a comes before b in moduleSqlToList's ORDER BY (both non-exact).
pub open spec fn sql_before(a: V, b: V) -> bool {
    if a.major != b.major { a.major > b.major }
    else if a.minor != b.minor { a.minor > b.minor }
    else if a.patch != b.patch { a.patch > b.patch }
    else if a.pre.len() == 0 { false }
    else if b.pre.len() == 0 { true }
    else { cmp_text(pre_text(a.pre), pre_text(b.pre)) > 0 }
}

// Expressions in the fixture: any version, "<n>.x", or one exact version.
pub enum E { Any, MajorX(nat), Exact(V) }
pub open spec fn satisfies(v: V, e: E) -> bool {
    match e { E::Any => true, E::MajorX(m) => v.major == m && v.pre.len() == 0, E::Exact(x) => v == x }
}
pub open spec fn eligible(v: V, e: E, prerelease: bool) -> bool { (prerelease || v.pre.len() == 0) && satisfies(v, e) }

// Index walk: highest key first among eligible offers.
pub open spec fn index_first_before(a: V, b: V) -> bool { cmp_key(key(a), key(b)) > 0 }
// moduleSqlToList: an exact text match first, then sql_before; first row kept.
pub open spec fn sql_rank_before(a: V, b: V, e: E) -> bool {
    let ea = match e { E::Exact(x) => a == x, _ => false };
    let eb = match e { E::Exact(x) => b == x, _ => false };
    if ea != eb { ea } else { sql_before(a, b) }
}

pub open spec fn best_by(offers: Seq<V>, e: E, prerelease: bool, sql: bool) -> Option<V>
    decreases offers.len()
{
    if offers.len() == 0 { None }
    else {
        let rest = best_by(offers.drop_last(), e, prerelease, sql);
        let v = offers.last();
        if !eligible(v, e, prerelease) { rest }
        else {
            match rest {
                None => Some(v),
                Some(r) => if (if sql { sql_rank_before(v, r, e) } else { index_first_before(v, r) }) { Some(v) } else { Some(r) },
            }
        }
    }
}
// Main.Install takes GetAt(1) of the per-module result built from moduleSqlToList's first row.
pub open spec fn installed(offers: Seq<V>, e: E, prerelease: bool) -> Option<V> { best_by(offers, e, prerelease, true) }
pub open spec fn semver_max(offers: Seq<V>, e: E, prerelease: bool) -> Option<V> { best_by(offers, e, prerelease, false) }

// Fixture versions and codes used by the native reader.
pub open spec fn BETA() -> nat { 98 }   // 'b'
pub open spec fn RC() -> nat { 114 }    // 'r'
pub open spec fn v(M: nat, m: nat, p: nat) -> V { V { major: M, minor: m, patch: p, pre: seq![] } }
pub open spec fn v190() -> V { v(1, 9, 0) }
pub open spec fn v1100() -> V { v(1, 10, 0) }
pub open spec fn v200b1() -> V { V { major: 2, minor: 0, patch: 0, pre: seq![Id::Word(BETA()), Id::Num(1)] } }
pub open spec fn v090() -> V { v(0, 9, 0) }
pub open spec fn v100rc2() -> V { V { major: 1, minor: 0, patch: 0, pre: seq![Id::Word(RC()), Id::Num(2)] } }
pub open spec fn v100rc10() -> V { V { major: 1, minor: 0, patch: 0, pre: seq![Id::Word(RC()), Id::Num(10)] } }
pub open spec fn v200() -> V { v(2, 0, 0) }
pub open spec fn code(o: Option<V>) -> int {
    match o {
        None => 0,
        Some(x) => if x == v190() { 1 } else if x == v1100() { 2 } else if x == v200b1() { 3 } else if x == v090() { 4 }
            else if x == v100rc2() { 5 } else if x == v100rc10() { 6 } else if x == v200() { 7 } else { 99 },
    }
}
pub open spec fn repo_b() -> Seq<V> { seq![v190(), v1100(), v200b1()] }
pub open spec fn repo_c() -> Seq<V> { seq![v090(), v100rc2(), v100rc10()] }
pub open spec fn repo_d() -> Seq<V> { seq![v200(), v200b1()] }

// Index order as a ranked list (position 1 = first row of the cache query).
pub open spec fn rank(offers: Seq<V>, x: V, prerelease: bool) -> int
    decreases offers.len()
{
    if offers.len() == 0 { 1 }
    else { rank(offers.drop_last(), x, prerelease) + if eligible(offers.last(), E::Any, prerelease) && index_first_before(offers.last(), x) { 1int } else { 0int } }
}
pub open spec fn count(offers: Seq<V>, prerelease: bool) -> int
    decreases offers.len()
{
    if offers.len() == 0 { 0 } else { count(offers.drop_last(), prerelease) + if eligible(offers.last(), E::Any, prerelease) { 1int } else { 0int } }
}
pub open spec fn pos(offers: Seq<V>, x: V, prerelease: bool) -> int { if eligible(x, E::Any, prerelease) { rank(offers, x, prerelease) } else { 0 } }

// Releases: index order, SQL order and semantic order agree (the iteration31 premise).
pub proof fn releases_agree(a: V, b: V)
    requires a.pre.len() == 0, b.pre.len() == 0, (a.major, a.minor, a.patch) != (b.major, b.minor, b.patch)
    ensures sql_before(a, b) == (semver_cmp(a, b) > 0), index_first_before(a, b) == (semver_cmp(a, b) > 0)
{
    assert(key(a) =~= seq![Sub::N(a.major as int), Sub::N(a.minor as int), Sub::N(a.patch as int), Sub::N(2)]);
    assert(key(b) =~= seq![Sub::N(b.major as int), Sub::N(b.minor as int), Sub::N(b.patch as int), Sub::N(2)]);
    reveal_with_fuel(cmp_key, 5);
}

// A release and its own prerelease: the index and semantic order put the release first; the SQL
// order puts the prerelease first.
pub proof fn release_versus_own_prerelease(r: V, p: V)
    requires r.pre.len() == 0, p.pre.len() > 0, (r.major, r.minor, r.patch) == (p.major, p.minor, p.patch)
    ensures semver_cmp(r, p) > 0, !sql_before(r, p), sql_before(p, r)
{}

// Fixture outcomes: the cache query follows semantic order; the install does not.
pub proof fn fixture_neighbors()
    ensures
        semver_max(repo_d(), E::Any, true) == Some(v200()), installed(repo_d(), E::Any, true) == Some(v200b1()),
        semver_max(repo_c(), E::Any, true) == Some(v100rc10()), installed(repo_c(), E::Any, true) == Some(v100rc2()),
        installed(repo_b(), E::MajorX(1), true) == Some(v1100()), semver_max(repo_b(), E::MajorX(1), true) == Some(v1100()),
        installed(repo_d(), E::Exact(v200()), true) == Some(v200()),
        pos(repo_c(), v100rc10(), true) == 1, pos(repo_c(), v100rc2(), true) == 2,
        count(repo_c(), false) == 1, pos(repo_c(), v090(), false) == 1,
{
    assert(semver_max(repo_d(), E::Any, true) == Some(v200())) by (compute);
    assert(installed(repo_d(), E::Any, true) == Some(v200b1())) by (compute);
    assert(semver_max(repo_c(), E::Any, true) == Some(v100rc10())) by (compute);
    assert(installed(repo_c(), E::Any, true) == Some(v100rc2())) by (compute);
    assert(installed(repo_b(), E::MajorX(1), true) == Some(v1100())) by (compute);
    assert(semver_max(repo_b(), E::MajorX(1), true) == Some(v1100())) by (compute);
    assert(installed(repo_d(), E::Exact(v200()), true) == Some(v200())) by (compute);
    assert(pos(repo_c(), v100rc10(), true) == 1) by (compute);
    assert(pos(repo_c(), v100rc2(), true) == 2) by (compute);
    assert(count(repo_c(), false) == 1) by (compute);
    assert(pos(repo_c(), v090(), false) == 1) by (compute);
}

} // verus!

fn main() {}
