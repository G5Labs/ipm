// Iteration33: list-dependents report derived from installed/repository dependency graphs.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter33/bindings.json):
//   Utils/Module.cls::BuildAllDependencyGraphs   roots = installed modules (installed mode) or repository
//                                                manifests with dependencies (-r); graph errors -> warnings
//   Storage/Module.cls::BuildDependencyGraph     (already bound elsewhere) resolution of each dependency; records a
//                                                parent edge only when the parent is not the graph root
//   Utils/Module.cls::GetDependentsList          flat: roots whose graph contains the target, optional version filter
//   Utils/Module.cls::GetDependentsAsTree, InvertDependencies   tree: walk recorded parent edges up from the target
//   Main.cls::ListDependents, PrintTree          printed lines, warnings, tree padding
//
// Domain: fixture modules a..g as 0..6, each declaring at most one dependency with an inclusive lower
// bound; versions as codes (1.0.0 = 100). Repository r33 offers a, b, f at 100. Provider premises:
// SemanticVersionExpression comparison agrees with integer lower bounds; installed-module requirement
// collection excludes the graph's own modules (iteration31); the SQL order of roots is not modelled.
#![allow(non_snake_case)]
use vstd::prelude::*;

verus! {

pub open spec fn N() -> int { 7 }
pub open spec fn A() -> int { 0 }
pub open spec fn B() -> int { 1 }
pub open spec fn C() -> int { 2 }
pub open spec fn D() -> int { 3 }
pub open spec fn E() -> int { 4 }
pub open spec fn F() -> int { 5 }
pub open spec fn G() -> int { 6 }

// Declared manifests (identical in loose directories and r33).
pub open spec fn dep_of(m: int) -> int {
    if m == 1 { 0 } else if m == 2 { 1 } else if m == 3 { 0 } else if m == 5 { 0 } else if m == 6 { 2 } else { -1 }
}
pub open spec fn dep_min(m: int) -> int { if m == 3 { 200 } else { 100 } }
pub open spec fn repo_ver(m: int) -> int { if m == 0 || m == 1 || m == 5 { 100 } else { 0 } }

pub struct World { pub inst: Seq<int> }   // installed version code, 0 when absent

pub open spec fn installed(w: World, m: int) -> bool { 0 <= m < w.inst.len() && w.inst[m] > 0 }

// Node at depth k of root r's graph (k = 0 is the root), -1 when the chain ends.
pub open spec fn chain_node(r: int, k: nat) -> int
    decreases k
{
    if k == 0 { r } else { let p = chain_node(r, (k - 1) as nat); if p < 0 { -1 } else { dep_of(p) } }
}
pub open spec fn depth_from(r: int, t: int, k: nat) -> int
    decreases 8 - k
{
    if k >= 8 { 0 } else if chain_node(r, k) < 0 { 0 } else if chain_node(r, k) == t { k as int } else { depth_from(r, t, k + 1) }
}
// Depth of t in r's graph; 0 means absent (the root itself is not its own dependency).
pub open spec fn depth_in(r: int, t: int) -> int { depth_from(r, t, 1) }
pub open spec fn in_graph(r: int, t: int) -> bool { depth_in(r, t) > 0 }
pub open spec fn graph_member(r: int, y: int) -> bool { y == r || in_graph(r, y) }

// Installed modules outside this graph that require x must also be satisfied by v.
pub open spec fn others_ok_from(w: World, r: int, x: int, v: int, y: int) -> bool
    decreases N() - y
{
    if y >= N() || y < 0 { true }
    else {
        (!(installed(w, y) && !graph_member(r, y) && dep_of(y) == x) || v >= dep_min(y))
        && others_ok_from(w, r, x, v, y + 1)
    }
}
pub open spec fn acceptable(w: World, r: int, x: int, min: int, v: int) -> bool { v >= min && others_ok_from(w, r, x, v, 0) }

// Installed mode: keep a satisfying installed version, otherwise take a satisfying repository offer.
pub open spec fn resolve_installed(w: World, r: int, x: int, min: int) -> int {
    if installed(w, x) && acceptable(w, r, x, min, w.inst[x]) { w.inst[x] }
    else if repo_ver(x) > 0 && acceptable(w, r, x, min, repo_ver(x)) && (!installed(w, x) || repo_ver(x) >= w.inst[x]) { repo_ver(x) }
    else { -1 }
}
// Repository mode ignores installed modules entirely.
pub open spec fn resolve_repo(x: int, min: int) -> int { if repo_ver(x) > 0 && repo_ver(x) >= min { repo_ver(x) } else { -1 } }

pub open spec fn resolve(w: World, repo: bool, r: int, x: int, min: int) -> int {
    if repo { resolve_repo(x, min) } else { resolve_installed(w, r, x, min) }
}
// Version resolved for the node at depth k (k >= 1) of r's graph.
pub open spec fn node_ver(w: World, repo: bool, r: int, k: nat) -> int {
    resolve(w, repo, r, chain_node(r, k), dep_min(chain_node(r, (k - 1) as nat)))
}
pub open spec fn graph_ok_from(w: World, repo: bool, r: int, k: nat) -> bool
    decreases 8 - k
{
    if k >= 8 || chain_node(r, k) < 0 { true } else { node_ver(w, repo, r, k) >= 0 && graph_ok_from(w, repo, r, k + 1) }
}
pub open spec fn graph_ok(w: World, repo: bool, r: int) -> bool { graph_ok_from(w, repo, r, 1) }

// Graph roots: every installed module, or repository manifests that declare a dependency.
pub open spec fn is_root(w: World, repo: bool, r: int) -> bool {
    if repo { repo_ver(r) > 0 && dep_of(r) >= 0 } else { installed(w, r) }
}
pub open spec fn root_ver(w: World, repo: bool, r: int) -> int { if repo { repo_ver(r) } else { w.inst[r] } }

// GetDependentsList / flat ListDependents line for root r.
pub open spec fn listed(w: World, repo: bool, t: int, vf: int, r: int) -> bool {
    is_root(w, repo, r) && graph_ok(w, repo, r) && in_graph(r, t)
    && (vf == 0 || node_ver(w, repo, r, depth_in(r, t) as nat) == vf)
}
// BuildAllDependencyGraphs error list: roots whose graph failed, whatever the target.
pub open spec fn warned(w: World, repo: bool, r: int) -> bool { is_root(w, repo, r) && !graph_ok(w, repo, r) }

pub open spec fn any_warned_from(w: World, repo: bool, r: int) -> bool
    decreases N() - r
{
    if r >= N() || r < 0 { false } else { warned(w, repo, r) || any_warned_from(w, repo, r + 1) }
}

// Tree: from root r, InvertDependencies walks parent edges from t. BuildDependencyGraph records a
// parent edge only for a non-root parent, so the walk stops at the depth-1 node and never reaches r.
pub open spec fn tree_path_has(r: int, t: int, x: int) -> bool {
    in_graph(r, t) && (x == t || (in_graph(r, x) && depth_in(r, x) < depth_in(r, t)))
}
pub open spec fn tree_root_ok(w: World, r: int, t: int) -> bool { listed(w, false, t, 0, r) }

pub open spec fn tree_has_from(w: World, t: int, x: int, r: int) -> bool
    decreases N() - r
{
    if r >= N() || r < 0 { false } else { (tree_root_ok(w, r, t) && tree_path_has(r, t, x)) || tree_has_from(w, t, x, r + 1) }
}
pub open spec fn tree_has(w: World, t: int, x: int) -> bool { tree_has_from(w, t, x, 0) }

// Tree depth of x below t, taken from the first selected root containing x on its path.
pub open spec fn tree_depth_from(w: World, t: int, x: int, r: int) -> int
    decreases N() - r
{
    if r >= N() || r < 0 { 0 }
    else if tree_root_ok(w, r, t) && tree_path_has(r, t, x) { depth_in(r, t) - depth_in(r, x) }
    else { tree_depth_from(w, t, x, r + 1) }
}
pub open spec fn tree_depth(w: World, t: int, x: int) -> int { tree_depth_from(w, t, x, 0) }
pub open spec fn tree_ver_from(w: World, t: int, x: int, r: int) -> int
    decreases N() - r
{
    if r >= N() || r < 0 { 0 }
    else if tree_root_ok(w, r, t) && tree_path_has(r, t, x) { node_ver(w, false, r, depth_in(r, x) as nat) }
    else { tree_ver_from(w, t, x, r + 1) }
}
pub open spec fn tree_ver(w: World, t: int, x: int) -> int { tree_ver_from(w, t, x, 0) }

// PrintTree column of a node's name (1-based): depth 0 at column 1, then "├──" and two characters per level.
pub open spec fn col(d: int) -> int { if d == 0 { 1 } else { 3 * d + 1 } }

pub open spec fn tree_count_from(w: World, t: int, x: int) -> int
    decreases N() - x
{
    if x >= N() || x < 0 { 0 } else { (if tree_has(w, t, x) { 1int } else { 0int }) + tree_count_from(w, t, x + 1) }
}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

// Native worlds.
pub open spec fn w0() -> World { World { inst: seq![200int, 100, 100, 100, 100, 0, 100] } }   // a2, b, c, d, e, g
pub open spec fn w2() -> World { World { inst: seq![0int, 100, 100, 100, 100, 0, 100] } }     // after uninstall a -force
pub open spec fn w3() -> World { World { inst: seq![0int, 100, 100, 0, 100, 0, 100] } }       // after uninstall d -force

// Neighbours distinguished by this iteration.

// Flat report is transitive; the version argument filters on the version resolved in each dependent's graph.
pub proof fn flat_neighbors()
    ensures
        listed(w0(), false, A(), 0, B()), listed(w0(), false, A(), 0, C()), listed(w0(), false, A(), 0, G()), listed(w0(), false, A(), 0, D()),
        !listed(w0(), false, A(), 0, E()),
        !listed(w0(), false, A(), 100, B()), listed(w0(), false, A(), 200, G()),
        listed(w0(), false, B(), 0, G()), !listed(w0(), false, B(), 0, D()),
{
    reveal_with_fuel(chain_node, 8); reveal_with_fuel(depth_from, 8); reveal_with_fuel(graph_ok_from, 8); reveal_with_fuel(others_ok_from, 8);
    assert(listed(w0(), false, A(), 0, B())) by (compute);
    assert(listed(w0(), false, A(), 0, C())) by (compute);
    assert(listed(w0(), false, A(), 0, G())) by (compute);
    assert(listed(w0(), false, A(), 0, D())) by (compute);
    assert(!listed(w0(), false, A(), 0, E())) by (compute);
    assert(!listed(w0(), false, A(), 100, B())) by (compute);
    assert(listed(w0(), false, A(), 200, G())) by (compute);
    assert(listed(w0(), false, B(), 0, G())) by (compute);
    assert(!listed(w0(), false, B(), 0, D())) by (compute);
}

// As built, the tree omits every graph root: direct dependents b and d, and top-level dependents c and g
// as roots, are absent; only the target and intermediate modules on some root's path are printed.
pub proof fn tree_omits_roots()
    ensures
        tree_has(w0(), A(), A()), tree_has(w0(), A(), B()), tree_has(w0(), A(), C()),
        !tree_has(w0(), A(), D()), !tree_has(w0(), A(), G()),
        tree_depth(w0(), A(), C()) == 2,
        col(tree_depth(w0(), A(), C())) == 6, col(tree_depth(w0(), A(), B())) == 4,
        tree_has(w0(), B(), C()), !tree_has(w0(), B(), G()),
        !tree_has(w0(), D(), D()),
{
    reveal_with_fuel(chain_node, 8); reveal_with_fuel(depth_from, 8); reveal_with_fuel(graph_ok_from, 8); reveal_with_fuel(others_ok_from, 8);
    reveal_with_fuel(tree_has_from, 8); reveal_with_fuel(tree_depth_from, 8);
    assert(tree_has(w0(), A(), A())) by (compute);
    assert(tree_has(w0(), A(), B())) by (compute);
    assert(tree_has(w0(), A(), C())) by (compute);
    assert(!tree_has(w0(), A(), D())) by (compute);
    assert(!tree_has(w0(), A(), G())) by (compute);
    assert(tree_depth(w0(), A(), C()) == 2) by (compute);
    assert(tree_depth(w0(), A(), B()) == 1) by (compute);
    assert(tree_has(w0(), B(), C())) by (compute);
    assert(!tree_has(w0(), B(), G())) by (compute);
    assert(!tree_has(w0(), D(), D())) by (compute);
}

// Repository mode reports repository manifests only; installed c, d, g are absent and f appears.
pub proof fn repo_neighbors()
    ensures
        listed(w0(), true, A(), 0, B()), listed(w0(), true, A(), 0, F()),
        !listed(w0(), true, A(), 0, C()), !listed(w0(), true, A(), 0, D()),
        !listed(w0(), true, A(), 200, B()),
{
    reveal_with_fuel(chain_node, 8); reveal_with_fuel(depth_from, 8); reveal_with_fuel(graph_ok_from, 8);
    assert(listed(w0(), true, A(), 0, B())) by (compute);
    assert(listed(w0(), true, A(), 0, F())) by (compute);
    assert(!listed(w0(), true, A(), 0, C())) by (compute);
    assert(!listed(w0(), true, A(), 0, D())) by (compute);
    assert(!listed(w0(), true, A(), 200, B())) by (compute);
}

// Removing the target: d's installed requirement (>=2.0.0) also constrains b's, c's and g's graphs, so
// every dependent becomes a warning; once d is removed they resolve a from r33 and are listed at 1.0.0.
pub proof fn removed_target_neighbors()
    ensures
        warned(w2(), false, B()), warned(w2(), false, C()), warned(w2(), false, G()), warned(w2(), false, D()),
        !listed(w2(), false, A(), 0, B()),
        !warned(w3(), false, B()), listed(w3(), false, A(), 100, B()), listed(w3(), false, A(), 100, G()),
{
    reveal_with_fuel(chain_node, 8); reveal_with_fuel(depth_from, 8); reveal_with_fuel(graph_ok_from, 8); reveal_with_fuel(others_ok_from, 8);
    assert(warned(w2(), false, B())) by (compute);
    assert(warned(w2(), false, C())) by (compute);
    assert(warned(w2(), false, G())) by (compute);
    assert(warned(w2(), false, D())) by (compute);
    assert(!listed(w2(), false, A(), 0, B())) by (compute);
    assert(!warned(w3(), false, B())) by (compute);
    assert(listed(w3(), false, A(), 100, B())) by (compute);
    assert(listed(w3(), false, A(), 100, G())) by (compute);
}

} // verus!

fn main() {}
