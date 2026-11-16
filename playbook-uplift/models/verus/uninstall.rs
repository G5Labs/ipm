// L1/L2 models of IPM uninstall and the Clean phase (ipm 571e04a1), with refinement proofs and defect witnesses.
//
// L2 transcribes the algorithm, with line anchors, over an unbounded set of modules and resources:
//   Main.cls Uninstall #L2701-L2751 (the "zpm" name guard, CheckModuleNamespace #L2534-L2550, the full-cleanup
//   question, the shell-exit flag); Storage/Module.cls Uninstall #L727-L757 and ExecutePhases #L446-L720
//   (lookup by Name then ExternalName #L456-L463; every resource processor's OnBeforePhase("Clean") #L587-L600,
//   where WebApplication.cls#L98-L101 deletes web apps; the self-uninstall log #L626-L638);
//   Lifecycle/Module.cls %Clean #L8-L90 (check, Unconfigure hooks, ##super, the IPM-only block);
//   Lifecycle/Base.cls CheckBeforeClean #L258-L302 and %Clean #L305-L507 (resources at Clean.Level>0
//   #L327-L417 with FileCopy.cls#L218-L242 KeepOnUninstall and Default/Global.cls#L76-L80; recursion over
//   Dependencies #L431-L490 with the Cycle list; record and stored manifest delete #L492-L500).
// Loops are recursive spec functions over Seq prefixes; the dependency recursion is mutual recursion with
// fuel (exec_clean / dep_loop / dep_step). Fuel only bounds the nesting depth; running out is an error outcome.
// The dependency graph that BuildAllDependencyGraphs returns is data (Module.graph), next to the true set of
// needs (Module.needs); dependency resolution belongs to modules 03-04.
//
// L1 is the intended contract, one predicate per clause, from the UNIN statements:
//   l1_blocked_unchanged (UNIN-B28, B31), l1_honest (B30, B80: success exactly when the module is gone, both
//   directions), l1_rows_honest (B81: a log row says success exactly when its module is gone, both directions),
//   l1_dependents_protect (B28, B33), l1_only_own (B5, B6, B21), l1_kept_files (B19), l1_complete (B5, B11, B12, B18),
//   l1_self_guard (B51-B62), l1_ipm_kept (IPM is removed only when named), l1_clean_keeps_target (B66),
//   l1_not_found (B3, B68).
// Proved: each clause holds for every uninstall (and clean) under the named premises below.
// Uninstall runs no transaction and the model has no crash step: a crash during Clean is not modelled
// (uninstall.TRUST.md U12).
// Named premises (each dropped once by a control):
//   PW  a blocked target owns no web app                    (F-UNIN-1)
//   PB  the target is not blocked                            (F-UNIN-6, DEC-UNIN-1)
//   PG  every installed module's dependency graph builds     (F-UNIN-4)
//   PGA every global resource of the target names its global (F-UNIN-5)
//   PF  no two file copies share a target folder             (UNIN-B21)
//   PN  IPM is named exactly "zpm"                           (F-UNIN-3)
//   PI  with -r, IPM is not among the target's needs          (model-derived; see unin_witness_recurse_removes_ipm)
//   PCW clean runs on a module with no web app and no installer class (F-UNIN-1, clean part)
//   PNF no full cleanup (UninstallAll is not modelled)
//   PR  no -r (nested Clean writes success rows for skipped needs and deletes needs' records without their manifests)
// PREMISE premise_pw = PW
// PREMISE premise_pb = PB
// PREMISE premise_pg = PG
// PREMISE premise_pga = PGA
// PREMISE premise_pf = PF
// PREMISE premise_pn = PN
// PREMISE premise_pi = PI
// PREMISE premise_pcw = PCW
// PREMISE premise_pnf = PNF
// PREMISE premise_pr = PR
use vstd::prelude::*;

verus! {

// ================================================================ data

pub enum RKind {
    Code { mapped: bool },                              // class, routine, include (IsMapped: borrowed from a shared database)
    Web,                                                // WebApplication or CSPApplication
    Files { folder: nat, file: bool, keep: bool, protected: bool }, // FileCopy: target folder (or the folder of a single-file
                                                        // target), KeepOnUninstall, protected directory
    Gbl { named: bool, preserve: bool },                // Default/Global: Global attribute set, Preserve
}

pub struct Res { pub kind: RKind, pub owner: nat }

pub struct Module {
    pub key: nat,                 // Name, upper-cased (SQLUPPER index: nc-unin-env-name-case)
    pub ext_key: nat,             // ExternalName, upper-cased
    pub global_scope: bool,
    pub installer: bool,          // has an InstallerClass
    pub hook_ok: bool,            // its OnUnconfigureComponent / OnUnInstallComponent succeed
    pub deps: Seq<nat>,           // Dependencies, manifest order
    pub res: Seq<nat>,            // GetOrderedResourceList
    pub needs: Set<nat>,          // every module it needs, directly or not (ground truth)
    pub graph: Option<Set<nat>>,  // BuildDependencyGraph result; None = error, module dropped (Utils/Module.cls#L615-L622)
}

pub struct HRow { pub m: Option<nat>, pub ok: bool }

pub struct W {
    pub mods: Map<nat, Module>,   // ^IPM.Storage.ModuleD
    pub streams: Set<nat>,        // stored manifests ^IPM.StudioDoc.ModuleStreamD
    pub res: Map<nat, Res>,       // resource declarations (static)
    pub present: Set<nat>,        // artifacts present: code compiled, web app defined, files on disk, global data
    pub hooks: Seq<nat>,          // installer Unconfigure hooks run, in order
    pub hist: Seq<HRow>,          // uninstall history rows (fields belong to module 02)
    pub exit_flag: bool,          // ^||%IPM.SelfUninstallFlag
    pub prompted: bool,           // the full-cleanup question was asked
    pub mapped_from: bool,        // this namespace borrows IPM from another one
}

pub struct P { pub level: nat, pub force: bool, pub recurse: bool, pub global: bool, pub nested: bool, pub full: bool, pub cycle: Set<nat> }

pub struct Out { pub w: W, pub ok: bool, pub skip: bool }

pub open spec fn ipm_id() -> nat { 0 }
pub open spec fn k_zpm() -> nat { 0 }
pub open spec fn k_ipm() -> nat { 1 }
pub open spec fn sd() -> nat { 0 }   // class %IPM.StudioDocument.Module, one of IPM's own code resources

pub struct Typed { pub key: nat, pub exact_zpm: bool }   // the module name as typed: its upper-case key, and whether it is exactly "zpm"

// ================================================================ well-formedness (not a premise: what the stores look like)

pub open spec fn owns(w: W, m: nat, r: nat) -> bool { w.res.contains_key(r) && w.res[r].owner == m }

pub open spec fn wf(w: W) -> bool {
    &&& forall|m: nat, i: int| #![trigger w.mods[m].res[i]]
            w.mods.contains_key(m) && 0 <= i < w.mods[m].res.len() ==> owns(w, m, w.mods[m].res[i])
    &&& forall|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) ==> !w.mods[m].needs.contains(m)
    &&& forall|m: nat, i: int| #![trigger w.mods[m].deps[i]]
            w.mods.contains_key(m) && 0 <= i < w.mods[m].deps.len() ==>
            w.mods[m].needs.contains(w.mods[m].deps[i]) &&
            (w.mods.contains_key(w.mods[m].deps[i]) ==> w.mods[w.mods[m].deps[i]].needs.subset_of(w.mods[m].needs))
    &&& forall|a: nat, b: nat| #![trigger w.mods[a], w.mods[b]]
            w.mods.contains_key(a) && w.mods.contains_key(b) && a != b ==>
            w.mods[a].key != w.mods[b].key && w.mods[a].ext_key != w.mods[b].ext_key
    &&& w.res.contains_key(sd()) && w.res[sd()].owner == ipm_id() && w.res[sd()].kind is Code
    &&& forall|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) && m != ipm_id() ==>
            w.mods[m].key != k_zpm() && w.mods[m].key != k_ipm() && w.mods[m].ext_key != k_zpm() && w.mods[m].ext_key != k_ipm()
    &&& w.mods.contains_key(ipm_id()) ==> w.mods[ipm_id()].key == k_zpm() && w.mods[ipm_id()].ext_key == k_ipm()
            && w.mods[ipm_id()].res.contains(sd()) && w.res[sd()].kind == (RKind::Code { mapped: false })
            && w.mods[ipm_id()].deps.len() == 0                 // IPM's manifest declares no <Dependencies> (module.xml)
}

// A command can run: IPM's classes are compiled in this namespace; a typed "zpm" has key ZPM.
pub open spec fn wf_cmd(w: W, n: Typed) -> bool { wf(w) && w.present.contains(sd()) && (n.exact_zpm ==> n.key == k_zpm()) }

// ================================================================ L2: the algorithm

// %IPM.Storage.Module NameOpen, then ExternalNameOpen (Storage/Module.cls#L456-L463; both indexes case-blind).
pub open spec fn lookup(w: W, k: nat) -> Option<nat> { lookup_m(w.mods, k) }

pub open spec fn lookup_m(mods: Map<nat, Module>, k: nat) -> Option<nat> {
    if exists|m: nat| #![trigger mods[m]] mods.contains_key(m) && mods[m].key == k {
        Some(choose|m: nat| #![trigger mods[m]] mods.contains_key(m) && mods[m].key == k)
    } else if exists|m: nat| #![trigger mods[m]] mods.contains_key(m) && mods[m].ext_key == k {
        Some(choose|m: nat| #![trigger mods[m]] mods.contains_key(m) && mods[m].ext_key == k)
    } else { None }
}

pub open spec fn is_web(w: W, r: nat) -> bool { w.res.contains_key(r) && w.res[r].kind is Web }

// ExecutePhases #L587-L600: OnBeforePhase("Clean") of every resource, in order; WebApplication.cls#L98-L101 deletes.
pub open spec fn before_clean(w: W, rs: Seq<nat>) -> W
    decreases rs.len()
{
    if rs.len() == 0 { w } else {
        let w1 = before_clean(w, rs.drop_last());
        if is_web(w1, rs.last()) { W { present: w1.present.remove(rs.last()), ..w1 } } else { w1 }
    }
}

// GetDependentsList (Utils/Module.cls#L460-L499): modules whose graph built and contains t; Cycle excluded (Base.cls#L284).
pub open spec fn code_dependent(w: W, t: nat, cycle: Set<nat>, m: nat) -> bool {
    w.mods.contains_key(m) && w.mods[m].graph is Some && w.mods[m].graph->0.contains(t) && !cycle.contains(m)
}

// CheckBeforeClean (Base.cls#L258-L302).
pub open spec fn check_skip(w: W, gs: bool, t: nat, p: P) -> bool {
    (gs && !p.global) || (!p.force && p.level > 0 && exists|m: nat| code_dependent(w, t, p.cycle, m))
}

pub open spec fn in_folder(k: RKind, f: nat) -> bool { match k { RKind::Files { folder, .. } => folder == f, _ => false } }
pub open spec fn folder_set(w: W, f: nat) -> Set<nat> { w.res.dom().filter(|r: nat| in_folder(w.res[r].kind, f)) }
pub open spec fn is_mapped(w: W, r: nat) -> bool { w.res.contains_key(r) && w.res[r].kind == (RKind::Code { mapped: true }) }

pub struct Acc { pub w: W, pub del: Set<nat> }

// One resource of %Clean's loop (Base.cls#L327-L357).
pub open spec fn clean_step(a: Acc, r: nat) -> Acc {
    if !a.w.res.contains_key(r) { a } else {
        match a.w.res[r].kind {
            // FileCopy.cls#L218-L242: handled; keep flag: nothing; a single file: %File.Delete of that file; a protected
            // folder: nothing; else RemoveDirectoryTree: every file in the target folder goes, whoever copied it.
            RKind::Files { folder, file, keep, protected } =>
                if keep { a }
                else if file { Acc { w: W { present: a.w.present.remove(r), ..a.w }, ..a } }
                else if protected { a }
                else { Acc { w: W { present: a.w.present.difference(folder_set(a.w, folder)), ..a.w }, ..a } },
            // Default/Global.cls#L76-L80: handled; killed only when the Global attribute is set and Preserve is off.
            RKind::Gbl { named, preserve } =>
                if named && !preserve { Acc { w: W { present: a.w.present.remove(r), ..a.w }, ..a } } else { a },
            // Not handled: ResolveChildren adds the item to tDeleteArray (#L344-L356).
            RKind::Code { .. } => Acc { del: a.del.insert(r), ..a },
            // Web apps were deleted in OnBeforePhase; their children start with "/" and are skipped (#L347-L349).
            RKind::Web => a,
        }
    }
}

pub open spec fn clean_res(w: W, rs: Seq<nat>) -> Acc
    decreases rs.len()
{
    if rs.len() == 0 { Acc { w: w, del: Set::empty() } } else { clean_step(clean_res(w, rs.drop_last()), rs.last()) }
}

// #L359-L416: delete every collected item, skipping mapped ones unless Clean.GlobalScope.
pub open spec fn code_gone(w: W, del: Set<nat>, rm_mapped: bool) -> Set<nat> {
    del.filter(|r: nat| rm_mapped || !is_mapped(w, r))
}

pub open spec fn clean_code(w: W, rs: Seq<nat>, rm_mapped: bool) -> W {
    let a = clean_res(w, rs);
    W { present: a.w.present.difference(code_gone(a.w, a.del, rm_mapped)), ..a.w }
}

// %IPM.StudioDocument.Module:Delete (StudioDocument/Module.cls#L205-L227): stream and record in one transaction.
pub open spec fn record_delete(o: Out, t: nat) -> Out {
    Out { w: W { mods: o.w.mods.remove(t), streams: o.w.streams.remove(t), ..o.w }, ..o }
}

// Lifecycle/Module.cls#L27-L56: the IPM-only block; a full cleanup kills every module record and stored manifest.
pub open spec fn ipm_block(o: Out, t: nat, p: P) -> Out {
    if t == ipm_id() && p.full { Out { w: W { mods: Map::empty(), streams: Set::empty(), ..o.w }, ..o } } else { o }
}

// ExecutePhases(t, "Clean") followed by Lifecycle/Module %Clean and Base %Clean.
pub open spec fn exec_clean(w: W, t: nat, p: P, fuel: nat) -> Out
    decreases fuel, 1nat
{
    if !w.mods.contains_key(t) { Out { w: w, ok: false, skip: false } }                         // "Module '%1' not found."
    else {
        let m = w.mods[t];
        let w1 = before_clean(w, m.res);                                                         // #L587-L600
        if check_skip(w1, m.global_scope, t, p) { Out { w: w1, ok: true, skip: true } }          // Module.cls#L12-L15
        else {
            let w2 = if m.installer { W { hooks: w1.hooks.push(t), ..w1 } } else { w1 };          // Unconfigure, Module.cls#L17
            if m.installer && !m.hook_ok { Out { w: w2, ok: false, skip: false } }
            else {
                let p1 = P { cycle: p.cycle.insert(t), ..p };                                    // Base.cls#L309
                if check_skip(w2, m.global_scope, t, p1) { Out { w: w2, ok: true, skip: true } } // Base.cls#L322-L326
                else {
                    let w3 = after_code(w2, m, p);                                               // #L327-L417
                    let o = after_deps(w3, m, p1, fuel);                                         // #L431-L490
                    if !o.ok { o }
                    else {
                        let o2 = if p.level > 0 && !p.nested && o.w.present.contains(sd()) { record_delete(o, t) } else { o }; // #L492-L500
                        ipm_block(o2, t, p)
                    }
                }
            }
        }
    }
}

pub open spec fn after_code(w2: W, m: Module, p: P) -> W { if p.level > 0 { clean_code(w2, m.res, p.global) } else { w2 } }

pub open spec fn after_deps(w3: W, m: Module, p1: P, fuel: nat) -> Out
    decreases fuel, 0nat
{
    if !p1.recurse { Out { w: w3, ok: true, skip: false } }
    else if fuel == 0 { Out { w: w3, ok: false, skip: false } }
    else { dep_loop(w3, m.deps, p1, (fuel - 1) as nat) }
}

// The loop over Dependencies (#L432-L487); a failed dependency stops it (#L470, #L489).
pub open spec fn dep_loop(w: W, ds: Seq<nat>, p: P, fuel: nat) -> Out
    decreases fuel, ds.len() + 3
{
    if ds.len() == 0 { Out { w: w, ok: true, skip: false } }
    else {
        let o = dep_loop(w, ds.drop_last(), p, fuel);
        if !o.ok { o } else { dep_step(o.w, ds.last(), p, fuel) }
    }
}

pub open spec fn dep_step(w: W, d: nat, p: P, fuel: nat) -> Out
    decreases fuel, 2nat
{
    if p.cycle.contains(d) || !w.mods.contains_key(d) { Out { w: w, ok: true, skip: false } }    // #L434-L438
    else {
        // Nested Clean at Level+1 with Nested=1; Force is kept (zkill:'tRecurse never fires inside if tRecurse).
        let o = exec_clean(w, d, P { level: p.level + 1, nested: true, ..p }, fuel);
        if !o.ok { Out { w: W { hist: o.w.hist.push(HRow { m: Some(d), ok: false }), ..o.w }, ok: false, skip: false } }
        // The nested Clean deleted IPM's own classes (d is zpm): ##class(%IPM.Storage.Module) no longer exists, the
        // next call throws <CLASS DOES NOT EXIST> and %Clean's catch returns it (IRIS: nc-repo-model-unin-recurse-ipm-dependency).
        else if !o.w.present.contains(sd()) { Out { w: o.w, ok: false, skip: false } }
        else {
            let w4 = if !o.skip { W { mods: o.w.mods.remove(d), ..o.w } } else { o.w };             // NameDelete: record only
            Out { w: W { hist: w4.hist.push(HRow { m: Some(d), ok: true }), ..w4 }, ok: true, skip: false }
        }
    }
}

// Storage/Module.cls Uninstall #L727-L757 (Clean.Level=1); for IPM the log is finalized before %Clean (#L626-L638).
pub open spec fn storage_uninstall(w: W, k: nat, force: bool, recurse: bool, global: bool, full: bool, fuel: nat) -> Out {
    match lookup(w, k) {
        None => Out { w: W { hist: w.hist.push(HRow { m: None, ok: false }), ..w }, ok: false, skip: false },
        Some(t) => {
            let p = P { level: 1, force: force, recurse: recurse, global: global, nested: false, full: full, cycle: Set::empty() };
            if t == ipm_id() {
                exec_clean(W { hist: w.hist.push(HRow { m: Some(t), ok: true }), ..w }, t, p, fuel)
            } else {
                let o = exec_clean(w, t, p, fuel);
                Out { w: W { hist: o.w.hist.push(HRow { m: Some(t), ok: o.ok }), ..o.w }, ..o }
            }
        }
    }
}

// Main.cls Uninstall #L2701-L2751: the self-uninstall steps run only when the typed name is exactly "zpm" (#L2712).
pub open spec fn main_uninstall(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat) -> Out {
    if n.exact_zpm {
        if w.mapped_from { Out { w: w, ok: false, skip: false } }                                // CheckModuleNamespace #L2534-L2539
        else {
            let w1 = if !force { W { prompted: true, ..w } } else { w };                         // #L2733-L2737
            let full = !force && answer;
            storage_uninstall(W { exit_flag: true, ..w1 }, n.key, force, recurse, global, full, fuel) // #L2746
        }
    } else { storage_uninstall(w, n.key, force, recurse, global, false, fuel) }
}

// `<module> clean` (Main.cls RunOnePhase #L2753-L2766): Clean.Level 0, Clean.Recurse 1 by default, no log.
pub open spec fn clean_cmd(w: W, k: nat, force: bool, global: bool, fuel: nat) -> Out {
    match lookup(w, k) {
        None => Out { w: w, ok: false, skip: false },
        Some(t) => exec_clean(w, t, P { level: 0, force: force, recurse: true, global: global, nested: false, full: false, cycle: Set::empty() }, fuel),
    }
}

// ================================================================ premises

pub open spec fn has_web(w: W, t: nat) -> bool { exists|i: int| #![trigger w.mods[t].res[i]] 0 <= i < w.mods[t].res.len() && is_web(w, w.mods[t].res[i]) }

pub open spec fn intent_dependent(w: W, t: nat) -> bool { exists|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) && w.mods[m].needs.contains(t) }

pub open spec fn intent_blocked(w: W, t: nat, force: bool, global: bool) -> bool {
    (w.mods[t].global_scope && !global) || (!force && intent_dependent(w, t))
}

pub open spec fn premise_pw(w: W, t: nat) -> bool { !has_web(w, t) }
pub open spec fn premise_pb(w: W, t: nat, force: bool, global: bool) -> bool { !intent_blocked(w, t, force, global) }
pub open spec fn premise_pg(w: W) -> bool {
    forall|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) ==> w.mods[m].graph == Some(w.mods[m].needs)
}
pub open spec fn premise_pga(w: W, t: nat) -> bool {
    forall|i: int| #![trigger w.mods[t].res[i]] 0 <= i < w.mods[t].res.len() && w.res.contains_key(w.mods[t].res[i]) ==>
        !(w.res[w.mods[t].res[i]].kind matches RKind::Gbl { named: false, .. })
}
// PF: a folder copy that uninstall deletes holds no other file copy.
pub open spec fn premise_pf(w: W) -> bool {
    forall|r1: nat, r2: nat, f: nat| #![trigger cuts(w.res, r1, f), in_folder(w.res[r2].kind, f)]
        cuts(w.res, r1, f) && w.res.contains_key(r2) && in_folder(w.res[r2].kind, f) ==> r1 == r2
}
pub open spec fn premise_pn(w: W, n: Typed) -> bool { lookup(w, n.key) == Some(ipm_id()) ==> n.exact_zpm }
pub open spec fn premise_pi(w: W, t: nat, recurse: bool) -> bool { recurse ==> !w.mods[t].needs.contains(ipm_id()) }
pub open spec fn premise_pcw(w: W, t: nat) -> bool { !has_web(w, t) && !w.mods[t].installer }
pub open spec fn premise_pnf(n: Typed, force: bool, answer: bool) -> bool { !(n.exact_zpm && !force && answer) }
pub open spec fn premise_pr(recurse: bool) -> bool { !recurse }

// ================================================================ L1: the intended contract

pub open spec fn removed(w: W, m: nat) -> bool { !w.mods.contains_key(m) || (m == ipm_id() && !w.present.contains(sd())) }

pub open spec fn same_ns(a: W, b: W) -> bool {
    a.present == b.present && a.mods == b.mods && a.streams == b.streams && a.hooks == b.hooks
}

pub open spec fn scope(w: W, t: nat, recurse: bool) -> Set<nat> { if recurse { w.mods[t].needs.insert(t) } else { set![t] } }

pub open spec fn exempt(w: W, r: nat, global: bool) -> bool {
    w.res.contains_key(r) && match w.res[r].kind {
        RKind::Files { file, keep, protected, .. } => keep || (protected && !file),
        RKind::Code { mapped } => mapped && !global,
        RKind::Gbl { preserve, .. } => preserve,
        RKind::Web => false,
    }
}

// UNIN-B28/B31: a blocked uninstall changes nothing in the namespace.
pub open spec fn l1_blocked_unchanged(w: W, t: nat, force: bool, global: bool, o: Out) -> bool {
    intent_blocked(w, t, force, global) ==> same_ns(w, o.w)
}

// UNIN-B30 (DEC-UNIN-1 open), UNIN-B80: success is reported exactly when the module is gone. Both directions: a
// command that removed the module reports success; one that leaves it (failed, refused or skipped) does not.
pub open spec fn l1_honest(w: W, t: nat, o: Out) -> bool { o.ok <==> removed(o.w, t) }

// UNIN-B28/B33: without -force, a module that another installed module needs stays installed.
pub open spec fn l1_dependents_protect(w: W, t: nat, force: bool, o: Out) -> bool {
    !force && intent_dependent(w, t) ==> o.w.mods.contains_key(t) && o.w.mods[t] == w.mods[t]
}

// UNIN-B5/B6/B21: only the resources and records of the target (and, with -r, of its needs) go.
pub open spec fn l1_only_own(w: W, t: nat, recurse: bool, o: Out) -> bool {
    &&& forall|r: nat| #![trigger o.w.present.contains(r)] w.present.contains(r) && !o.w.present.contains(r) ==>
            w.res.contains_key(r) && scope(w, t, recurse).contains(w.res[r].owner)
    &&& forall|m: nat| #![trigger o.w.mods.contains_key(m)] w.mods.contains_key(m) && !o.w.mods.contains_key(m) ==> scope(w, t, recurse).contains(m)
}

// UNIN-B19: files copied with the keep flag stay.
pub open spec fn l1_kept_files(w: W, o: Out) -> bool {
    forall|r: nat| #![trigger o.w.present.contains(r)]
        w.present.contains(r) && w.res.contains_key(r) && (w.res[r].kind matches RKind::Files { keep: true, .. }) ==> o.w.present.contains(r)
}

// UNIN-B5/B11/B12/B18: once the record is gone, every resource of the module is gone unless it is meant to stay.
pub open spec fn l1_complete(w: W, t: nat, global: bool, o: Out) -> bool {
    !o.w.mods.contains_key(t) ==> forall|i: int| #![trigger w.mods[t].res[i]] 0 <= i < w.mods[t].res.len() ==>
        exempt(w, w.mods[t].res[i], global) || !o.w.present.contains(w.mods[t].res[i])
}

// UNIN-B51..B62: uninstalling IPM, by any of its names, runs the guard (refused, or asked and the shell exits).
pub open spec fn l1_self_guard(w: W, force: bool, o: Out) -> bool {
    (o.w.exit_flag && (o.w.prompted || force)) || (!o.ok && same_ns(w, o.w))
}

// IPM is removed only when the operator names it.
pub open spec fn l1_ipm_kept(w: W, o: Out) -> bool {
    o.w.present.contains(sd()) && (w.mods.contains_key(ipm_id()) ==> o.w.mods.contains_key(ipm_id()))
}

// UNIN-B66: Clean leaves the module it runs on unchanged (its record, its resources, no undo hook).
pub open spec fn l1_clean_keeps_target(w: W, t: nat, o: Out) -> bool {
    &&& o.w.mods.contains_key(t) && o.w.mods[t] == w.mods[t]
    &&& forall|r: nat| #![trigger o.w.present.contains(r)] owns(w, t, r) && w.present.contains(r) ==> o.w.present.contains(r)
    &&& forall|i: int| w.hooks.len() <= i < o.w.hooks.len() ==> o.w.hooks[i] != t
}

// UNIN-B30, B42, B81: every history row this command writes for a module says success exactly when that module is
// gone (both directions).
pub open spec fn l1_rows_honest(w: W, o: Out) -> bool {
    forall|i: int| #![trigger o.w.hist[i]] w.hist.len() <= i < o.w.hist.len() && o.w.hist[i].m is Some
        ==> (o.w.hist[i].ok <==> removed(o.w, o.w.hist[i].m->0))
}

// UNIN-B8/B9: a module whose record goes also loses its stored manifest.
pub open spec fn l1_manifest_with_record(w: W, o: Out) -> bool {
    forall|m: nat| #![trigger o.w.mods.contains_key(m)] w.mods.contains_key(m) && !o.w.mods.contains_key(m) ==> !o.w.streams.contains(m)
}

// UNIN-B3/B68: a name that is not installed fails and changes nothing.
pub open spec fn l1_not_found(w: W, o: Out) -> bool { !o.ok && same_ns(w, o.w) }

// ================================================================ frame (what one Clean may touch)

pub open spec fn cuts(res: Map<nat, Res>, r2: nat, f: nat) -> bool {
    res.contains_key(r2) && match res[r2].kind { RKind::Files { folder, file, keep, protected } => folder == f && !file && !keep && !protected, _ => false }
}

pub open spec fn may_delete(res: Map<nat, Res>, r: nat, a: Set<nat>) -> bool {
    res.contains_key(r) && match res[r].kind {
        RKind::Files { folder, file, keep, .. } => (file && !keep && a.contains(res[r].owner))
            || exists|r2: nat| #[trigger] cuts(res, r2, folder) && a.contains(res[r2].owner),
        _ => a.contains(res[r].owner),
    }
}

pub open spec fn frame(w: W, w2: W, a: Set<nat>, full: bool) -> bool {
    &&& w2.res == w.res
    &&& w2.present.subset_of(w.present)
    &&& forall|r: nat| #![trigger w2.present.contains(r)] w.present.contains(r) && !w2.present.contains(r) ==> may_delete(w.res, r, a)
    &&& forall|m: nat| #![trigger w2.mods.contains_key(m)] w2.mods.contains_key(m) ==> w.mods.contains_key(m) && w2.mods[m] == w.mods[m]
    &&& !full ==> forall|m: nat| #![trigger w2.mods.contains_key(m)] w.mods.contains_key(m) && !w2.mods.contains_key(m) ==> a.contains(m)
    &&& w2.streams.subset_of(w.streams)
    &&& w.hooks.len() <= w2.hooks.len() && w2.hooks.subrange(0, w.hooks.len() as int) == w.hooks
    &&& forall|i: int| w.hooks.len() <= i < w2.hooks.len() ==> a.contains(#[trigger] w2.hooks[i])
    &&& w2.exit_flag == w.exit_flag && w2.prompted == w.prompted && w2.mapped_from == w.mapped_from
}

proof fn unin_frame_refl(w: W, a: Set<nat>, full: bool)
    ensures frame(w, w, a, full)
{
    assert(w.hooks.subrange(0, w.hooks.len() as int) =~= w.hooks);
}

proof fn unin_frame_trans(w1: W, w2: W, w3: W, a: Set<nat>, full: bool)
    requires frame(w1, w2, a, full), frame(w2, w3, a, full)
    ensures frame(w1, w3, a, full)
{
    assert forall|r: nat| #![trigger w3.present.contains(r)] w1.present.contains(r) && !w3.present.contains(r) implies may_delete(w1.res, r, a) by {
        if w2.present.contains(r) { assert(may_delete(w2.res, r, a)); } else { assert(!w2.present.contains(r)); }
    }
    assert forall|m: nat| #![trigger w3.mods.contains_key(m)] w3.mods.contains_key(m) implies w1.mods.contains_key(m) && w3.mods[m] == w1.mods[m] by {
        assert(w2.mods.contains_key(m));
    }
    if !full {
        assert forall|m: nat| #![trigger w3.mods.contains_key(m)] w1.mods.contains_key(m) && !w3.mods.contains_key(m) implies a.contains(m) by {
            if w2.mods.contains_key(m) { } else { assert(!w2.mods.contains_key(m)); }
        }
    }
    assert(w3.hooks.subrange(0, w1.hooks.len() as int) =~= w3.hooks.subrange(0, w2.hooks.len() as int).subrange(0, w1.hooks.len() as int));
    assert forall|i: int| w1.hooks.len() <= i < w3.hooks.len() implies a.contains(#[trigger] w3.hooks[i]) by {
        if i < w2.hooks.len() {
            assert(w3.hooks.subrange(0, w2.hooks.len() as int)[i] == w3.hooks[i]);
            assert(a.contains(w2.hooks[i]));
        }
    }
}

proof fn unin_frame_mono(w1: W, w2: W, a: Set<nat>, b: Set<nat>, full: bool)
    requires frame(w1, w2, a, full), a.subset_of(b)
    ensures frame(w1, w2, b, full)
{
    assert forall|r: nat| w1.present.contains(r) && !w2.present.contains(r) implies may_delete(w1.res, r, b) by {
        assert(may_delete(w1.res, r, a));
        match w1.res[r].kind {
            RKind::Files { folder, file, keep, .. } => {
                if !(file && !keep && a.contains(w1.res[r].owner)) {
                    let r2 = choose|r2: nat| #[trigger] cuts(w1.res, r2, folder) && a.contains(w1.res[r2].owner);
                    assert(cuts(w1.res, r2, folder) && b.contains(w1.res[r2].owner));
                }
            },
            _ => {},
        }
    }
}

proof fn unin_wf_frame(w: W, w2: W, a: Set<nat>, full: bool)
    requires wf(w), frame(w, w2, a, full)
    ensures wf(w2)
{
    assert forall|m: nat, i: int| #![trigger w2.mods[m].deps[i]]
        w2.mods.contains_key(m) && 0 <= i < w2.mods[m].deps.len() implies
        w2.mods[m].needs.contains(w2.mods[m].deps[i]) &&
        (w2.mods.contains_key(w2.mods[m].deps[i]) ==> w2.mods[w2.mods[m].deps[i]].needs.subset_of(w2.mods[m].needs)) by {
        assert(w.mods[m].deps[i] == w2.mods[m].deps[i]);
        if w2.mods.contains_key(w2.mods[m].deps[i]) { assert(w.mods.contains_key(w.mods[m].deps[i])); }
    }
    assert forall|a1: nat, b1: nat| #![trigger w2.mods[a1], w2.mods[b1]]
        w2.mods.contains_key(a1) && w2.mods.contains_key(b1) && a1 != b1 implies
        w2.mods[a1].key != w2.mods[b1].key && w2.mods[a1].ext_key != w2.mods[b1].ext_key by {
        assert(w.mods[a1] == w2.mods[a1] && w.mods[b1] == w2.mods[b1]);
    }
    assert forall|m: nat, i: int| #![trigger w2.mods[m].res[i]]
        w2.mods.contains_key(m) && 0 <= i < w2.mods[m].res.len() implies owns(w2, m, w2.mods[m].res[i]) by {
        assert(w.mods[m].res[i] == w2.mods[m].res[i]);
    }
    assert forall|m: nat| #![trigger w2.mods[m]] w2.mods.contains_key(m) implies !w2.mods[m].needs.contains(m) by {
        assert(w.mods[m] == w2.mods[m]);
    }
}

// ---------------------------------------------------------------- the two resource loops

pub open spec fn webs(w: W, rs: Seq<nat>) -> Set<nat> { rs.to_set().filter(|r: nat| is_web(w, r)) }

proof fn unin_before_exact(w: W, rs: Seq<nat>)
    ensures before_clean(w, rs) == (W { present: w.present.difference(webs(w, rs)), ..w })
    decreases rs.len()
{
    if rs.len() == 0 {
        assert(w.present.difference(webs(w, rs)) =~= w.present);
    } else {
        unin_before_exact(w, rs.drop_last());
        let w1 = before_clean(w, rs.drop_last());
        assert forall|r: nat| #![trigger rs.contains(r)] rs.contains(r) <==> (rs.drop_last().contains(r) || r == rs.last()) by {
            if rs.contains(r) {
                let i = choose|i: int| 0 <= i < rs.len() && rs[i] == r;
                if i < rs.len() - 1 { assert(rs.drop_last()[i] == r); }
            }
            if rs.drop_last().contains(r) {
                let i = choose|i: int| 0 <= i < rs.drop_last().len() && rs.drop_last()[i] == r;
                assert(rs[i] == r);
            }
            if r == rs.last() { assert(rs[rs.len() - 1] == r); }
        }
        if is_web(w1, rs.last()) {
            assert(w1.present.remove(rs.last()) =~= w.present.difference(webs(w, rs)));
        } else {
            assert(w1.present =~= w.present.difference(webs(w, rs)));
        }
    }
}

// Every resource in rs belongs to a module in a.
pub open spec fn owned_by(w: W, rs: Seq<nat>, a: Set<nat>) -> bool {
    forall|i: int| #![trigger rs[i]] 0 <= i < rs.len() ==> w.res.contains_key(rs[i]) && a.contains(w.res[rs[i]].owner)
}

proof fn unin_before_frame(w: W, rs: Seq<nat>, a: Set<nat>, full: bool)
    requires owned_by(w, rs, a)
    ensures frame(w, before_clean(w, rs), a, full)
{
    unin_before_exact(w, rs);
    unin_frame_refl(w, a, full);
    assert forall|r: nat| #![trigger before_clean(w, rs).present.contains(r)]
        w.present.contains(r) && !before_clean(w, rs).present.contains(r) implies may_delete(w.res, r, a) by {
        assert(webs(w, rs).contains(r));
        let i = choose|i: int| 0 <= i < rs.len() && rs[i] == r;
        assert(w.res.contains_key(rs[i]) && a.contains(w.res[rs[i]].owner));
    }
}

// What the %Clean resource loop leaves: a frame, the collected items, and the deletions it guarantees.
pub open spec fn res_loop_post(w: W, rs: Seq<nat>, acc: Acc, a: Set<nat>) -> bool {
    &&& frame(w, acc.w, a, false)
    &&& acc.w.mods == w.mods && acc.w.streams == w.streams && acc.w.hooks == w.hooks && acc.w.hist == w.hist
    &&& forall|r: nat| #![trigger acc.del.contains(r)] acc.del.contains(r) ==> rs.contains(r) && w.res.contains_key(r) && w.res[r].kind is Code
    &&& forall|i: int| #![trigger rs[i]] 0 <= i < rs.len() && w.res.contains_key(rs[i]) ==>
            (w.res[rs[i]].kind is Code ==> acc.del.contains(rs[i])) &&
            ((w.res[rs[i]].kind is Files && !exempt(w, rs[i], false)) ==> !acc.w.present.contains(rs[i])) &&
            ((w.res[rs[i]].kind matches RKind::Gbl { named: true, preserve: false }) ==> !acc.w.present.contains(rs[i]))
}

proof fn unin_clean_res(w: W, rs: Seq<nat>, a: Set<nat>)
    requires owned_by(w, rs, a)
    ensures res_loop_post(w, rs, clean_res(w, rs), a)
    decreases rs.len()
{
    if rs.len() == 0 {
        unin_frame_refl(w, a, false);
    } else {
        let pre = rs.drop_last();
        assert(owned_by(w, pre, a)) by { assert forall|i: int| 0 <= i < pre.len() implies #[trigger] w.res.contains_key(pre[i]) && a.contains(w.res[pre[i]].owner) by { assert(pre[i] == rs[i]); } }
        unin_clean_res(w, pre, a);
        let acc0 = clean_res(w, pre);
        let r = rs.last();
        let acc = clean_step(acc0, r);
        assert(rs[rs.len() - 1] == r);
        assert(w.res.contains_key(r) && a.contains(w.res[r].owner));
        assert forall|x: nat| pre.contains(x) implies rs.contains(x) by {
            let i = choose|i: int| 0 <= i < pre.len() && pre[i] == x; assert(rs[i] == x);
        }
        // the step is a frame from acc0
        assert(frame(acc0.w, acc.w, a, false)) by {
            unin_frame_refl(acc0.w, a, false);
            match acc0.w.res[r].kind {
                RKind::Files { folder, file, keep, protected } => {
                    if !keep && !file && !protected {
                        assert forall|x: nat| #![trigger acc.w.present.contains(x)] acc0.w.present.contains(x) && !acc.w.present.contains(x) implies may_delete(acc0.w.res, x, a) by {
                            assert(folder_set(acc0.w, folder).contains(x));
                            assert(cuts(acc0.w.res, r, acc0.w.res[x].kind->folder));
                        }
                    }
                },
                _ => {},
            }
        }
        unin_frame_trans(w, acc0.w, acc.w, a, false);
        assert forall|i: int| #![trigger rs[i]] 0 <= i < rs.len() && w.res.contains_key(rs[i]) implies
            (w.res[rs[i]].kind is Code ==> acc.del.contains(rs[i])) &&
            ((w.res[rs[i]].kind is Files && !exempt(w, rs[i], false)) ==> !acc.w.present.contains(rs[i])) &&
            ((w.res[rs[i]].kind matches RKind::Gbl { named: true, preserve: false }) ==> !acc.w.present.contains(rs[i])) by {
            if i < rs.len() - 1 {
                assert(pre[i] == rs[i]);
            } else {
                match w.res[r].kind {
                    RKind::Files { folder, file, .. } => { if !file { assert(folder_set(acc0.w, folder).contains(r)); } },
                    _ => {},
                }
            }
        }
    }
}

proof fn unin_clean_code(w: W, rs: Seq<nat>, rm: bool, a: Set<nat>)
    requires owned_by(w, rs, a)
    ensures
        frame(w, clean_code(w, rs, rm), a, false),
        clean_code(w, rs, rm).mods == w.mods, clean_code(w, rs, rm).streams == w.streams,
        clean_code(w, rs, rm).hooks == w.hooks, clean_code(w, rs, rm).hist == w.hist,
        forall|i: int| #![trigger rs[i]] 0 <= i < rs.len() && w.res.contains_key(rs[i]) && !exempt(w, rs[i], rm) && !is_web(w, rs[i])
            && !(w.res[rs[i]].kind matches RKind::Gbl { named: false, .. }) ==> !clean_code(w, rs, rm).present.contains(rs[i]),
{
    unin_clean_res(w, rs, a);
    let acc = clean_res(w, rs);
    let w2 = clean_code(w, rs, rm);
    assert(frame(acc.w, w2, a, false)) by {
        unin_frame_refl(acc.w, a, false);
        assert forall|x: nat| #![trigger w2.present.contains(x)] acc.w.present.contains(x) && !w2.present.contains(x) implies may_delete(acc.w.res, x, a) by {
            assert(acc.del.contains(x));
            let i = choose|i: int| 0 <= i < rs.len() && rs[i] == x;
            assert(w.res.contains_key(rs[i]) && a.contains(w.res[rs[i]].owner));
        }
    }
    unin_frame_trans(w, acc.w, w2, a, false);
    assert forall|i: int| #![trigger rs[i]] 0 <= i < rs.len() && w.res.contains_key(rs[i]) && !exempt(w, rs[i], rm) && !is_web(w, rs[i])
        && !(w.res[rs[i]].kind matches RKind::Gbl { named: false, .. }) implies !w2.present.contains(rs[i]) by {
        match w.res[rs[i]].kind {
            RKind::Code { mapped } => { assert(acc.del.contains(rs[i])); assert(code_gone(acc.w, acc.del, rm).contains(rs[i])); },
            _ => {},
        }
    }
}

// ---------------------------------------------------------------- lookup

proof fn unin_lookup_key(w: W, k: nat, m: nat)
    requires wf(w), w.mods.contains_key(m), w.mods[m].key == k
    ensures lookup(w, k) == Some(m)
{
    let c = choose|x: nat| #![trigger w.mods[x]] w.mods.contains_key(x) && w.mods[x].key == k;
    if c != m { assert(w.mods[c].key != w.mods[m].key); }
}

proof fn unin_lookup_ext(w: W, k: nat, m: nat)
    requires wf(w), w.mods.contains_key(m), w.mods[m].ext_key == k, !exists|x: nat| #![trigger w.mods[x]] w.mods.contains_key(x) && w.mods[x].key == k
    ensures lookup(w, k) == Some(m)
{
    let c = choose|x: nat| #![trigger w.mods[x]] w.mods.contains_key(x) && w.mods[x].ext_key == k;
    if c != m { assert(w.mods[c].ext_key != w.mods[m].ext_key); }
}

proof fn unin_lookup_only(w: W, k: nat, m: nat)
    requires w.mods.contains_key(m), w.mods[m].key == k, forall|x: nat| #![trigger w.mods[x]] w.mods.contains_key(x) && w.mods[x].key == k ==> x == m
    ensures lookup(w, k) == Some(m)
{}

proof fn unin_lookup_found(w: W, k: nat)
    requires lookup(w, k) is Some
    ensures w.mods.contains_key(lookup(w, k)->0)
{}

// A typed "zpm" can only reach IPM.
proof fn unin_exact_is_ipm(w: W, n: Typed)
    requires wf_cmd(w, n), n.exact_zpm, lookup(w, n.key) is Some
    ensures lookup(w, n.key)->0 == ipm_id()
{
    let t = lookup(w, n.key)->0;
    if t != ipm_id() {
        if exists|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) && w.mods[m].key == n.key { assert(w.mods[t].key == k_zpm()); }
        else { assert(w.mods[t].ext_key == k_zpm()); }
    }
}

// ---------------------------------------------------------------- shared walks of the code path

pub open spec fn top_w(w: W, n: Typed, force: bool, t: nat) -> W {
    let w0 = if n.exact_zpm { W { exit_flag: true, ..(if !force { W { prompted: true, ..w } } else { w }) } } else { w };
    if t == ipm_id() { W { hist: w0.hist.push(HRow { m: Some(t), ok: true }), ..w0 } } else { w0 }
}

pub open spec fn top_full(n: Typed, force: bool, answer: bool) -> bool { n.exact_zpm && !force && answer }

pub open spec fn top_p(force: bool, recurse: bool, global: bool, full: bool) -> P {
    P { level: 1, force: force, recurse: recurse, global: global, nested: false, full: full, cycle: Set::empty() }
}

// Main.Uninstall reduces to one top-level Clean on a namespace whose stores are those of the start.
proof fn unin_main_shape(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some
    ensures ({
        let t = lookup(w, n.key)->0;
        let o = main_uninstall(w, n, force, recurse, global, answer, fuel);
        let w1 = top_w(w, n, force, t);
        let e = exec_clean(w1, t, top_p(force, recurse, global, top_full(n, force, answer)), fuel);
        &&& w.mods.contains_key(t)
        &&& (n.exact_zpm && w.mapped_from) ==> (o.w == w && !o.ok)
        &&& !(n.exact_zpm && w.mapped_from) ==> (
                wf(w1) && w1.mods == w.mods && w1.res == w.res && w1.present == w.present && w1.streams == w.streams
                && w1.hooks == w.hooks && w1.mapped_from == w.mapped_from
                && w1.exit_flag == (n.exact_zpm || w.exit_flag) && w1.prompted == ((n.exact_zpm && !force) || w.prompted)
                && o.ok == e.ok && o.w.present == e.w.present && o.w.mods == e.w.mods && o.w.streams == e.w.streams
                && o.w.hooks == e.w.hooks && o.w.exit_flag == e.w.exit_flag && o.w.prompted == e.w.prompted)
    })
{
    unin_lookup_found(w, n.key);
    let t = lookup(w, n.key)->0;
    if n.exact_zpm { unin_exact_is_ipm(w, n); }
    if !(n.exact_zpm && w.mapped_from) {
        let w0 = if n.exact_zpm { W { exit_flag: true, ..(if !force { W { prompted: true, ..w } } else { w }) } } else { w };
        assert(lookup(w0, n.key) == lookup(w, n.key));
    }
}

// Before %Clean decides anything: web apps go (OnBeforePhase), then the Unconfigure hook.
proof fn unin_pre(w: W, t: nat, p: P)
    requires wf(w), w.mods.contains_key(t)
    ensures ({
        let m = w.mods[t];
        let a = scope_p(w, t, p);
        let wb = before_clean(w, m.res);
        let w2 = if m.installer { W { hooks: wb.hooks.push(t), ..wb } } else { wb };
        &&& wb == W { present: w.present.difference(webs(w, m.res)), ..w }
        &&& owned_by(w, m.res, a) && owned_by(w2, m.res, a)
        &&& frame(w, wb, a, p.full) && frame(w, w2, a, p.full) && wf(wb) && wf(w2)
        &&& w2.mods == w.mods && w2.res == w.res && w2.streams == w.streams && w2.hist == w.hist
        &&& (!has_web(w, t) ==> wb == w)
    })
{
    let m = w.mods[t];
    let a = scope_p(w, t, p);
    assert(owned_by(w, m.res, a)) by {
        assert forall|i: int| 0 <= i < m.res.len() implies #[trigger] w.res.contains_key(m.res[i]) && a.contains(w.res[m.res[i]].owner) by {
            assert(owns(w, t, w.mods[t].res[i]));
        }
    }
    unin_before_exact(w, m.res);
    unin_before_frame(w, m.res, a, p.full);
    let wb = before_clean(w, m.res);
    let w2 = if m.installer { W { hooks: wb.hooks.push(t), ..wb } } else { wb };
    assert(frame(wb, w2, a, p.full)) by {
        unin_frame_refl(wb, a, p.full);
        assert(w2.hooks.subrange(0, wb.hooks.len() as int) =~= wb.hooks);
    }
    unin_frame_trans(w, wb, w2, a, p.full);
    if !has_web(w, t) {
        assert(webs(w, m.res) =~= Set::empty()) by {
            assert forall|r: nat| !webs(w, m.res).contains(r) by {
                if webs(w, m.res).contains(r) {
                    let i = choose|i: int| 0 <= i < m.res.len() && m.res[i] == r;
                    assert(is_web(w, w.mods[t].res[i]));
                }
            }
        }
        assert(w.present.difference(webs(w, m.res)) =~= w.present);
    }
}

proof fn unin_frame_full(w: W, w2: W, a: Set<nat>, full: bool)
    requires frame(w, w2, a, false)
    ensures frame(w, w2, a, full)
{}

// After the decision: code at Clean.Level>0, then the dependency recursion.
proof fn unin_tail(w2: W, t: nat, m: Module, p: P, fuel: nat)
    requires wf(w2), w2.mods.contains_key(t), m == w2.mods[t], owned_by(w2, m.res, scope_p(w2, t, p))
    ensures ({
        let a = scope_p(w2, t, p);
        let w3 = after_code(w2, m, p);
        let o = after_deps(w3, m, P { cycle: p.cycle.insert(t), ..p }, fuel);
        &&& frame(w2, w3, a, false) && wf(w3) && w3.mods == w2.mods && w3.streams == w2.streams && w3.hooks == w2.hooks
        &&& frame(w3, o.w, m.needs, p.full)
        &&& !p.recurse ==> o == Out { w: w3, ok: true, skip: false }
        &&& w3.hist == w2.hist
        &&& forall|i: int| #![trigger m.res[i]] p.level > 0 && 0 <= i < m.res.len() && w2.res.contains_key(m.res[i])
                && !exempt(w2, m.res[i], p.global) && !is_web(w2, m.res[i])
                && !(w2.res[m.res[i]].kind matches RKind::Gbl { named: false, .. }) ==> !w3.present.contains(m.res[i])
    })
    decreases fuel, 0nat
{
    let a = scope_p(w2, t, p);
    let w3 = after_code(w2, m, p);
    if p.level > 0 { unin_clean_code(w2, m.res, p.global, a); } else { unin_frame_refl(w2, a, false); }
    unin_wf_frame(w2, w3, a, false);
    let p1 = P { cycle: p.cycle.insert(t), ..p };
    if p.recurse && fuel > 0 {
        assert(deps_within(w3, m.deps, m.needs)) by {
            assert forall|i: int| #![trigger m.deps[i]] 0 <= i < m.deps.len() && w3.mods.contains_key(m.deps[i])
                implies w3.mods[m.deps[i]].needs.insert(m.deps[i]).subset_of(m.needs) by { assert(w2.mods[t].deps[i] == m.deps[i]); }
        }
        unin_loop_frame(w3, m.deps, p1, (fuel - 1) as nat, m.needs);
    } else { unin_frame_refl(w3, m.needs, p.full); }
}

// ---------------------------------------------------------------- the recursion (mutual induction)

pub open spec fn scope_p(w: W, t: nat, p: P) -> Set<nat> { if p.recurse { w.mods[t].needs.insert(t) } else { set![t] } }

proof fn unin_exec_frame(w: W, t: nat, p: P, fuel: nat)
    requires wf(w)
    ensures frame(w, exec_clean(w, t, p, fuel).w, scope_p(w, t, p), p.full)
    decreases fuel, 1nat
{
    let a = scope_p(w, t, p);
    if !w.mods.contains_key(t) { unin_frame_refl(w, a, p.full); return; }
    let m = w.mods[t];
    unin_pre(w, t, p);
    let wb = before_clean(w, m.res);
    if check_skip(wb, m.global_scope, t, p) { return; }
    let w2 = if m.installer { W { hooks: wb.hooks.push(t), ..wb } } else { wb };
    if m.installer && !m.hook_ok { return; }
    let p1 = P { cycle: p.cycle.insert(t), ..p };
    if check_skip(w2, m.global_scope, t, p1) { return; }
    assert(scope_p(w2, t, p) == a);
    unin_tail(w2, t, m, p, fuel);
    let w3 = after_code(w2, m, p);
    let o = after_deps(w3, m, p1, fuel);
    unin_frame_full(w2, w3, a, p.full);
    unin_frame_trans(w, w2, w3, a, p.full);
    if p.recurse { unin_frame_mono(w3, o.w, m.needs, a, p.full); } else { unin_frame_refl(w3, a, p.full); }
    unin_frame_trans(w, w3, o.w, a, p.full);
    if !o.ok { return; }
    let o2 = if p.level > 0 && !p.nested && o.w.present.contains(sd()) { record_delete(o, t) } else { o };
    assert(frame(o.w, o2.w, a, p.full)) by { unin_frame_refl(o.w, a, p.full); assert(a.contains(t)); }
    unin_frame_trans(w, o.w, o2.w, a, p.full);
    let o3 = ipm_block(o2, t, p);
    assert(frame(o2.w, o3.w, a, p.full)) by { unin_frame_refl(o2.w, a, p.full); }
    unin_frame_trans(w, o2.w, o3.w, a, p.full);
}

pub open spec fn deps_within(w: W, ds: Seq<nat>, a: Set<nat>) -> bool {
    forall|i: int| #![trigger ds[i]] 0 <= i < ds.len() && w.mods.contains_key(ds[i]) ==> w.mods[ds[i]].needs.insert(ds[i]).subset_of(a)
}

proof fn unin_loop_frame(w: W, ds: Seq<nat>, p: P, fuel: nat, a: Set<nat>)
    requires wf(w), p.recurse, deps_within(w, ds, a)
    ensures frame(w, dep_loop(w, ds, p, fuel).w, a, p.full)
    decreases fuel, ds.len() + 3
{
    if ds.len() == 0 { unin_frame_refl(w, a, p.full); return; }
    let pre = ds.drop_last();
    assert(deps_within(w, pre, a)) by {
        assert forall|i: int| 0 <= i < pre.len() && w.mods.contains_key(#[trigger] pre[i]) implies w.mods[pre[i]].needs.insert(pre[i]).subset_of(a) by { assert(pre[i] == ds[i]); }
    }
    unin_loop_frame(w, pre, p, fuel, a);
    let o = dep_loop(w, pre, p, fuel);
    if !o.ok { return; }
    unin_wf_frame(w, o.w, a, p.full);
    let d = ds.last();
    assert(ds[ds.len() - 1] == d);
    if o.w.mods.contains_key(d) { assert(w.mods.contains_key(d) && o.w.mods[d] == w.mods[d]); }
    unin_step_frame(o.w, d, p, fuel, a);
    unin_frame_trans(w, o.w, dep_step(o.w, d, p, fuel).w, a, p.full);
}

proof fn unin_step_frame(w: W, d: nat, p: P, fuel: nat, a: Set<nat>)
    requires wf(w), p.recurse, w.mods.contains_key(d) ==> w.mods[d].needs.insert(d).subset_of(a)
    ensures frame(w, dep_step(w, d, p, fuel).w, a, p.full)
    decreases fuel, 2nat
{
    if p.cycle.contains(d) || !w.mods.contains_key(d) { unin_frame_refl(w, a, p.full); return; }
    let pd = P { level: p.level + 1, nested: true, ..p };
    unin_exec_frame(w, d, pd, fuel);
    let o = exec_clean(w, d, pd, fuel);
    unin_frame_mono(w, o.w, scope_p(w, d, pd), a, p.full);
    if o.ok && !o.w.present.contains(sd()) { return; }
    let w4 = if o.ok && !o.skip { W { mods: o.w.mods.remove(d), ..o.w } } else { o.w };
    let w5 = W { hist: w4.hist.push(HRow { m: Some(d), ok: o.ok }), ..w4 };
    assert(frame(o.w, w5, a, p.full)) by { unin_frame_refl(o.w, a, p.full); assert(a.contains(d)); }
    unin_frame_trans(w, o.w, w5, a, p.full);
    assert(dep_step(w, d, p, fuel).w == w5);
}

// ================================================================ refinement: one lemma per clause and operation

// The code's dependents check and the intended one agree under PG.
proof fn unin_check_agrees(w: W, t: nat, p: P)
    requires wf(w), premise_pg(w), w.mods.contains_key(t),
    ensures (exists|m: nat| code_dependent(w, t, p.cycle, m)) ==> intent_dependent(w, t),
            p.cycle =~= Set::<nat>::empty() && intent_dependent(w, t) ==> exists|m: nat| code_dependent(w, t, p.cycle, m),
{
    if exists|m: nat| code_dependent(w, t, p.cycle, m) {
        let m = choose|m: nat| code_dependent(w, t, p.cycle, m);
        assert(w.mods[m].graph == Some(w.mods[m].needs));
    }
    if p.cycle =~= Set::<nat>::empty() && intent_dependent(w, t) {
        let m = choose|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) && w.mods[m].needs.contains(t);
        assert(w.mods[m].graph == Some(w.mods[m].needs));
        assert(!p.cycle.contains(m));
        assert(code_dependent(w, t, p.cycle, m));
    }
}

// UNIN-B28/B31 under PG and PW.
pub proof fn unin_refines_blocked_unchanged(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some,
        premise_pg(w), premise_pw(w, lookup(w, n.key)->0),
    ensures l1_blocked_unchanged(w, lookup(w, n.key)->0, force, global, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    if !intent_blocked(w, t, force, global) || (n.exact_zpm && w.mapped_from) { return; }
    let w1 = top_w(w, n, force, t);
    let p = top_p(force, recurse, global, top_full(n, force, answer));
    let m = w1.mods[t];
    assert(!has_web(w1, t));
    unin_pre(w1, t, p);
    assert(premise_pg(w1));
    unin_check_agrees(w1, t, p);
    assert(check_skip(before_clean(w1, m.res), m.global_scope, t, p));
}

// UNIN-B30, B80 under PB (not blocked), PG, PI and PNF; both directions. `recurse ==> fuel > 0` only says the
// model's nesting bound is not already spent at the top (uninstall.TRUST.md U4).
pub proof fn unin_refines_honest(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some,
        premise_pb(w, lookup(w, n.key)->0, force, global),
        premise_pg(w), premise_pi(w, lookup(w, n.key)->0, recurse), premise_pnf(n, force, answer),
        recurse ==> fuel > 0,
    ensures l1_honest(w, lookup(w, n.key)->0, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    if n.exact_zpm && w.mapped_from {
        // refused: nothing changed, and the module (with IPM's classes) is still there
        assert(w.mods.contains_key(t) && w.present.contains(sd()));
        return;
    }
    let w1 = top_w(w, n, force, t);
    let p = top_p(force, recurse, global, false);
    assert(top_full(n, force, answer) == false);
    let e = exec_clean(w1, t, p, fuel);
    let m = w1.mods[t];
    unin_pre(w1, t, p);
    let wb = before_clean(w1, m.res);
    let w2 = if m.installer { W { hooks: wb.hooks.push(t), ..wb } } else { wb };
    assert(premise_pg(wb));
    unin_check_agrees(wb, t, p);
    assert(!check_skip(wb, m.global_scope, t, p));
    assert(w2.present.contains(sd())) by { assert(!webs(w1, m.res).contains(sd())); }
    assert(w2.mods.contains_key(t));
    if m.installer && !m.hook_ok {
        // the Unconfigure hook failed: failure reported, record and IPM's classes kept
        assert(e == Out { w: w2, ok: false, skip: false });
        return;
    }
    let p1 = P { cycle: p.cycle.insert(t), ..p };
    assert(!check_skip(w2, m.global_scope, t, p1)) by {
        if exists|x: nat| code_dependent(w2, t, p1.cycle, x) {
            let x = choose|x: nat| code_dependent(w2, t, p1.cycle, x);
            assert(code_dependent(wb, t, p.cycle, x));
        }
    }
    let a = scope_p(w1, t, p);
    assert(scope_p(w2, t, p) == a);
    unin_tail(w2, t, m, p, fuel);
    let w3 = after_code(w2, m, p);
    let o = after_deps(w3, m, p1, fuel);
    assert(!m.needs.contains(t));
    if t == ipm_id() {
        let i = choose|i: int| 0 <= i < m.res.len() && m.res[i] == sd();
        assert(!exempt(w2, m.res[i], global) && !is_web(w2, m.res[i]));
        assert(!w3.present.contains(sd()));
        // IPM declares no dependencies, so the dependency step succeeds: the work done is reported as success
        assert(m.deps.len() == 0);
        if recurse { assert(dep_loop(w3, m.deps, p1, (fuel - 1) as nat) == Out { w: w3, ok: true, skip: false }); }
        assert(o.ok);
        assert(!o.w.present.contains(sd()));
        assert(e == o);
    } else {
        assert(w3.present.contains(sd())) by {
            if !w3.present.contains(sd()) { assert(may_delete(w2.res, sd(), a)); }
        }
        assert(o.w.present.contains(sd())) by {
            if !o.w.present.contains(sd()) { assert(may_delete(w3.res, sd(), m.needs)); }
        }
        if !o.ok {
            // a failed dependency: failure reported and the target's record kept (the recursion only removes needs)
            assert(w3.mods.contains_key(t));
            assert(o.w.mods.contains_key(t)) by { if !o.w.mods.contains_key(t) { assert(m.needs.contains(t)); } }
            assert(e == o);
        }
    }
}

// UNIN-B28/B33 under PG.
pub proof fn unin_refines_dependents_protect(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some, premise_pg(w),
    ensures l1_dependents_protect(w, lookup(w, n.key)->0, force, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    if force || !intent_dependent(w, t) || (n.exact_zpm && w.mapped_from) { return; }
    let w1 = top_w(w, n, force, t);
    let p = top_p(force, recurse, global, top_full(n, force, answer));
    let m = w1.mods[t];
    unin_pre(w1, t, p);
    let wb = before_clean(w1, m.res);
    assert(premise_pg(wb));
    assert(intent_dependent(wb, t));
    unin_check_agrees(wb, t, p);
    assert(check_skip(wb, m.global_scope, t, p));
}

// UNIN-B5/B6/B21 under PF and PNF.
pub proof fn unin_refines_only_own(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some, premise_pf(w), premise_pnf(n, force, answer),
    ensures l1_only_own(w, lookup(w, n.key)->0, recurse, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    if n.exact_zpm && w.mapped_from { return; }
    let w1 = top_w(w, n, force, t);
    let p = top_p(force, recurse, global, false);
    assert(top_full(n, force, answer) == false);
    unin_exec_frame(w1, t, p, fuel);
    let e = exec_clean(w1, t, p, fuel);
    let a = scope(w, t, recurse);
    assert(scope_p(w1, t, p) == a);
    assert forall|r: nat| #![trigger e.w.present.contains(r)] w.present.contains(r) && !e.w.present.contains(r)
        implies w.res.contains_key(r) && a.contains(w.res[r].owner) by {
        assert(may_delete(w1.res, r, a));
        match w.res[r].kind {
            RKind::Files { folder, file, keep, .. } => {
                if !(file && !keep && a.contains(w.res[r].owner)) {
                    let r2 = choose|r2: nat| #[trigger] cuts(w1.res, r2, folder) && a.contains(w1.res[r2].owner);
                    assert(cuts(w.res, r2, folder) && in_folder(w.res[r].kind, folder));
                }
            },
            _ => {},
        }
    }
}

// UNIN-B19 under PF and PNF.
pub proof fn unin_refines_kept_files(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some, premise_pf(w), premise_pnf(n, force, answer),
    ensures l1_kept_files(w, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    if n.exact_zpm && w.mapped_from { return; }
    let w1 = top_w(w, n, force, t);
    let p = top_p(force, recurse, global, false);
    assert(top_full(n, force, answer) == false);
    unin_exec_frame(w1, t, p, fuel);
    let e = exec_clean(w1, t, p, fuel);
    let a = scope_p(w1, t, p);
    assert forall|r: nat| #![trigger e.w.present.contains(r)]
        w.present.contains(r) && w.res.contains_key(r) && (w.res[r].kind matches RKind::Files { keep: true, .. }) implies e.w.present.contains(r) by {
        if !e.w.present.contains(r) {
            assert(may_delete(w1.res, r, a));
            let folder = w.res[r].kind->folder;
            let r2 = choose|r2: nat| #[trigger] cuts(w1.res, r2, folder) && a.contains(w1.res[r2].owner);
            assert(cuts(w.res, r2, folder) && in_folder(w.res[r].kind, folder));
        }
    }
}

// UNIN-B5/B11/B12/B18 under PGA and PNF.
pub proof fn unin_refines_complete(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some, premise_pga(w, lookup(w, n.key)->0), premise_pnf(n, force, answer),
    ensures l1_complete(w, lookup(w, n.key)->0, global, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    if n.exact_zpm && w.mapped_from { return; }
    let w1 = top_w(w, n, force, t);
    let p = top_p(force, recurse, global, false);
    assert(top_full(n, force, answer) == false);
    let e = exec_clean(w1, t, p, fuel);
    if e.w.mods.contains_key(t) { return; }
    let m = w1.mods[t];
    unin_pre(w1, t, p);
    let wb = before_clean(w1, m.res);
    let w2 = if m.installer { W { hooks: wb.hooks.push(t), ..wb } } else { wb };
    let p1 = P { cycle: p.cycle.insert(t), ..p };
    assert(!check_skip(wb, m.global_scope, t, p));
    assert(!(m.installer && !m.hook_ok));
    assert(!check_skip(w2, m.global_scope, t, p1));
    let a = scope_p(w1, t, p);
    assert(scope_p(w2, t, p) == a);
    unin_tail(w2, t, m, p, fuel);
    let w3 = after_code(w2, m, p);
    let o = after_deps(w3, m, p1, fuel);
    assert(!m.needs.contains(t));
    assert(o.ok);
    assert forall|i: int| #![trigger w.mods[t].res[i]] 0 <= i < w.mods[t].res.len() implies
        exempt(w, w.mods[t].res[i], global) || !e.w.present.contains(w.mods[t].res[i]) by {
        let r = m.res[i];
        assert(owns(w1, t, w1.mods[t].res[i]));
        if !exempt(w, r, global) {
            if is_web(w1, r) { assert(webs(w1, m.res).contains(r)); }
            else {
                assert(!(w.res[r].kind matches RKind::Gbl { named: false, .. }));
                assert(!w3.present.contains(r));
            }
        }
    }
}

// UNIN-B51..B62 under PN.
pub proof fn unin_refines_self_guard(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) == Some(ipm_id()), premise_pn(w, n),
    ensures l1_self_guard(w, force, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    if w.mapped_from { return; }
    let w1 = top_w(w, n, force, ipm_id());
    unin_exec_frame(w1, ipm_id(), top_p(force, recurse, global, top_full(n, force, answer)), fuel);
}

// IPM stays when another module is uninstalled, under PI.
pub proof fn unin_refines_ipm_kept(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some, lookup(w, n.key)->0 != ipm_id(),
        premise_pi(w, lookup(w, n.key)->0, recurse),
    ensures l1_ipm_kept(w, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    if n.exact_zpm { unin_exact_is_ipm(w, n); }
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    let w1 = top_w(w, n, force, t);
    let p = top_p(force, recurse, global, false);
    unin_exec_frame(w1, t, p, fuel);
    let a = scope_p(w1, t, p);
    let e = exec_clean(w1, t, p, fuel);
    assert(!a.contains(ipm_id()));
    if !e.w.present.contains(sd()) { assert(may_delete(w1.res, sd(), a)); }
}

// UNIN-B66 under PCW and PF.
pub proof fn unin_refines_clean_keeps_target(w: W, k: nat, force: bool, global: bool, fuel: nat)
    requires wf(w), lookup(w, k) is Some, premise_pcw(w, lookup(w, k)->0), premise_pf(w),
    ensures l1_clean_keeps_target(w, lookup(w, k)->0, clean_cmd(w, k, force, global, fuel))
{
    let t = lookup(w, k)->0;
    unin_lookup_found(w, k);
    let p = P { level: 0, force: force, recurse: true, global: global, nested: false, full: false, cycle: Set::empty() };
    let m = w.mods[t];
    let e = exec_clean(w, t, p, fuel);
    unin_pre(w, t, p);
    if check_skip(w, m.global_scope, t, p) { return; }
    let p1 = P { cycle: p.cycle.insert(t), ..p };
    if check_skip(w, m.global_scope, t, p1) { return; }
    unin_tail(w, t, m, p, fuel);
    assert(after_code(w, m, p) == w);
    let o = after_deps(w, m, p1, fuel);
    assert(e.w == o.w);
    assert(!m.needs.contains(t));
    assert(e.w.mods.contains_key(t)) by { if !o.w.mods.contains_key(t) { assert(m.needs.contains(t)); } }
    assert forall|i: int| w.hooks.len() <= i < e.w.hooks.len() implies #[trigger] e.w.hooks[i] != t by {
        assert(m.needs.contains(o.w.hooks[i]));
    }
    assert forall|r: nat| #![trigger e.w.present.contains(r)] owns(w, t, r) && w.present.contains(r) implies e.w.present.contains(r) by {
        if !o.w.present.contains(r) {
            assert(may_delete(w.res, r, m.needs));
            match w.res[r].kind {
                RKind::Files { folder, file, keep, .. } => {
                    if !(file && !keep && m.needs.contains(w.res[r].owner)) {
                        let r2 = choose|r2: nat| #[trigger] cuts(w.res, r2, folder) && m.needs.contains(w.res[r2].owner);
                        assert(cuts(w.res, r2, folder) && in_folder(w.res[r].kind, folder));
                    }
                },
                _ => {},
            }
        }
    }
}

// UNIN-B30/B42/B81 under PR, PB, PG and PNF, for a module other than IPM: the one row written is honest (both directions).
pub proof fn unin_refines_rows_honest(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some, lookup(w, n.key)->0 != ipm_id(),
        premise_pr(recurse), premise_pb(w, lookup(w, n.key)->0, force, global),
        premise_pg(w), premise_pnf(n, force, answer),
    ensures l1_rows_honest(w, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    if n.exact_zpm { unin_exact_is_ipm(w, n); }
    unin_refines_honest(w, n, force, recurse, global, answer, fuel);
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    let p = top_p(force, recurse, global, false);
    let e = exec_clean(w, t, p, fuel);
    let m = w.mods[t];
    unin_pre(w, t, p);
    let wb = before_clean(w, m.res);
    let w2 = if m.installer { W { hooks: wb.hooks.push(t), ..wb } } else { wb };
    assert(scope_p(w2, t, p) == scope_p(w, t, p));
    unin_tail(w2, t, m, p, fuel);
    assert(e.w.hist == w.hist);
    let o = main_uninstall(w, n, force, recurse, global, answer, fuel);
    assert(o.w.hist == w.hist.push(HRow { m: Some(t), ok: e.ok }));
    assert(o.ok == e.ok && o.w.mods == e.w.mods && o.w.present == e.w.present);
    assert(o.w.hist[w.hist.len() as int] == HRow { m: Some(t), ok: o.ok });
}

// UNIN-B8/B9 under PR and PNF: without -r, a record never goes without its stored manifest.
pub proof fn unin_refines_manifest_with_record(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is Some, premise_pr(recurse), premise_pnf(n, force, answer),
    ensures l1_manifest_with_record(w, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    let t = lookup(w, n.key)->0;
    unin_main_shape(w, n, force, recurse, global, answer, fuel);
    if n.exact_zpm && w.mapped_from { return; }
    let w1 = top_w(w, n, force, t);
    let p = top_p(force, recurse, global, false);
    assert(top_full(n, force, answer) == false);
    let e = exec_clean(w1, t, p, fuel);
    let m = w1.mods[t];
    unin_pre(w1, t, p);
    let wb = before_clean(w1, m.res);
    let w2 = if m.installer { W { hooks: wb.hooks.push(t), ..wb } } else { wb };
    assert(scope_p(w2, t, p) == scope_p(w1, t, p));
    unin_tail(w2, t, m, p, fuel);
}

// UNIN-B3: a name that is not installed fails and changes nothing (no premise).
pub proof fn unin_refines_not_found(w: W, n: Typed, force: bool, recurse: bool, global: bool, answer: bool, fuel: nat)
    requires wf_cmd(w, n), lookup(w, n.key) is None,
    ensures l1_not_found(w, main_uninstall(w, n, force, recurse, global, answer, fuel))
{
    if n.exact_zpm && !w.mapped_from {
        let w0 = W { exit_flag: true, ..(if !force { W { prompted: true, ..w } } else { w }) };
        assert(lookup(w0, n.key) == lookup(w, n.key));
    }
}

// UNIN-B68: the same for clean.
pub proof fn unin_refines_clean_not_found(w: W, k: nat, force: bool, global: bool, fuel: nat)
    requires lookup(w, k) is None,
    ensures l1_not_found(w, clean_cmd(w, k, force, global, fuel))
{}

// ================================================================ defect witnesses (proved)

pub open spec fn mk(key: nat, ext: nat, deps: Seq<nat>, res: Seq<nat>, needs: Set<nat>, graph: Option<Set<nat>>) -> Module {
    Module { key: key, ext_key: ext, global_scope: false, installer: false, hook_ok: true, deps: deps, res: res, needs: needs, graph: graph }
}
pub open spec fn ipm_mod() -> Module { mk(k_zpm(), k_ipm(), seq![], seq![sd()], Set::empty(), Some(Set::empty())) }
pub open spec fn code(owner: nat) -> Res { Res { kind: RKind::Code { mapped: false }, owner: owner } }
pub open spec fn base(mods: Map<nat, Module>, res: Map<nat, Res>, present: Set<nat>) -> W {
    W { mods: mods, streams: mods.dom(), res: res, present: present, hooks: seq![], hist: seq![],
        exit_flag: false, prompted: false, mapped_from: false }
}
pub open spec fn plain(k: nat) -> Typed { Typed { key: k, exact_zpm: false } }

// F-UNIN-1 and F-UNIN-6 (IRIS: nc-unin-1-blocked-uninstall). nc-web-dep (id 2: class 10, web app 11) is needed by
// nc-web-top (id 3). `uninstall nc-web-dep` is blocked, reports success, logs success, and deletes the web app.
pub open spec fn w_blocked() -> W {
    let dep = mk(2, 20, seq![], seq![10nat, 11nat], Set::empty(), Some(Set::empty()));
    let top = mk(3, 30, seq![2nat], seq![12nat], set![2nat], Some(set![2nat]));
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, dep).insert(3nat, top),
         Map::empty().insert(0nat, code(0)).insert(10nat, code(2)).insert(11nat, Res { kind: RKind::Web, owner: 2 }).insert(12nat, code(3)),
         set![0nat, 10nat, 11nat, 12nat])
}

pub proof fn unin_witness_blocked_deletes_webapp()
    ensures ({
        let w = w_blocked();
        let o = main_uninstall(w, plain(2), false, false, false, false, 3);
        &&& lookup(w, 2) == Some(2nat) && intent_blocked(w, 2, false, false) && !premise_pw(w, 2)
        &&& o.ok && o.w.hist == seq![HRow { m: Some(2nat), ok: true }]
        &&& o.w.mods.contains_key(2) && w.present.contains(11) && !o.w.present.contains(11)
        &&& !l1_blocked_unchanged(w, 2, false, false, o) && !l1_honest(w, 2, o)
    })
{
    let w = w_blocked();
    unin_lookup_only(w, 2, 2);
    assert(is_web(w, w.mods[2].res[1]));
    assert(intent_dependent(w, 2)) by { assert(w.mods.contains_key(3) && w.mods[3].needs.contains(2)); }
    let p = top_p(false, false, false, false);
    let m = w.mods[2];
    reveal_with_fuel(before_clean, 3);
    let w1 = before_clean(w, m.res);
    assert(code_dependent(w1, 2, p.cycle, 3));
    assert(check_skip(w1, false, 2, p));
    assert(w1.present == w.present.remove(11));
    let e = exec_clean(w, 2, p, 3);
    assert(e == Out { w: w1, ok: true, skip: true });
    let fin = main_uninstall(w, plain(2), false, false, false, false, 3);
    assert(fin.w == W { hist: w1.hist.push(HRow { m: Some(2nat), ok: true }), ..w1 } && fin.ok);
}

// F-UNIN-3 (IRIS: nc-unin-self-external-name-IPM). `uninstall IPM` resolves IPM by its external name, skips the
// guard (no question, no shell exit) and deletes IPM's classes.
pub open spec fn w_self() -> W {
    base(Map::empty().insert(0nat, ipm_mod()), Map::empty().insert(0nat, code(0)), set![0nat])
}

pub proof fn unin_witness_external_name_bypass()
    ensures ({
        let w = w_self();
        let n = plain(k_ipm());
        let o = main_uninstall(w, n, false, false, false, false, 3);
        &&& wf_cmd(w, n) && lookup(w, k_ipm()) == Some(ipm_id()) && !premise_pn(w, n)
        &&& o.ok && !o.w.present.contains(sd()) && !o.w.exit_flag && !o.w.prompted
        &&& !l1_self_guard(w, false, o)
    })
{
    let w = w_self();
    assert(!exists|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) && w.mods[m].key == k_ipm());
    assert(w.mods.contains_key(0) && w.mods[0].ext_key == k_ipm());
    let p = top_p(false, false, false, false);
    let wh = W { hist: w.hist.push(HRow { m: Some(ipm_id()), ok: true }), ..w };
    reveal_with_fuel(before_clean, 2);
    reveal_with_fuel(clean_res, 2);
    assert(!check_skip(wh, false, 0, p));
    assert(code_gone(clean_res(wh, seq![0nat]).w, clean_res(wh, seq![0nat]).del, false).contains(0));
    let w3 = clean_code(wh, seq![0nat], false);
    assert(!w3.present.contains(0));
    assert(after_code(wh, wh.mods[0], p) == w3);
    assert(after_deps(w3, wh.mods[0], P { cycle: p.cycle.insert(0), ..p }, 3) == Out { w: w3, ok: true, skip: false });
    let e = exec_clean(wh, 0, p, 3);
    assert(e == Out { w: w3, ok: true, skip: false });
}

// F-UNIN-4 (IRIS: nc-unin-5-unresolvable-dependent). nc-ghost-b (id 3) needs nc-ghost-a (id 2), but its graph does
// not build; `uninstall nc-ghost-a` removes it.
pub open spec fn w_ghost() -> W {
    let a = mk(2, 20, seq![], seq![10nat], Set::empty(), Some(Set::empty()));
    let b = mk(3, 30, seq![2nat], seq![12nat], set![2nat], None);
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, a).insert(3nat, b),
         Map::empty().insert(0nat, code(0)).insert(10nat, code(2)).insert(12nat, code(3)), set![0nat, 10nat, 12nat])
}

pub proof fn unin_witness_unresolvable_dependent()
    ensures ({
        let w = w_ghost();
        let o = main_uninstall(w, plain(2), false, false, false, false, 3);
        &&& lookup(w, 2) == Some(2nat) && intent_dependent(w, 2) && !premise_pg(w)
        &&& o.ok && !o.w.mods.contains_key(2) && !o.w.present.contains(10) && o.w.mods.contains_key(3)
        &&& !l1_dependents_protect(w, 2, false, o)
    })
{
    let w = w_ghost();
    unin_lookup_only(w, 2, 2);
    assert(w.mods.contains_key(3) && w.mods[3].needs.contains(2));
    assert(w.mods[3].graph != Some(w.mods[3].needs));
    let p = top_p(false, false, false, false);
    reveal_with_fuel(before_clean, 2);
    reveal_with_fuel(clean_res, 2);
    assert(!exists|m: nat| code_dependent(w, 2, p.cycle, m));
    let p1 = P { cycle: p.cycle.insert(2), ..p };
    assert(!exists|m: nat| code_dependent(w, 2, p1.cycle, m));
    assert(code_gone(clean_res(w, seq![10nat]).w, clean_res(w, seq![10nat]).del, false).contains(10));
    let m = w.mods[2];
    let w3 = clean_code(w, seq![10nat], false);
    assert(!w3.present.contains(10) && w3.present.contains(sd()));
    assert(after_code(w, m, p) == w3);
    assert(after_deps(w3, m, p1, 3) == Out { w: w3, ok: true, skip: false });
    let e = exec_clean(w, 2, p, 3);
    assert(e == record_delete(Out { w: w3, ok: true, skip: false }, 2));
}

// F-UNIN-5 (IRIS: nc-unin-6-global-resources). A global resource without the Global attribute stays.
pub open spec fn w_global() -> W {
    let g = mk(2, 20, seq![], seq![13nat], Set::empty(), Some(Set::empty()));
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, g),
         Map::empty().insert(0nat, code(0)).insert(13nat, Res { kind: RKind::Gbl { named: false, preserve: false }, owner: 2 }),
         set![0nat, 13nat])
}

pub proof fn unin_witness_global_kept()
    ensures ({
        let w = w_global();
        let o = main_uninstall(w, plain(2), false, false, false, false, 3);
        &&& lookup(w, 2) == Some(2nat) && !premise_pga(w, 2)
        &&& o.ok && !o.w.mods.contains_key(2) && o.w.present.contains(13)
        &&& !l1_complete(w, 2, false, o)
    })
{
    let w = w_global();
    unin_lookup_only(w, 2, 2);
    assert(w.mods[2].res[0] == 13);
    let p = top_p(false, false, false, false);
    reveal_with_fuel(before_clean, 2);
    reveal_with_fuel(clean_res, 2);
    assert(!exists|m: nat| code_dependent(w, 2, p.cycle, m));
    let p1 = P { cycle: p.cycle.insert(2), ..p };
    assert(!exists|m: nat| code_dependent(w, 2, p1.cycle, m));
    assert(!exempt(w, 13, false));
    let m = w.mods[2];
    let w3 = clean_code(w, seq![13nat], false);
    assert(w3.present.contains(13) && w3.present.contains(sd()));
    assert(after_code(w, m, p) == w3);
    assert(after_deps(w3, m, p1, 3) == Out { w: w3, ok: true, skip: false });
    let e = exec_clean(w, 2, p, 3);
    assert(e == record_delete(Out { w: w3, ok: true, skip: false }, 2));
}

// UNIN-B21 against UNIN-B19 (IRIS: nc-unin-7-shared-target-folder). fc-keep-on-uninstall (id 2) copied into
// folder 7 with the keep flag; nc-fc-other (id 3) copied into the same folder without it. Removing nc-fc-other
// deletes the kept files.
pub open spec fn w_shared() -> W {
    let k = mk(2, 20, seq![], seq![14nat], Set::empty(), Some(Set::empty()));
    let x = mk(3, 30, seq![], seq![15nat], Set::empty(), Some(Set::empty()));
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, k).insert(3nat, x),
         Map::empty().insert(0nat, code(0))
            .insert(14nat, Res { kind: RKind::Files { folder: 7, file: false, keep: true, protected: false }, owner: 2 })
            .insert(15nat, Res { kind: RKind::Files { folder: 7, file: false, keep: false, protected: false }, owner: 3 }),
         set![0nat, 14nat, 15nat])
}

pub proof fn unin_witness_shared_folder()
    ensures ({
        let w = w_shared();
        let o = main_uninstall(w, plain(3), false, false, false, false, 3);
        &&& lookup(w, 3) == Some(3nat) && !premise_pf(w)
        &&& o.ok && !o.w.mods.contains_key(3) && w.present.contains(14) && !o.w.present.contains(14)
        &&& !l1_kept_files(w, o) && !l1_only_own(w, 3, false, o)
    })
{
    let w = w_shared();
    unin_lookup_only(w, 3, 3);
    assert(cuts(w.res, 15, 7) && in_folder(w.res[14].kind, 7));
    let p = top_p(false, false, false, false);
    reveal_with_fuel(before_clean, 2);
    reveal_with_fuel(clean_res, 2);
    assert(!exists|m: nat| code_dependent(w, 3, p.cycle, m));
    let p1 = P { cycle: p.cycle.insert(3), ..p };
    assert(!exists|m: nat| code_dependent(w, 3, p1.cycle, m));
    assert(folder_set(w, 7).contains(14));
    let m = w.mods[3];
    assert(before_clean(w, m.res) == w);
    let w3 = clean_code(w, seq![15nat], false);
    assert(clean_res(w, seq![15nat]).w.present == w.present.difference(folder_set(w, 7)));
    assert(!w3.present.contains(14) && w3.present.contains(sd()));
    assert(after_code(w, m, p) == w3);
    assert(after_deps(w3, m, p1, 3) == Out { w: w3, ok: true, skip: false });
    let e = exec_clean(w, 3, p, 3);
    assert(e.ok && !e.w.mods.contains_key(3) && !e.w.present.contains(14));
    assert(!scope(w, 3, false).contains(2));
}

// F-UNIN-1, clean part (IRIS: nc-unin-2-clean-own-module). `nc-web-w clean` deletes the module's web app and runs
// its installer's undo hooks, though the module stays.
pub open spec fn w_clean() -> W {
    let x = Module { installer: true, ..mk(2, 20, seq![], seq![10nat, 11nat], Set::empty(), Some(Set::empty())) };
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, x),
         Map::empty().insert(0nat, code(0)).insert(10nat, code(2)).insert(11nat, Res { kind: RKind::Web, owner: 2 }),
         set![0nat, 10nat, 11nat])
}

pub proof fn unin_witness_clean_own_module()
    ensures ({
        let w = w_clean();
        let o = clean_cmd(w, 2, false, false, 3);
        &&& lookup(w, 2) == Some(2nat) && !premise_pcw(w, 2)
        &&& o.ok && o.w.mods.contains_key(2) && o.w.present.contains(10) && !o.w.present.contains(11) && o.w.hooks == seq![2nat]
        &&& !l1_clean_keeps_target(w, 2, o)
    })
{
    let w = w_clean();
    unin_lookup_only(w, 2, 2);
    assert(is_web(w, w.mods[2].res[1]));
    reveal_with_fuel(before_clean, 3);
    reveal_with_fuel(dep_loop, 2);
    assert(owns(w, 2, 11));
    let p0 = P { level: 0, force: false, recurse: true, global: false, nested: false, full: false, cycle: Set::empty() };
    let w1 = before_clean(w, w.mods[2].res);
    assert(w1.present == w.present.remove(11));
    let w2 = W { hooks: w1.hooks.push(2), ..w1 };
    assert(!check_skip(w1, false, 2, p0));
    assert(!check_skip(w2, false, 2, P { cycle: p0.cycle.insert(2), ..p0 }));
    assert(after_code(w2, w.mods[2], p0) == w2);
    assert(after_deps(w2, w.mods[2], P { cycle: p0.cycle.insert(2), ..p0 }, 3) == Out { w: w2, ok: true, skip: false });
    let e = exec_clean(w, 2, p0, 3);
    assert(e == Out { w: w2, ok: true, skip: false });
}

// F-UNIN-8 (IRIS: nc-repo-model-unin-recurse-ipm-dependency). `uninstall nc-rec-ipmdep -r`, where nc-rec-ipmdep
// (id 2) declares a need on zpm and nothing else needs zpm. The recursion cleans IPM itself, with no guard and no
// question: IPM's classes are deleted, then the command fails with <CLASS DOES NOT EXIST>; both records stay.
pub open spec fn w_ipmdep() -> W {
    let x = mk(2, 20, seq![0nat], seq![10nat], set![0nat], Some(set![0nat]));
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, x),
         Map::empty().insert(0nat, code(0)).insert(10nat, code(2)), set![0nat, 10nat])
}

proof fn unin_wf_ipmdep()
    ensures wf(w_ipmdep())
{
    let w = w_ipmdep();
    assert forall|m: nat, i: int| #![trigger w.mods[m].res[i]]
        w.mods.contains_key(m) && 0 <= i < w.mods[m].res.len() implies owns(w, m, w.mods[m].res[i]) by {
        if m == 0 { assert(w.mods[m].res[i] == 0); } else { assert(m == 2); assert(w.mods[m].res[i] == 10); }
    }
    assert forall|m: nat, i: int| #![trigger w.mods[m].deps[i]]
        w.mods.contains_key(m) && 0 <= i < w.mods[m].deps.len() implies
        w.mods[m].needs.contains(w.mods[m].deps[i]) &&
        (w.mods.contains_key(w.mods[m].deps[i]) ==> w.mods[w.mods[m].deps[i]].needs.subset_of(w.mods[m].needs)) by {
        assert(m == 2); assert(w.mods[m].deps[i] == 0);
    }
    assert(w.mods[0].res.contains(sd())) by { assert(w.mods[0].res[0] == sd()); }
}

pub proof fn unin_witness_recurse_removes_ipm()
    ensures ({
        let w = w_ipmdep();
        let o = main_uninstall(w, plain(2), false, true, false, false, 3);
        &&& wf_cmd(w, plain(2)) && lookup(w, 2) == Some(2nat) && !premise_pi(w, 2, true)
        &&& !o.ok && !o.w.present.contains(sd()) && !o.w.present.contains(10) && !o.w.exit_flag && !o.w.prompted
        &&& o.w.mods.contains_key(2) && o.w.mods.contains_key(0)
        &&& !l1_ipm_kept(w, o)
    })
{
    let w = w_ipmdep();
    unin_wf_ipmdep();
    unin_lookup_only(w, 2, 2);
    let p = top_p(false, true, false, false);
    reveal_with_fuel(before_clean, 2);
    reveal_with_fuel(clean_res, 2);
    reveal_with_fuel(dep_loop, 2);
    let m = w.mods[2];
    assert(before_clean(w, m.res) == w);
    assert(!exists|x: nat| code_dependent(w, 2, p.cycle, x));
    let p1 = P { cycle: p.cycle.insert(2), ..p };
    assert(!exists|x: nat| code_dependent(w, 2, p1.cycle, x));
    let w3 = clean_code(w, seq![10nat], false);
    assert(code_gone(clean_res(w, seq![10nat]).w, clean_res(w, seq![10nat]).del, false).contains(10));
    assert(after_code(w, m, p) == w3);
    let pd = P { level: 2, nested: true, ..p1 };
    let m0 = w3.mods[0];
    assert(before_clean(w3, m0.res) == w3);
    assert(!exists|x: nat| code_dependent(w3, 0, pd.cycle, x));
    let pd1 = P { cycle: pd.cycle.insert(0), ..pd };
    assert(!exists|x: nat| code_dependent(w3, 0, pd1.cycle, x));
    let w5 = clean_code(w3, seq![0nat], false);
    assert(code_gone(clean_res(w3, seq![0nat]).w, clean_res(w3, seq![0nat]).del, false).contains(0));
    assert(after_code(w3, m0, pd) == w5);
    assert(after_deps(w5, m0, pd1, 2) == Out { w: w5, ok: true, skip: false });
    let e0 = exec_clean(w3, 0, pd, 2);
    assert(e0 == Out { w: w5, ok: true, skip: false });
    let s0 = dep_step(w3, 0, p1, 2);
    assert(s0 == Out { w: w5, ok: false, skip: false });
    let o = after_deps(w3, m, p1, 3);
    assert(o == dep_loop(w3, seq![0nat], p1, 2));
    assert(dep_loop(w3, seq![0nat], p1, 2) == s0);
}

// F-UNIN-1 and F-UNIN-6 inside -r (IRIS: nc-repo-model-unin-recurse-skipped-dep). nc-rec-top (id 3) and
// nc-rec-other (id 4) both need nc-web-dep (id 2: class 10, web app 11). `uninstall nc-rec-top -r` skips
// nc-web-dep, deletes its web app first, and logs its uninstall as a success.
pub open spec fn w_recskip() -> W {
    let dep = mk(2, 20, seq![], seq![10nat, 11nat], Set::empty(), Some(Set::empty()));
    let top = mk(3, 30, seq![2nat], seq![12nat], set![2nat], Some(set![2nat]));
    let other = mk(4, 40, seq![2nat], seq![13nat], set![2nat], Some(set![2nat]));
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, dep).insert(3nat, top).insert(4nat, other),
         Map::empty().insert(0nat, code(0)).insert(10nat, code(2)).insert(11nat, Res { kind: RKind::Web, owner: 2 })
            .insert(12nat, code(3)).insert(13nat, code(4)),
         set![0nat, 10nat, 11nat, 12nat, 13nat])
}

pub proof fn unin_witness_recurse_skipped_dep()
    ensures ({
        let w = w_recskip();
        let o = main_uninstall(w, plain(3), false, true, false, false, 3);
        &&& lookup(w, 3) == Some(3nat) && !premise_pr(true)
        &&& o.ok && !o.w.mods.contains_key(3) && o.w.mods.contains_key(2) && o.w.present.contains(10) && !o.w.present.contains(11)
        &&& o.w.hist == seq![HRow { m: Some(2nat), ok: true }, HRow { m: Some(3nat), ok: true }]
        &&& !l1_rows_honest(w, o)
    })
{
    let w = w_recskip();
    unin_lookup_only(w, 3, 3);
    let p = top_p(false, true, false, false);
    reveal_with_fuel(before_clean, 3);
    reveal_with_fuel(clean_res, 2);
    reveal_with_fuel(dep_loop, 2);
    let m = w.mods[3];
    assert(before_clean(w, m.res) == w);
    assert(!exists|x: nat| code_dependent(w, 3, p.cycle, x));
    let p1 = P { cycle: p.cycle.insert(3), ..p };
    assert(!exists|x: nat| code_dependent(w, 3, p1.cycle, x));
    let w3 = clean_code(w, seq![12nat], false);
    assert(code_gone(clean_res(w, seq![12nat]).w, clean_res(w, seq![12nat]).del, false).contains(12));
    assert(after_code(w, m, p) == w3);
    let pd = P { level: 2, nested: true, ..p1 };
    let m2 = w3.mods[2];
    let wb = before_clean(w3, m2.res);
    assert(is_web(w3, 11));
    assert(wb.present == w3.present.remove(11));
    assert(code_dependent(wb, 2, pd.cycle, 4));
    let e2 = exec_clean(w3, 2, pd, 2);
    assert(e2 == Out { w: wb, ok: true, skip: true });
    let s2 = dep_step(w3, 2, p1, 2);
    assert(s2.w == W { hist: wb.hist.push(HRow { m: Some(2nat), ok: true }), ..wb } && s2.ok);
    let o = after_deps(w3, m, p1, 3);
    assert(o == dep_loop(w3, seq![2nat], p1, 2));
    assert(dep_loop(w3, seq![2nat], p1, 2) == s2);
    assert(o.w.present.contains(sd()));
    assert(o == s2);
    let e = exec_clean(w, 3, p, 3);
    assert(e == ipm_block(record_delete(s2, 3), 3, p));
    assert(e.ok && !e.w.mods.contains_key(3) && e.w.mods.contains_key(2) && !e.w.present.contains(11) && e.w.present.contains(10));
    let fin = main_uninstall(w, plain(3), false, true, false, false, 3);
    assert(fin.w == W { hist: e.w.hist.push(HRow { m: Some(3nat), ok: true }), ..e.w } && fin.ok);
    assert(fin.w.hist[0] == HRow { m: Some(2nat), ok: true });
}

// F-UNIN-9 (IRIS: nc-repo-model-unin-recurse-removed-dep). nc-rec-top (id 3) needs nc-web-dep (id 2); nothing else
// does. `uninstall nc-rec-top -r` removes nc-web-dep's record by NameDelete and leaves its stored manifest.
pub open spec fn w_recremove() -> W {
    let dep = mk(2, 20, seq![], seq![10nat], Set::empty(), Some(Set::empty()));
    let top = mk(3, 30, seq![2nat], seq![12nat], set![2nat], Some(set![2nat]));
    base(Map::empty().insert(0nat, ipm_mod()).insert(2nat, dep).insert(3nat, top),
         Map::empty().insert(0nat, code(0)).insert(10nat, code(2)).insert(12nat, code(3)),
         set![0nat, 10nat, 12nat])
}

pub proof fn unin_witness_recurse_leaves_manifest()
    ensures ({
        let w = w_recremove();
        let o = main_uninstall(w, plain(3), false, true, false, false, 3);
        &&& lookup(w, 3) == Some(3nat) && !premise_pr(true)
        &&& o.ok && !o.w.mods.contains_key(3) && !o.w.mods.contains_key(2) && !o.w.streams.contains(3) && o.w.streams.contains(2)
        &&& !l1_manifest_with_record(w, o)
    })
{
    let w = w_recremove();
    unin_lookup_only(w, 3, 3);
    let p = top_p(false, true, false, false);
    reveal_with_fuel(before_clean, 2);
    reveal_with_fuel(clean_res, 2);
    reveal_with_fuel(dep_loop, 2);
    let m = w.mods[3];
    assert(before_clean(w, m.res) == w);
    assert(!exists|x: nat| code_dependent(w, 3, p.cycle, x));
    let p1 = P { cycle: p.cycle.insert(3), ..p };
    assert(!exists|x: nat| code_dependent(w, 3, p1.cycle, x));
    let w3 = clean_code(w, seq![12nat], false);
    assert(code_gone(clean_res(w, seq![12nat]).w, clean_res(w, seq![12nat]).del, false).contains(12));
    assert(after_code(w, m, p) == w3);
    let pd = P { level: 2, nested: true, ..p1 };
    let m2 = w3.mods[2];
    assert(before_clean(w3, m2.res) == w3);
    assert(!exists|x: nat| code_dependent(w3, 2, pd.cycle, x));
    let pd1 = P { cycle: pd.cycle.insert(2), ..pd };
    assert(!exists|x: nat| code_dependent(w3, 2, pd1.cycle, x));
    let w5 = clean_code(w3, seq![10nat], false);
    assert(code_gone(clean_res(w3, seq![10nat]).w, clean_res(w3, seq![10nat]).del, false).contains(10));
    assert(after_code(w3, m2, pd) == w5);
    assert(after_deps(w5, m2, pd1, 2) == Out { w: w5, ok: true, skip: false });
    assert(exec_clean(w3, 2, pd, 2) == Out { w: w5, ok: true, skip: false });
    let s2 = dep_step(w3, 2, p1, 2);
    assert(s2.ok && !s2.w.mods.contains_key(2) && s2.w.streams.contains(2) && s2.w.present.contains(sd()));
    let o = after_deps(w3, m, p1, 3);
    assert(o == dep_loop(w3, seq![2nat], p1, 2));
    assert(dep_loop(w3, seq![2nat], p1, 2) == s2);
    assert(o == s2);
    assert(!check_skip(w, m.global_scope, 3, p) && !check_skip(w, m.global_scope, 3, p1));
    let e = exec_clean(w, 3, p, 3);
    assert(e == ipm_block(record_delete(s2, 3), 3, p));
    assert(e.ok && !e.w.mods.contains_key(3) && !e.w.mods.contains_key(2) && !e.w.streams.contains(3) && e.w.streams.contains(2));
    let fin = main_uninstall(w, plain(3), false, true, false, false, 3);
    assert(fin.w.mods == e.w.mods && fin.w.streams == e.w.streams && fin.ok == e.ok);
    assert(w.mods.contains_key(2) && !fin.w.mods.contains_key(2) && fin.w.streams.contains(2));
}

fn main() {}

} // verus!
