// L1/L2 models of IPM namespace ownership and mapping (ipm 571e04a1), with refinement proofs.
//
// L2 transcribes, over an unbounded set of namespaces (a Map keyed by namespace id), the code of
//   - `enable -map [-repos]`      Main.cls EnableIPM #L3611-L3708 (code loop #L3626-L3651, repo loop #L3653-L3692,
//                                 batch activation #L3694-L3698);
//   - `enable -community`         Main.cls #L3564-L3575 (repo -delete-all, repo -reset-defaults, enable -map -repos -globally);
//   - `unmap [-repos-only]`       Main.cls UnmapIPM #L3927-L4004;
//   - the installer               preload/cls/IPM/Installer.cls setup, manifest #L25-L39;
//   - `install zpm` guard         Main.cls CheckModuleNamespace #L2534-L2540;
//   - the IPMVersion check        Storage/SystemRequirements.cls CheckIPMVersion #L85-L99;
// over the IRIS mapping environment contract below (E1-E7), validated by probes on IRIS 2026.1
// (models/explorer/nsp/nsenv.py, observations nc-nsp-* and nc-nsp-model-env-*).
//
// Namespace ids are ints >= 0; namespace n owns database n (its routine and global database). lib_db() is IRISLIB.
// A command is one L2 step. Inside a step the code's own order is kept: the loops write the configuration
// (Config.Map*, "cfg") and read the active table ("act"); activation copies cfg to act. Each loop iteration reads
// only the active table and its own namespace's cfg, so the list loop is transcribed as a set-wide map (the
// explorer's fine model runs the literal ordered list loops and checks that this folding changes nothing).
//
// L1 is what an operator sees per namespace: whose IPM code it runs, whose registries it uses, its own registry
// data and its own zpm record.
//
// Proved (nsp_lemma_*): every command refines its L1 effect under named premises; wf is preserved.
// Proved defect witnesses (nsp_witness_*): F-NSP-4, F-NSP-5, F-NSP-6, F-NSP-2.
// Named premises (each is a spec fn premise_*; mapped to observations in ns_mapping.TRUST.md):
//   SRC   the source of enable is a local install whose IPM code and repo settings are in its own database;
//   UNIQ  no other local install with a later name reaches the source's database (GetMapInfo keeps the last);
//   TGT   targets exist, and a target with a %IPM package mapping to another database also has a routine mapping;
//   REPOS (defect, F-NSP-4) with -repos, every target that is not a local install already reaches IPM code;
//   INFO  every unmap target that reaches IPM code without its own record is listed by GetMapInfo;
//   RTN   (defect, F-NSP-6) no unmap target that GetMapInfo lists has a %IPM.* routine mapping;
//   INST  (defect, F-NSP-5) the installer runs only where every %IPM mapping is absent or points at its own database;
//   OWN   (defect, F-NSP-2) the IPMVersion check runs where the IPM code is the namespace's own.
// No command here has a modelled success/failure result (L2 steps return a state only), so there is no
// "worked => success / failed => failure" clause to state; enable's printed result is F-NSP-1 (fine machine only).
// Premise declarations read by scripts/ontology_index.py (a P tag must name each label its lemma requires):
// PREMISE premise_src = SRC
// PREMISE premise_uniq = UNIQ
// PREMISE premise_tgt = TGT
// PREMISE premise_repos = REPOS
// PREMISE premise_info = INFO
// PREMISE premise_rtn = RTN
// PREMISE premise_inst = INST
// PREMISE premise_own = OWN
use vstd::prelude::*;

verus! {

// ---------------------------------------------------------------- IRIS mapping environment (contract)

pub open spec fn lib_db() -> int { -1 }

// One namespace's %IPM entries: package mapping %IPM, routine mapping %IPM.* (type ALL), and the global mappings
// IPM.Repo.Definition{D,S,I} (created and removed together by IPM, so one field).
pub struct Maps { pub pkg: Option<int>, pub rtn: Option<int>, pub repo: Option<int> }

pub struct Ns {
    pub cfg: Maps,          // Config.MapPackages/MapRoutines/MapGlobals (iris.cpf)
    pub act: Maps,          // the active mapping table: what lookups and new processes see
    pub code: bool,         // database n holds compiled IPM code
    pub rec: Option<int>,   // the zpm module record (its version) in namespace n's own data
    pub rows: bool,         // database n holds registry rows (^IPM.Repo.DefinitionD has subnodes)
}

pub struct S { pub ns: Map<int, Ns> }

pub open spec fn no_maps() -> Maps { Maps { pkg: None, rtn: None, repo: None } }

// E1 %SYS.Namespace.GetPackageDest(n, "%IPM"): the %IPM.* routine mapping wins, then the package mapping,
//    else IRISLIB (O:nc-nsp-installer-repoint, O:nc-nsp-installer-existing-pkg-mapping, O:nc-nsp-N6-unmap-recheck-nca,
//    O:nc-nsp-N2-enable-map, O:nc-nsp-model-env-precedence).
pub open spec fn pkg_dest(m: Maps) -> int {
    match m.rtn { Some(d) => d, None => match m.pkg { Some(d) => d, None => lib_db() } }
}

// E1' GetRoutineDest(n, "%IPM.Main.1"): the same database as GetPackageDest in every combination probed: a package
//     mapping also maps the package's routines (O:nc-nsp-model-env-precedence: pkg USER only gives USER).
pub open spec fn rtn_dest(m: Maps) -> int { pkg_dest(m) }

// E3 GetGlobalDest(n, "IPM.Repo.DefinitionD"): the mapped database, else n's own (O:nc-nsp-N3-enable-map-repos-rerun).
pub open spec fn glo_dest(m: Maps, n: int) -> int { match m.repo { Some(d) => d, None => n } }

pub open spec fn has_code(s: S, d: int) -> bool { d != lib_db() && s.ns.dom().contains(d) && s.ns[d].code }

pub open spec fn rows_at(s: S, d: int) -> bool { d != lib_db() && s.ns.dom().contains(d) && s.ns[d].rows }

// E6 IsIPMEnabled (Main.cls#L4333) and $$$comClassDefined("%IPM.Storage.Module"): the database the %IPM package
//    resolves to holds IPM code (O:nc-nsp-N6-shell-after-unmap, O:nc-nsp-N5-enable-remote-broken, O:nc-nsp-N2-enable-map).
pub open spec fn has_ipm(s: S, n: int) -> bool { has_code(s, pkg_dest(s.ns[n].act)) }

// E5 IPMInstalled (Main.cls#L4122): IPM code reachable and a zpm record in n's own data; module records are
//    namespace data, never reached through a package mapping (O:nc-nsp-N9-ipmversion-mapped: ncaZpmRecord "-").
pub open spec fn installed(s: S, n: int) -> bool { has_ipm(s, n) && s.ns[n].rec is Some }

// E2 activation (ActivateMappings, PerformBatchActivation Module.cls#L1869-L1918): the namespace's active table
//    becomes its configuration (O:nc-nsp-model-env-activation: a created mapping is not seen before activation).
pub open spec fn activate(s: S, to: Set<int>) -> S {
    S { ns: Map::new(s.ns.dom(),
                     |n: int| if to.contains(n) { Ns { act: s.ns[n].cfg, ..s.ns[n] } } else { s.ns[n] }) }
}

// ---------------------------------------------------------------- GetMapInfo (Main.cls#L4106-L4120, #L1628-L1654)

// GetListModules("*", "zpm"): namespaces where %IPM.Storage.Module is defined and a zpm record exists.
pub open spec fn is_source(s: S, x: int) -> bool { s.ns.dom().contains(x) && installed(s, x) }

// AddIPMMappedNamespaces: mappedFrom(GetPackageDest(source)) = source, in name order, so the last source sharing a
// database wins; a namespace that is not a source and resolves %IPM to that database is mapped from it.
pub open spec fn mapped_from(s: S, t: int, x: int) -> bool {
    &&& s.ns.dom().contains(t) && !is_source(s, t) && is_source(s, x)
    &&& pkg_dest(s.ns[x].act) == pkg_dest(s.ns[t].act)
    &&& forall |y: int| #[trigger] is_source(s, y) && pkg_dest(s.ns[y].act) == pkg_dest(s.ns[t].act) ==> y <= x
}

pub open spec fn listed(s: S, t: int) -> bool { exists |x: int| mapped_from(s, t, x) }

// ---------------------------------------------------------------- enable -map [-repos]

// Code loop body (#L3628-L3650): skip a local install; else MapPackageEquivalently (Build.cls#L537) and
// MapRoutineEquivalently (Build.cls#L570) with skip-activation, which create a mapping only when none exists
// (AddPackageMapping Module.cls#L1631, AddRoutineMapping #L1690).
pub open spec fn map_code_one(s: S, src: int, t: int) -> Ns {
    let c = s.ns[t].cfg;
    if installed(s, t) { s.ns[t] }
    else {
        Ns { cfg: Maps { pkg: if c.pkg is None { Some(pkg_dest(s.ns[src].act)) } else { c.pkg },
                         rtn: if c.rtn is None { Some(rtn_dest(s.ns[src].act)) } else { c.rtn }, ..c },
             ..s.ns[t] }
    }
}

// Repo loop body (#L3656-L3690), run on the state after the code loop (nothing activated yet):
// skip unless GetMapInfo (computed once, #L3654) says t is mapped from src; skip if t's ^IPM.Repo.DefinitionD has
// rows; else MapGlobalEquivalently for D, S, I. That call passes skip-activation in AddGlobalMapping's collation
// slot (Build.cls#L628 vs Module.cls#L1736), so the mapping is activated at once (ActivateMappings(t)).
pub open spec fn repo_mapped(s1: S, src: int, t: int) -> bool {
    mapped_from(s1, t, src) && !rows_at(s1, glo_dest(s1.ns[t].act, t))
}

pub open spec fn map_repo_one(s1: S, src: int, t: int) -> Ns {
    let c = s1.ns[t].cfg;
    if repo_mapped(s1, src, t) {
        let m = Maps { repo: if c.repo is None { Some(glo_dest(s1.ns[src].act, src)) } else { c.repo }, ..c };
        Ns { cfg: m, act: m, ..s1.ns[t] }
    } else { s1.ns[t] }
}

pub open spec fn enable_map(s: S, src: int, ts: Set<int>, repos: bool) -> S {
    let s1 = S { ns: Map::new(s.ns.dom(),
                              |n: int| if ts.contains(n) { map_code_one(s, src, n) } else { s.ns[n] }) };
    let s2 = if repos {
        S { ns: Map::new(s1.ns.dom(),
                         |n: int| if ts.contains(n) { map_repo_one(s1, src, n) } else { s1.ns[n] }) }
    } else { s1 };
    // nsToActivate: every target that is not a local install (#L3647), every repo-mapped target (#L3686).
    activate(s2, s.ns.dom().filter(|n: int| ts.contains(n) && (!installed(s, n) || (repos && repo_mapped(s1, src, n)))))
}

// enable -community (#L3564-L3575): repo -delete-all and repo -reset-defaults in src leave src's database with
// registry rows (the default registry); then enable -map -repos -globally (every namespace; %ALL is not one).
pub open spec fn reset_repos(s: S, src: int) -> S {
    let d = glo_dest(s.ns[src].act, src);
    if s.ns.dom().contains(d) { S { ns: s.ns.insert(d, Ns { rows: true, ..s.ns[d] }) } } else { s }
}

pub open spec fn community(s: S, src: int) -> S {
    let r = reset_repos(s, src);
    enable_map(r, src, r.ns.dom(), true)
}

// ---------------------------------------------------------------- unmap [-repos-only]

pub enum RtnType { All, Other }

// RemoveRoutineMapping (Module.cls#L1707-L1724): the mapping name is pMapping_"_"_pType unless pType = "ALL".
// E7: IPM only ever creates "%IPM.*" with type ALL, so a mapping named "%IPM.*_1" never exists (O:nc-nsp-N6-unmap-recheck-nca).
pub open spec fn remove_rtn(m: Maps, ty: RtnType) -> Maps {
    match ty { RtnType::All => Maps { rtn: None, ..m }, RtnType::Other => m }
}

pub open spec fn unmap_one(s: S, t: int, repos_only: bool) -> Ns {
    let c = s.ns[t].cfg;
    if !listed(s, t) { s.ns[t] }                                      // "No mapping found ... Skipping." (#L3969)
    else {
        let c1 = Maps { repo: None, ..c };                            // RemoveGlobalMapping D, S, I (#L3980)
        let c2 = if repos_only { c1 } else {
            // RemovePackageMapping(ns, "%IPM", 1, .pkgChanged) (#L3992)
            // RemoveRoutineMapping(ns, "%IPM.*", 1, .rtnChanged) (#L3998): 1 lands in pType, not pSkipActivation
            remove_rtn(Maps { pkg: None, ..c1 }, RtnType::All)
        };
        Ns { cfg: c2, ..s.ns[t] }
    }
}

pub open spec fn unmap(s: S, ts: Set<int>, repos_only: bool) -> S {
    let s1 = S { ns: Map::new(s.ns.dom(),
                              |n: int| if ts.contains(n) { unmap_one(s, n, repos_only) } else { s.ns[n] }) };
    // nsToActivate: a namespace whose mapping was removed (#L3982, #L3994, #L4000)
    activate(s1, s.ns.dom().filter(|n: int| ts.contains(n) && s1.ns[n].cfg != s.ns[n].cfg))
}

// ---------------------------------------------------------------- installer and install zpm

// IPM.Installer setup in namespace n (Installer.cls#L25-L39): <ClassMapping Package="%IPM" From="${CURNSRoutineDB}"/>
// and <RoutineMapping Routines="%IPM.*" Type="ALL" .../> add a mapping only when none exists (E4,
// O:nc-nsp-installer-repoint: cpf keeps Package_%IPM=USER) and are activated; Import compiles the IPM code into the
// database %IPM now resolves to; ZPMLoad creates the zpm record of version v.
pub open spec fn installer(s: S, n: int, v: int) -> S {
    let c = s.ns[n].cfg;
    let m = Maps { pkg: if c.pkg is None { Some(n) } else { c.pkg }, rtn: if c.rtn is None { Some(n) } else { c.rtn }, ..c };
    let d = pkg_dest(m);
    let ns1 = s.ns.insert(n, Ns { cfg: m, act: m, rec: Some(v), ..s.ns[n] });
    S { ns: if ns1.dom().contains(d) { ns1.insert(d, Ns { code: true, ..ns1[d] }) } else { ns1 } }
}

// `install zpm` in n: refused when GetMapInfo lists n (CheckModuleNamespace #L2534-L2537); otherwise IPM's own
// module is loaded again where %IPM resolves, and the record gets version v.
pub open spec fn install_zpm(s: S, n: int, v: int) -> S {
    if listed(s, n) { s }
    else {
        let d = pkg_dest(s.ns[n].act);
        let ns1 = s.ns.insert(n, Ns { rec: Some(v), ..s.ns[n] });
        S { ns: if ns1.dom().contains(d) { ns1.insert(d, Ns { code: true, ..ns1[d] }) } else { ns1 } }
    }
}

// CheckIPMVersion (SystemRequirements.cls#L91-L95): SELECT VersionString ... WHERE Name = 'zpm' in the current namespace.
pub open spec fn check_ipm_version(s: S, n: int) -> Option<int> { s.ns[n].rec }

// ---------------------------------------------------------------- well-formedness (inductive)

pub open spec fn wf_ns(s: S, n: int) -> bool {
    let a = s.ns[n].act;
    &&& n >= 0
    &&& s.ns[n].cfg == a
    &&& (a.rtn is Some ==> has_code(s, a.rtn->0))
    &&& (a.rtn is None && a.pkg is Some ==> has_code(s, a.pkg->0))
    &&& (a.repo is Some ==> a.repo->0 != n && a.repo->0 != lib_db() && s.ns.dom().contains(a.repo->0))
}

pub open spec fn wf(s: S) -> bool { forall |n: int| #[trigger] s.ns.dom().contains(n) ==> wf_ns(s, n) }

// ---------------------------------------------------------------- L1

pub struct View { pub code: Option<int>, pub repos: int, pub own_rows: bool, pub ver: Option<int> }

pub open spec fn local(v: View) -> bool { v.code is Some && v.ver is Some }
pub open spec fn mapped(v: View) -> bool { v.code is Some && v.ver is None }

pub open spec fn view(s: S, n: int) -> View {
    View { code: if has_ipm(s, n) { Some(pkg_dest(s.ns[n].act)) } else { None },
           repos: glo_dest(s.ns[n].act, n), own_rows: s.ns[n].rows, ver: s.ns[n].rec }
}

pub open spec fn abs(s: S) -> Map<int, View> { Map::new(s.ns.dom(), |n: int| view(s, n)) }

pub open spec fn rows_of(a: Map<int, View>, n: int) -> bool { a.dom().contains(a[n].repos) && a[a[n].repos].own_rows }

// NSP-B18, B20, B29: a target that is not a local install and runs no IPM code gets the source's code; any other
// keeps what it has. NSP-B23, B24, B25: with -repos, a target whose code now comes from the source and that has
// neither registry rows nor a repo mapping of its own uses the source's registries.
pub open spec fn enable_view(a: Map<int, View>, src: int, ts: Set<int>, repos: bool, n: int) -> View {
    let v = a[n];
    if !ts.contains(n) || local(v) { v }
    else {
        let code = if v.code is Some { v.code } else { a[src].code };
        let share = repos && code == a[src].code && v.repos == n && !rows_of(a, n);
        View { code: code, repos: if share { a[src].repos } else { v.repos }, ..v }
    }
}

pub open spec fn l1_enable(a: Map<int, View>, src: int, ts: Set<int>, repos: bool) -> Map<int, View> {
    Map::new(a.dom(), |n: int| enable_view(a, src, ts, repos, n))
}

// NSP-B30: the source's registries are set to the default one, then code and registries are shared everywhere.
pub open spec fn l1_reset(a: Map<int, View>, src: int) -> Map<int, View> {
    Map::new(a.dom(),
             |n: int| if n == a[src].repos { View { own_rows: true, ..a[n] } } else { a[n] })
}

pub open spec fn l1_community(a: Map<int, View>, src: int) -> Map<int, View> {
    let r = l1_reset(a, src);
    l1_enable(r, src, r.dom(), true)
}

// NSP-B41, B42, B43, B44: a target that runs IPM code without its own record loses the code (unless -repos-only)
// and uses its own registries again; any other target is left as it is.
pub open spec fn unmap_view(a: Map<int, View>, ts: Set<int>, repos_only: bool, n: int) -> View {
    let v = a[n];
    if !ts.contains(n) || !mapped(v) { v }
    else { View { code: if repos_only { v.code } else { None }, repos: n, ..v } }
}

pub open spec fn l1_unmap(a: Map<int, View>, ts: Set<int>, repos_only: bool) -> Map<int, View> {
    Map::new(a.dom(), |n: int| unmap_view(a, ts, repos_only, n))
}

// NSP-B1, B2, B47, B49: installing IPM in n gives n its own code and record; where n runs IPM by mapping the
// install is either refused (nothing changes) or makes n independent (DEC-NSP-1: its registries either way).
pub open spec fn independent(a: Map<int, View>, n: int, v: int, own_repos: bool) -> Map<int, View> {
    a.insert(n, View { code: Some(n), ver: Some(v), repos: if own_repos { n } else { a[n].repos }, ..a[n] })
}

pub open spec fn l1_install(a: Map<int, View>, b: Map<int, View>, n: int, v: int) -> bool {
    if mapped(a[n]) { b == a || b == independent(a, n, v, true) || b == independent(a, n, v, false) }
    else { b == independent(a, n, v, false) }
}

// NSP-B72: an IPM version need is checked against the IPM the namespace runs.
pub open spec fn version_in_use(a: Map<int, View>, n: int) -> Option<int> {
    match a[n].code { Some(d) => if a.dom().contains(d) { a[d].ver } else { None }, None => None }
}

// ---------------------------------------------------------------- premises

pub open spec fn premise_src(s: S, src: int) -> bool {
    s.ns.dom().contains(src) && installed(s, src) && pkg_dest(s.ns[src].act) == src && s.ns[src].act.repo is None
}

pub open spec fn premise_uniq(s: S, src: int) -> bool {
    forall |y: int| #[trigger] is_source(s, y) && pkg_dest(s.ns[y].act) == src ==> y <= src
}

pub open spec fn premise_tgt(s: S, src: int, ts: Set<int>) -> bool {
    forall |t: int| #[trigger] ts.contains(t) ==> s.ns.dom().contains(t)
        && (s.ns[t].act.rtn is Some || s.ns[t].act.pkg is None || s.ns[t].act.pkg == Some(src))
}

pub open spec fn premise_repos(s: S, ts: Set<int>, repos: bool) -> bool {
    repos ==> forall |t: int| #[trigger] ts.contains(t) && !installed(s, t) ==> has_ipm(s, t)
}

pub open spec fn premise_info(s: S, ts: Set<int>) -> bool {
    forall |t: int| #[trigger] ts.contains(t) && s.ns.dom().contains(t) && mapped(view(s, t)) ==> listed(s, t)
}

pub open spec fn premise_rtn(s: S, ts: Set<int>, repos_only: bool) -> bool {
    !repos_only ==> forall |t: int| #[trigger] ts.contains(t) && listed(s, t) ==> s.ns[t].act.rtn is None
}

pub open spec fn premise_inst(s: S, n: int) -> bool {
    let a = s.ns[n].act;
    (a.pkg is None || a.pkg == Some(n)) && (a.rtn is None || a.rtn == Some(n))
}

pub open spec fn premise_own(s: S, n: int) -> bool { pkg_dest(s.ns[n].act) == n }

// ---------------------------------------------------------------- frame facts

pub proof fn nsp_lemma_activate(s: S, to: Set<int>)
    ensures
        activate(s, to).ns.dom() =~= s.ns.dom(),
        forall |n: int| #[trigger] activate(s, to).ns.dom().contains(n) ==>
            activate(s, to).ns[n].cfg == s.ns[n].cfg && activate(s, to).ns[n].code == s.ns[n].code
            && activate(s, to).ns[n].rec == s.ns[n].rec && activate(s, to).ns[n].rows == s.ns[n].rows
            && activate(s, to).ns[n].act == (if to.contains(n) { s.ns[n].cfg } else { s.ns[n].act }),
{}

// ---------------------------------------------------------------- enable -map: refinement

pub proof fn nsp_lemma_enable_one(s: S, src: int, ts: Set<int>, repos: bool, n: int)
    requires wf(s), premise_src(s, src), premise_uniq(s, src), premise_tgt(s, src, ts), premise_repos(s, ts, repos),
             s.ns.dom().contains(n),
    ensures enable_map(s, src, ts, repos).ns.dom() =~= s.ns.dom(),
            view(enable_map(s, src, ts, repos), n) == enable_view(abs(s), src, ts, repos, n),
            wf_ns(enable_map(s, src, ts, repos), n),
{
    let s1 = S { ns: Map::new(s.ns.dom(),
                              |k: int| if ts.contains(k) { map_code_one(s, src, k) } else { s.ns[k] }) };
    let s2 = if repos {
        S { ns: Map::new(s1.ns.dom(),
                         |k: int| if ts.contains(k) { map_repo_one(s1, src, k) } else { s1.ns[k] }) }
    } else { s1 };
    let to = s.ns.dom().filter(|k: int| ts.contains(k) && (!installed(s, k) || (repos && repo_mapped(s1, src, k))));
    let post = enable_map(s, src, ts, repos);
    assert(post == activate(s2, to));
    assert(s1.ns.dom() =~= s.ns.dom());
    assert(s2.ns.dom() =~= s.ns.dom());
    nsp_lemma_activate(s2, to);
    // code, records and rows of every database are unchanged; s1 has s's active table
    assert forall |k: int| #[trigger] s.ns.dom().contains(k) implies
        post.ns[k].code == s.ns[k].code && post.ns[k].rec == s.ns[k].rec && post.ns[k].rows == s.ns[k].rows
        && s1.ns[k].act == s.ns[k].act && s1.ns[k].code == s.ns[k].code && s1.ns[k].rec == s.ns[k].rec
        && s1.ns[k].rows == s.ns[k].rows by {}
    assert forall |d: int| has_code(post, d) == has_code(s, d) && has_code(s1, d) == has_code(s, d)
        && rows_at(s1, d) == rows_at(s, d) by {}
    assert forall |k: int| s.ns.dom().contains(k) implies (#[trigger] installed(s1, k)) == installed(s, k) by {}
    assert forall |k: int| (#[trigger] is_source(s1, k)) == is_source(s, k) by {}
    let a = abs(s);
    assert(a[src] == view(s, src));
    assert(a[n] == view(s, n));
    assert(wf_ns(s, src));
    assert(wf_ns(s, n));
    if ts.contains(n) && !local(view(s, n)) {
        assert(!installed(s, n));
        assert(n != src);
        assert(mapped_from(s1, n, src) == (has_ipm(s, n) && pkg_dest(s.ns[n].act) == src)) by {
            if has_ipm(s, n) && pkg_dest(s.ns[n].act) == src {
                assert forall |y: int| #[trigger] is_source(s1, y) && pkg_dest(s1.ns[y].act) == pkg_dest(s1.ns[n].act)
                    implies y <= src by { assert(is_source(s, y)); }
            }
        }
        if repos && !has_ipm(s, n) { assert(false); }
        assert(rows_of(a, n) == rows_at(s, glo_dest(s.ns[n].act, n)));
    }
}

pub proof fn nsp_lemma_enable(s: S, src: int, ts: Set<int>, repos: bool)
    requires wf(s), premise_src(s, src), premise_uniq(s, src), premise_tgt(s, src, ts), premise_repos(s, ts, repos),
    ensures abs(enable_map(s, src, ts, repos)) == l1_enable(abs(s), src, ts, repos),
            wf(enable_map(s, src, ts, repos)),
{
    let post = enable_map(s, src, ts, repos);
    assert forall |n: int| s.ns.dom().contains(n) implies
        #[trigger] view(post, n) == enable_view(abs(s), src, ts, repos, n) && wf_ns(post, n) by {
        nsp_lemma_enable_one(s, src, ts, repos, n);
    }
    if s.ns.dom().contains(src) { nsp_lemma_enable_one(s, src, ts, repos, src); }
    assert(post.ns.dom() =~= s.ns.dom());
    assert(abs(post) =~= l1_enable(abs(s), src, ts, repos));
}

// ---------------------------------------------------------------- unmap: refinement

pub proof fn nsp_lemma_unmap_one(s: S, ts: Set<int>, repos_only: bool, n: int)
    requires wf(s), premise_info(s, ts), premise_rtn(s, ts, repos_only), s.ns.dom().contains(n),
    ensures unmap(s, ts, repos_only).ns.dom() =~= s.ns.dom(),
            view(unmap(s, ts, repos_only), n) == unmap_view(abs(s), ts, repos_only, n),
            wf_ns(unmap(s, ts, repos_only), n),
{
    let s1 = S { ns: Map::new(s.ns.dom(),
                              |k: int| if ts.contains(k) { unmap_one(s, k, repos_only) } else { s.ns[k] }) };
    let to = s.ns.dom().filter(|k: int| ts.contains(k) && s1.ns[k].cfg != s.ns[k].cfg);
    let post = unmap(s, ts, repos_only);
    assert(post == activate(s1, to));
    assert(s1.ns.dom() =~= s.ns.dom());
    nsp_lemma_activate(s1, to);
    assert forall |k: int| #[trigger] s.ns.dom().contains(k) implies
        post.ns[k].code == s.ns[k].code && post.ns[k].rec == s.ns[k].rec && post.ns[k].rows == s.ns[k].rows by {}
    assert forall |d: int| #[trigger] has_code(post, d) == has_code(s, d) by {}
    assert(wf_ns(s, n));
    let a = abs(s);
    assert(a[n] == view(s, n));
    if ts.contains(n) && listed(s, n) {
        let x = choose |x: int| mapped_from(s, n, x);
        assert(is_source(s, x));
        assert(has_ipm(s, n));
        assert(!installed(s, n));
        assert(mapped(view(s, n)));
    }
    if ts.contains(n) && mapped(view(s, n)) { assert(listed(s, n)); }
}

pub proof fn nsp_lemma_unmap(s: S, ts: Set<int>, repos_only: bool)
    requires wf(s), premise_info(s, ts), premise_rtn(s, ts, repos_only),
    ensures abs(unmap(s, ts, repos_only)) == l1_unmap(abs(s), ts, repos_only),
            wf(unmap(s, ts, repos_only)),
{
    let post = unmap(s, ts, repos_only);
    assert forall |n: int| s.ns.dom().contains(n) implies
        #[trigger] view(post, n) == unmap_view(abs(s), ts, repos_only, n) && wf_ns(post, n) by {
        nsp_lemma_unmap_one(s, ts, repos_only, n);
    }
    assert(post.ns.dom() =~= s.ns.dom());
    assert(abs(post) =~= l1_unmap(abs(s), ts, repos_only));
}

// ---------------------------------------------------------------- installer: refinement

pub proof fn nsp_lemma_installer(s: S, n: int, v: int)
    requires wf(s), s.ns.dom().contains(n), premise_inst(s, n),
    ensures l1_install(abs(s), abs(installer(s, n, v)), n, v),
            abs(installer(s, n, v)) == independent(abs(s), n, v, false),
            wf(installer(s, n, v)),
{
    let post = installer(s, n, v);
    assert(wf_ns(s, n));
    let c = s.ns[n].cfg;
    let m = Maps { pkg: Some(n), rtn: Some(n), ..c };
    assert(pkg_dest(m) == n);
    assert(post.ns.dom() =~= s.ns.dom());
    assert(post.ns[n] == Ns { cfg: m, act: m, rec: Some(v), code: true, ..s.ns[n] });
    assert forall |k: int| #[trigger] s.ns.dom().contains(k) && k != n implies post.ns[k] == s.ns[k] by {}
    // a namespace other than n that resolves %IPM to n did so before only if n held code (wf)
    assert forall |k: int| #[trigger] s.ns.dom().contains(k) && k != n implies view(post, k) == view(s, k) && wf_ns(post, k) by {
        assert(wf_ns(s, k));
        let d = pkg_dest(s.ns[k].act);
        if d == n && !s.ns[n].code {
            assert(s.ns[k].act.rtn is Some || s.ns[k].act.pkg is Some);
            assert(has_code(s, d));
        }
        if s.ns[k].act.repo is Some { assert(post.ns.dom().contains(s.ns[k].act.repo->0)); }
    }
    let a = abs(s);
    let b = abs(post);
    assert(view(post, n) == View { code: Some(n), ver: Some(v), ..a[n] });
    assert(b =~= independent(a, n, v, false));
    assert forall |k: int| #[trigger] post.ns.dom().contains(k) implies wf_ns(post, k) by {
        if k == n {
            assert(has_code(post, n));
            if s.ns[n].act.repo is Some { assert(post.ns.dom().contains(s.ns[n].act.repo->0)); }
        }
    }
}

// install zpm where GetMapInfo lists the namespace is refused: nothing changes.
pub proof fn nsp_lemma_install_zpm_refused(s: S, n: int, v: int)
    requires wf(s), s.ns.dom().contains(n), listed(s, n),
    ensures abs(install_zpm(s, n, v)) == abs(s), l1_install(abs(s), abs(install_zpm(s, n, v)), n, v),
{
    let x = choose |x: int| mapped_from(s, n, x);
    assert(is_source(s, x));
    assert(has_ipm(s, n) && !installed(s, n));
    assert(abs(s)[n] == view(s, n));
}

// ---------------------------------------------------------------- IPMVersion read: refinement

pub proof fn nsp_lemma_version_read(s: S, n: int)
    requires wf(s), s.ns.dom().contains(n), has_ipm(s, n), premise_own(s, n),
    ensures check_ipm_version(s, n) == version_in_use(abs(s), n),
{
    assert(abs(s)[n] == view(s, n));
}

// ---------------------------------------------------------------- community: refinement

pub proof fn nsp_lemma_community(s: S, src: int)
    requires wf(s), premise_src(s, src),
             premise_uniq(reset_repos(s, src), src),
             premise_tgt(reset_repos(s, src), src, s.ns.dom()),
             premise_repos(reset_repos(s, src), s.ns.dom(), true),
    ensures abs(community(s, src)) == l1_community(abs(s), src), wf(community(s, src)),
{
    let r = reset_repos(s, src);
    assert(glo_dest(s.ns[src].act, src) == src);
    assert(r.ns.dom() =~= s.ns.dom());
    assert forall |k: int| #[trigger] s.ns.dom().contains(k) implies
        r.ns[k].act == s.ns[k].act && r.ns[k].cfg == s.ns[k].cfg && r.ns[k].code == s.ns[k].code && r.ns[k].rec == s.ns[k].rec by {}
    assert forall |d: int| #[trigger] has_code(r, d) == has_code(s, d) by {}
    assert(wf(r)) by { assert forall |k: int| #[trigger] r.ns.dom().contains(k) implies wf_ns(r, k) by { assert(wf_ns(s, k)); } }
    assert(abs(r) =~= l1_reset(abs(s), src)) by {
        assert(abs(s)[src] == view(s, src));
    }
    nsp_lemma_enable(r, src, r.ns.dom(), true);
}

// ---------------------------------------------------------------- defect witnesses (proved)

pub open spec fn own_maps(n: int) -> Maps { Maps { pkg: Some(n), rtn: Some(n), repo: None } }
pub open spec fn ns_local(n: int, v: int) -> Ns { Ns { cfg: own_maps(n), act: own_maps(n), code: true, rec: Some(v), rows: true } }
pub open spec fn ns_empty() -> Ns { Ns { cfg: no_maps(), act: no_maps(), code: false, rec: None, rows: false } }
pub open spec fn ns_mapped(src: int, repo: bool) -> Ns {
    let m = Maps { pkg: Some(src), rtn: Some(src), repo: if repo { Some(src) } else { None } };
    Ns { cfg: m, act: m, code: false, rec: None, rows: false }
}

// USER = 1 (local install, version 10, registries), NCD = 2 (no IPM).
pub open spec fn w4() -> S { S { ns: Map::empty().insert(1int, ns_local(1, 10)).insert(2int, ns_empty()) } }

// F-NSP-4 (O:nc-nsp-N3-enable-map-repos, -rerun): `enable -map -repos -ns NCD` gives NCD the code but not the
// registries; L1 says it uses USER's registries. A second run maps them.
pub proof fn nsp_witness_repos_second_run()
    ensures ({
        let s = w4();
        let ts = Set::empty().insert(2int);
        let p1 = enable_map(s, 1, ts, true);
        let p2 = enable_map(p1, 1, ts, true);
        &&& wf(s) && premise_src(s, 1) && premise_uniq(s, 1) && premise_tgt(s, 1, ts) && !premise_repos(s, ts, true)
        &&& abs(p1)[2].code == Some(1int) && abs(p1)[2].repos == 2
        &&& l1_enable(abs(s), 1, ts, true)[2].repos == 1
        &&& abs(p1) != l1_enable(abs(s), 1, ts, true)
        &&& abs(p2)[2].repos == 1
    })
{
    let s = w4();
    let ts = Set::empty().insert(2int);
    assert(s.ns.dom() =~= Set::empty().insert(1int).insert(2int));
    assert(wf(s)) by { assert forall |n: int| #[trigger] s.ns.dom().contains(n) implies wf_ns(s, n) by {} }
    assert(premise_uniq(s, 1)) by {
        assert forall |y: int| #[trigger] is_source(s, y) && pkg_dest(s.ns[y].act) == 1 implies y <= 1 by {}
    }
    assert(!installed(s, 2) && !has_ipm(s, 2));
    let p1 = enable_map(s, 1, ts, true);
    nsp_lemma_activate(p1, Set::empty());
    let s1 = S { ns: Map::new(s.ns.dom(),
                              |k: int| if ts.contains(k) { map_code_one(s, 1, k) } else { s.ns[k] }) };
    assert(s1.ns[2].act == no_maps());
    assert(!mapped_from(s1, 2, 1));
    assert(!repo_mapped(s1, 1, 2));
    assert(p1.ns[2].act == Maps { pkg: Some(1int), rtn: Some(1int), repo: None });
    assert(p1.ns[1] == s.ns[1]);
    assert(abs(p1)[2] == view(p1, 2));
    let a = abs(s);
    assert(a[2] == view(s, 2) && a[1] == view(s, 1));
    assert(a.dom().contains(2int));
    assert(l1_enable(a, 1, ts, true)[2] == enable_view(a, 1, ts, true, 2));
    assert(!rows_of(a, 2));
    // second run: NCD now resolves %IPM to USER's database, so GetMapInfo lists it as mapped from USER
    assert(p1.ns.dom() =~= Set::empty().insert(1int).insert(2int));
    let q1 = S { ns: Map::new(p1.ns.dom(),
                              |k: int| if ts.contains(k) { map_code_one(p1, 1, k) } else { p1.ns[k] }) };
    assert(installed(p1, 1) && !installed(p1, 2) && has_ipm(p1, 2));
    assert(q1.ns[2] == p1.ns[2]);
    assert(q1.ns[1] == p1.ns[1]);
    assert(mapped_from(q1, 2, 1)) by {
        assert forall |y: int| #[trigger] is_source(q1, y) && pkg_dest(q1.ns[y].act) == pkg_dest(q1.ns[2].act) implies y <= 1 by {
            assert(q1.ns.dom().contains(y));
        }
    }
    assert(repo_mapped(q1, 1, 2));
    let p2 = enable_map(p1, 1, ts, true);
    assert(p2.ns[2].act.repo == Some(1int));
    assert(abs(p2)[2] == view(p2, 2));
}

// F-NSP-5 (O:nc-nsp-N7-installer-in-mapped, O:nc-nsp-N7-unmap-after-local-install): the installer in NCD, whose code
// and registries are mapped from USER, leaves the mappings: NCD gets a zpm record but still runs USER's code,
// which is neither a refusal nor an independent install. unmap then no longer lists NCD.
pub open spec fn w5() -> S { S { ns: Map::empty().insert(1int, ns_local(1, 10)).insert(2int, ns_mapped(1, true)) } }

pub proof fn nsp_witness_installer_in_mapped()
    ensures ({
        let s = w5();
        let p = installer(s, 2, 10);
        &&& wf(s) && !premise_inst(s, 2) && mapped(abs(s)[2])
        &&& abs(p)[2].code == Some(1int) && abs(p)[2].ver == Some(10int) && abs(p)[2].repos == 1
        &&& !l1_install(abs(s), abs(p), 2, 10)
        &&& !listed(p, 2) && unmap(p, Set::empty().insert(2int), false) == activate(p, Set::empty())
    })
{
    let s = w5();
    assert(s.ns.dom() =~= Set::empty().insert(1int).insert(2int));
    assert(wf(s)) by { assert forall |n: int| #[trigger] s.ns.dom().contains(n) implies wf_ns(s, n) by {} }
    let p = installer(s, 2, 10);
    assert(p.ns[2] == Ns { rec: Some(10int), ..s.ns[2] });
    assert(p.ns[1] == s.ns[1]);
    assert(p.ns.dom() =~= s.ns.dom());
    let a = abs(s);
    let b = abs(p);
    assert(a[2] == view(s, 2) && b[2] == view(p, 2));
    assert(b != a) by { assert(b[2] != a[2]); }
    assert(b != independent(a, 2, 10, true)) by { assert(independent(a, 2, 10, true)[2].code == Some(2int)); }
    assert(b != independent(a, 2, 10, false)) by { assert(independent(a, 2, 10, false)[2].code == Some(2int)); }
    assert(is_source(p, 2));
    assert(!listed(p, 2));
    let ts = Set::empty().insert(2int);
    let u1 = S { ns: Map::new(p.ns.dom(),
                              |k: int| if ts.contains(k) { unmap_one(p, k, false) } else { p.ns[k] }) };
    assert(u1 =~= p) by { assert(u1.ns =~= p.ns); }
    let to = p.ns.dom().filter(|k: int| ts.contains(k) && u1.ns[k].cfg != p.ns[k].cfg);
    assert(to =~= Set::empty());
}

// F-NSP-6 (O:nc-nsp-N6-unmap-mapped, -recheck-nca, -shell-after-unmap): unmap of NCE (code and registries mapped
// from USER) removes the package and repo mappings, but the %IPM.* routine mapping stays and NCE still runs USER's
// IPM. L1 says NCE runs no IPM code.
pub proof fn nsp_witness_unmap_leaves_routines()
    ensures ({
        let s = w5();
        let ts = Set::empty().insert(2int);
        let p = unmap(s, ts, false);
        &&& wf(s) && premise_info(s, ts) && !premise_rtn(s, ts, false)
        &&& p.ns[2].act == Maps { pkg: None, rtn: Some(1int), repo: None }
        &&& abs(p)[2].code == Some(1int) && abs(p)[2].repos == 2
        &&& l1_unmap(abs(s), ts, false)[2].code is None
        &&& abs(p) != l1_unmap(abs(s), ts, false)
    })
{
    let s = w5();
    let ts = Set::empty().insert(2int);
    assert(s.ns.dom() =~= Set::empty().insert(1int).insert(2int));
    assert(wf(s)) by { assert forall |n: int| #[trigger] s.ns.dom().contains(n) implies wf_ns(s, n) by {} }
    assert(mapped_from(s, 2, 1)) by {
        assert forall |y: int| #[trigger] is_source(s, y) && pkg_dest(s.ns[y].act) == pkg_dest(s.ns[2].act) implies y <= 1 by {
            assert(s.ns.dom().contains(y));
        }
    }
    assert(listed(s, 2));
    assert(premise_info(s, ts)) by {
        assert forall |t: int| #[trigger] ts.contains(t) && s.ns.dom().contains(t) && mapped(view(s, t)) implies listed(s, t) by {}
    }
    let p = unmap(s, ts, false);
    let s1 = S { ns: Map::new(s.ns.dom(),
                              |k: int| if ts.contains(k) { unmap_one(s, k, false) } else { s.ns[k] }) };
    assert(s1.ns[2].cfg == Maps { pkg: None, rtn: Some(1int), repo: None });
    let to = s.ns.dom().filter(|k: int| ts.contains(k) && s1.ns[k].cfg != s.ns[k].cfg);
    assert(to.contains(2));
    nsp_lemma_activate(s1, to);
    assert(p.ns[2].act == Maps { pkg: None, rtn: Some(1int), repo: None });
    assert(p.ns[1] == s.ns[1]);
    assert(abs(p)[2] == view(p, 2));
    let a = abs(s);
    assert(a[2] == view(s, 2));
    assert(l1_unmap(a, ts, false)[2] == unmap_view(a, ts, false, 2));
}

// F-NSP-2 (O:nc-nsp-N9-ipmversion-mapped-snap): in NCA (mapped from USER, which has IPM version 10) the IPMVersion
// check reads no version; the IPM NCA runs has version 10.
pub proof fn nsp_witness_version_empty_when_mapped()
    ensures ({
        let s = w5();
        &&& wf(s) && has_ipm(s, 2) && !premise_own(s, 2)
        &&& check_ipm_version(s, 2) is None && version_in_use(abs(s), 2) == Some(10int)
    })
{
    let s = w5();
    assert(s.ns.dom() =~= Set::empty().insert(1int).insert(2int));
    assert(wf(s)) by { assert forall |n: int| #[trigger] s.ns.dom().contains(n) implies wf_ns(s, n) by {} }
    let a = abs(s);
    assert(a[2] == view(s, 2) && a[1] == view(s, 1));
}

// Positive: the same enable with NCD already reached (as %IPM.Test.Utils.CreateNamespace leaves it) maps the
// registries in one run (why the BatchActivate tests pass, F-NSP-4).
pub proof fn nsp_witness_repos_one_run_when_premapped()
    ensures ({
        let s = S { ns: Map::empty().insert(1int, ns_local(1, 10)).insert(2int,
                        Ns { cfg: Maps { pkg: Some(1int), rtn: None, repo: None }, act: Maps { pkg: Some(1int), rtn: None, repo: None },
                             code: false, rec: None, rows: false }) };
        let ts = Set::empty().insert(2int);
        &&& premise_repos(s, ts, true) && premise_tgt(s, 1, ts)
        &&& abs(enable_map(s, 1, ts, true))[2].repos == 1
    })
{
    let s = S { ns: Map::empty().insert(1int, ns_local(1, 10)).insert(2int,
                    Ns { cfg: Maps { pkg: Some(1int), rtn: None, repo: None }, act: Maps { pkg: Some(1int), rtn: None, repo: None },
                         code: false, rec: None, rows: false }) };
    let ts = Set::empty().insert(2int);
    assert(s.ns.dom() =~= Set::empty().insert(1int).insert(2int));
    assert(wf(s)) by { assert forall |n: int| #[trigger] s.ns.dom().contains(n) implies wf_ns(s, n) by {} }
    assert(premise_uniq(s, 1)) by {
        assert forall |y: int| #[trigger] is_source(s, y) && pkg_dest(s.ns[y].act) == 1 implies y <= 1 by {}
    }
    assert(premise_repos(s, ts, true)) by {
        assert forall |t: int| #[trigger] ts.contains(t) && !installed(s, t) implies has_ipm(s, t) by {}
    }
    nsp_lemma_enable(s, 1, ts, true);
    let a = abs(s);
    assert(a[2] == view(s, 2) && a[1] == view(s, 1));
    assert(a.dom().contains(1int));
    assert(rows_of(a, 2) == false);
    assert(l1_enable(a, 1, ts, true)[2] == enable_view(a, 1, ts, true, 2));
}

// Positive: unmap of a namespace whose only %IPM mapping is the package mapping (as %IPM.Test.Utils.CreateNamespace
// leaves it) meets every unmap premise and removes its IPM.
pub open spec fn w6() -> S {
    S { ns: Map::empty().insert(1int, ns_local(1, 10)).insert(2int,
            Ns { cfg: Maps { pkg: Some(1int), rtn: None, repo: Some(1int) }, act: Maps { pkg: Some(1int), rtn: None, repo: Some(1int) },
                 code: false, rec: None, rows: false }) }
}

pub proof fn nsp_witness_unmap_package_only()
    ensures ({
        let s = w6();
        let ts = Set::empty().insert(2int);
        &&& wf(s) && premise_info(s, ts) && premise_rtn(s, ts, false)
        &&& abs(s)[2].code == Some(1int) && abs(unmap(s, ts, false))[2].code is None && abs(unmap(s, ts, false))[2].repos == 2
    })
{
    let s = w6();
    let ts = Set::empty().insert(2int);
    assert(s.ns.dom() =~= Set::empty().insert(1int).insert(2int));
    assert(wf(s)) by { assert forall |n: int| #[trigger] s.ns.dom().contains(n) implies wf_ns(s, n) by {} }
    assert(mapped_from(s, 2, 1)) by {
        assert forall |y: int| #[trigger] is_source(s, y) && pkg_dest(s.ns[y].act) == pkg_dest(s.ns[2].act) implies y <= 1 by {
            assert(s.ns.dom().contains(y));
        }
    }
    assert(premise_info(s, ts)) by {
        assert forall |t: int| #[trigger] ts.contains(t) && s.ns.dom().contains(t) && mapped(view(s, t)) implies listed(s, t) by {}
    }
    assert(premise_rtn(s, ts, false)) by {
        assert forall |t: int| #[trigger] ts.contains(t) && listed(s, t) implies s.ns[t].act.rtn is None by {}
    }
    nsp_lemma_unmap(s, ts, false);
    let a = abs(s);
    assert(a[2] == view(s, 2));
    assert(l1_unmap(a, ts, false)[2] == unmap_view(a, ts, false, 2));
}

// Positive: the installer in a namespace with no IPM meets INST and gives a local install (NSP-B1, B2).
pub proof fn nsp_witness_fresh_install()
    ensures ({
        let s = w4();
        let p = installer(s, 2, 10);
        &&& wf(s) && premise_inst(s, 2) && abs(s)[2].code is None
        &&& abs(p)[2].code == Some(2int) && local(abs(p)[2]) && abs(p)[1] == abs(s)[1]
    })
{
    let s = w4();
    assert(s.ns.dom() =~= Set::empty().insert(1int).insert(2int));
    assert(wf(s)) by { assert forall |n: int| #[trigger] s.ns.dom().contains(n) implies wf_ns(s, n) by {} }
    nsp_lemma_installer(s, 2, 10);
    let a = abs(s);
    assert(a[2] == view(s, 2) && a[1] == view(s, 1));
    assert(independent(a, 2, 10, false)[1] == a[1]);
}

// F-NSP-7 (O:nc-nsp-model-repos-premapped, O:nc-nsp-model-mapinfo): a second namespace with a zpm record that reaches
// USER's database and sorts after USER (USER-VERIFY = 3, left by IPM's verify phase) wins GetMapInfo's tie, so
// `enable -map -repos` from USER skips a target that already runs USER's code (premise UNIQ fails).
pub open spec fn w7() -> S {
    S { ns: Map::empty().insert(1int, ns_local(1, 10))
            .insert(2int, Ns { cfg: Maps { pkg: Some(1int), rtn: None, repo: None }, act: Maps { pkg: Some(1int), rtn: None, repo: None },
                               code: false, rec: None, rows: false })
            .insert(3int, Ns { rec: Some(10int), ..ns_mapped(1, false) }) }
}

pub proof fn nsp_witness_repos_tie_other_source()
    ensures ({
        let s = w7();
        let ts = Set::empty().insert(2int);
        &&& wf(s) && premise_src(s, 1) && premise_tgt(s, 1, ts) && premise_repos(s, ts, true) && !premise_uniq(s, 1)
        &&& mapped_from(s, 2, 3) && !mapped_from(s, 2, 1)
        &&& abs(enable_map(s, 1, ts, true))[2].repos == 2
        &&& l1_enable(abs(s), 1, ts, true)[2].repos == 1
    })
{
    let s = w7();
    let ts = Set::empty().insert(2int);
    assert(s.ns.dom() =~= Set::empty().insert(1int).insert(2int).insert(3int));
    assert(wf(s)) by { assert forall |n: int| #[trigger] s.ns.dom().contains(n) implies wf_ns(s, n) by {} }
    assert(is_source(s, 3) && is_source(s, 1) && !is_source(s, 2));
    assert(mapped_from(s, 2, 3)) by {
        assert forall |y: int| #[trigger] is_source(s, y) && pkg_dest(s.ns[y].act) == pkg_dest(s.ns[2].act) implies y <= 3 by {
            assert(s.ns.dom().contains(y));
        }
    }
    assert(!premise_uniq(s, 1)) by { assert(is_source(s, 3) && pkg_dest(s.ns[3].act) == 1); }
    assert(premise_repos(s, ts, true)) by {
        assert forall |t: int| #[trigger] ts.contains(t) && !installed(s, t) implies has_ipm(s, t) by {}
    }
    let s1 = S { ns: Map::new(s.ns.dom(), |k: int| if ts.contains(k) { map_code_one(s, 1, k) } else { s.ns[k] }) };
    assert(s1.ns[2].act == s.ns[2].act && s1.ns[1] == s.ns[1] && s1.ns[3] == s.ns[3]);
    assert(s1.ns.dom() =~= s.ns.dom());
    assert(is_source(s1, 3));
    assert(!mapped_from(s1, 2, 1)) by { assert(is_source(s1, 3) && pkg_dest(s1.ns[3].act) == pkg_dest(s1.ns[2].act)); }
    assert(!repo_mapped(s1, 1, 2));
    let p = enable_map(s, 1, ts, true);
    assert(p.ns[2].act.repo is None);
    assert(abs(p)[2] == view(p, 2));
    let a = abs(s);
    assert(a[2] == view(s, 2) && a[1] == view(s, 1));
    assert(a.dom().contains(1int));
    assert(l1_enable(a, 1, ts, true)[2] == enable_view(a, 1, ts, true, 2));
}

fn main() {}

} // verus!
