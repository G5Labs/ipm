// L1/L2 models of IPM's dependency resolver (ipm 571e04a1, Storage/Module.cls BuildDependencyGraph #L898-L1048 and
// ProcessSingleDependencyIterative #L1068-L1295), with an inductive invariant, per-operation refinement (soundness)
// proofs, and proved witnesses for DEC-DEP-1, F-DEP-1, F-DEP-2 and F-DEP-3.
//
// Abstraction (each item is in update_steps.TRUST.md with what checks it):
//   - module names are nats ordered as IRIS collates the lower-case names (the order $order walks workQueue);
//   - versions are nats ordered as SemanticVersion.Follows; a version expression is the set of versions it allows,
//     so And is set intersection and a clash is "no version allowed by every constraint" (trusted: the expression
//     algebra is exact; DEP-B9 fixed it in 0.10.8);
//   - the registry search (Repo/Manager.cls SearchRepositoriesForModule) returns the versions of a module newest first,
//     and a manifest is a function of (module, version), the same for the installed copy (trusted, checked by the twin
//     against IRIS runs nc-dep-11, 11b, 12, 13, 17);
//   - platform filtering of deployed modules and snapshot re-reading are not modelled beyond the `refresh` input.
// No bound on modules, versions, requirements or graph depth: soundness is a safety invariant over unbounded maps.
// Termination is not claimed. Named premises: PU (no manifest names one module twice among its active requirements)
// and PR (no active requirement names the root module).
//
// The outcome is two-way: a resolution ends Done only with a sound choice, and ends Failed only for a stated reason
// (`why`, a ghost record of where the code stopped), which L1 checks against the graph: either an active requirement
// closes a chain of recorded requirements back onto a module on it (a cycle), or an active requirement of a module in
// the graph has no offered or installed version that meets it, the requirements recorded on its module, the installed
// modules' requirements and the no-downgrade floor. The recorded requirements are those of the versions the resolver
// already chose, so a failure can still be caused by an earlier greedy choice (DEC-DEP-1,
// dep_witness_greedy_rejects_solvable): L1 says why a resolution failed, not that no solution exists.
// PREMISE premise_pu = PU
// PREMISE premise_pr = PR
use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------- input

// A requirement: <ModuleReference> with Name, Version (as the set of versions it allows), exact or not, Scope.
pub struct Req { pub name: nat, pub range: ISet<nat>, pub exact: bool, pub scope: Option<nat> }

// A requirement of an installed module (%IPM_Storage.ModuleItem_Dependencies).
pub struct IReq { pub by: nat, pub on: nat, pub range: ISet<nat> }

pub struct Inp {
    pub root: nat,
    pub root_ver: nat,
    pub avail: Map<nat, Seq<nat>>,          // registry: versions per module, newest first
    pub man: Map<(nat, nat), Seq<Req>>,     // manifest of (module, version)
    pub inst: Map<nat, nat>,                // installed versions
    pub refresh: Set<nat>,                  // installed snapshots re-read from registries (pForceSnapshotReload)
    pub ireqs: Seq<IReq>,                   // requirements of installed modules
    pub known: Set<nat>,                    // GetKnownDependencies(root) and CoUpdatingModules
    pub phases: Set<nat>,                   // pPhases
    pub qphases: Set<nat>,                  // phases of the quick check: pCheckNestedScoped ? pPhases : ""
    pub ignore: bool,                       // pIgnoreInstalledModules
    pub permit: bool,                       // pPermitDowngrade
}

pub open spec fn manifest(g: Inp, n: nat, v: nat) -> Seq<Req> {
    if g.man.contains_key((n, v)) { g.man[(n, v)] } else { Seq::<Req>::empty() }
}

pub open spec fn versions(g: Inp, n: nat) -> Seq<nat> {
    if g.avail.contains_key(n) { g.avail[n] } else { Seq::<nat>::empty() }
}

// HasScope (#L834-L855): an unscoped requirement always counts; a scoped one only when its phase runs.
pub open spec fn active(ph: Set<nat>, q: Req) -> bool {
    match q.scope { Option::None => true, Option::Some(sc) => ph.contains(sc) }
}

// ---------------------------------------------------------------- L1: what a resolution must satisfy

pub open spec fn has(g: Inp, sel: Map<nat, nat>, n: nat) -> bool { n == g.root || sel.contains_key(n) }

pub open spec fn vsel(g: Inp, sel: Map<nat, nat>, n: nat) -> nat { if n == g.root { g.root_ver } else { sel[n] } }

// DEP-B1, B6, B15-B20: every active requirement of the root and of every chosen module is met by the chosen version.
pub open spec fn reqs_met(g: Inp, sel: Map<nat, nat>) -> bool {
    forall|m: nat, i: int| #![trigger manifest(g, m, vsel(g, sel, m))[i]]
        has(g, sel, m) && 0 <= i < manifest(g, m, vsel(g, sel, m)).len() && active(g.phases, manifest(g, m, vsel(g, sel, m))[i]) ==> {
            let q = manifest(g, m, vsel(g, sel, m))[i];
            has(g, sel, q.name) && q.range.contains(vsel(g, sel, q.name))
        }
}

// DEP-B21, B22: requirements of installed modules that stay outside the command are met.
pub open spec fn installed_met(g: Inp, sel: Map<nat, nat>) -> bool {
    !g.ignore ==> forall|i: int| #![trigger g.ireqs[i]]
        0 <= i < g.ireqs.len() && sel.contains_key(g.ireqs[i].on) && !g.known.contains(g.ireqs[i].by) && !sel.contains_key(g.ireqs[i].by)
            ==> g.ireqs[i].range.contains(sel[g.ireqs[i].on])
}

// DEP-B24: no installed module is moved to an older version unless PermitDowngrade.
pub open spec fn no_downgrade(g: Inp, sel: Map<nat, nat>) -> bool {
    !g.ignore && !g.permit ==> forall|d: nat| #![trigger sel.contains_key(d)] sel.contains_key(d) && g.inst.contains_key(d) ==> sel[d] >= g.inst[d]
}

// Every chosen version exists: offered by a registry, or the installed one.
pub open spec fn exists_ok(g: Inp, sel: Map<nat, nat>) -> bool {
    forall|d: nat| #![trigger sel.contains_key(d)] sel.contains_key(d) ==> versions(g, d).contains(sel[d]) || (g.inst.contains_key(d) && sel[d] == g.inst[d])
}

pub open spec fn sound(g: Inp, sel: Map<nat, nat>) -> bool {
    reqs_met(g, sel) && installed_met(g, sel) && no_downgrade(g, sel) && exists_ok(g, sel)
}

// DEC-DEP-1 (option B): resolution succeeds whenever a sound choice exists.
pub open spec fn solvable(g: Inp) -> bool { exists|sel: Map<nat, nat>| sound(g, sel) }

// ---------------------------------------------------------------- L2 state

pub enum St { Running, Done, Failed }

// Ghost: where a failed resolution stopped. Cycle(n, path): the cycle test of module n on its queue path (#L950-L953);
// NoVersion(m, mv, i): requirement i of module m at version mv found no version (#L1183, #L1284-L1291).
pub enum Why { Running, Cycle(nat, Seq<nat>), NoVersion(nat, nat, nat) }

// workQueue(depth, name) = $listbuild(modulePath, scope)
pub struct Entry { pub path: Seq<nat>, pub scope: Option<nat> }

// A requirement of the module being processed (index into its manifest) with its depModulePath.
pub struct Item { pub idx: nat, pub dpath: Seq<nat> }

pub struct Rs {
    pub sel: Map<nat, nat>,                  // pDependencyGraph(name): chosen version
    pub recs: Map<(nat, nat, nat), ISet<nat>>, // pDependencyGraph(dep, "parent version") = requirement
    pub cur: Map<nat, Entry>,                // workQueue(currentDepth)
    pub next: Map<nat, Entry>,               // workQueue(currentDepth + 1)
    pub par: nat,                            // module being processed
    pub par_ver: nat,
    pub pend: Seq<Item>,                     // its requirements still to process (exact first, then fuzzy)
    pub st: St,
    pub why: Why,                            // ghost
}

pub open spec fn init(g: Inp) -> Rs {
    Rs { sel: Map::<nat, nat>::empty(), recs: Map::<(nat, nat, nat), ISet<nat>>::empty(),
         cur: Map::<nat, Entry>::empty().insert(g.root, Entry { path: seq![g.root], scope: Option::None }),
         next: Map::<nat, Entry>::empty(), par: g.root, par_ver: g.root_ver, pend: Seq::<Item>::empty(), st: St::Running,
         why: Why::Running }
}

// moduleCache(name): the root object, or the manifest of the version chosen last.
pub open spec fn cache_ver(g: Inp, s: Rs, n: nat) -> nat { if s.sel.contains_key(n) { s.sel[n] } else { g.root_ver } }

// ---------------------------------------------------------------- L2: dequeue a module (#L918-L968)

// depModulePath (#L943-L947) and the cycle test (#L950-L953).
pub open spec fn dpath(q: Req, path: Seq<nat>) -> Seq<nat> {
    if q.scope is Some && path.len() <= 1 { Seq::<nat>::empty() } else { path }
}

pub open spec fn has_cycle(ph: Set<nat>, reqs: Seq<Req>, path: Seq<nat>) -> bool {
    exists|i: int| #![trigger reqs[i]] 0 <= i < reqs.len() && active(ph, reqs[i]) && dpath(reqs[i], path).contains(reqs[i].name)
}

// Classification (#L956-L962): active requirements, exact ones (in manifest order) or fuzzy ones.
pub open spec fn items_from(ph: Set<nat>, reqs: Seq<Req>, path: Seq<nat>, exact: bool, i: nat) -> Seq<Item>
    decreases reqs.len() - i
{
    if i >= reqs.len() { Seq::<Item>::empty() } else {
        let rest = items_from(ph, reqs, path, exact, i + 1);
        if active(ph, reqs[i as int]) && reqs[i as int].exact == exact {
            seq![Item { idx: i, dpath: dpath(reqs[i as int], path) }] + rest
        } else { rest }
    }
}

pub open spec fn classify(ph: Set<nat>, reqs: Seq<Req>, path: Seq<nat>) -> Seq<Item> {
    items_from(ph, reqs, path, true, 0) + items_from(ph, reqs, path, false, 0)
}

pub open spec fn is_min(m: Map<nat, Entry>, n: nat) -> bool {
    m.contains_key(n) && forall|x: nat| #![trigger m.contains_key(x)] m.contains_key(x) ==> n <= x
}

#[verifier::opaque]
pub open spec fn step_dequeue(g: Inp, s: Rs, n: nat) -> Rs {
    let e = s.cur[n];
    let path = if e.scope is Some && e.path.len() == 1 { Seq::<nat>::empty() } else { e.path };
    let v = cache_ver(g, s, n);
    let reqs = manifest(g, n, v);
    if has_cycle(g.phases, reqs, path) { Rs { st: St::Failed, why: Why::Cycle(n, path), ..s } }
    else { Rs { cur: s.cur.remove(n), par: n, par_ver: v, pend: classify(g.phases, reqs, path), ..s } }
}

// ---------------------------------------------------------------- L2: process one requirement (#L1068-L1295)

// Requirements already recorded on the module (#L1096-L1104).
pub open spec fn rec_ok(s: Rs, d: nat, v: nat) -> bool {
    forall|key: (nat, nat, nat)| #![trigger s.recs.contains_key(key)] s.recs.contains_key(key) && key.0 == d ==> s.recs[key].contains(v)
}

// Installed modules' requirements, except those of known, co-updating or already chosen modules (#L1106-L1121).
pub open spec fn excluded(g: Inp, s: Rs, b: nat) -> bool { g.known.contains(b) || s.sel.contains_key(b) }

pub open spec fn inst_ok(g: Inp, s: Rs, d: nat, v: nat) -> bool {
    g.ignore || forall|i: int| #![trigger g.ireqs[i]]
        0 <= i < g.ireqs.len() && g.ireqs[i].on == d && !excluded(g, s, g.ireqs[i].by) ==> g.ireqs[i].range.contains(v)
}

pub open spec fn base(g: Inp, s: Rs, q: Req, v: nat) -> bool { q.range.contains(v) && rec_ok(s, q.name, v) && inst_ok(g, s, q.name, v) }

// Keep the installed version (#L1130-L1146).
pub open spec fn keep_installed(g: Inp, s: Rs, q: Req) -> bool {
    &&& !g.ignore && g.inst.contains_key(q.name) && base(g, s, q, g.inst[q.name])
    &&& (!s.sel.contains_key(q.name) || s.sel[q.name] == g.inst[q.name])
    &&& !g.refresh.contains(q.name)
}

// ">=installed (currently installed, downgrade not permitted)" (#L1148-L1154).
pub open spec fn floor_ok(g: Inp, d: nat, v: nat) -> bool {
    (!g.ignore && g.inst.contains_key(d) && !g.permit) ==> v >= g.inst[d]
}

pub open spec fn cand(g: Inp, s: Rs, q: Req, v: nat) -> bool { base(g, s, q, v) && floor_ok(g, q.name, v) }

// Quick check of a fuzzy candidate's own requirements against the graph (#L1243-L1263).
pub open spec fn quick_ok(g: Inp, s: Rs, d: nat, c: nat) -> bool {
    forall|k: int| #![trigger manifest(g, d, c)[k]]
        0 <= k < manifest(g, d, c).len() && active(g.qphases, manifest(g, d, c)[k]) && s.sel.insert(d, c).contains_key(manifest(g, d, c)[k].name)
            ==> manifest(g, d, c)[k].range.contains(s.sel.insert(d, c)[manifest(g, d, c)[k].name])
}

// Exact: the first (newest) match (#L1185-L1208).
pub open spec fn first_cand(g: Inp, s: Rs, q: Req, vs: Seq<nat>, i: nat) -> Option<nat>
    decreases vs.len() - i
{
    if i >= vs.len() { Option::None }
    else if cand(g, s, q, vs[i as int]) { Option::Some(vs[i as int]) }
    else { first_cand(g, s, q, vs, i + 1) }
}

// Fuzzy: the first match that passes the quick check, else the last match (#L1209-L1291).
pub open spec fn fuzzy_pick(g: Inp, s: Rs, q: Req, vs: Seq<nat>, i: nat, last: Option<nat>) -> Option<nat>
    decreases vs.len() - i
{
    if i >= vs.len() { last }
    else if cand(g, s, q, vs[i as int]) {
        if quick_ok(g, s, q.name, vs[i as int]) { Option::Some(vs[i as int]) }
        else { fuzzy_pick(g, s, q, vs, i + 1, Option::Some(vs[i as int])) }
    } else { fuzzy_pick(g, s, q, vs, i + 1, last) }
}

pub open spec fn pick(g: Inp, s: Rs, q: Req) -> Option<nat> {
    if keep_installed(g, s, q) { Option::Some(g.inst[q.name]) }
    else if q.exact { first_cand(g, s, q, versions(g, q.name), 0) }
    else { fuzzy_pick(g, s, q, versions(g, q.name), 0, Option::None) }
}

#[verifier::opaque]
pub open spec fn step_process(g: Inp, s: Rs) -> Rs {
    let it = s.pend[0];
    let q = manifest(g, s.par, s.par_ver)[it.idx as int];
    match pick(g, s, q) {
        Option::None => Rs { st: St::Failed, why: Why::NoVersion(s.par, s.par_ver, it.idx), ..s },
        Option::Some(v) => Rs {
            sel: s.sel.insert(q.name, v),
            recs: s.recs.insert((q.name, s.par, s.par_ver), q.range),
            next: s.next.insert(q.name, Entry { path: it.dpath.push(q.name), scope: q.scope }),
            pend: s.pend.subrange(1, s.pend.len() as int),
            ..s
        },
    }
}

// ---------------------------------------------------------------- L2: next depth (#L1024-L1027) or the end

#[verifier::opaque]
pub open spec fn step_level(g: Inp, s: Rs) -> Rs {
    if s.next.dom() == Set::<nat>::empty() { Rs { st: St::Done, ..s } }
    else { Rs { cur: s.next, next: Map::<nat, Entry>::empty(), ..s } }
}

pub enum Choice { Dequeue(nat), Process, Level }

pub open spec fn enabled(s: Rs, c: Choice) -> bool {
    s.st == St::Running && match c {
        Choice::Dequeue(n) => s.pend.len() == 0 && is_min(s.cur, n),
        Choice::Process => s.pend.len() > 0,
        Choice::Level => s.pend.len() == 0 && s.cur.dom() == Set::<nat>::empty(),
    }
}

pub open spec fn step(g: Inp, s: Rs, c: Choice) -> Rs {
    match c {
        Choice::Dequeue(n) => step_dequeue(g, s, n),
        Choice::Process => step_process(g, s),
        Choice::Level => step_level(g, s),
    }
}

// ---------------------------------------------------------------- premises

// PU: no manifest names one module twice among its active requirements (else the second overwrites the first's record).
pub open spec fn premise_pu(g: Inp) -> bool {
    forall|n: nat, v: nat, i: int, j: int| #![trigger manifest(g, n, v)[i], manifest(g, n, v)[j]]
        0 <= i < manifest(g, n, v).len() && 0 <= j < manifest(g, n, v).len() && i != j
        && active(g.phases, manifest(g, n, v)[i]) && active(g.phases, manifest(g, n, v)[j])
            ==> manifest(g, n, v)[i].name != manifest(g, n, v)[j].name
}

// PR: no active requirement names the root (the cycle test catches it only on unscoped paths).
pub open spec fn premise_pr(g: Inp) -> bool {
    forall|n: nat, v: nat, i: int| #![trigger manifest(g, n, v)[i]]
        0 <= i < manifest(g, n, v).len() && active(g.phases, manifest(g, n, v)[i]) ==> manifest(g, n, v)[i].name != g.root
}

// ---------------------------------------------------------------- L1 machine and abstraction

pub struct L1 { pub st: St, pub sel: Map<nat, nat>, pub recs: Map<(nat, nat, nat), ISet<nat>>, pub why: Why }

pub open spec fn abs(s: Rs) -> L1 { L1 { st: s.st, sel: s.sel, recs: s.recs, why: s.why } }

// Module a needs module b by a recorded requirement (pDependencyGraph(b, "a version") is set for some version).
pub open spec fn edge(recs: Map<(nat, nat, nat), ISet<nat>>, a: nat, b: nat) -> bool {
    exists|v: nat| #![trigger recs.contains_key((b, a, v))] recs.contains_key((b, a, v))
}

// A chain of recorded requirements p[0] -> p[1] -> ... (each needs the next).
pub open spec fn chain(recs: Map<(nat, nat, nat), ISet<nat>>, p: Seq<nat>) -> bool {
    forall|k: int| #![trigger p[k]] 0 <= k < p.len() - 1 ==> edge(recs, p[k], p[k + 1])
}

// DEP-B11: a cycle: an active requirement of module n (at its version in the graph) names a module on a chain of
// recorded requirements that ends at n.
pub open spec fn cycle_reason(g: Inp, b: L1, n: nat, p: Seq<nat>) -> bool {
    &&& has(g, b.sel, n) && p.len() > 0 && p.last() == n && chain(b.recs, p)
    &&& exists|i: int| #![trigger manifest(g, n, vsel(g, b.sel, n))[i]]
            0 <= i < manifest(g, n, vsel(g, b.sel, n)).len() && active(g.phases, manifest(g, n, vsel(g, b.sel, n))[i])
            && p.contains(manifest(g, n, vsel(g, b.sel, n))[i].name)
}

// DEP-B6, B10, B21-B24: version v of q's module is admissible: allowed by q, by every requirement recorded on the
// module, by installed modules outside the graph, and not below the installed version unless PermitDowngrade.
pub open spec fn admissible(g: Inp, b: L1, q: Req, v: nat) -> bool {
    &&& q.range.contains(v)
    &&& forall|key: (nat, nat, nat)| #![trigger b.recs.contains_key(key)] b.recs.contains_key(key) && key.0 == q.name ==> b.recs[key].contains(v)
    &&& (g.ignore || forall|i: int| #![trigger g.ireqs[i]]
            0 <= i < g.ireqs.len() && g.ireqs[i].on == q.name && !g.known.contains(g.ireqs[i].by) && !b.sel.contains_key(g.ireqs[i].by)
                ==> g.ireqs[i].range.contains(v))
    &&& ((!g.ignore && g.inst.contains_key(q.name) && !g.permit) ==> v >= g.inst[q.name])
}

// No offered version is admissible, and the installed one is not kept (ignored, refreshed, another version already
// chosen, or not admissible).
pub open spec fn no_version(g: Inp, b: L1, q: Req) -> bool {
    &&& forall|v: nat| #![trigger versions(g, q.name).contains(v)] versions(g, q.name).contains(v) ==> !admissible(g, b, q, v)
    &&& !(!g.ignore && g.inst.contains_key(q.name) && !g.refresh.contains(q.name)
          && (!b.sel.contains_key(q.name) || b.sel[q.name] == g.inst[q.name]) && admissible(g, b, q, g.inst[q.name]))
}

// DEP-B7, B10: requirement i of module m (a module in the graph) has no admissible version.
pub open spec fn version_reason(g: Inp, b: L1, m: nat, mv: nat, i: nat) -> bool {
    &&& has(g, b.sel, m) && i < manifest(g, m, mv).len() && active(g.phases, manifest(g, m, mv)[i as int])
    &&& no_version(g, b, manifest(g, m, mv)[i as int])
}

pub open spec fn fail_justified(g: Inp, b: L1) -> bool {
    match b.why {
        Why::Cycle(n, p) => cycle_reason(g, b, n, p),
        Why::NoVersion(m, mv, i) => version_reason(g, b, m, mv, i),
        Why::Running => false,
    }
}

pub open spec fn l1_next(g: Inp, a: L1, b: L1) -> bool {
    ||| a == b
    ||| (a.st == St::Running && b.st == St::Running)
    ||| (a.st == St::Running && b.st == St::Failed && fail_justified(g, b))
    ||| (a.st == St::Running && b.st == St::Done && sound(g, b.sel))
}

// ---------------------------------------------------------------- invariant

pub open spec fn rec_is(s: Rs, q: Req, m: nat, v: nat) -> bool {
    s.recs.contains_key((q.name, m, v)) && s.recs[(q.name, m, v)] == q.range
}

pub open spec fn pend_has(p: Seq<Item>, i: nat) -> bool { exists|k: int| #![trigger p[k]] 0 <= k < p.len() && p[k].idx == i }

// Module m's active requirements are recorded, except those still pending.
pub open spec fn covered_but(g: Inp, s: Rs, m: nat, v: nat, p: Seq<Item>) -> bool {
    forall|i: int| #![trigger manifest(g, m, v)[i]]
        0 <= i < manifest(g, m, v).len() && active(g.phases, manifest(g, m, v)[i]) ==> rec_is(s, manifest(g, m, v)[i], m, v) || pend_has(p, i as nat)
}

pub open spec fn queued(s: Rs, m: nat) -> bool { s.cur.contains_key(m) || s.next.contains_key(m) }

pub open spec fn cov(g: Inp, s: Rs, m: nat) -> bool {
    ||| queued(s, m)
    ||| (m == s.par && cache_ver(g, s, m) == s.par_ver && covered_but(g, s, m, s.par_ver, s.pend))
    ||| covered_but(g, s, m, cache_ver(g, s, m), Seq::<Item>::empty())
}

pub open spec fn inv_recs(g: Inp, s: Rs) -> bool {
    forall|key: (nat, nat, nat)| #![trigger s.recs.contains_key(key)] s.recs.contains_key(key) ==> s.sel.contains_key(key.0) && s.recs[key].contains(s.sel[key.0])
}

pub open spec fn inv_inst(g: Inp, s: Rs) -> bool {
    forall|i: int| #![trigger g.ireqs[i]] !g.ignore && 0 <= i < g.ireqs.len() && s.sel.contains_key(g.ireqs[i].on) && !excluded(g, s, g.ireqs[i].by)
        ==> g.ireqs[i].range.contains(s.sel[g.ireqs[i].on])
}

pub open spec fn inv_sel(g: Inp, s: Rs) -> bool {
    forall|d: nat| #![trigger s.sel.contains_key(d)] s.sel.contains_key(d) ==> floor_ok(g, d, s.sel[d])
        && (versions(g, d).contains(s.sel[d]) || (g.inst.contains_key(d) && s.sel[d] == g.inst[d]))
}

pub open spec fn inv_cov(g: Inp, s: Rs) -> bool { forall|m: nat| #![trigger has(g, s.sel, m)] has(g, s.sel, m) ==> cov(g, s, m) }

pub open spec fn inv_par(g: Inp, s: Rs) -> bool { s.pend.len() > 0 ==> cache_ver(g, s, s.par) == s.par_ver || queued(s, s.par) }

pub open spec fn inv_pend(g: Inp, s: Rs) -> bool {
    forall|k: int| #![trigger s.pend[k]] 0 <= k < s.pend.len() ==>
        s.pend[k].idx < manifest(g, s.par, s.par_ver).len() && active(g.phases, manifest(g, s.par, s.par_ver)[s.pend[k].idx as int])
}

// A queue entry's module is in the graph and its path is a chain of recorded requirements ending at it.
pub open spec fn entry_ok(g: Inp, s: Rs, n: nat, e: Entry) -> bool {
    has(g, s.sel, n) && e.path.len() > 0 && e.path.last() == n && chain(s.recs, e.path)
}

pub open spec fn inv_queue(g: Inp, s: Rs) -> bool {
    &&& forall|n: nat| #![trigger s.cur.contains_key(n)] s.cur.contains_key(n) ==> entry_ok(g, s, n, s.cur[n])
    &&& forall|n: nat| #![trigger s.next.contains_key(n)] s.next.contains_key(n) ==> entry_ok(g, s, n, s.next[n])
    &&& (s.pend.len() > 0 ==> has(g, s.sel, s.par))
    &&& forall|k: int| #![trigger s.pend[k].dpath] 0 <= k < s.pend.len() ==>
            s.pend[k].dpath.len() == 0 || (s.pend[k].dpath.last() == s.par && chain(s.recs, s.pend[k].dpath))
}

pub open spec fn inv(g: Inp, s: Rs) -> bool {
    &&& !s.sel.contains_key(g.root)
    &&& inv_queue(g, s)   // queue paths are chains of recorded requirements (for the cycle reason)
    &&& inv_recs(g, s)    // recorded requirements are met by the chosen version
    &&& inv_inst(g, s)    // installed requirements from modules outside the graph are met
    &&& inv_sel(g, s)     // chosen versions exist and are not downgrades
    &&& inv_cov(g, s)     // every module in the graph is queued, being processed, or has its requirements recorded
    &&& inv_par(g, s)
    &&& inv_pend(g, s)
}

// ---------------------------------------------------------------- lemmas on the pick

proof fn dep_lemma_first_cand(g: Inp, s: Rs, q: Req, vs: Seq<nat>, i: nat)
    ensures first_cand(g, s, q, vs, i) matches Option::Some(v) ==> cand(g, s, q, v) && vs.contains(v)
    decreases vs.len() - i
{
    if i < vs.len() {
        dep_lemma_first_cand(g, s, q, vs, i + 1);
    }
}

proof fn dep_lemma_fuzzy_pick(g: Inp, s: Rs, q: Req, vs: Seq<nat>, i: nat, last: Option<nat>)
    requires last matches Option::Some(v) ==> cand(g, s, q, v) && vs.contains(v)
    ensures fuzzy_pick(g, s, q, vs, i, last) matches Option::Some(v) ==> cand(g, s, q, v) && vs.contains(v)
    decreases vs.len() - i
{
    if i < vs.len() {
        if cand(g, s, q, vs[i as int]) {
            dep_lemma_fuzzy_pick(g, s, q, vs, i + 1, Option::Some(vs[i as int]));
        } else {
            dep_lemma_fuzzy_pick(g, s, q, vs, i + 1, last);
        }
    }
}

// The chosen version meets every constraint the code applied (#L1092-L1154).
proof fn dep_lemma_pick(g: Inp, s: Rs, q: Req)
    ensures pick(g, s, q) matches Option::Some(v) ==> base(g, s, q, v) && floor_ok(g, q.name, v)
        && (versions(g, q.name).contains(v) || (g.inst.contains_key(q.name) && v == g.inst[q.name]))
{
    dep_lemma_first_cand(g, s, q, versions(g, q.name), 0);
    dep_lemma_fuzzy_pick(g, s, q, versions(g, q.name), 0, Option::None);
}

// Classification lists every active requirement and only active ones.
proof fn dep_lemma_items_from(ph: Set<nat>, reqs: Seq<Req>, path: Seq<nat>, exact: bool, i: nat)
    ensures
        forall|k: int| #![trigger items_from(ph, reqs, path, exact, i)[k]] 0 <= k < items_from(ph, reqs, path, exact, i).len() ==>
            i <= items_from(ph, reqs, path, exact, i)[k].idx < reqs.len() && active(ph, reqs[items_from(ph, reqs, path, exact, i)[k].idx as int]),
        forall|j: int| #![trigger reqs[j]] i <= j < reqs.len() && active(ph, reqs[j]) && reqs[j].exact == exact ==>
            pend_has(items_from(ph, reqs, path, exact, i), j as nat),
    decreases reqs.len() - i
{
    if i < reqs.len() {
        dep_lemma_items_from(ph, reqs, path, exact, i + 1);
        let rest = items_from(ph, reqs, path, exact, i + 1);
        let all = items_from(ph, reqs, path, exact, i);
        if active(ph, reqs[i as int]) && reqs[i as int].exact == exact {
            assert forall|k: int| #![trigger all[k]] 0 <= k < all.len() implies i <= all[k].idx < reqs.len() && active(ph, reqs[all[k].idx as int]) by {
                if k > 0 { assert(all[k] == rest[k - 1]); }
            }
            assert forall|j: int| #![trigger reqs[j]] i <= j < reqs.len() && active(ph, reqs[j]) && reqs[j].exact == exact implies pend_has(all, j as nat) by {
                if j == i { assert(all[0].idx == j); }
                else {
                    assert(pend_has(rest, j as nat));
                    let k = choose|k: int| 0 <= k < rest.len() && rest[k].idx == j as nat;
                    assert(all[k + 1] == rest[k]);
                }
            }
        } else {
            assert forall|j: int| #![trigger reqs[j]] i <= j < reqs.len() && active(ph, reqs[j]) && reqs[j].exact == exact implies pend_has(all, j as nat) by {
                if j > i { assert(pend_has(rest, j as nat)); }
            }
        }
    } else {
        assert(items_from(ph, reqs, path, exact, i) == Seq::<Item>::empty());
    }
}

proof fn dep_lemma_classify(ph: Set<nat>, reqs: Seq<Req>, path: Seq<nat>)
    ensures
        forall|k: int| #![trigger classify(ph, reqs, path)[k]] 0 <= k < classify(ph, reqs, path).len() ==>
            classify(ph, reqs, path)[k].idx < reqs.len() && active(ph, reqs[classify(ph, reqs, path)[k].idx as int]),
        forall|j: int| #![trigger reqs[j]] 0 <= j < reqs.len() && active(ph, reqs[j]) ==> pend_has(classify(ph, reqs, path), j as nat),
{
    let a = items_from(ph, reqs, path, true, 0);
    let b = items_from(ph, reqs, path, false, 0);
    dep_lemma_items_from(ph, reqs, path, true, 0);
    dep_lemma_items_from(ph, reqs, path, false, 0);
    let c = classify(ph, reqs, path);
    assert forall|k: int| #![trigger c[k]] 0 <= k < c.len() implies c[k].idx < reqs.len() && active(ph, reqs[c[k].idx as int]) by {
        if k < a.len() { assert(c[k] == a[k]); } else { assert(c[k] == b[k - a.len()]); }
    }
    assert forall|j: int| #![trigger reqs[j]] 0 <= j < reqs.len() && active(ph, reqs[j]) implies pend_has(c, j as nat) by {
        if reqs[j].exact {
            assert(pend_has(a, j as nat));
            let k = choose|k: int| 0 <= k < a.len() && a[k].idx == j as nat;
            assert(c[k] == a[k]);
        } else {
            assert(pend_has(b, j as nat));
            let k = choose|k: int| 0 <= k < b.len() && b[k].idx == j as nat;
            assert(c[k + a.len()] == b[k]);
        }
    }
}

// Classified items carry the module's queue path or an empty one (dpath, #L943-L947).
proof fn dep_lemma_items_path(ph: Set<nat>, reqs: Seq<Req>, path: Seq<nat>, exact: bool, i: nat)
    ensures forall|k: int| #![trigger items_from(ph, reqs, path, exact, i)[k]] 0 <= k < items_from(ph, reqs, path, exact, i).len() ==>
        items_from(ph, reqs, path, exact, i)[k].dpath.len() == 0 || items_from(ph, reqs, path, exact, i)[k].dpath == path
    decreases reqs.len() - i
{
    if i < reqs.len() {
        dep_lemma_items_path(ph, reqs, path, exact, i + 1);
        let rest = items_from(ph, reqs, path, exact, i + 1);
        let all = items_from(ph, reqs, path, exact, i);
        if active(ph, reqs[i as int]) && reqs[i as int].exact == exact {
            assert forall|k: int| #![trigger all[k]] 0 <= k < all.len() implies all[k].dpath.len() == 0 || all[k].dpath == path by {
                if k > 0 { assert(all[k] == rest[k - 1]); }
            }
        }
    }
}

proof fn dep_lemma_classify_path(ph: Set<nat>, reqs: Seq<Req>, path: Seq<nat>)
    ensures forall|k: int| #![trigger classify(ph, reqs, path)[k]] 0 <= k < classify(ph, reqs, path).len() ==>
        classify(ph, reqs, path)[k].dpath.len() == 0 || classify(ph, reqs, path)[k].dpath == path
{
    let a = items_from(ph, reqs, path, true, 0);
    let b = items_from(ph, reqs, path, false, 0);
    dep_lemma_items_path(ph, reqs, path, true, 0);
    dep_lemma_items_path(ph, reqs, path, false, 0);
    let c = classify(ph, reqs, path);
    assert forall|k: int| #![trigger c[k]] 0 <= k < c.len() implies c[k].dpath.len() == 0 || c[k].dpath == path by {
        if k < a.len() { assert(c[k] == a[k]); } else { assert(c[k] == b[k - a.len()]); }
    }
}

// Recording a requirement keeps every chain.
proof fn dep_lemma_chain_insert(recs: Map<(nat, nat, nat), ISet<nat>>, key: (nat, nat, nat), r: ISet<nat>, p: Seq<nat>)
    requires chain(recs, p)
    ensures chain(recs.insert(key, r), p)
{
    assert forall|k: int| #![trigger p[k]] 0 <= k < p.len() - 1 implies edge(recs.insert(key, r), p[k], p[k + 1]) by {
        assert(edge(recs, p[k], p[k + 1]));
        let v = choose|v: nat| #![trigger recs.contains_key((p[k + 1], p[k], v))] recs.contains_key((p[k + 1], p[k], v));
        assert(recs.insert(key, r).contains_key((p[k + 1], p[k], v)));
    }
}

proof fn dep_lemma_chain_push(recs: Map<(nat, nat, nat), ISet<nat>>, p: Seq<nat>, x: nat)
    requires chain(recs, p), p.len() > 0 ==> edge(recs, p.last(), x)
    ensures chain(recs, p.push(x))
{
    let q = p.push(x);
    assert forall|k: int| #![trigger q[k]] 0 <= k < q.len() - 1 implies edge(recs, q[k], q[k + 1]) by {
        if k < p.len() - 1 {
            assert(q[k] == p[k] && q[k + 1] == p[k + 1]);
            assert(edge(recs, p[k], p[k + 1]));
        } else {
            assert(q[k] == p.last() && q[k + 1] == x);
        }
    }
}

// The code's candidate test is the L1 admissibility test.
proof fn dep_lemma_cand_admissible(g: Inp, s: Rs, b: L1, q: Req, v: nat)
    requires b.recs == s.recs, b.sel == s.sel
    ensures cand(g, s, q, v) == admissible(g, b, q, v)
{}

proof fn dep_lemma_first_cand_none(g: Inp, s: Rs, q: Req, vs: Seq<nat>, i: nat)
    ensures first_cand(g, s, q, vs, i) is None ==> forall|k: int| #![trigger vs[k]] i <= k < vs.len() ==> !cand(g, s, q, vs[k])
    decreases vs.len() - i
{
    if i < vs.len() { dep_lemma_first_cand_none(g, s, q, vs, i + 1); }
}

proof fn dep_lemma_fuzzy_none(g: Inp, s: Rs, q: Req, vs: Seq<nat>, i: nat, last: Option<nat>)
    ensures fuzzy_pick(g, s, q, vs, i, last) is None ==>
        last is None && forall|k: int| #![trigger vs[k]] i <= k < vs.len() ==> !cand(g, s, q, vs[k])
    decreases vs.len() - i
{
    if i < vs.len() {
        if cand(g, s, q, vs[i as int]) {
            dep_lemma_fuzzy_none(g, s, q, vs, i + 1, Option::Some(vs[i as int]));
        } else {
            dep_lemma_fuzzy_none(g, s, q, vs, i + 1, last);
        }
    }
}

// When the pick finds nothing, no version is admissible (L1 no_version).
proof fn dep_lemma_pick_none(g: Inp, s: Rs, b: L1, q: Req)
    requires pick(g, s, q) is None, b.recs == s.recs, b.sel == s.sel
    ensures no_version(g, b, q)
{
    let vs = versions(g, q.name);
    dep_lemma_first_cand_none(g, s, q, vs, 0);
    dep_lemma_fuzzy_none(g, s, q, vs, 0, Option::None);
    assert forall|v: nat| #![trigger versions(g, q.name).contains(v)] versions(g, q.name).contains(v) implies !admissible(g, b, q, v) by {
        let k = choose|k: int| 0 <= k < vs.len() && vs[k] == v;
        assert(!cand(g, s, q, vs[k]));
        dep_lemma_cand_admissible(g, s, b, q, v);
    }
    if g.inst.contains_key(q.name) { dep_lemma_cand_admissible(g, s, b, q, g.inst[q.name]); }
}

// ---------------------------------------------------------------- per-step lemmas (each reveals only its step)

pub proof fn dep_inv_init(g: Inp)
    ensures inv(g, init(g))
{
    let s = init(g);
    assert forall|m: nat| #![trigger has(g, s.sel, m)] has(g, s.sel, m) implies cov(g, s, m) by {
        assert(m == g.root);
        assert(s.cur.contains_key(m));
    }
    assert forall|n: nat| #![trigger s.cur.contains_key(n)] s.cur.contains_key(n) implies entry_ok(g, s, n, s.cur[n]) by {
        assert(n == g.root);
        assert(s.cur[n].path == seq![g.root]);
    }
}

proof fn dep_step_dequeue(g: Inp, s: Rs, n: nat)
    requires inv(g, s), enabled(s, Choice::Dequeue(n))
    ensures inv(g, step_dequeue(g, s, n)), l1_next(g, abs(s), abs(step_dequeue(g, s, n)))
{
    reveal(step_dequeue);
    let t = step_dequeue(g, s, n);
    let e = s.cur[n];
    let path = if e.scope is Some && e.path.len() == 1 { Seq::<nat>::empty() } else { e.path };
    let v = cache_ver(g, s, n);
    let reqs = manifest(g, n, v);
    if !has_cycle(g.phases, reqs, path) {
        assert(t.recs == s.recs && t.sel == s.sel && t.next == s.next && t.cur == s.cur.remove(n));
        dep_lemma_classify(g.phases, reqs, path);
        assert forall|m: nat| #![trigger has(g, t.sel, m)] has(g, t.sel, m) implies cov(g, t, m) by {
            assert(cov(g, s, m));
            if m == n {
                if !queued(t, m) {
                    assert forall|i: int| #![trigger manifest(g, m, v)[i]]
                        0 <= i < manifest(g, m, v).len() && active(g.phases, manifest(g, m, v)[i]) implies
                        rec_is(t, manifest(g, m, v)[i], m, v) || pend_has(t.pend, i as nat) by {
                        assert(reqs[i] == manifest(g, m, v)[i]);
                    }
                }
            } else if s.cur.contains_key(m) {
                assert(t.cur.contains_key(m));
            } else if !s.next.contains_key(m) && m == s.par && cache_ver(g, s, m) == s.par_ver {
                // the previous module is finished: nothing pending
                assert forall|i: int| #![trigger manifest(g, m, s.par_ver)[i]]
                    0 <= i < manifest(g, m, s.par_ver).len() && active(g.phases, manifest(g, m, s.par_ver)[i]) implies
                    rec_is(t, manifest(g, m, cache_ver(g, t, m))[i], m, cache_ver(g, t, m)) || pend_has(Seq::<Item>::empty(), i as nat) by {
                    if !rec_is(s, manifest(g, m, s.par_ver)[i], m, s.par_ver) {
                        assert(pend_has(s.pend, i as nat));
                        let k = choose|k: int| 0 <= k < s.pend.len() && s.pend[k].idx == i as nat;
                        assert(false);
                    }
                }
                assert(covered_but(g, t, m, cache_ver(g, t, m), Seq::<Item>::empty()));
            }
        }
        assert forall|k: int| #![trigger t.pend[k]] 0 <= k < t.pend.len() implies
            t.pend[k].idx < manifest(g, t.par, t.par_ver).len() && active(g.phases, manifest(g, t.par, t.par_ver)[t.pend[k].idx as int]) by {
            assert(t.pend[k] == classify(g.phases, reqs, path)[k]);
        }
        dep_lemma_classify_path(g.phases, reqs, path);
        assert(entry_ok(g, s, n, e));
        assert forall|m: nat| #![trigger t.cur.contains_key(m)] t.cur.contains_key(m) implies entry_ok(g, t, m, t.cur[m]) by {
            assert(s.cur.contains_key(m));
        }
        assert forall|m: nat| #![trigger t.next.contains_key(m)] t.next.contains_key(m) implies entry_ok(g, t, m, t.next[m]) by {
            assert(s.next.contains_key(m));
        }
        assert forall|k: int| #![trigger t.pend[k].dpath] 0 <= k < t.pend.len() implies
            t.pend[k].dpath.len() == 0 || (t.pend[k].dpath.last() == t.par && chain(t.recs, t.pend[k].dpath)) by {
            assert(t.pend[k] == classify(g.phases, reqs, path)[k]);
        }
        assert(inv_queue(g, t));
        assert(inv_recs(g, t)); assert(inv_inst(g, t)); assert(inv_sel(g, t)); assert(inv_cov(g, t)); assert(inv_par(g, t)); assert(inv_pend(g, t));
    } else {
        // the cycle reason: the path is a chain of recorded requirements ending at n, and a requirement of n names a
        // module on it
        let i = choose|i: int| #![trigger reqs[i]] 0 <= i < reqs.len() && active(g.phases, reqs[i]) && dpath(reqs[i], path).contains(reqs[i].name);
        assert(entry_ok(g, s, n, e));
        assert(path == e.path) by {
            if path != e.path { assert(path.len() == 0); assert(!dpath(reqs[i], path).contains(reqs[i].name)); }
        }
        assert(path.contains(reqs[i].name));
        assert(v == vsel(g, t.sel, n));
        assert(manifest(g, n, vsel(g, t.sel, n))[i] == reqs[i]);
        assert(cycle_reason(g, abs(t), n, path));
        assert(inv_queue(g, t));
    }
}

proof fn dep_step_process(g: Inp, s: Rs)
    requires inv(g, s), enabled(s, Choice::Process), premise_pu(g), premise_pr(g)
    ensures inv(g, step_process(g, s)), l1_next(g, abs(s), abs(step_process(g, s)))
{
    reveal(step_process);
    let t = step_process(g, s);
    let it = s.pend[0];
    let q = manifest(g, s.par, s.par_ver)[it.idx as int];
    let d = q.name;
    dep_lemma_pick(g, s, q);
    assert(active(g.phases, q));
    assert(d != g.root);
    if let Option::Some(v) = pick(g, s, q) {
        let key = (d, s.par, s.par_ver);
        assert(t.recs == s.recs.insert(key, q.range) && t.sel == s.sel.insert(d, v) && t.cur == s.cur && t.par == s.par && t.par_ver == s.par_ver);
        // recorded requirements
        assert forall|k2: (nat, nat, nat)| #![trigger t.recs.contains_key(k2)] t.recs.contains_key(k2) implies t.sel.contains_key(k2.0) && t.recs[k2].contains(t.sel[k2.0]) by {
            if k2 != key {
                assert(s.recs.contains_key(k2));
                assert(s.recs[k2] == t.recs[k2]);
                if k2.0 == d { assert(s.recs[k2].contains(v)); }
            }
        }
        // installed requirements
        assert forall|i: int| #![trigger g.ireqs[i]] !g.ignore && 0 <= i < g.ireqs.len() && t.sel.contains_key(g.ireqs[i].on) && !excluded(g, t, g.ireqs[i].by)
            implies g.ireqs[i].range.contains(t.sel[g.ireqs[i].on]) by {
            assert(!excluded(g, s, g.ireqs[i].by));
        }
        // coverage
        assert forall|m: nat| #![trigger has(g, t.sel, m)] has(g, t.sel, m) implies cov(g, t, m) by {
            if m == d {
                assert(t.next.contains_key(m));
            } else {
                assert(has(g, s.sel, m));
                assert(cov(g, s, m));
                assert(cache_ver(g, t, m) == cache_ver(g, s, m));
                if queued(s, m) {
                    assert(queued(t, m));
                } else if m == s.par && cache_ver(g, s, m) == s.par_ver && covered_but(g, s, m, s.par_ver, s.pend) {
                    let mv = manifest(g, m, s.par_ver);
                    assert forall|i: int| #![trigger mv[i]] 0 <= i < mv.len() && active(g.phases, mv[i]) implies
                        rec_is(t, mv[i], m, s.par_ver) || pend_has(t.pend, i as nat) by {
                        if i == it.idx as int {
                            assert(rec_is(t, mv[i], m, s.par_ver));
                        } else if rec_is(s, mv[i], m, s.par_ver) {
                            assert(mv[i].name != d) by { assert(mv[i].name != mv[it.idx as int].name); }
                            assert(rec_is(t, mv[i], m, s.par_ver));
                        } else {
                            assert(pend_has(s.pend, i as nat));
                            let k = choose|k: int| 0 <= k < s.pend.len() && s.pend[k].idx == i as nat;
                            assert(k != 0);
                            assert(t.pend[k - 1] == s.pend[k]);
                        }
                    }
                } else {
                    let cv = cache_ver(g, s, m);
                    let mv = manifest(g, m, cv);
                    assert(covered_but(g, s, m, cv, Seq::<Item>::empty()));
                    assert forall|i: int| #![trigger mv[i]] 0 <= i < mv.len() && active(g.phases, mv[i]) implies
                        rec_is(t, mv[i], m, cv) || pend_has(Seq::<Item>::empty(), i as nat) by {
                        assert(rec_is(s, mv[i], m, cv) || pend_has(Seq::<Item>::empty(), i as nat));
                        if pend_has(Seq::<Item>::empty(), i as nat) {
                            let k = choose|k: int| 0 <= k < Seq::<Item>::empty().len() && Seq::<Item>::empty()[k].idx == i as nat;
                        }
                        if (mv[i].name, m, cv) == key {
                            assert(m == s.par);
                        }
                    }
                }
            }
        }
        assert forall|k: int| #![trigger t.pend[k]] 0 <= k < t.pend.len() implies
            t.pend[k].idx < manifest(g, t.par, t.par_ver).len() && active(g.phases, manifest(g, t.par, t.par_ver)[t.pend[k].idx as int]) by {
            assert(t.pend[k] == s.pend[k + 1]);
        }
        assert(t.pend.len() > 0 ==> cache_ver(g, t, t.par) == t.par_ver || queued(t, t.par)) by {
            if t.pend.len() > 0 && !queued(t, t.par) {
                assert(t.par != d);
                assert(!queued(s, s.par));
            }
        }
        // queue paths stay chains; the new entry extends its parent's path by the requirement just recorded
        assert forall|m: nat| #![trigger t.cur.contains_key(m)] t.cur.contains_key(m) implies entry_ok(g, t, m, t.cur[m]) by {
            assert(entry_ok(g, s, m, s.cur[m]));
            dep_lemma_chain_insert(s.recs, key, q.range, s.cur[m].path);
        }
        assert forall|m: nat| #![trigger t.next.contains_key(m)] t.next.contains_key(m) implies entry_ok(g, t, m, t.next[m]) by {
            if m == d {
                let p = it.dpath;
                assert(p.len() == 0 || (p.last() == s.par && chain(s.recs, p)));
                dep_lemma_chain_insert(s.recs, key, q.range, p);
                assert(t.recs.contains_key((d, s.par, s.par_ver)));
                if p.len() > 0 { assert(edge(t.recs, p.last(), d)); }
                dep_lemma_chain_push(t.recs, p, d);
                assert(t.next[m].path == p.push(d));
            } else {
                assert(s.next.contains_key(m));
                assert(entry_ok(g, s, m, s.next[m]));
                dep_lemma_chain_insert(s.recs, key, q.range, s.next[m].path);
            }
        }
        assert forall|k: int| #![trigger t.pend[k].dpath] 0 <= k < t.pend.len() implies
            t.pend[k].dpath.len() == 0 || (t.pend[k].dpath.last() == t.par && chain(t.recs, t.pend[k].dpath)) by {
            assert(t.pend[k] == s.pend[k + 1]);
            let p = s.pend[k + 1].dpath;
            if p.len() > 0 { dep_lemma_chain_insert(s.recs, key, q.range, p); }
        }
        assert(inv_queue(g, t));
        assert(inv_recs(g, t)); assert(inv_inst(g, t)); assert(inv_sel(g, t)); assert(inv_cov(g, t)); assert(inv_par(g, t)); assert(inv_pend(g, t));
    } else {
        // the version reason: requirement it.idx of the module being processed has no admissible version
        dep_lemma_pick_none(g, s, abs(t), q);
        assert(it.idx < manifest(g, s.par, s.par_ver).len());
        assert(version_reason(g, abs(t), s.par, s.par_ver, it.idx));
        assert(inv_queue(g, t));
    }
}

proof fn dep_step_level(g: Inp, s: Rs)
    requires inv(g, s), enabled(s, Choice::Level)
    ensures inv(g, step_level(g, s)), l1_next(g, abs(s), abs(step_level(g, s)))
{
    reveal(step_level);
    let t = step_level(g, s);
    assert(t.recs == s.recs && t.sel == s.sel && t.pend == s.pend && t.par == s.par && t.par_ver == s.par_ver);
    assert forall|m: nat| #![trigger has(g, s.sel, m)] has(g, s.sel, m) implies
        queued(t, m) || covered_but(g, s, m, cache_ver(g, s, m), Seq::<Item>::empty()) by {
        assert(cov(g, s, m));
        assert(!s.cur.contains_key(m)) by { if s.cur.contains_key(m) { assert(s.cur.dom().contains(m)); } }
        if s.next.contains_key(m) {
            assert(t.next.contains_key(m) || t.cur.contains_key(m) || t.st == St::Done);
        } else if m == s.par && cache_ver(g, s, m) == s.par_ver {
            let mv = manifest(g, m, s.par_ver);
            assert forall|i: int| #![trigger mv[i]] 0 <= i < mv.len() && active(g.phases, mv[i]) implies
                rec_is(s, mv[i], m, s.par_ver) || pend_has(Seq::<Item>::empty(), i as nat) by {
                if pend_has(s.pend, i as nat) {
                    let k = choose|k: int| 0 <= k < s.pend.len() && s.pend[k].idx == i as nat;
                }
            }
        }
    }
    if s.next.dom() == Set::<nat>::empty() {
        // Done: every module's active requirements are recorded, so met
        assert forall|m: nat, i: int| #![trigger manifest(g, m, vsel(g, s.sel, m))[i]]
            has(g, s.sel, m) && 0 <= i < manifest(g, m, vsel(g, s.sel, m)).len() && active(g.phases, manifest(g, m, vsel(g, s.sel, m))[i]) implies
            has(g, s.sel, manifest(g, m, vsel(g, s.sel, m))[i].name) && manifest(g, m, vsel(g, s.sel, m))[i].range.contains(vsel(g, s.sel, manifest(g, m, vsel(g, s.sel, m))[i].name)) by {
            let cv = cache_ver(g, s, m);
            assert(vsel(g, s.sel, m) == cv);
            assert(!s.next.contains_key(m)) by { if s.next.contains_key(m) { assert(s.next.dom().contains(m)); } }
            assert(covered_but(g, s, m, cv, Seq::<Item>::empty()));
            let q = manifest(g, m, cv)[i];
            if pend_has(Seq::<Item>::empty(), i as nat) {
                let k = choose|k: int| 0 <= k < Seq::<Item>::empty().len() && Seq::<Item>::empty()[k].idx == i as nat;
            }
            assert(rec_is(s, q, m, cv));
            assert(s.recs.contains_key((q.name, m, cv)));
        }
        assert(reqs_met(g, s.sel));
        assert(installed_met(g, s.sel)) by {
            assert forall|i: int| #![trigger g.ireqs[i]]
                0 <= i < g.ireqs.len() && s.sel.contains_key(g.ireqs[i].on) && !g.known.contains(g.ireqs[i].by) && !s.sel.contains_key(g.ireqs[i].by) && !g.ignore
                implies g.ireqs[i].range.contains(s.sel[g.ireqs[i].on]) by {
                assert(!excluded(g, s, g.ireqs[i].by));
            }
        }
    } else {
        assert forall|m: nat| #![trigger has(g, t.sel, m)] has(g, t.sel, m) implies cov(g, t, m) by {
            assert(queued(t, m) || covered_but(g, s, m, cache_ver(g, s, m), Seq::<Item>::empty()));
        }
        assert forall|m: nat| #![trigger t.cur.contains_key(m)] t.cur.contains_key(m) implies entry_ok(g, t, m, t.cur[m]) by {
            assert(s.next.contains_key(m));
        }
    }
    assert(inv_queue(g, t));
    assert(inv_recs(g, t)); assert(inv_inst(g, t)); assert(inv_sel(g, t)); assert(inv_cov(g, t)); assert(inv_par(g, t)); assert(inv_pend(g, t));
}

// ---------------------------------------------------------------- refinement (dispatcher)

// Soundness: every resolver step keeps the invariant, and a resolution that ends Done meets every active constraint.
pub proof fn dep_refines(g: Inp, s: Rs, c: Choice)
    requires inv(g, s), enabled(s, c), premise_pu(g), premise_pr(g)
    ensures inv(g, step(g, s, c)), l1_next(g, abs(s), abs(step(g, s, c)))
{
    match c {
        Choice::Dequeue(n) => dep_step_dequeue(g, s, n),
        Choice::Process => dep_step_process(g, s),
        Choice::Level => dep_step_level(g, s),
    }
}

// The resolver is deterministic: at most one choice is enabled in any state.
pub proof fn dep_deterministic(s: Rs, c1: Choice, c2: Choice)
    requires enabled(s, c1), enabled(s, c2)
    ensures c1 == c2
{
    if let Choice::Dequeue(a) = c1 {
        if let Choice::Dequeue(b) = c2 {
            assert(a <= b && b <= a);
        } else if c2 == Choice::Level {
            assert(s.cur.dom().contains(a));
        }
    } else if c1 == Choice::Level {
        if let Choice::Dequeue(b) = c2 { assert(s.cur.dom().contains(b)); }
    }
}

// ---------------------------------------------------------------- witnesses

// Range helpers: ^M.0.0 is [M*100, (M+1)*100); version 1.1.0 is 110.
pub open spec fn caret(m: nat) -> ISet<nat> { ISet::new(|v: nat| m * 100 <= v && v < (m + 1) * 100) }

pub open spec fn fz(n: nat, r: ISet<nat>) -> Req { Req { name: n, range: r, exact: false, scope: Option::None } }

// nc-dep-11: root(0) needs a(1) ^1 and b(2) ^1; a 1.1.0 needs c(3) ^2; a 1.0.0 and b 1.0.0 need c ^1.
pub open spec fn g11() -> Inp {
    Inp {
        root: 0, root_ver: 100,
        avail: map![1nat => seq![110nat, 100nat], 2nat => seq![100nat], 3nat => seq![200nat, 100nat]],
        man: map![(0nat, 100nat) => seq![fz(1, caret(1)), fz(2, caret(1))],
                  (1nat, 110nat) => seq![fz(3, caret(2))],
                  (1nat, 100nat) => seq![fz(3, caret(1))],
                  (2nat, 100nat) => seq![fz(3, caret(1))]],
        inst: Map::<nat, nat>::empty(), refresh: Set::<nat>::empty(), ireqs: Seq::<IReq>::empty(),
        known: set![0nat], phases: Set::<nat>::empty(), qphases: Set::<nat>::empty(), ignore: false, permit: false,
    }
}

pub open spec fn run11() -> Rs {
    let g = g11();
    let s1 = step(g, init(g), Choice::Dequeue(0));
    let s2 = step(g, s1, Choice::Process);
    let s3 = step(g, s2, Choice::Process);
    let s4 = step(g, s3, Choice::Level);
    let s5 = step(g, s4, Choice::Dequeue(1));
    let s6 = step(g, s5, Choice::Process);
    let s7 = step(g, s6, Choice::Dequeue(2));
    step(g, s7, Choice::Process)
}

// DEC-DEP-1: the greedy resolver rejects a solvable instance (IRIS: nc-dep-11-greedy-resolution fails with a clash on
// nc-dep-c naming nc-dep-a 1.1.0; nc-dep-11b-solvable-with-pin succeeds with a, b, c at 1.0.0).
pub proof fn dep_witness_greedy_rejects_solvable()
    ensures
        sound(g11(), map![1nat => 100nat, 2nat => 100nat, 3nat => 100nat]),
        solvable(g11()),
        run11().st == St::Failed,
        ({ let g = g11();
           let s1 = step(g, init(g), Choice::Dequeue(0));
           let s2 = step(g, s1, Choice::Process);
           let s3 = step(g, s2, Choice::Process);
           let s4 = step(g, s3, Choice::Level);
           let s5 = step(g, s4, Choice::Dequeue(1));
           let s6 = step(g, s5, Choice::Process);
           let s7 = step(g, s6, Choice::Dequeue(2));
           &&& enabled(init(g), Choice::Dequeue(0)) && enabled(s1, Choice::Process) && enabled(s2, Choice::Process)
           &&& enabled(s3, Choice::Level) && enabled(s4, Choice::Dequeue(1)) && enabled(s5, Choice::Process)
           &&& enabled(s6, Choice::Dequeue(2)) && enabled(s7, Choice::Process)
           &&& s3.sel[1] == 110 && s6.sel[3] == 200 }),
{
    reveal(step_dequeue); reveal(step_process); reveal(step_level);
    reveal_with_fuel(items_from, 3);
    reveal_with_fuel(first_cand, 3);
    reveal_with_fuel(fuzzy_pick, 3);
    let g = g11();
    let sol = map![1nat => 100nat, 2nat => 100nat, 3nat => 100nat];
    assert(caret(1).contains(100));
    assert(caret(1).contains(110));
    assert(caret(2).contains(200));
    assert(!caret(1).contains(200));
    assert(!caret(2).contains(100));
    assert(!caret(1).contains(200nat));
    // the solution
    assert(reqs_met(g, sol)) by {
        assert forall|m: nat, i: int| #![trigger manifest(g, m, vsel(g, sol, m))[i]]
            has(g, sol, m) && 0 <= i < manifest(g, m, vsel(g, sol, m)).len() && active(g.phases, manifest(g, m, vsel(g, sol, m))[i]) implies
            has(g, sol, manifest(g, m, vsel(g, sol, m))[i].name) && manifest(g, m, vsel(g, sol, m))[i].range.contains(vsel(g, sol, manifest(g, m, vsel(g, sol, m))[i].name)) by {
            assert(m == 0 || m == 1 || m == 2 || m == 3);
        }
    }
    assert(exists_ok(g, sol)) by {
        assert forall|d: nat| #![trigger sol.contains_key(d)] sol.contains_key(d) implies versions(g, d).contains(sol[d]) || (g.inst.contains_key(d) && sol[d] == g.inst[d]) by {
            if d == 1 { assert(versions(g, 1)[1] == 100); } else if d == 2 { assert(versions(g, 2)[0] == 100); } else { assert(versions(g, 3)[1] == 100); }
        }
    }
    assert(sound(g, sol));
    // the run
    let s0 = init(g);
    assert(is_min(s0.cur, 0)) by { assert forall|x: nat| #![trigger s0.cur.contains_key(x)] s0.cur.contains_key(x) implies 0 <= x by {} }
    let s1 = step(g, s0, Choice::Dequeue(0));
    assert(manifest(g, 0, 100) == seq![fz(1, caret(1)), fz(2, caret(1))]);
    assert(!has_cycle(g.phases, manifest(g, 0, 100), seq![0nat])) by {
        assert forall|i: int| #![trigger manifest(g, 0, 100)[i]] 0 <= i < manifest(g, 0, 100).len() implies
            !dpath(manifest(g, 0, 100)[i], seq![0nat]).contains(manifest(g, 0, 100)[i].name) by {
            assert(seq![0nat][0] == 0);
        }
    }
    assert(s1.pend.len() == 2 && s1.pend[0].idx == 0 && s1.pend[1].idx == 1);
    let s2 = step(g, s1, Choice::Process);
    assert(quick_ok(g, s1, 1, 110));
    assert(cand(g, s1, fz(1, caret(1)), 110));
    assert(s2.sel[1] == 110);
    let s3 = step(g, s2, Choice::Process);
    assert(rec_ok(s2, 2, 100)) by {
        assert forall|key: (nat, nat, nat)| #![trigger s2.recs.contains_key(key)] s2.recs.contains_key(key) && key.0 == 2 implies s2.recs[key].contains(100) by {
            assert(key == (1nat, 0nat, 100nat));
        }
    }
    assert(quick_ok(g, s2, 2, 100));
    assert(cand(g, s2, fz(2, caret(1)), 100));
    assert(s3.sel[2] == 100 && s3.pend.len() == 0);
    assert(s3.cur.dom() =~= Set::<nat>::empty());
    let s4 = step(g, s3, Choice::Level);
    assert(s3.next.dom().contains(1));
    assert(s4.cur.contains_key(1) && s4.cur.contains_key(2));
    assert(s4.cur.dom() =~= set![1nat, 2nat]);
    assert(is_min(s4.cur, 1)) by { assert forall|x: nat| #![trigger s4.cur.contains_key(x)] s4.cur.contains_key(x) implies 1 <= x by { assert(s4.cur.dom().contains(x)); } }
    let s5 = step(g, s4, Choice::Dequeue(1));
    assert(cache_ver(g, s4, 1) == 110);
    assert(s4.cur[1].path == seq![0nat, 1nat]);
    assert(!has_cycle(g.phases, manifest(g, 1, 110), seq![0nat, 1nat])) by {
        assert forall|i: int| #![trigger manifest(g, 1, 110)[i]] 0 <= i < manifest(g, 1, 110).len() implies
            !dpath(manifest(g, 1, 110)[i], seq![0nat, 1nat]).contains(manifest(g, 1, 110)[i].name) by {
            assert(i == 0);
            assert(!seq![0nat, 1nat].contains(3)) by { assert(seq![0nat, 1nat][0] == 0 && seq![0nat, 1nat][1] == 1); }
        }
    }
    assert(s5.pend.len() == 1 && s5.pend[0].idx == 0 && s5.par == 1 && s5.par_ver == 110);
    let s6 = step(g, s5, Choice::Process);
    assert(rec_ok(s5, 3, 200)) by {
        assert forall|key: (nat, nat, nat)| #![trigger s5.recs.contains_key(key)] s5.recs.contains_key(key) && key.0 == 3 implies s5.recs[key].contains(200) by {
            assert(key == (1nat, 0nat, 100nat) || key == (2nat, 0nat, 100nat));
        }
    }
    assert(quick_ok(g, s5, 3, 200));
    assert(cand(g, s5, fz(3, caret(2)), 200));
    assert(s6.sel[3] == 200);
    assert(s6.cur.dom() =~= set![2nat]);
    assert(is_min(s6.cur, 2)) by { assert forall|x: nat| #![trigger s6.cur.contains_key(x)] s6.cur.contains_key(x) implies 2 <= x by { assert(s6.cur.dom().contains(x)); } }
    let s7 = step(g, s6, Choice::Dequeue(2));
    assert(s6.cur[2].path == seq![0nat, 2nat]);
    assert(cache_ver(g, s6, 2) == 100);
    assert(!has_cycle(g.phases, manifest(g, 2, 100), seq![0nat, 2nat])) by {
        assert forall|i: int| #![trigger manifest(g, 2, 100)[i]] 0 <= i < manifest(g, 2, 100).len() implies
            !dpath(manifest(g, 2, 100)[i], seq![0nat, 2nat]).contains(manifest(g, 2, 100)[i].name) by {
            assert(i == 0);
            assert(!seq![0nat, 2nat].contains(3)) by { assert(seq![0nat, 2nat][0] == 0 && seq![0nat, 2nat][1] == 2); }
        }
    }
    assert(s7.pend.len() == 1 && s7.par == 2 && s7.par_ver == 100);
    let q = fz(3, caret(1));
    assert(manifest(g, 2, 100)[0] == q);
    // c is recorded at ^2 (by a 1.1.0); ^1 and ^2 share no version: no candidate
    assert(s7.recs.contains_key((3nat, 1nat, 110nat)) && s7.recs[(3nat, 1nat, 110nat)] == caret(2));
    assert forall|v: nat| !cand(g, s7, q, v) by {
        if q.range.contains(v) && rec_ok(s7, 3, v) {
            assert(s7.recs[(3nat, 1nat, 110nat)].contains(v));
        }
    }
    assert(!keep_installed(g, s7, q));
    assert(fuzzy_pick(g, s7, q, versions(g, 3), 0, Option::None) == Option::<nat>::None) by {
        assert(versions(g, 3) == seq![200nat, 100nat]);
        assert(!cand(g, s7, q, 200));
        assert(!cand(g, s7, q, 100));
    }
    assert(run11().st == St::Failed);
}

// F-DEP-1: with PermitDowngrade the resolver picks an older version of an installed module, and the load then refuses
// any downgrade (Utils/Module.cls#L1213-L1216). nc-dep-12: complex-a(1) 1.0.0 needs complex-c(2) ^2 and complex-d(3)
// ^1; complex-c 3.0.0 is installed (IRIS: nc-dep-12-permit-downgrade).
pub open spec fn g12(permit: bool) -> Inp {
    Inp {
        root: 1, root_ver: 100,
        avail: map![2nat => seq![300nat, 200nat], 3nat => seq![100nat], 4nat => seq![300nat, 200nat]],
        man: map![(1nat, 100nat) => seq![fz(2, caret(2)), fz(3, caret(1))],
                  (2nat, 200nat) => seq![fz(4, caret(3))], (2nat, 300nat) => seq![fz(4, caret(3))]],
        inst: map![2nat => 300nat, 4nat => 300nat], refresh: Set::<nat>::empty(),
        ireqs: seq![IReq { by: 2, on: 4, range: caret(3) }],
        known: set![1nat], phases: Set::<nat>::empty(), qphases: Set::<nat>::empty(), ignore: false, permit: permit,
    }
}

// Utils/Module.cls#L1213-L1216: a load whose target version is older than the installed one is refused.
pub open spec fn load_refuses(installed: nat, target: nat) -> bool { installed > target }

// L1 for PermitDowngrade (DEP-B25): a resolution made with the flag can be loaded.
pub open spec fn permit_honoured(g: Inp, sel: Map<nat, nat>) -> bool {
    g.permit ==> forall|d: nat| #![trigger sel.contains_key(d)] sel.contains_key(d) && g.inst.contains_key(d) ==> !load_refuses(g.inst[d], sel[d])
}

pub proof fn dep_witness_permit_downgrade_refused()
    ensures ({
        let g0 = g12(false);
        let g = g12(true);
        let a1 = step(g0, step(g0, init(g0), Choice::Dequeue(1)), Choice::Process);
        let s1 = step(g, init(g), Choice::Dequeue(1));
        let s2 = step(g, s1, Choice::Process);
        &&& a1.st == St::Failed
        &&& s2.st == St::Running && s2.sel[2] == 200 && g.inst[2] == 300
        &&& inv(g, s2)
        &&& !permit_honoured(g, s2.sel)
    })
{
    reveal(step_dequeue); reveal(step_process);
    reveal_with_fuel(items_from, 3);
    reveal_with_fuel(fuzzy_pick, 3);
    assert(caret(2).contains(200));
    assert(!caret(2).contains(300));
    let g0 = g12(false);
    let s0 = init(g0);
    assert(is_min(s0.cur, 1)) by { assert forall|x: nat| #![trigger s0.cur.contains_key(x)] s0.cur.contains_key(x) implies 1 <= x by {} }
    let b1 = step(g0, s0, Choice::Dequeue(1));
    assert(!has_cycle(g0.phases, manifest(g0, 1, 100), seq![1nat])) by {
        assert forall|i: int| #![trigger manifest(g0, 1, 100)[i]] 0 <= i < manifest(g0, 1, 100).len() implies
            !dpath(manifest(g0, 1, 100)[i], seq![1nat]).contains(manifest(g0, 1, 100)[i].name) by { assert(seq![1nat][0] == 1); }
    }
    assert(b1.pend[0].idx == 0 && b1.par == 1 && b1.par_ver == 100);
    let q = fz(2, caret(2));
    assert(!keep_installed(g0, b1, q));
    assert(!cand(g0, b1, q, 300));
    assert(!cand(g0, b1, q, 200));
    let g = g12(true);
    let t0 = init(g);
    dep_inv_init(g);
    assert(is_min(t0.cur, 1)) by { assert forall|x: nat| #![trigger t0.cur.contains_key(x)] t0.cur.contains_key(x) implies 1 <= x by {} }
    let s1 = step(g, t0, Choice::Dequeue(1));
    dep_refines(g, t0, Choice::Dequeue(1));
    assert(!has_cycle(g.phases, manifest(g, 1, 100), seq![1nat])) by {
        assert forall|i: int| #![trigger manifest(g, 1, 100)[i]] 0 <= i < manifest(g, 1, 100).len() implies
            !dpath(manifest(g, 1, 100)[i], seq![1nat]).contains(manifest(g, 1, 100)[i].name) by { assert(seq![1nat][0] == 1); }
    }
    assert(s1.pend[0].idx == 0 && s1.par == 1 && s1.par_ver == 100 && s1.pend.len() == 2);
    assert(premise_pu(g)) by {
        assert forall|n: nat, v: nat, i: int, j: int| #![trigger manifest(g, n, v)[i], manifest(g, n, v)[j]]
            0 <= i < manifest(g, n, v).len() && 0 <= j < manifest(g, n, v).len() && i != j
            && active(g.phases, manifest(g, n, v)[i]) && active(g.phases, manifest(g, n, v)[j])
            implies manifest(g, n, v)[i].name != manifest(g, n, v)[j].name by {
            if g.man.contains_key((n, v)) {
                assert((n, v) == (1nat, 100nat) || (n, v) == (2nat, 200nat) || (n, v) == (2nat, 300nat));
            }
        }
    }
    assert(premise_pr(g)) by {
        assert forall|n: nat, v: nat, i: int| #![trigger manifest(g, n, v)[i]]
            0 <= i < manifest(g, n, v).len() && active(g.phases, manifest(g, n, v)[i]) implies manifest(g, n, v)[i].name != g.root by {
            if g.man.contains_key((n, v)) {
                assert((n, v) == (1nat, 100nat) || (n, v) == (2nat, 200nat) || (n, v) == (2nat, 300nat));
            }
        }
    }
    let s2 = step(g, s1, Choice::Process);
    dep_refines(g, s1, Choice::Process);
    assert(!keep_installed(g, s1, q));
    assert(!cand(g, s1, q, 300));
    assert(cand(g, s1, q, 200));
    assert(quick_ok(g, s1, 2, 200));
    assert(s2.sel[2] == 200);
    assert(load_refuses(g.inst[2], s2.sel[2]));
}

// F-DEP-2: the lock file lists dependencies in name order (an array keyed by name, LockFile.cls#L25, #L69).
// L1 (LockFile.cls#L24, DEP-B39): each listed module comes after the modules it needs.
pub open spec fn name_order(o: Seq<nat>, names: Set<nat>) -> bool {
    (forall|a: int, b: int| #![trigger o[a], o[b]] 0 <= a < b < o.len() ==> o[a] < o[b])
    && (forall|x: nat| #![trigger names.contains(x)] names.contains(x) <==> o.contains(x))
}

pub open spec fn needs_first(o: Seq<nat>, needs: Set<(nat, nat)>) -> bool {
    forall|a: int, b: int| #![trigger o[a], o[b]] 0 <= a < o.len() && 0 <= b < o.len() && needs.contains((o[a], o[b])) ==> b < a
}

// nc-dep-13: nc-dep-aa-mid(1) needs nc-dep-zz-base(2) (IRIS: nc-dep-13-lockfile-order).
pub proof fn dep_witness_lockfile_name_order()
    ensures forall|o: Seq<nat>| name_order(o, set![1nat, 2nat]) ==> !needs_first(o, set![(1nat, 2nat)])
{
    assert forall|o: Seq<nat>| name_order(o, set![1nat, 2nat]) implies !needs_first(o, set![(1nat, 2nat)]) by {
        assert(set![1nat, 2nat].contains(1));
        assert(set![1nat, 2nat].contains(2));
        assert(o.contains(1nat) && o.contains(2nat));
        let a = choose|a: int| 0 <= a < o.len() && o[a] == 1nat;
        let b = choose|b: int| 0 <= b < o.len() && o[b] == 2nat;
        if b < a { assert(o[b] < o[a]); }
        assert(a != b);
        assert(set![(1nat, 2nat)].contains((o[a], o[b])));
    }
}

// F-DEP-3: list-dependents looks the typed name up among lower-case graph keys (Utils/Module.cls#L482).
// Names are sequences of character codes.
pub open spec fn lower(c: nat) -> nat { if 65 <= c && c <= 90 { c + 32 } else { c } }

pub open spec fn lower_all(s: Seq<nat>) -> Seq<nat> { Seq::new(s.len(), |i: int| lower(s[i])) }

pub open spec fn listed(keys: Set<Seq<nat>>, typed: Seq<nat>) -> bool { keys.contains(typed) }

// L1 (DEP-B49): the name may be typed in either case.
pub open spec fn case_blind(keys: Set<Seq<nat>>, typed: Seq<nat>) -> bool { keys.contains(lower_all(typed)) ==> listed(keys, typed) }

// "Up" against the key "up" (IRIS: nc-dep-16-list-dependents-case, Update-Simple against update-simple).
pub proof fn dep_witness_list_dependents_case()
    ensures !case_blind(set![seq![117nat, 112nat]], seq![85nat, 112nat])
{
    let typed = seq![85nat, 112nat];
    assert(lower_all(typed) =~= seq![117nat, 112nat]);
    assert(typed != seq![117nat, 112nat]) by { assert(typed[0] != seq![117nat, 112nat][0]); }
}

fn main() {}

} // verus!
