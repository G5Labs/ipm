#![allow(non_snake_case)]
// Iteration35: `list-installed` flat, sorted and tree reports.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter35/bindings.json):
//   Main.cls::ListInstalled          flat (GetListModules + label "External (name)") or tree (GetListModule + dependency rows)
//   Main.cls::GetListModules         installed rows: name, version, external name, developer mode, root
//   Main.cls::SortList               IPM first; name / name-desc by lowercase label; version by zero-padded
//                                    triple then prerelease text ("~" for a release) then case-sensitive label
//   Main.cls::DisplayModules         label, version, "(DeveloperMode)", root with -showsource
//   Main.cls::GetListModule, AccumulateTreeRecursive, PrintTree
//                                    roots = installed modules no installed module depends on, in lowercase
//                                    name order; each dependency printed under each parent, "[missing]" when
//                                    not installed; -s has no effect
//
// Labels and names are character-code sequences. Premises (native-qualified): IRIS string subscript
// collation equals code-point order for these ASCII labels, zero-padding makes major/minor/patch numeric,
// and the fixture declares at most one dependency per module, so sibling order inside a tree is not modelled.
use vstd::prelude::*;

verus! {

pub struct Mod { pub label: Seq<nat>, pub name: Seq<nat>, pub major: nat, pub minor: nat, pub patch: nat, pub pre: Seq<nat>, pub dev: bool, pub ipm: bool, pub deps: Seq<int> }

pub open spec fn lower_c(c: nat) -> nat { if 65 <= c && c <= 90 { c + 32 } else { c } }
pub open spec fn lower(s: Seq<nat>) -> Seq<nat> { Seq::new(s.len(), |i: int| lower_c(s[i])) }
pub open spec fn cmp_nat(a: nat, b: nat) -> int { if a < b { -1 } else if a > b { 1 } else { 0 } }
pub open spec fn cmp_text(a: Seq<nat>, b: Seq<nat>) -> int
    decreases a.len()
{
    if a.len() == 0 && b.len() == 0 { 0 } else if a.len() == 0 { -1 } else if b.len() == 0 { 1 }
    else if a[0] != b[0] { cmp_nat(a[0], b[0]) } else { cmp_text(a.drop_first(), b.drop_first()) }
}

// SortList keys. A release's prerelease key is "~" (code 126).
pub open spec fn pre_key(m: Mod) -> Seq<nat> { if m.pre.len() == 0 { seq![126nat] } else { m.pre } }
pub open spec fn version_cmp(a: Mod, b: Mod) -> int {
    if a.major != b.major { cmp_nat(a.major, b.major) }
    else if a.minor != b.minor { cmp_nat(b.minor, a.minor) }
    else if a.patch != b.patch { cmp_nat(a.patch, b.patch) }
    else if cmp_text(pre_key(a), pre_key(b)) != 0 { cmp_text(pre_key(a), pre_key(b)) }
    else { cmp_text(a.label, b.label) }
}
// mode 0 name, 1 name-desc, 2 version
pub open spec fn before(a: Mod, b: Mod, mode: int) -> bool {
    if mode == 0 { cmp_text(lower(a.label), lower(b.label)) < 0 }
    else if mode == 1 { cmp_text(lower(a.label), lower(b.label)) > 0 }
    else { version_cmp(a, b) < 0 }
}

pub open spec fn count_before(ms: Seq<Mod>, present: Seq<bool>, i: int, mode: int, j: int) -> int
    decreases j
{
    if j <= 0 { 0 }
    else {
        count_before(ms, present, i, mode, j - 1)
            + if j - 1 != i && present[j - 1] && !ms[j - 1].ipm && before(ms[j - 1], ms[i], mode) { 1int } else { 0int }
    }
}
pub open spec fn any_ipm(ms: Seq<Mod>, present: Seq<bool>, j: int) -> bool
    decreases j
{
    if j <= 0 { false } else { (present[j - 1] && ms[j - 1].ipm) || any_ipm(ms, present, j - 1) }
}
// Flat report position (1-based, 0 when absent). IPM is always first.
pub open spec fn flat_pos(ms: Seq<Mod>, present: Seq<bool>, i: int, mode: int) -> int {
    if !present[i] { 0 }
    else if ms[i].ipm { 1 }
    else { 1 + (if any_ipm(ms, present, ms.len() as int) { 1int } else { 0int }) + count_before(ms, present, i, mode, ms.len() as int) }
}

// Tree: a module is a root when it is installed and no installed module lists it as a dependency.
pub open spec fn listed_by(ms: Seq<Mod>, present: Seq<bool>, i: int, j: int) -> bool
    decreases j
{
    if j <= 0 { false }
    else { (present[j - 1] && ms[j - 1].deps.len() > 0 && ms[j - 1].deps[0] == i) || listed_by(ms, present, i, j - 1) }
}
pub open spec fn is_root(ms: Seq<Mod>, present: Seq<bool>, i: int) -> bool { present[i] && !listed_by(ms, present, i, ms.len() as int) }
pub open spec fn root_rank(ms: Seq<Mod>, present: Seq<bool>, i: int, j: int) -> int
    decreases j
{
    if j <= 0 { 1 }
    else { root_rank(ms, present, i, j - 1) + if j - 1 != i && is_root(ms, present, j - 1) && cmp_text(ms[j - 1].name, ms[i].name) < 0 { 1int } else { 0int } }
}
pub open spec fn root_at(ms: Seq<Mod>, present: Seq<bool>, k: int, j: int) -> int
    decreases j
{
    if j <= 0 { -1 }
    else if is_root(ms, present, j - 1) && root_rank(ms, present, j - 1, ms.len() as int) == k { j - 1 }
    else { root_at(ms, present, k, j - 1) }
}
pub struct Row { pub m: int, pub depth: int }
// A printed node and its (single) dependency chain; a missing module prints with no children.
pub open spec fn expand(ms: Seq<Mod>, present: Seq<bool>, m: int, depth: int, fuel: nat) -> Seq<Row>
    decreases fuel
{
    if fuel == 0 { seq![Row { m, depth }] }
    else if present[m] && ms[m].deps.len() > 0 { seq![Row { m, depth }] + expand(ms, present, ms[m].deps[0], depth + 1, (fuel - 1) as nat) }
    else { seq![Row { m, depth }] }
}
pub open spec fn tree_from(ms: Seq<Mod>, present: Seq<bool>, k: int, fuel: nat) -> Seq<Row>
    decreases fuel
{
    if fuel == 0 { seq![] }
    else {
        let r = root_at(ms, present, k, ms.len() as int);
        if r < 0 { seq![] } else { expand(ms, present, r, 0, 4) + tree_from(ms, present, k + 1, (fuel - 1) as nat) }
    }
}
pub open spec fn tree(ms: Seq<Mod>, present: Seq<bool>) -> Seq<Row> { tree_from(ms, present, 1, ms.len() as nat) }

pub open spec fn first_pos(rows: Seq<Row>, m: int) -> int
    decreases rows.len()
{
    if rows.len() == 0 { 0 }
    else if first_pos(rows.drop_last(), m) != 0 { first_pos(rows.drop_last(), m) }
    else if rows.last().m == m { rows.len() as int } else { 0 }
}
pub open spec fn occurrences(rows: Seq<Row>, m: int) -> int
    decreases rows.len()
{
    if rows.len() == 0 { 0 } else { occurrences(rows.drop_last(), m) + if rows.last().m == m { 1int } else { 0int } }
}
pub open spec fn depth_at(rows: Seq<Row>, p: int) -> int { if 1 <= p && p <= rows.len() { rows[p - 1].depth } else { 0 } }
pub open spec fn second_pos(rows: Seq<Row>, m: int) -> int
    decreases rows.len()
{
    if rows.len() == 0 { 0 }
    else if occurrences(rows.drop_last(), m) == 1 && rows.last().m == m { rows.len() as int }
    else { second_pos(rows.drop_last(), m) }
}

// Fixture: 0 zpm ("IPM (zpm)"), 1 i35a, 2 i35b, 3 i35c, 4 i35d, 5 i35e (dev), 6 i35x 1.10.0,
// 7 i35y 1.9.0, 8 i35pa 1.0.0-rc.2, 9 i35pb 1.0.0-rc.10, 10 i35n ("Zeta35 (i35n)").
pub open spec fn i35(s: Seq<nat>) -> Seq<nat> { seq![105nat, 51, 53] + s }
pub open spec fn mk(name: Seq<nat>, major: nat, minor: nat, patch: nat, pre: Seq<nat>, deps: Seq<int>) -> Mod {
    Mod { label: name, name, major, minor, patch, pre, dev: false, ipm: false, deps }
}
pub open spec fn rc(n: Seq<nat>) -> Seq<nat> { seq![114nat, 99, 46] + n }
pub open spec fn mods() -> Seq<Mod> {
    seq![
        Mod { label: seq![73nat, 80, 77, 32, 40, 122, 112, 109, 41], name: seq![122nat, 112, 109], major: 0, minor: 10, patch: 11, pre: seq![], dev: false, ipm: true, deps: seq![] },
        mk(i35(seq![97nat]), 1, 0, 0, seq![], seq![]),
        mk(i35(seq![98nat]), 1, 0, 0, seq![], seq![1int]),
        mk(i35(seq![99nat]), 1, 0, 0, seq![], seq![2int]),
        mk(i35(seq![100nat]), 1, 0, 0, seq![], seq![1int]),
        Mod { dev: true, ..mk(i35(seq![101nat]), 1, 0, 0, seq![], seq![]) },
        mk(i35(seq![120nat]), 1, 10, 0, seq![], seq![]),
        mk(i35(seq![121nat]), 1, 9, 0, seq![], seq![]),
        mk(i35(seq![112nat, 97]), 1, 0, 0, rc(seq![50nat]), seq![]),
        mk(i35(seq![112nat, 98]), 1, 0, 0, rc(seq![49nat, 48]), seq![]),
        Mod { label: seq![90nat, 101, 116, 97, 51, 53, 32, 40, 105, 51, 53, 110, 41], ..mk(i35(seq![110nat]), 1, 0, 0, seq![], seq![]) },
    ]
}
pub open spec fn all() -> Seq<bool> { Seq::new(11, |i: int| true) }
pub open spec fn cut() -> Seq<bool> { Seq::new(11, |i: int| i != 1) }

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
pub open spec fn vcode(ms: Seq<Mod>, present: Seq<bool>, i: int) -> int {
    let m = ms[i];
    if !present[i] { 9 } else if m.ipm { 8 }
    else if m.pre.len() > 0 { if m.pre.len() == 4 { 4 } else { 5 } }
    else if m.minor == 10 { 2 } else if m.minor == 9 { 3 } else { 1 }
}

// The neighbours this iteration distinguishes.
pub proof fn sort_neighbors()
    ensures
        flat_pos(mods(), all(), 0, 1) == 1,
        flat_pos(mods(), all(), 10, 0) == 11, flat_pos(mods(), all(), 10, 1) == 2,
        flat_pos(mods(), all(), 7, 2) < flat_pos(mods(), all(), 6, 2),
        flat_pos(mods(), all(), 9, 2) < flat_pos(mods(), all(), 8, 2),
        flat_pos(mods(), all(), 6, 0) < flat_pos(mods(), all(), 7, 0),
{
    assert(flat_pos(mods(), all(), 0, 1) == 1) by (compute);
    assert(flat_pos(mods(), all(), 10, 0) == 11) by (compute);
    assert(flat_pos(mods(), all(), 10, 1) == 2) by (compute);
    assert(flat_pos(mods(), all(), 7, 2) == 10) by (compute);
    assert(flat_pos(mods(), all(), 6, 2) == 11) by (compute);
    assert(flat_pos(mods(), all(), 9, 2) == 2) by (compute);
    assert(flat_pos(mods(), all(), 8, 2) == 3) by (compute);
    assert(flat_pos(mods(), all(), 6, 0) == 9) by (compute);
    assert(flat_pos(mods(), all(), 7, 0) == 10) by (compute);
}

pub proof fn tree_neighbors()
    ensures
        occurrences(tree(mods(), all()), 1) == 2,
        depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 1)) == 2,
        depth_at(tree(mods(), all()), second_pos(tree(mods(), all()), 1)) == 1,
        first_pos(tree(mods(), all()), 2) == 2,
        tree(mods(), all()).len() == 12,
        occurrences(tree(mods(), cut()), 1) == 2,
        tree(mods(), cut()).len() == 12,
{
    assert(occurrences(tree(mods(), all()), 1) == 2) by (compute);
    assert(depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 1)) == 2) by (compute);
    assert(depth_at(tree(mods(), all()), second_pos(tree(mods(), all()), 1)) == 1) by (compute);
    assert(first_pos(tree(mods(), all()), 2) == 2) by (compute);
    assert(tree(mods(), all()).len() == 12) by (compute);
    assert(occurrences(tree(mods(), cut()), 1) == 2) by (compute);
    assert(tree(mods(), cut()).len() == 12) by (compute);
}

} // verus!

fn main() {}
