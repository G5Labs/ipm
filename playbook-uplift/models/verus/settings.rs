// L1/L2 models of IPM settings, default modifiers and registry selection (ipm 571e04a1), with refinement proofs
// and defect witnesses.
//
// L2 transcribes, with line anchors:
//   config: Main.cls Config #L2063-L2093 (statuses of the helpers are dropped: `do`), Repo/UniversalSettings.cls
//     PrintOne #L77-L85, ResetToDefault #L87-L110, UpdateOne #L112-L146, GetValue/GetDefaultValue #L150-L164 and
//     SetValue #L166-L182 (all in %SYS: one store per instance), SetTestReportFormat #L225-L232;
//   default modifiers: Main.cls #L1682-L1696 and ProcessDefaultModifiers #L1726-L1754, global ^%IPM.DefaultModifiers
//     (inc/IPM/Common.inc#L31), merged under the parsed command at #L1066-L1070; IRIS keeps ^% globals in one
//     database for every namespace (environment contract, observed: nc-repo-env-percent-global-shared);
//   registry selection: Repo/Http/Definition.cls PublishingServer #L144-L180 over the per-kind queries
//     Remote/Definition.cls#L117-L129 and Oras/Definition.cls#L143-L155 (first row of each, no ORDER BY: rows come
//     in ID order, nc-repo-model-publish-row-order), Lifecycle/Base.cls %Publish #L1596-L1611, Main.cls Search
//     #L2037-L2061 (every row), Repo/Manager.cls SearchRepositoriesForModule #L66-L170 (enabled rows; results
//     keyed versions(major,minor,patch,sub,repoID,name), read newest first, then by repoID).
// Registries are a Seq in ID order (index i is ID i+1); loops are recursive spec functions over prefixes.
//
// L1 is the intended contract from the REPO statements (and INST-B4 for the install tie-break).
// Named premises (each dropped once by a control):
//   PS   the key's default node holds the installer's seeded value          (F-REPO-4)
//   PNS  a single namespace uses IPM on the instance                         (F-REPO-5)
//   PH   some enabled remote or ORAS registry holds the publish flag          (F-REPO-3, DEC-REPO-1)
//   PU   at most one enabled registry holds the publish flag                  (REPO-B59)
//   PK   the flag holder is the first enabled registry of its kind            (model-derived, nc-repo-model-*)
//   PE   the registry that publish or search uses is enabled                  (F-REPO-7, DEC-REPO-2)
//   PT   equal top versions are offered in sort order                         (F-INST-4)
// PNS makes the per-namespace L1 for default modifiers vacuous (with one namespace there is nowhere else for a
// setting to count), so repo_refines_dm_set is not evidence for REPO-B106; it only shows the model agrees with that
// L1 in the one case where the two cannot differ. F-REPO-5 carries the witness (settings.TRUST.md S7).
// Both directions of "a command that worked reports success": config set / delete of a known key report success
// (repo_refines_config_set, repo_refines_config_delete); publish with no -repo finds a target exactly when a holder
// exists (l1_publish, both arms). The other direction for config (a failed set reports failure) is F-REPO-8.
// PREMISE premise_ps = PS
// PREMISE premise_pns = PNS
// PREMISE premise_ph = PH
// PREMISE premise_pu = PU
// PREMISE premise_pk = PK
// PREMISE premise_pe_named = PE
// PREMISE premise_pe_all = PE
// PREMISE premise_pt = PT
use vstd::prelude::*;

verus! {

// ================================================================ config

pub struct Node { pub val: Option<nat>, pub dflt: Option<nat> }   // ^IPM.settings(k) and ^IPM.settings(k,"default")

pub struct Cfg {
    pub nodes: Map<nat, Node>,   // ^IPM.settings in %SYS
    pub conf: Set<nat>,          // CONFIGURABLE (static)
    pub seed: Map<nat, nat>,     // defaults the installer seeds (preload/cls/IPM/Installer.cls#L114-L122) (static)
}

pub open spec fn empty_v() -> nat { 0 }      // ""
pub open spec fn k_trf() -> nat { 1 }        // TestReportFormat
pub open spec fn k_mroot() -> nat { 2 }      // ModuleRoot
pub open spec fn mroot_default() -> nat { 3 } // NormalizeDirectory(DataDirectory()_"ipm/")
pub open spec fn valid_trf(v: nat) -> bool { v == empty_v() || v == 10 || v == 11 || v == 12 }   // "", json, yaml, toon

pub enum St { Ok, Err }

pub open spec fn get_value(c: Cfg, k: nat) -> nat {
    if c.nodes.contains_key(k) && c.nodes[k].val is Some { c.nodes[k].val->0 } else { empty_v() }
}

pub open spec fn default_value(c: Cfg, k: nat) -> nat {
    if c.nodes.contains_key(k) && c.nodes[k].dflt is Some { c.nodes[k].dflt->0 } else { empty_v() }
}

// SetValue (#L166-L182): the first value stored for a key becomes its default when it has none.
pub open spec fn set_value(c: Cfg, k: nat, v: nat) -> Cfg {
    let old = if c.nodes.contains_key(k) { c.nodes[k] } else { Node { val: None, dflt: None } };
    let d = if old.dflt is None { Some(v) } else { old.dflt };
    Cfg { nodes: c.nodes.insert(k, Node { val: Some(v), dflt: d }), ..c }
}

// UpdateOne (#L112-L146). ModuleRoot's value is taken as already normalized and writable.
pub open spec fn update_one(c: Cfg, k: nat, v: nat) -> (Cfg, St) {
    if !c.conf.contains(k) { (c, St::Ok) }                                           // "Config key ... not found"
    else if k == k_trf() { if valid_trf(v) { (set_value(c, k, v), St::Ok) } else { (c, St::Err) } }
    else if k == k_mroot() && v == empty_v() { (c, St::Err) }
    else { (set_value(c, k, v), St::Ok) }
}

// ResetToDefault (#L87-L110).
pub open spec fn reset(c: Cfg, k: nat) -> Cfg {
    if !c.conf.contains(k) { c }
    else {
        let dv = if k == k_trf() { empty_v() } else if k == k_mroot() { mroot_default() } else { default_value(c, k) };
        set_value(c, k, dv)
    }
}

// The shell (Main.cls Config #L2063-L2093). `ns` is the caller's namespace; the helpers switch to %SYS.
// Every branch reports success to the shell: the helpers' statuses are dropped.
pub open spec fn config_set(c: Cfg, ns: nat, k: nat, v: nat) -> (Cfg, St) {
    if v == empty_v() { (c, St::Ok) } else { (update_one(c, k, v).0, St::Ok) }
}
pub open spec fn config_get(c: Cfg, ns: nat, k: nat) -> Option<nat> {
    if c.conf.contains(k) { Some(get_value(c, k)) } else { None }                    // None: "not found" printed
}
pub open spec fn config_delete(c: Cfg, ns: nat, k: nat) -> (Cfg, St) { (reset(c, k), St::Ok) }

// ---------------------------------------------------------------- L1 for config

pub open spec fn factory(c: Cfg, k: nat) -> nat {
    if k == k_trf() { empty_v() } else if k == k_mroot() { mroot_default() }
    else if c.seed.contains_key(k) { c.seed[k] } else { empty_v() }
}

pub open spec fn valid_set(c: Cfg, k: nat, v: nat) -> bool {
    c.conf.contains(k) && v != empty_v() && (k == k_trf() ==> valid_trf(v))
}

// What `config get` shows for each key.
pub open spec fn cfg_abs(c: Cfg) -> Map<nat, nat> { Map::new(c.conf, |k: nat| get_value(c, k)) }

pub open spec fn l1_cfg_set(a: Map<nat, nat>, conf: Set<nat>, k: nat, v: nat) -> Map<nat, nat> {
    if conf.contains(k) { a.insert(k, v) } else { a }
}

// REPO-B101 (DEC-REPO-4 open): the command succeeds exactly when the key is known and the value valid.
pub open spec fn l1_config_status(c: Cfg, k: nat, v: nat, st: St) -> bool { (st == St::Ok) == valid_set(c, k, v) }

// PS: the key's default node holds the seeded value (or the key resets to a computed default).
pub open spec fn premise_ps(c: Cfg, k: nat) -> bool {
    k == k_trf() || k == k_mroot() || (c.seed.contains_key(k) && c.nodes.contains_key(k) && c.nodes[k].dflt == Some(c.seed[k]))
}

// REPO-B98: config set changes one setting, reports success, and config get reads it back.
pub proof fn repo_refines_config_set(c: Cfg, ns: nat, k: nat, v: nat)
    requires valid_set(c, k, v)
    ensures
        cfg_abs(config_set(c, ns, k, v).0) == l1_cfg_set(cfg_abs(c), c.conf, k, v),
        config_get(config_set(c, ns, k, v).0, ns, k) == Some(v),
        l1_config_status(c, k, v, config_set(c, ns, k, v).1),
{
    let c2 = config_set(c, ns, k, v).0;
    assert(cfg_abs(c2) =~= l1_cfg_set(cfg_abs(c), c.conf, k, v));
}

// REPO-B97: settings are shared by every namespace (one store in %SYS).
pub proof fn repo_settings_shared(c: Cfg, ns1: nat, ns2: nat, k: nat, v: nat)
    requires valid_set(c, k, v)
    ensures config_get(config_set(c, ns1, k, v).0, ns2, k) == Some(v)
{}

// SetValue never changes a default node once it exists, so PS is kept by every command.
pub proof fn repo_ps_kept(c: Cfg, k: nat, k2: nat, v: nat)
    requires premise_ps(c, k)
    ensures premise_ps(set_value(c, k2, v), k), premise_ps(update_one(c, k2, v).0, k), premise_ps(reset(c, k2), k)
{}

// REPO-B99 under PS: config delete puts the setting back to its factory default.
pub proof fn repo_refines_config_delete(c: Cfg, ns: nat, k: nat)
    requires c.conf.contains(k), premise_ps(c, k)
    ensures
        cfg_abs(config_delete(c, ns, k).0) == cfg_abs(c).insert(k, factory(c, k)),
        config_delete(c, ns, k).1 == St::Ok,
{
    let c2 = config_delete(c, ns, k).0;
    assert(cfg_abs(c2) =~= cfg_abs(c).insert(k, factory(c, k)));
}

// REPO-B101 (DEC-REPO-4 open): an unknown name or a bad value leaves the settings unchanged.
pub proof fn repo_config_errors_unchanged(c: Cfg, ns: nat, k: nat, v: nat)
    requires !c.conf.contains(k) || (k == k_trf() && !valid_trf(v))
    ensures config_set(c, ns, k, v).0 == c, config_delete(c, ns, k).0 == (if c.conf.contains(k) { reset(c, k) } else { c })
{}

// F-REPO-4 (IRIS: nc-repo-6-config-delete). HistoryRetain (key 5) has no seeded default. set 30, set 1, delete:
// the value is 30 again, not the factory default.
pub open spec fn cfg0() -> Cfg { Cfg { nodes: Map::empty(), conf: set![1nat, 2nat, 5nat], seed: Map::empty() } }

pub proof fn repo_witness_delete_restores_first_value()
    ensures ({
        let c0 = cfg0();
        let c1 = config_set(c0, 0, 5, 30).0;
        let c2 = config_set(c1, 0, 5, 1).0;
        let c3 = config_delete(c2, 0, 5).0;
        &&& !premise_ps(c2, 5)
        &&& config_get(c2, 0, 5) == Some(1nat) && config_get(c3, 0, 5) == Some(30nat) && factory(c2, 5) == empty_v()
        &&& cfg_abs(c3) != cfg_abs(c2).insert(5, factory(c2, 5))
    })
{
    let c3 = config_delete(config_set(config_set(cfg0(), 0, 5, 30).0, 0, 5, 1).0, 0, 5).0;
    assert(cfg_abs(c3)[5] == 30);
}

// F-REPO-8 (IRIS: nc-repo-8-config-errors): an unknown key and a bad TestReportFormat both report success;
// the intended contract (REPO-B101) says the command fails.
pub proof fn repo_witness_config_errors_report_ok()
    ensures
        config_set(cfg0(), 0, 9, 1).1 == St::Ok && !cfg0().conf.contains(9),
        config_set(cfg0(), 0, k_trf(), 99).1 == St::Ok && !valid_trf(99) && config_get(config_set(cfg0(), 0, k_trf(), 99).0, 0, k_trf()) == Some(empty_v()),
        !l1_config_status(cfg0(), 9, 1, config_set(cfg0(), 0, 9, 1).1),
        !l1_config_status(cfg0(), k_trf(), 99, config_set(cfg0(), 0, k_trf(), 99).1),
{}

// ================================================================ default modifiers

pub struct Dm { pub dbs: Map<nat, Map<nat, nat>> }   // ^%IPM.DefaultModifiers in each database

// IRIS: a ^% global lives in the system database for every namespace (nc-repo-env-percent-global-shared).
pub open spec fn pct_db(ns: nat) -> nat { 0 }

pub open spec fn dm_store(s: Dm, ns: nat) -> Map<nat, nat> {
    if s.dbs.contains_key(pct_db(ns)) { s.dbs[pct_db(ns)] } else { Map::empty() }
}
// -set: merge $$$ZPMDefaultModifiers = pDefaultArray (#L1691-L1693): new values win, old ones stay.
pub open spec fn dm_set(s: Dm, ns: nat, d: Map<nat, nat>) -> Dm { Dm { dbs: s.dbs.insert(pct_db(ns), dm_store(s, ns).union_prefer_right(d)) } }
pub open spec fn dm_get(s: Dm, ns: nat) -> Map<nat, nat> { dm_store(s, ns) }
// -delete: kill $$$ZPMDefaultModifiers (#L1695).
pub open spec fn dm_delete(s: Dm, ns: nat) -> Dm { Dm { dbs: s.dbs.insert(pct_db(ns), Map::empty()) } }
// #L1066-L1070: defaults merged first, then the parsed command.
pub open spec fn effective(s: Dm, ns: nat, cmd: Map<nat, nat>) -> Map<nat, nat> { dm_get(s, ns).union_prefer_right(cmd) }

// L1: one list per namespace (REPO-B106).
pub open spec fn dm_l1_set(a: Map<nat, Map<nat, nat>>, ns: nat, d: Map<nat, nat>) -> Map<nat, Map<nat, nat>> {
    a.insert(ns, (if a.contains_key(ns) { a[ns] } else { Map::empty() }).union_prefer_right(d))
}
pub open spec fn dm_abs(s: Dm, nss: Set<nat>) -> Map<nat, Map<nat, nat>> { Map::new(nss, |ns: nat| dm_store(s, ns)) }

// PNS: one namespace uses IPM.
pub open spec fn premise_pns(nss: Set<nat>, ns: nat) -> bool { forall|x: nat| #[trigger] nss.contains(x) ==> x == ns }

// REPO-B103/B104: -set adds without dropping, -get reads, -delete clears; REPO-B105: a typed option wins.
pub proof fn repo_dm_laws(s: Dm, ns: nat, d: Map<nat, nat>, cmd: Map<nat, nat>, k: nat)
    ensures
        dm_get(s, ns).contains_key(k) ==> dm_get(dm_set(s, ns, d), ns).contains_key(k),
        d.contains_key(k) ==> dm_get(dm_set(s, ns, d), ns)[k] == d[k],
        !d.contains_key(k) && dm_get(s, ns).contains_key(k) ==> dm_get(dm_set(s, ns, d), ns)[k] == dm_get(s, ns)[k],
        dm_get(dm_delete(s, ns), ns) == Map::<nat, nat>::empty(),
        cmd.contains_key(k) ==> effective(s, ns, cmd)[k] == cmd[k],
{}

// REPO-B106 under PNS: -set changes the list of the acting namespace only.
pub proof fn repo_refines_dm_set(s: Dm, nss: Set<nat>, ns: nat, d: Map<nat, nat>)
    requires nss.contains(ns), premise_pns(nss, ns)
    ensures dm_abs(dm_set(s, ns, d), nss) == dm_l1_set(dm_abs(s, nss), ns, d)
{
    assert(dm_abs(dm_set(s, ns, d), nss) =~= dm_l1_set(dm_abs(s, nss), ns, d));
}

// F-REPO-5 (IRIS: nc-repo-7-default-modifiers-namespaces): set in USER (1), listed in NCMAP (2).
pub proof fn repo_witness_dm_shared()
    ensures ({
        let s0 = Dm { dbs: Map::empty() };
        let d = Map::<nat, nat>::empty().insert(7, 1);
        let nss = set![1nat, 2nat];
        &&& !premise_pns(nss, 1)
        &&& dm_get(dm_set(s0, 1, d), 2).contains_key(7) && !dm_get(s0, 2).contains_key(7)
        &&& dm_abs(dm_set(s0, 1, d), nss) != dm_l1_set(dm_abs(s0, nss), 1, d)
    })
{
    let s0 = Dm { dbs: Map::empty() };
    let d = Map::<nat, nat>::empty().insert(7, 1);
    let nss = set![1nat, 2nat];
    assert(nss.contains(2));
    assert(dm_abs(dm_set(s0, 1, d), nss)[2].contains_key(7));
    assert(!dm_l1_set(dm_abs(s0, nss), 1, d)[2].contains_key(7));
}

// ================================================================ registries

pub enum Kind { Remote, Oras, Folder }

pub struct Reg {
    pub name: nat, pub kind: Kind, pub enabled: bool,
    pub deploy: bool,                 // DeploymentEnabled (the publish flag)
    pub token: bool, pub userpw: bool,
    pub sort_override: Option<nat>,   // OverriddenSortOrder
    pub offer: Option<nat>,           // newest version of the wanted module it offers (install lookup)
}

pub open spec fn wf_regs(rs: Seq<Reg>) -> bool {
    forall|i: int, j: int| #![trigger rs[i], rs[j]] 0 <= i < rs.len() && 0 <= j < rs.len() && i != j ==> rs[i].name != rs[j].name
}

// PublishingServer per kind: CASE ... END PublishOrder.
pub open spec fn publish_order(r: Reg) -> nat { if r.deploy { 1 } else if r.token { 2 } else if r.userpw { 3 } else { 999 } }

// The first row of the per-kind query: the first enabled registry of that kind in ID order.
pub open spec fn first_of(rs: Seq<Reg>, k: Kind) -> Option<int>
    decreases rs.len()
{
    if rs.len() == 0 { None } else {
        let f = first_of(rs.drop_last(), k);
        if f is Some { f }
        else if rs.last().kind == k && rs.last().enabled { Some(rs.len() - 1) } else { None }
    }
}

// qHandle(PublishOrder_"/"_Name), read with $order: smaller order first, then name.
pub open spec fn key_lt(a: Reg, b: Reg) -> bool {
    publish_order(a) < publish_order(b) || (publish_order(a) == publish_order(b) && a.name < b.name)
}

pub open spec fn publishing_server(rs: Seq<Reg>) -> Option<int> {
    let r = first_of(rs, Kind::Remote);
    let o = first_of(rs, Kind::Oras);
    if r is None { o } else if o is None { r }
    else if key_lt(rs[o->0], rs[r->0]) { o } else { r }
}

pub open spec fn find_name(rs: Seq<Reg>, n: nat) -> Option<int>
    decreases rs.len()
{
    if rs.len() == 0 { None } else if rs.last().name == n { Some(rs.len() - 1) } else { find_name(rs.drop_last(), n) }
}

// %Publish (Base.cls#L1596-L1625): a named registry is opened with no Enabled check; else PublishingServer.
// A folder registry has no publish service (#L1619-L1623).
pub open spec fn publish_target(rs: Seq<Reg>, named: Option<nat>) -> Option<int> {
    let t = match named { Some(n) => find_name(rs, n), None => publishing_server(rs) };
    match t { Some(i) => if rs[i].kind == Kind::Folder { None } else { Some(i) }, None => None }
}

// search (Main.cls#L2047): every row of %IPM_Repo.Definition.
pub open spec fn search_lists(rs: Seq<Reg>, i: int) -> bool { 0 <= i < rs.len() }

// SearchRepositoriesForModule: enabled registries; newest version first, then the lowest repoID.
pub open spec fn install_pick(rs: Seq<Reg>) -> Option<int>
    decreases rs.len()
{
    if rs.len() == 0 { None } else {
        let b = install_pick(rs.drop_last());
        let i = rs.len() - 1;
        if !rs[i].enabled || rs[i].offer is None { b }
        else if b is None || rs[i].offer->0 > rs[b->0].offer->0 { Some(i) } else { b }
    }
}

// ---------------------------------------------------------------- L1 for registries

// Help (Main.cls#L56-L57): publish goes to the registry for which deployment is enabled.
pub open spec fn holder(rs: Seq<Reg>, i: int) -> bool { 0 <= i < rs.len() && rs[i].enabled && rs[i].deploy && rs[i].kind != Kind::Folder }

pub open spec fn l1_publish(rs: Seq<Reg>, named: Option<nat>, pick: Option<int>) -> bool {
    match named {
        Some(n) => match pick {
            Some(i) => 0 <= i < rs.len() && rs[i].name == n && rs[i].enabled && rs[i].kind != Kind::Folder,
            None => !exists|i: int| #![trigger rs[i]] 0 <= i < rs.len() && rs[i].name == n && rs[i].enabled && rs[i].kind != Kind::Folder,
        },
        None => match pick {
            Some(i) => holder(rs, i),
            None => !exists|i: int| #[trigger] holder(rs, i),
        },
    }
}

pub open spec fn l1_search(rs: Seq<Reg>, i: int) -> bool { 0 <= i < rs.len() && rs[i].enabled }

// INST-B4: equal versions go to the registry with the lowest sort order (Definition.cls SortOrder #L87-L94).
pub open spec fn sort_order(rs: Seq<Reg>, i: int) -> nat {
    match rs[i].sort_override { Some(s) => s, None => if rs[i].kind == Kind::Folder { 2 } else { (3 + i) as nat } }
}

pub open spec fn better(rs: Seq<Reg>, i: int, j: int) -> bool {   // i is preferred to j
    rs[i].offer->0 > rs[j].offer->0 || (rs[i].offer->0 == rs[j].offer->0 && sort_order(rs, i) < sort_order(rs, j))
}

pub open spec fn l1_install(rs: Seq<Reg>, pick: Option<int>) -> bool {
    match pick {
        Some(i) => 0 <= i < rs.len() && rs[i].enabled && rs[i].offer is Some &&
            forall|j: int| #![trigger rs[j]] 0 <= j < rs.len() && rs[j].enabled && rs[j].offer is Some && j != i ==> !better(rs, j, i),
        None => !exists|j: int| #![trigger rs[j]] 0 <= j < rs.len() && rs[j].enabled && rs[j].offer is Some,
    }
}

// ---------------------------------------------------------------- premises

pub open spec fn premise_ph(rs: Seq<Reg>) -> bool { exists|i: int| #[trigger] holder(rs, i) }
pub open spec fn premise_pu(rs: Seq<Reg>) -> bool { forall|i: int, j: int| holder(rs, i) && holder(rs, j) ==> i == j }
pub open spec fn premise_pk(rs: Seq<Reg>) -> bool {
    forall|i: int| #[trigger] holder(rs, i) ==> first_of(rs, rs[i].kind) == Some(i)
}
pub open spec fn premise_pe_named(rs: Seq<Reg>, n: nat) -> bool {
    forall|i: int| #![trigger rs[i]] 0 <= i < rs.len() && rs[i].name == n ==> rs[i].enabled
}
pub open spec fn premise_pe_all(rs: Seq<Reg>) -> bool { forall|i: int| #![trigger rs[i]] 0 <= i < rs.len() ==> rs[i].enabled }
pub open spec fn premise_pt(rs: Seq<Reg>) -> bool {
    forall|i: int, j: int| #![trigger rs[i], rs[j]] 0 <= i < j < rs.len() && rs[i].enabled && rs[j].enabled && rs[i].offer is Some
        && rs[j].offer is Some && rs[i].offer->0 == rs[j].offer->0 ==> sort_order(rs, i) <= sort_order(rs, j)
}

// ---------------------------------------------------------------- loop lemmas

proof fn repo_first_of(rs: Seq<Reg>, k: Kind)
    ensures
        first_of(rs, k) is Some ==> ({ let i = first_of(rs, k)->0; 0 <= i < rs.len() && rs[i].kind == k && rs[i].enabled
            && forall|j: int| #![trigger rs[j]] 0 <= j < i ==> !(rs[j].kind == k && rs[j].enabled) }),
        first_of(rs, k) is None ==> forall|j: int| #![trigger rs[j]] 0 <= j < rs.len() ==> !(rs[j].kind == k && rs[j].enabled),
    decreases rs.len()
{
    if rs.len() > 0 {
        let pre = rs.drop_last();
        repo_first_of(pre, k);
        assert forall|j: int| 0 <= j < pre.len() implies #[trigger] rs[j] == pre[j] by {}
    }
}

proof fn repo_find_name(rs: Seq<Reg>, n: nat)
    ensures
        find_name(rs, n) is Some ==> ({ let i = find_name(rs, n)->0; 0 <= i < rs.len() && rs[i].name == n }),
        find_name(rs, n) is None ==> forall|j: int| #![trigger rs[j]] 0 <= j < rs.len() ==> rs[j].name != n,
    decreases rs.len()
{
    if rs.len() > 0 {
        let pre = rs.drop_last();
        repo_find_name(pre, n);
        assert forall|j: int| 0 <= j < pre.len() implies #[trigger] rs[j] == pre[j] by {}
    }
}

proof fn repo_install_pick(rs: Seq<Reg>)
    ensures
        install_pick(rs) is Some ==> ({ let i = install_pick(rs)->0; 0 <= i < rs.len() && rs[i].enabled && rs[i].offer is Some
            && forall|j: int| #![trigger rs[j]] 0 <= j < rs.len() && rs[j].enabled && rs[j].offer is Some ==>
                   rs[j].offer->0 < rs[i].offer->0 || (rs[j].offer->0 == rs[i].offer->0 && i <= j) }),
        install_pick(rs) is None ==> !exists|j: int| #![trigger rs[j]] 0 <= j < rs.len() && rs[j].enabled && rs[j].offer is Some,
    decreases rs.len()
{
    if rs.len() > 0 {
        let pre = rs.drop_last();
        repo_install_pick(pre);
        assert forall|j: int| 0 <= j < pre.len() implies #[trigger] rs[j] == pre[j] by {}
    }
}

// ---------------------------------------------------------------- refinement

// REPO-B57/B58: with no -repo, publish never picks a registry that is not enabled, nor a folder registry (no premise).
pub proof fn repo_publish_default_enabled(rs: Seq<Reg>)
    ensures publish_target(rs, None) is Some ==> ({ let i = publish_target(rs, None)->0;
        0 <= i < rs.len() && rs[i].enabled && rs[i].kind != Kind::Folder })
{
    repo_first_of(rs, Kind::Remote);
    repo_first_of(rs, Kind::Oras);
}

// REPO-B53: publish to a name that no registry has fails (no premise).
pub proof fn repo_publish_named_missing(rs: Seq<Reg>, n: nat)
    requires forall|i: int| #![trigger rs[i]] 0 <= i < rs.len() ==> rs[i].name != n
    ensures publish_target(rs, Some(n)) is None
{
    repo_find_name(rs, n);
}

// REPO-B55/B56 under PH, PU and PK: with no -repo, publish uses the registry that holds the publish flag.
pub proof fn repo_refines_publish_default(rs: Seq<Reg>)
    requires premise_ph(rs), premise_pu(rs), premise_pk(rs)
    ensures l1_publish(rs, None, publish_target(rs, None))
{
    let h = choose|i: int| #[trigger] holder(rs, i);
    assert(first_of(rs, rs[h].kind) == Some(h));
    repo_first_of(rs, Kind::Remote);
    repo_first_of(rs, Kind::Oras);
    let r = first_of(rs, Kind::Remote);
    let o = first_of(rs, Kind::Oras);
    if r is Some && r->0 != h { assert(!holder(rs, r->0)); }
    if o is Some && o->0 != h { assert(!holder(rs, o->0)); }
}

// REPO-B52/B54 under PE: publish -r R uses R.
pub proof fn repo_refines_publish_named(rs: Seq<Reg>, n: nat)
    requires wf_regs(rs), premise_pe_named(rs, n)
    ensures l1_publish(rs, Some(n), publish_target(rs, Some(n)))
{
    repo_find_name(rs, n);
    if find_name(rs, n) is Some {
        let i = find_name(rs, n)->0;
        assert forall|j: int| #![trigger rs[j]] 0 <= j < rs.len() && rs[j].name == n implies j == i by {}
    }
}

// REPO-B82 under PE: search lists the enabled registries.
pub proof fn repo_refines_search(rs: Seq<Reg>)
    requires premise_pe_all(rs)
    ensures forall|i: int| #[trigger] search_lists(rs, i) == l1_search(rs, i)
{}

// REPO-B12: install never takes a module from a registry that is not enabled (no premise).
pub proof fn repo_install_enabled_only(rs: Seq<Reg>)
    ensures install_pick(rs) is Some ==> rs[install_pick(rs)->0].enabled && rs[install_pick(rs)->0].offer is Some
{
    repo_install_pick(rs);
}

// INST-B4 under PT: equal versions go to the registry with the lowest sort order.
pub proof fn repo_refines_install_pick(rs: Seq<Reg>)
    requires premise_pt(rs)
    ensures l1_install(rs, install_pick(rs))
{
    repo_install_pick(rs);
    if install_pick(rs) is Some {
        let i = install_pick(rs)->0;
        assert forall|j: int| #![trigger rs[j]] 0 <= j < rs.len() && rs[j].enabled && rs[j].offer is Some && j != i
            implies !better(rs, j, i) by {
            if rs[j].offer->0 == rs[i].offer->0 { assert(i < j); }
        }
    }
}

// ---------------------------------------------------------------- witnesses

pub open spec fn reg(name: nat, kind: Kind, enabled: bool, deploy: bool, token: bool) -> Reg {
    Reg { name: name, kind: kind, enabled: enabled, deploy: deploy, token: token, userpw: false, sort_override: None, offer: None }
}

// F-REPO-3 (IRIS: nc-repo-1-publish-no-flag): no registry holds the flag; a token-only remote registry is used.
pub proof fn repo_witness_publish_fallback()
    ensures ({
        let rs = seq![reg(1, Kind::Remote, true, false, true)];
        &&& !premise_ph(rs) && publish_target(rs, None) == Some(0int)
        &&& !l1_publish(rs, None, publish_target(rs, None))
    })
{
    let rs = seq![reg(1, Kind::Remote, true, false, true)];
    reveal_with_fuel(first_of, 2);
    assert(!holder(rs, 0));
    assert(!premise_ph(rs)) by { assert forall|i: int| !#[trigger] holder(rs, i) by { if 0 <= i < 1 { assert(i == 0); } } }
}

// Model-derived (nc-repo-model-publish-row-order): remote registry A (ID 1, no flag, no credentials) was made before
// remote registry B (ID 2) took the publish flag; publish with no -repo goes to A.
pub proof fn repo_witness_publish_flag_ignored()
    ensures ({
        let rs = seq![reg(1, Kind::Remote, true, false, false), reg(2, Kind::Remote, true, true, false)];
        &&& premise_ph(rs) && premise_pu(rs) && !premise_pk(rs)
        &&& publish_target(rs, None) == Some(0int) && holder(rs, 1)
        &&& !l1_publish(rs, None, publish_target(rs, None))
    })
{
    let rs = seq![reg(1, Kind::Remote, true, false, false), reg(2, Kind::Remote, true, true, false)];
    reveal_with_fuel(first_of, 3);
    assert(holder(rs, 1));
    assert(!holder(rs, 0));
    assert(premise_pu(rs)) by { assert forall|i: int, j: int| holder(rs, i) && holder(rs, j) implies i == j by { assert(i == 1 && j == 1); } }
    assert(first_of(rs, Kind::Remote) == Some(0int));
}

// F-REPO-7 (IRIS: nc-repo-2-disabled-registry): publish -r to a disabled ORAS registry goes ahead; search lists it.
pub proof fn repo_witness_disabled_registry_used()
    ensures ({
        let rs = seq![reg(1, Kind::Oras, false, false, false)];
        &&& !premise_pe_named(rs, 1) && !premise_pe_all(rs)
        &&& publish_target(rs, Some(1)) == Some(0int) && !l1_publish(rs, Some(1), publish_target(rs, Some(1)))
        &&& search_lists(rs, 0) && !l1_search(rs, 0)
    })
{
    let rs = seq![reg(1, Kind::Oras, false, false, false)];
    reveal_with_fuel(find_name, 2);
}

// F-INST-4 (IRIS: nc-inst-10b-registry-tie-break): r2 (ID 1, sort order 2) and r1 (ID 2, sort order 1) both offer
// version 1; install takes r2.
pub proof fn repo_witness_install_tie_break()
    ensures ({
        let rs = seq![Reg { sort_override: Some(2), offer: Some(1), ..reg(2, Kind::Remote, true, false, false) },
                      Reg { sort_override: Some(1), offer: Some(1), ..reg(1, Kind::Remote, true, false, false) }];
        &&& !premise_pt(rs) && install_pick(rs) == Some(0int) && better(rs, 1, 0)
        &&& !l1_install(rs, install_pick(rs))
    })
{
    let rs = seq![Reg { sort_override: Some(2), offer: Some(1), ..reg(2, Kind::Remote, true, false, false) },
                  Reg { sort_override: Some(1), offer: Some(1), ..reg(1, Kind::Remote, true, false, false) }];
    reveal_with_fuel(install_pick, 3);
    assert(rs[1].enabled && rs[1].offer is Some);
}

fn main() {}

} // verus!
