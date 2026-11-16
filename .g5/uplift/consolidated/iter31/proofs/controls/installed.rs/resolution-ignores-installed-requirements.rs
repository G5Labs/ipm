// IPM 571e04a1: installed modules' declared requirements constrain dependency choice and
// repository loads; directory loads are not checked.
// Domain: three fixture modules (A, B, P), one dependency edge each from A and P onto B,
// full x.y.z release versions encoded as naturals (major*100+minor*10+patch) and ranges
// with an inclusive lower bound and optional exclusive upper bound. Provider premises:
// SemanticVersionExpression And/IsSatisfiedBy agree with this range conjunction on these
// versions, and the filesystem repository search lists B candidates highest first.
// Not represented: prerelease/build ordering, partial-version parsing (C:D6), snapshot
// reloads, PermitDowngrade, co-updating exclusion, cycles, deeper graphs, message order.
use vstd::prelude::*;
verus! {

pub struct Range { pub lo: nat, pub hi: Option<nat> }

pub open spec fn sat(v: nat, r: Range) -> bool {
    v >= r.lo && match r.hi { Some(h) => v < h, None => true }
}

// Declared requirements on B in the fixture manifests.
pub open spec fn a_req() -> Range { Range { lo: 100, hi: Some(200) } }   // >=1.0.0 <2.0.0
pub open spec fn p_req() -> Range { Range { lo: 160, hi: None } }        // >=1.6.0

// Repository offers for B, in search order (highest first).
pub open spec fn cand(i: nat) -> nat { if i == 0 { 200 } else if i == 1 { 170 } else { 150 } }
pub open spec fn cand_count() -> nat { 3 }

#[derive(PartialEq, Eq)]
pub enum Parent { A, P }

pub struct World { pub a: bool, pub p: bool, pub b: Option<nat> }

// The set of requirements a candidate version must meet. Installed requirements come
// from Storage/Module VersionRequirements (rows of installed modules' dependencies on B,
// minus excluded module names); GetRequiredVersionExpression And-s them.
pub struct Reqs { pub parent: Option<Range>, pub from_a: bool, pub from_p: bool, pub floor: Option<nat> }

pub open spec fn sat_all(v: nat, r: Reqs) -> bool {
    (match r.parent { Some(x) => sat(v, x), None => true })
    && (r.from_a ==> sat(v, a_req()))
    && (r.from_p ==> sat(v, p_req()))
    && (match r.floor { Some(f) => v >= f, None => true })
}

pub struct Excluded { pub a: bool, pub p: bool }

pub open spec fn installed_reqs(w: World, x: Excluded) -> Reqs {
    Reqs { parent: None, from_a: w.a && !x.a, from_p: w.p && !x.p, floor: None }
}

pub open spec fn parent_range(p: Parent) -> Range { match p { Parent::A => a_req(), Parent::P => p_req() } }
pub open spec fn graph_excluded(p: Parent) -> Excluded {
    match p { Parent::A => Excluded { a: true, p: false }, Parent::P => Excluded { a: false, p: true } }
}

pub open spec fn first_sat(r: Reqs) -> Option<nat> {
    if sat_all(cand(0), r) { Some(cand(0)) }
    else if sat_all(cand(1), r) { Some(cand(1)) }
    else if sat_all(cand(2), r) { Some(cand(2)) }
    else { None }
}

pub enum Choice { Keep(nat), Pick(nat), NoSatisfactory }

// Storage/Module ProcessSingleDependencyIterative: keep an installed B that satisfies the
// parent and installed requirements; otherwise search with those requirements plus the
// no-downgrade floor and take the first (highest) satisfying candidate.
pub open spec fn resolve_b(w: World, parent: Parent) -> Choice {
    let base = Reqs { parent: Some(parent_range(parent)), ..installed_reqs(w, Excluded { a: true, p: true }) };
    match w.b {
        Some(v) if sat_all(v, base) => Choice::Keep(v),
        _ => match first_sat(Reqs { floor: w.b, ..base }) {
            Some(v) => Choice::Pick(v),
            None => Choice::NoSatisfactory,
        },
    }
}

// Utils/Module LoadModuleReference: refuse a requested B version that fails the
// requirements of installed modules, excluding the modules of the current dependency
// graph and adding the graph's own requirement on B.
pub open spec fn reference_ok(w: World, v: nat, graph: Option<Parent>) -> bool {
    match graph {
        Some(p) => sat_all(v, Reqs { parent: Some(parent_range(p)), ..installed_reqs(w, graph_excluded(p)) }),
        None => sat_all(v, installed_reqs(w, Excluded { a: false, p: false })),
    }
}

pub struct Outcome { pub world: World, pub ok: bool, pub msg_installed: bool, pub msg_none: bool }

pub open spec fn done(w: World) -> Outcome { Outcome { world: w, ok: true, msg_installed: false, msg_none: false } }
pub open spec fn refused(w: World) -> Outcome { Outcome { world: w, ok: false, msg_installed: true, msg_none: false } }

pub open spec fn with_parent(w: World, p: Parent) -> World {
    match p { Parent::A => World { a: true, ..w }, Parent::P => World { p: true, ..w } }
}

// install of A or P from the repository: resolve B, load the chosen B reference, then the parent.
pub open spec fn install_parent(w: World, p: Parent) -> Outcome {
    match resolve_b(w, p) {
        Choice::Keep(_) => done(with_parent(w, p)),
        Choice::Pick(v) => if reference_ok(w, v, Some(p)) { done(with_parent(World { b: Some(v), ..w }, p)) } else { refused(w) },
        Choice::NoSatisfactory => Outcome { world: w, ok: false, msg_installed: false, msg_none: true },
    }
}

// Main.Install for B itself: take the first repository result matching the literal
// (installed requirements do not filter it), then LoadModuleReference with no graph.
pub open spec fn top_choice(literal: Option<nat>) -> Option<nat> {
    match literal {
        None => Some(cand(0)),
        Some(l) => if l == cand(0) || l == cand(1) || l == cand(2) { Some(l) } else { None },
    }
}

pub open spec fn install_b(w: World, literal: Option<nat>) -> Outcome {
    match top_choice(literal) {
        Some(v) => if reference_ok(w, v, None) { done(World { b: Some(v), ..w }) } else { refused(w) },
        None => Outcome { world: w, ok: false, msg_installed: false, msg_none: false },
    }
}

// load <directory>: the directory module is installed without the installed-requirement check.
pub open spec fn load_b_directory(w: World, v: nat) -> Outcome { done(World { b: Some(v), ..w }) }

pub open spec fn uninstall_a(w: World) -> Outcome { done(World { a: false, ..w }) }

pub open spec fn empty() -> World { World { a: false, p: false, b: None } }
pub open spec fn empty_outcome() -> Outcome { done(empty()) }

// Native schedule.
pub open spec fn s1_b15() -> Outcome { install_b(empty(), Some(150)) }
pub open spec fn s1_p() -> Outcome { install_parent(s1_b15().world, Parent::P) }
pub open spec fn s2_b15() -> Outcome { install_b(empty(), Some(150)) }
pub open spec fn s2_a() -> Outcome { install_parent(s2_b15().world, Parent::A) }
pub open spec fn s2_p() -> Outcome { install_parent(s2_a().world, Parent::P) }
pub open spec fn s3_b20() -> Outcome { install_b(s2_p().world, Some(200)) }
pub open spec fn s4_bany() -> Outcome { install_b(s3_b20().world, None) }
pub open spec fn s5_b17() -> Outcome { install_b(s4_bany().world, Some(170)) }
pub open spec fn s6_load20() -> Outcome { load_b_directory(s5_b17().world, 200) }
pub open spec fn s7_b15() -> Outcome { install_b(empty(), Some(150)) }
pub open spec fn s7_a() -> Outcome { install_parent(s7_b15().world, Parent::A) }
pub open spec fn s7_una() -> Outcome { uninstall_a(s7_a().world) }
pub open spec fn s7_b20() -> Outcome { install_b(s7_una().world, Some(200)) }

pub open spec fn code(b: Option<nat>) -> int { match b { Some(v) => v as int, None => 0 } }
pub open spec fn bi(x: bool) -> int { if x { 1 } else { 0 } }

// Resolution: the same P request picks 2.0.0 without A and 1.7.0 with A installed.
pub proof fn resolution_neighbor()
    ensures
        s1_p().ok && s1_p().world.b == Some(200nat),
        s2_a().ok && s2_a().world.b == Some(150nat),
        s2_p().ok && s2_p().world.b == Some(170nat),
{}

// Explicit and unversioned repository installs of B are refused by installed requirements;
// an explicit satisfying version succeeds; removing A admits 2.0.0.
pub proof fn reference_neighbor()
    ensures
        !s3_b20().ok && s3_b20().msg_installed && s3_b20().world.b == Some(170nat),
        !s4_bany().ok && s4_bany().msg_installed && s4_bany().world.b == Some(170nat),
        s5_b17().ok && s5_b17().world.b == Some(170nat),
        s7_b20().ok && s7_b20().world.b == Some(200nat) && !s7_b20().world.a,
{}

// Directory load installs 2.0.0 although installed A requires <2.0.0.
pub proof fn directory_neighbor()
    ensures
        s6_load20().ok && s6_load20().world.b == Some(200nat) && s6_load20().world.a
            && !sat(200, a_req()),
{}

// The unversioned refusal is not forced by the requirements: a satisfying offer exists.
pub proof fn unversioned_has_satisfying_offer()
    ensures
        sat_all(cand(1), installed_reqs(s3_b20().world, Excluded { a: false, p: false })),
        !sat_all(cand(0), installed_reqs(s3_b20().world, Excluded { a: false, p: false })),
{}

} // verus!
fn main() {}
