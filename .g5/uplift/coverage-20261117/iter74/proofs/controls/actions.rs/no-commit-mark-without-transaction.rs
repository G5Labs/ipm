// Iteration74: which commands write History rows, when each row is created, and what install, update and uninstall
// write into it (Action, Package, Version, Success, Committed, TimeEnd, Phases). IPM 571e04a1.
//
// Shared models, not copies:
//   history.rs   (consolidated/verus): Row, Action and `finalize` (AbstractHistory.cls:127-134, Committed || ISOK).
//   uninstall.rs (playbook-uplift): Clean, its dependency recursion and the rows it appends (storage_uninstall,
//                clean_cmd, dep_step). This file adds where those rows are created and what they hold.
// New here: Main.Install / Main.Update / Main.Reinstall / Main.Uninstall argument checks against the History init
// sites, the LoadNewModule SetCommitted sites under these callers, the logged phase list (GetCompletePhases and
// ExecutePhases), and the dispatch of phase commands.
//
// Domain: install and update over arbitrary inputs (names, versions as (text, rank), modes, failure points);
// uninstall over every uninstall.rs world whose target is not IPM itself (self-uninstall finalizes early,
// Storage/Module.cls:630-643, and is not modelled). `update -path` (Main.Load with the update log) and
// `uninstall -all` are not modelled.
#[path="../../../../../../Verus_verification/consolidated/verus/history.rs"]
pub mod history;
#[path="../../../../../../../playbook-uplift/models/verus/uninstall.rs"]
pub mod unin;
pub use history as h;
use vstd::prelude::*;

verus! {

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

// ================================================================ phases

// %IPM.DataType.PhaseName VALUELIST (DataType/PhaseName.cls:13), the order Lifecycle/Base.cls PHASES uses.
pub enum Ph { Clean, Initialize, Reload, Star, Validate, ExportData, Compile, Activate, Document, MakeDeployed, Test,
    Package, Verify, Publish, Configure, Unconfigure, ApplyUpdateSteps, Sync }

pub open spec fn phase_order() -> Seq<Ph> {
    seq![Ph::Clean, Ph::Initialize, Ph::Reload, Ph::Star, Ph::Validate, Ph::ExportData, Ph::Compile, Ph::Activate,
        Ph::Document, Ph::MakeDeployed, Ph::Test, Ph::Package, Ph::Verify, Ph::Publish, Ph::Configure, Ph::Unconfigure,
        Ph::ApplyUpdateSteps, Ph::Sync]
}

// GetCompletePhasesForOne (Lifecycle/Base.cls:165-189).
pub open spec fn chain(t: Ph) -> Seq<Ph> {
    let act = seq![Ph::Initialize, Ph::Reload, Ph::Star, Ph::Validate, Ph::Compile, Ph::Activate];
    match t {
        Ph::Clean => seq![Ph::Clean],
        Ph::Initialize => seq![Ph::Initialize],
        Ph::Reload => seq![Ph::Initialize, Ph::Reload, Ph::Star],
        Ph::Validate => seq![Ph::Initialize, Ph::Reload, Ph::Star, Ph::Validate],
        Ph::ExportData => seq![Ph::ExportData],
        Ph::Compile => seq![Ph::Initialize, Ph::Reload, Ph::Star, Ph::Validate, Ph::Compile],
        Ph::Activate => act,
        Ph::Document => seq![Ph::Document],
        Ph::MakeDeployed => seq![Ph::MakeDeployed],
        Ph::Test => act.push(Ph::Test),
        Ph::Package => act.push(Ph::Package),
        Ph::Verify => act.push(Ph::Package).push(Ph::Verify),
        Ph::Publish => act.push(Ph::Publish),
        Ph::ApplyUpdateSteps => act.push(Ph::ApplyUpdateSteps),
        Ph::Configure => seq![Ph::Configure],
        Ph::Unconfigure => seq![Ph::Unconfigure],
        Ph::Sync => seq![Ph::Sync],
        Ph::Star => seq![],
    }
}

pub open spec fn in_list(s: Seq<Ph>, p: Ph) -> bool
    decreases s.len()
{
    if s.len() == 0 { false } else { s.last() == p || in_list(s.drop_last(), p) }
}

// GetCompletePhases (Lifecycle/Base.cls:139-162): the members of the chain, in PHASES order.
pub open spec fn order_by(ps: Seq<Ph>, sel: Seq<Ph>) -> Seq<Ph>
    decreases ps.len()
{
    if ps.len() == 0 { seq![] } else {
        let rest = order_by(ps.drop_last(), sel);
        if in_list(sel, ps.last()) { rest.push(ps.last()) } else { rest }
    }
}

// ExecutePhases (Storage/Module.cls:557-572): "*" only refreshes the module object, before InitPhase.
pub open spec fn logged(ps: Seq<Ph>) -> Seq<Ph>
    decreases ps.len()
{
    if ps.len() == 0 { seq![] } else {
        let rest = logged(ps.drop_last());
        if ps.last() == Ph::Star { rest } else { rest.push(ps.last()) }
    }
}

pub open spec fn run_list(t: Ph) -> Seq<Ph> { logged(order_by(phase_order(), chain(t))) }

pub struct PE { pub name: Ph, pub ok: bool, pub ended: bool }

// A phase fails by throwing (caught, FinalizePhase(error), Storage/Module.cls:692-697) or by an <Invoke> returning
// an error status, which quits the try without FinalizePhase (:608-610, :669-671).
pub enum Fail { Thrown, Returned }

// The phase loop: InitPhase (:571), FinalizePhase(OK) (:709-711), stop at the first error (:707).
pub open spec fn run_from(ps: Seq<Ph>, i: nat, fail: Option<(nat, Fail)>) -> (Seq<PE>, bool)
    decreases ps.len() - i
{
    if i >= ps.len() { (seq![], true) }
    else if fail is Some && (fail->0).0 == i {
        (seq![PE { name: ps[i as int], ok: false, ended: (fail->0).1 == Fail::Thrown }], false)
    } else {
        let r = run_from(ps, i + 1, fail);
        (seq![PE { name: ps[i as int], ok: true, ended: true }] + r.0, r.1)
    }
}

// ================================================================ the record

pub struct Rec { pub row: h::Row, pub phases: Seq<PE> }

pub open spec fn placeholder() -> Seq<u8> { seq![48u8, 46, 48, 46, 48, 45, 48] }   // "0.0.0-0"

// AbstractHistory.cls:51-62 (Init, placeholder version, saved) and HistoryTemp.cls:9-26 (the permanent twin saved
// at once): the record exists, in progress, from this point.
pub open spec fn init(a: h::Action, pkg: Seq<u8>) -> Rec {
    Rec { row: h::Row { id: 0, namespace: 0, action: a, package: pkg, version: placeholder(), source: seq![],
        moniker: seq![], details: seq![], ended: false, status: 0, committed: false, finalized: false }, phases: seq![] }
}
pub open spec fn set_version(r: Rec, v: Seq<u8>) -> Rec {                       // AbstractHistory.cls:99-110
    Rec { row: h::Row { version: if v.len() == 0 { placeholder() } else { v }, ..r.row }, ..r }
}
pub open spec fn set_name(r: Rec, n: Seq<u8>) -> Rec { Rec { row: h::Row { package: n, ..r.row }, ..r } }   // :112-117
pub open spec fn set_committed(r: Rec) -> Rec { Rec { row: h::Row { committed: true, ..r.row }, ..r } }    // :120-125
pub open spec fn fin(r: Rec, ok: bool) -> Rec { Rec { row: h::finalize(r.row, if ok { 1 } else { 2 }), ..r } } // :127-134
pub open spec fn persist(r: Rec) -> Rec { Rec { row: h::Row { finalized: true, ..r.row }, ..r } }          // HistoryTemp.cls:30-67
pub open spec fn ok_of(r: Rec) -> bool { r.row.status == 1 }

// ================================================================ install and update (Main.Install, LoadNewModule)

pub struct Ver { pub text: Seq<u8>, pub rank: nat }

pub enum Mode { Tx, Dev, NoTx }

// Utils/Module.cls:1172 (NoTransaction or NoJournal: no transaction) and :1298-1309 (developer mode with transactions).
pub open spec fn mode_of(dev: bool, no_tx: bool, no_journal: bool) -> Mode {
    if no_tx || no_journal { Mode::NoTx } else if dev { Mode::Dev } else { Mode::Tx }
}

pub struct Lnm {
    pub name: Seq<u8>,               // manifest name (SetName, :1293)
    pub from: Option<Ver>,           // installed module (moduleCurrent, :1207-1212)
    pub to: Ver,                     // manifest version
    pub update_package: bool,
    pub force: bool,
    pub dev: bool, pub snapshot: bool, pub no_tx: bool, pub no_journal: bool,
    pub load_fails: bool,            // $system.OBJ.Load of module.xml fails (:1251-1264), before SetName
    pub deps_fail: bool,             // system requirements, dependencies or the second load fail (:1321-1333)
    pub phase_fail: Option<(nat, Fail)>,
    pub after_fails: bool,           // WriteAfterInstallMessage throws after the final commit (:1354)
}

// The developer mode LoadNewModule runs with: Main.Install passes DeveloperMode (Main.cls:2623); a top-level install
// keeps it, while an update goes through LoadModuleReference, which passes dev only for a snapshot
// (Utils/Module.cls:130-136) and LoadModuleFromDirectory applies it when Install is not set (:307-310).
pub open spec fn dev_eff(l: Lnm, upd: bool) -> bool { if upd { l.dev && l.snapshot } else { l.dev } }

// :1213-1222: downgrade, and an install over an older version with an update package (not for `update`).
pub open spec fn guard_refuses(l: Lnm, upd: bool) -> bool {
    match l.from {
        None => false,
        Some(f) => f.rank > l.to.rank || (!upd && !l.force && !dev_eff(l, upd) && l.to.rank > f.rank && l.update_package),
    }
}

// `kept`: the action's database changes are still there when LoadNewModule returns. Tx: everything before the final
// TCOMMIT is rolled back (:1358-1361); Dev: the TCOMMIT at :1311 keeps the module record; NoTx: nothing is rolled back
// once OBJ.Load has written.
pub struct Res { pub rec: Rec, pub ok: bool, pub kept: bool }

pub open spec fn lnm(r: Rec, l: Lnm, upd: bool) -> Res {
    let m = mode_of(dev_eff(l, upd), l.no_tx, l.no_journal);
    if guard_refuses(l, upd) { Res { rec: r, ok: false, kept: false } }
    else {
        let r1 = r;                              // :1225-1228
        if l.load_fails { Res { rec: r1, ok: false, kept: false } }
        else {
            let r2 = set_version(set_name(r1, l.name), l.to.text);                                 // :1292-1295
            let r3 = if m == Mode::Dev { set_committed(r2) } else { r2 };                         // :1309-1314
            let early = m != Mode::Tx;
            if l.deps_fail { Res { rec: r3, ok: false, kept: early } }
            else {
                let pr = run_from(run_list(if upd { Ph::ApplyUpdateSteps } else { Ph::Activate }), 0, l.phase_fail); // :1345-1346
                let r4 = Rec { phases: r3.phases + pr.0, ..r3 };
                if !pr.1 { Res { rec: r4, ok: false, kept: early } }
                else {
                    let r5 = if m == Mode::Tx { set_committed(r4) } else { r4 };                  // :1347-1351
                    Res { rec: r5, ok: !l.after_fails, kept: true }
                }
            }
        }
    }
}

pub struct Search { pub repos_enabled: nat, pub found: Option<Ver>, pub pre_refuse: bool }
pub struct InstallIn { pub name: Seq<u8>, pub search: Search, pub lnm: Lnm }

// Main.cls:2571-2670 after the record exists: name check, enabled repositories, search, SetSource/SetVersion of the
// result (:2631-2632), LoadModuleReference refusals before LoadNewModule (Utils/Module.cls:84-126, prelogged).
pub open spec fn install_body(r0: Rec, i: InstallIn, upd: bool) -> Res {
    if i.name.len() == 0 { Res { rec: r0, ok: false, kept: false } }                               // :2572-2574
    else if i.search.repos_enabled == 0 { Res { rec: r0, ok: false, kept: false } }                // :2580-2585
    else {
        match i.search.found {
            None => Res { rec: r0, ok: false, kept: false },                                         // :2635-2659
            Some(v) => {
                let r1 = set_version(r0, v.text);
                if i.search.pre_refuse { Res { rec: r1, ok: false, kept: false } } else { lnm(r1, i.lnm, upd) }
            }
        }
    }
}

// Main.Install with no caller log: InstallInit (:2565-2566) before any check, Finalize in both exits (:2666-2670),
// persisted to the permanent row by %OnClose.
pub open spec fn main_install(i: InstallIn) -> Rec {
    let r = install_body(init(h::Action::Install, i.name), i, false);
    persist(fin(r.rec, r.ok))
}

// A caller of the class method (%IPM.Main).Install itself gets the record even with no name.
pub open spec fn api_install(i: InstallIn) -> Seq<Rec> { seq![main_install(i)] }

// The CLI parser refuses an empty required `module` before dispatch (Main.cls:427, install `required="true"`).
pub open spec fn cli_install(i: InstallIn) -> Seq<Rec> {
    if i.name.len() == 0 { seq![] } else { seq![main_install(i)] }
}

pub open spec fn has_name(s: Seq<Seq<u8>>, n: Seq<u8>) -> bool
    decreases s.len()
{
    if s.len() == 0 { false } else { s.last() == n || has_name(s.drop_last(), n) }
}

// Main.Update (Main.cls:4413-4447): both refusals throw before UpdateInit (:4438); Install then runs with the update
// log (:4445), so Install's checks and Finalize write the update row.
pub struct UpdateIn { pub permit_downgrade: bool, pub installed: Seq<Seq<u8>>, pub inst: InstallIn }

pub open spec fn cli_update(u: UpdateIn) -> Seq<Rec> {
    let n = u.inst.name;
    if n.len() == 0 { seq![] }                                                                     // parser (:177)
    else if u.permit_downgrade { seq![] }                                                          // :4415-4417
    else if !has_name(u.installed, n) { seq![] }                                                   // :4422-4424
    else {
        let r = install_body(init(h::Action::Update, n), u.inst, true);
        seq![persist(fin(r.rec, r.ok))]
    }
}

// Main.Reinstall (:2673-2699): NameOpen of a name that is not installed throws before Install.
pub open spec fn cli_reinstall(installed: Seq<Seq<u8>>, i: InstallIn) -> Seq<Rec> {
    if i.name.len() == 0 { seq![] } else if !has_name(installed, i.name) { seq![] } else { cli_install(i) }
}

// The installed version after the command: the new version only when the changes were kept.
pub open spec fn after_version(before: Option<Ver>, i: InstallIn, upd: bool) -> Option<Ver> {
    if install_body(init(h::Action::Install, i.name), i, upd).kept { Some(i.lnm.to) } else { before }
}

// ================================================================ uninstall and clean (rows around uninstall.rs)

pub open spec fn pkg_of(k: nat) -> Seq<u8> { seq![(k % 256) as u8] }
// uninstall.rs modules carry no version. Every fixture module is 1.0.0, and SetVersion runs only when NameOpen finds it.
pub open spec fn fix_ver() -> Seq<u8> { seq![49u8, 46, 48, 46, 48] }                              // "1.0.0"

pub open spec fn by_name(w: unin::W, k: nat) -> bool {
    exists|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) && w.mods[m].key == k
}

// Storage/Module.cls:727-757: UninstallInit before anything else (:734), SetVersion when NameOpen finds the module
// (:735-738), ExecutePhases(Clean) whose "not found" throws before InitPhase (:461-463, :571), Finalize(tSC) (:752).
// No SetCommitted on this path: Committed comes from Finalize alone.
pub open spec fn top_rec(k: nat, named: bool, found: bool, ok: bool) -> Rec {
    let r0 = init(h::Action::Uninstall, pkg_of(k));
    let r1 = if named { set_version(r0, fix_ver()) } else { r0 };
    let r2 = if found { Rec { phases: seq![PE { name: Ph::Clean, ok: ok, ended: true }], ..r1 } } else { r1 };
    persist(fin(r2, ok))
}

pub open spec fn top_unin(w: unin::W, k: nat, ok: bool) -> Rec {
    top_rec(k, by_name(w, k), unin::lookup(w, k) is Some, ok)
}

// Lifecycle/Base.cls:439-485: each installed dependency the Clean recursion visits gets its own uninstall record,
// created with its Clean phase before the nested Clean runs and finalized with that Clean's status.
pub open spec fn dep_rec(hr: unin::HRow) -> Rec {
    let d = match hr.m { Some(x) => x, None => 0 };
    let r = Rec { phases: seq![PE { name: Ph::Clean, ok: hr.ok, ended: true }], ..set_version(init(h::Action::Uninstall, pkg_of(d)), fix_ver()) };
    persist(fin(r, hr.ok))
}

pub open spec fn appended(w: unin::W, o: unin::Out) -> Seq<unin::HRow> { o.w.hist.subrange(w.hist.len() as int, o.w.hist.len() as int) }

pub open spec fn plain(k: nat) -> unin::Typed { unin::Typed { key: k, exact_zpm: false } }

pub open spec fn unin_out(w: unin::W, k: nat, force: bool, recurse: bool, global: bool, fuel: nat) -> unin::Out {
    unin::main_uninstall(w, plain(k), force, recurse, global, false, fuel)
}

// Main.Uninstall (Main.cls:2701-2751): no name throws before Storage.Module:Uninstall (:2710-2712). The top record is
// created first, so in ID order it precedes the dependency records the Clean recursion creates. uninstall.rs appends
// the top row last, so it is moved to the front here.
pub open spec fn cli_uninstall(w: unin::W, name: Option<nat>, force: bool, recurse: bool, global: bool, fuel: nat) -> Seq<Rec> {
    match name {
        None => seq![],
        Some(k) => {
            let o = unin_out(w, k, force, recurse, global, fuel);
            let hs = appended(w, o);
            seq![top_unin(w, k, o.ok)] + hs.drop_last().map_values(|hr: unin::HRow| dep_rec(hr))
        }
    }
}

// `<module> clean` (module-action, ExecutePhases with no log; Clean.Recurse defaults to 1, Lifecycle/Base.cls:313):
// no record of its own, one per dependency the recursion visits.
pub open spec fn cli_clean(w: unin::W, k: nat, force: bool, global: bool, fuel: nat) -> Seq<Rec> {
    appended(w, unin::clean_cmd(w, k, force, global, fuel)).map_values(|hr: unin::HRow| dep_rec(hr))
}

// ================================================================ dispatch of commands without a record of their own

// Main.cls:1060-1145 dispatch. `list`, `sync` and phase commands reach no History init of their own: the only init
// sites are Main.Load (:2420), Main.Install (:2566), Main.Update (:4438), Storage.Module:Uninstall (:734), the Clean
// recursion (Lifecycle/Base.cls:440) and LoadModuleReference for a dependency (Utils/Module.cls:63-68).
// A phase command reaches the last two only through Clean and through LoadDependencies in %Test (Base.cls:1261) and
// %Verify (Base.cls:1557). `scoped` is the list of phase-scoped dependencies LoadDependencies installs (an input:
// BuildDependencyGraph is not modelled); each gets a `load` record (cmd is the phase, Utils/Module.cls:64-67).
pub enum Cmd { List, Sync, Phase(Ph) }

pub open spec fn dep_load_row(n: Seq<u8>, l: Lnm) -> Rec {
    let r = lnm(set_version(init(h::Action::Load, n), l.to.text), l, false);
    persist(fin(r.rec, r.ok))
}

pub open spec fn scoped_rows(scoped: Seq<(Seq<u8>, Lnm)>) -> Seq<Rec> {
    scoped.map_values(|x: (Seq<u8>, Lnm)| dep_load_row(x.0, x.1))
}

pub open spec fn cmd_rows(c: Cmd, w: unin::W, k: nat, fuel: nat, scoped: Seq<(Seq<u8>, Lnm)>) -> Seq<Rec> {
    match c {
        Cmd::List => seq![],
        Cmd::Sync => seq![],
        Cmd::Phase(t) =>
            if t == Ph::Clean { cli_clean(w, k, false, false, fuel) }
            else if in_list(run_list(t), Ph::Test) || in_list(run_list(t), Ph::Verify) { scoped_rows(scoped) }
            else { seq![] },
    }
}

// ================================================================ general theorems

pub open spec fn names_of(ps: Seq<PE>) -> Seq<Ph> { ps.map_values(|e: PE| e.name) }

pub proof fn logged_lists()
    ensures
        run_list(Ph::Activate) == seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile, Ph::Activate],
        run_list(Ph::ApplyUpdateSteps) == seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile, Ph::Activate, Ph::ApplyUpdateSteps],
        run_list(Ph::Clean) == seq![Ph::Clean],
        run_list(Ph::Reload) == seq![Ph::Initialize, Ph::Reload],
        run_list(Ph::Compile) == seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile],
        !in_list(run_list(Ph::Reload), Ph::Test) && !in_list(run_list(Ph::Reload), Ph::Verify),
        !in_list(run_list(Ph::Compile), Ph::Test) && !in_list(run_list(Ph::Compile), Ph::Verify),
        in_list(run_list(Ph::Test), Ph::Test),
{
    assert(run_list(Ph::Activate) == seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile, Ph::Activate]) by (compute_only);
    assert(run_list(Ph::ApplyUpdateSteps) == seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile, Ph::Activate, Ph::ApplyUpdateSteps]) by (compute_only);
    assert(run_list(Ph::Clean) == seq![Ph::Clean]) by (compute_only);
    assert(run_list(Ph::Reload) == seq![Ph::Initialize, Ph::Reload]) by (compute_only);
    assert(run_list(Ph::Compile) == seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile]) by (compute_only);
    assert(!in_list(run_list(Ph::Reload), Ph::Test) && !in_list(run_list(Ph::Reload), Ph::Verify)) by (compute_only);
    assert(!in_list(run_list(Ph::Compile), Ph::Test) && !in_list(run_list(Ph::Compile), Ph::Verify)) by (compute_only);
    assert(in_list(run_list(Ph::Test), Ph::Test)) by (compute_only);
}

// The phases a run logs are a prefix of the list; a failure at index i logs exactly i+1 entries and the last is the
// failing phase, ended exactly when it threw. No failure: every phase, each ended OK.
pub proof fn run_shape(ps: Seq<Ph>, i: nat, fail: Option<(nat, Fail)>)
    requires i <= ps.len()
    ensures ({
        let r = run_from(ps, i, fail);
        &&& r.0.len() <= ps.len() - i
        &&& forall|j: int| 0 <= j < r.0.len() ==> #[trigger] r.0[j].name == ps[i + j]
        &&& (fail is Some && i <= (fail->0).0 < ps.len()) ==> (!r.1 && r.0.len() == (fail->0).0 - i + 1
                && !r.0.last().ok && r.0.last().ended == ((fail->0).1 == Fail::Thrown)
                && forall|j: int| #![trigger r.0[j]] 0 <= j < r.0.len() - 1 ==> r.0[j].ok && r.0[j].ended)
        &&& !(fail is Some && i <= (fail->0).0 < ps.len()) ==> (r.1 && r.0.len() == ps.len() - i
                && forall|j: int| #![trigger r.0[j]] 0 <= j < r.0.len() ==> r.0[j].ok && r.0[j].ended)
    })
    decreases ps.len() - i
{
    if i < ps.len() && !(fail is Some && (fail->0).0 == i) {
        run_shape(ps, i + 1, fail);
        let r = run_from(ps, i + 1, fail);
        let e = PE { name: ps[i as int], ok: true, ended: true };
        assert(run_from(ps, i, fail).0 == seq![e] + r.0);
        assert forall|j: int| 0 <= j < (seq![e] + r.0).len() implies #[trigger] (seq![e] + r.0)[j].name == ps[i + j] by {
            if j > 0 { assert((seq![e] + r.0)[j] == r.0[j - 1]); }
        }
        if fail is Some && i <= (fail->0).0 < ps.len() {
            assert((seq![e] + r.0).last() == r.0.last());
            assert forall|j: int| #![trigger (seq![e] + r.0)[j]] 0 <= j < (seq![e] + r.0).len() - 1 implies (seq![e] + r.0)[j].ok && (seq![e] + r.0)[j].ended by {
                if j > 0 { assert((seq![e] + r.0)[j] == r.0[j - 1]); } else { assert((seq![e] + r.0)[0] == e); }
            }
        } else {
            assert forall|j: int| #![trigger (seq![e] + r.0)[j]] 0 <= j < (seq![e] + r.0).len() implies (seq![e] + r.0)[j].ok && (seq![e] + r.0)[j].ended by {
                if j > 0 { assert((seq![e] + r.0)[j] == r.0[j - 1]); } else { assert((seq![e] + r.0)[0] == e); }
            }
        }
    }
}

// history.actions#2 (API) and record#2: Main.Install writes exactly one finalized install record for every input,
// whatever check refuses it; its package is the typed name unless LoadNewModule renamed it.
pub proof fn install_always_recorded(i: InstallIn)
    ensures
        main_install(i).row.action == h::Action::Install,
        main_install(i).row.ended && main_install(i).row.finalized,
        i.name.len() == 0 ==> !ok_of(main_install(i)) && !main_install(i).row.committed && main_install(i).row.package == i.name,
        cli_install(i).len() == (if i.name.len() == 0 { 0int } else { 1int }),
        api_install(i).len() == 1,
{}

// history.actions#7 and record#8: update writes a record exactly when the name is non-empty, -PermitDowngrade is
// absent and the module is installed; the record is an update record.
pub proof fn update_recorded_iff(u: UpdateIn)
    ensures
        cli_update(u).len() == (if u.inst.name.len() > 0 && !u.permit_downgrade && has_name(u.installed, u.inst.name) { 1int } else { 0int }),
        cli_update(u).len() == 1 ==> cli_update(u)[0].row.action == h::Action::Update && cli_update(u)[0].row.ended,
{}

// history.actions#6 / record#8: reinstall of a name that is not installed writes nothing.
pub proof fn reinstall_not_installed(installed: Seq<Seq<u8>>, i: InstallIn)
    requires !has_name(installed, i.name)
    ensures cli_reinstall(installed, i).len() == 0
{}

// history.committed#3 / marking#3 for install and update: Committed is exactly "changes kept", except the early
// marker: without a transaction, a module.xml that does not load is marked committed with nothing kept.
pub open spec fn early_marker(i: InstallIn, upd: bool) -> bool {
    &&& i.name.len() > 0 && i.search.repos_enabled > 0 && i.search.found is Some && !i.search.pre_refuse
    &&& !guard_refuses(i.lnm, upd)
    &&& mode_of(dev_eff(i.lnm, upd), i.lnm.no_tx, i.lnm.no_journal) == Mode::NoTx
    &&& i.lnm.load_fails
}

pub proof fn install_committed_is_kept(i: InstallIn, upd: bool, a: h::Action)
    ensures ({
        let r = install_body(init(a, i.name), i, upd);
        let row = persist(fin(r.rec, r.ok)).row;
        &&& row.committed == (r.kept || early_marker(i, upd))
        &&& r.ok ==> r.kept
        &&& row.status == (if r.ok { 1int } else { 2int })
    })
{}

// history.phases#3/#4 for install and update: the logged phases are the run list cut at the failing phase.
pub proof fn install_phases(i: InstallIn, upd: bool, a: h::Action)
    requires
        i.name.len() > 0, i.search.repos_enabled > 0, i.search.found is Some, !i.search.pre_refuse,
        !guard_refuses(i.lnm, upd), !i.lnm.load_fails, !i.lnm.deps_fail,
    ensures ({
        let ps = run_list(if upd { Ph::ApplyUpdateSteps } else { Ph::Activate });
        let rec = install_body(init(a, i.name), i, upd).rec;
        let pr = run_from(ps, 0, i.lnm.phase_fail);
        &&& rec.phases == pr.0
        &&& (i.lnm.phase_fail is Some && (i.lnm.phase_fail->0).0 < ps.len()) ==>
                rec.phases.len() == (i.lnm.phase_fail->0).0 + 1 && !rec.phases.last().ok
                && rec.phases.last().name == ps[(i.lnm.phase_fail->0).0 as int]
        &&& !(i.lnm.phase_fail is Some && (i.lnm.phase_fail->0).0 < ps.len()) ==> rec.phases.len() == ps.len()
    })
{
    let ps = run_list(if upd { Ph::ApplyUpdateSteps } else { Ph::Activate });
    run_shape(ps, 0, i.lnm.phase_fail);
    let r1 = set_version(init(a, i.name), i.search.found->0.text);
    assert(r1.phases == Seq::<PE>::empty());
    let pr = run_from(ps, 0, i.lnm.phase_fail);
    assert(Seq::<PE>::empty() + pr.0 == pr.0);
}

// Refusals inside install never mark the record committed, in any mode (the guards run before :1227).
pub proof fn install_refusal_uncommitted(i: InstallIn, upd: bool, a: h::Action)
    requires i.name.len() == 0 || i.search.repos_enabled == 0 || i.search.found is None || i.search.pre_refuse || guard_refuses(i.lnm, upd)
    ensures ({
        let r = install_body(init(a, i.name), i, upd);
        !r.ok && !persist(fin(r.rec, r.ok)).row.committed && persist(fin(r.rec, r.ok)).phases.len() == 0
    })
{}

// history.committed.uninstall#1 / marking#4: no step on the uninstall path marks the record committed, so the top
// record reads Committed exactly when Clean returned OK; with no name there is no record.
pub proof fn uninstall_committed_is_success(w: unin::W, k: nat, force: bool, recurse: bool, global: bool, fuel: nat)
    ensures ({
        let o = unin_out(w, k, force, recurse, global, fuel);
        let rs = cli_uninstall(w, Some(k), force, recurse, global, fuel);
        &&& rs.len() >= 1
        &&& rs[0].row.action == h::Action::Uninstall && rs[0].row.package == pkg_of(k)
        &&& rs[0].row.committed == o.ok && ok_of(rs[0]) == o.ok && rs[0].row.ended
        &&& (unin::lookup(w, k) is None) ==> rs[0].phases.len() == 0
        &&& (unin::lookup(w, k) is Some) ==> rs[0].phases == seq![PE { name: Ph::Clean, ok: o.ok, ended: true }]
    })
{
    let rs = cli_uninstall(w, Some(k), force, recurse, global, fuel);
    let o = unin_out(w, k, force, recurse, global, fuel);
    let hs = appended(w, o);
    assert(rs[0] == top_unin(w, k, o.ok));
}

pub proof fn uninstall_no_name(w: unin::W, force: bool, recurse: bool, global: bool, fuel: nat)
    ensures cli_uninstall(w, None, force, recurse, global, fuel).len() == 0
{}

// Every dependency record matches its Clean's result: Committed exactly when that Clean returned OK.
pub proof fn dep_rec_committed(hr: unin::HRow)
    ensures dep_rec(hr).row.committed == hr.ok, ok_of(dep_rec(hr)) == hr.ok, dep_rec(hr).row.action == h::Action::Uninstall,
        dep_rec(hr).phases == seq![PE { name: Ph::Clean, ok: hr.ok, ended: true }]
{}

// ---------------------------------------------------------------- hist frames in uninstall.rs

proof fn before_clean_hist(w: unin::W, rs: Seq<nat>)
    ensures unin::before_clean(w, rs).hist == w.hist
    decreases rs.len()
{
    if rs.len() > 0 { before_clean_hist(w, rs.drop_last()); }
}

proof fn clean_res_hist(w: unin::W, rs: Seq<nat>)
    ensures unin::clean_res(w, rs).w.hist == w.hist
    decreases rs.len()
{
    if rs.len() > 0 {
        clean_res_hist(w, rs.drop_last());
    }
}

// A Clean that does not recurse into dependencies appends no record (uninstall.rs exec_clean).
proof fn exec_clean_no_deps_hist(w: unin::W, t: nat, p: unin::P, fuel: nat)
    requires !p.recurse || (w.mods.contains_key(t) && w.mods[t].deps.len() == 0 && fuel > 0)
    ensures unin::exec_clean(w, t, p, fuel).w.hist == w.hist
{
    if w.mods.contains_key(t) {
        let m = w.mods[t];
        before_clean_hist(w, m.res);
        let w1 = unin::before_clean(w, m.res);
        let w2 = if m.installer { unin::W { hooks: w1.hooks.push(t), ..w1 } } else { w1 };
        clean_res_hist(w2, m.res);
        let p1 = unin::P { cycle: p.cycle.insert(t), ..p };
        let w3 = unin::after_code(w2, m, p);
        assert(w3.hist == w.hist);
        if p.recurse {
            assert(unin::dep_loop(w3, m.deps, p1, (fuel - 1) as nat) == unin::Out { w: w3, ok: true, skip: false });
        }
    }
}

// history.actions#3: without -r, uninstall of any name other than IPM writes exactly one record.
pub proof fn uninstall_one_record(w: unin::W, k: nat, force: bool, global: bool, fuel: nat)
    requires unin::lookup(w, k) != Some(unin::ipm_id())
    ensures cli_uninstall(w, Some(k), force, false, global, fuel).len() == 1
{
    let o = unin_out(w, k, force, false, global, fuel);
    match unin::lookup(w, k) {
        None => {}
        Some(t) => {
            let p = unin::P { level: 1, force: force, recurse: false, global: global, nested: false, full: false, cycle: Set::empty() };
            exec_clean_no_deps_hist(w, t, p, fuel);
        }
    }
    assert(o.w.hist.len() == w.hist.len() + 1);
    assert(appended(w, o).len() == 1);
}

// history.actions#5 for clean: `<module> clean` on a module with no dependencies writes no record at all.
pub proof fn clean_without_deps_writes_nothing(w: unin::W, k: nat, force: bool, global: bool, fuel: nat)
    requires unin::lookup(w, k) is Some ==> (w.mods.contains_key(unin::lookup(w, k)->0) && w.mods[unin::lookup(w, k)->0].deps.len() == 0 && fuel > 0)
    ensures cli_clean(w, k, force, global, fuel).len() == 0
{
    match unin::lookup(w, k) {
        None => {}
        Some(t) => {
            let p = unin::P { level: 0, force: force, recurse: true, global: global, nested: false, full: false, cycle: Set::empty() };
            exec_clean_no_deps_hist(w, t, p, fuel);
        }
    }
}

// history.actions#5: list, sync and phase commands whose chain has neither Test nor Verify write no record; a
// phase command never writes a record for its own module (clean writes only dependency records).
pub proof fn other_commands_no_record(w: unin::W, k: nat, fuel: nat, scoped: Seq<(Seq<u8>, Lnm)>)
    ensures
        cmd_rows(Cmd::List, w, k, fuel, scoped).len() == 0,
        cmd_rows(Cmd::Sync, w, k, fuel, scoped).len() == 0,
        cmd_rows(Cmd::Phase(Ph::Reload), w, k, fuel, scoped).len() == 0,
        cmd_rows(Cmd::Phase(Ph::Compile), w, k, fuel, scoped).len() == 0,
        cmd_rows(Cmd::Phase(Ph::Activate), w, k, fuel, scoped).len() == 0,
        cmd_rows(Cmd::Phase(Ph::Test), w, k, fuel, scoped).len() == scoped.len(),
{
    logged_lists();
    assert(!in_list(run_list(Ph::Activate), Ph::Test) && !in_list(run_list(Ph::Activate), Ph::Verify)) by (compute_only);
}

// ================================================================ native instances (fixture inputs; decisions computed)

pub open spec fn s(x: Seq<u8>) -> Seq<u8> { x }
pub open spec fn v100() -> Ver { Ver { text: seq![49u8, 46, 48, 46, 48], rank: 100 } }    // 1.0.0
pub open spec fn v110() -> Ver { Ver { text: seq![49u8, 46, 49, 46, 48], rank: 110 } }    // 1.1.0
pub open spec fn n_h74a() -> Seq<u8> { seq![104u8, 55, 52, 97] }
pub open spec fn n_h74c() -> Seq<u8> { seq![104u8, 55, 52, 99] }
pub open spec fn n_h74u() -> Seq<u8> { seq![104u8, 55, 52, 117] }
pub open spec fn n_h74f() -> Seq<u8> { seq![104u8, 55, 52, 102] }
pub open spec fn n_nosuch() -> Seq<u8> { seq![104u8, 55, 52, 110, 111, 115, 117, 99, 104] }

pub open spec fn lnm0(name: Seq<u8>, from: Option<Ver>, to: Ver, upd_pkg: bool) -> Lnm {
    Lnm { name: name, from: from, to: to, update_package: upd_pkg, force: false, dev: false, snapshot: false,
        no_tx: false, no_journal: false, load_fails: false, deps_fail: false, phase_fail: None, after_fails: false }
}
pub open spec fn found(name: Seq<u8>, v: Ver, l: Lnm) -> InstallIn {
    InstallIn { name: name, search: Search { repos_enabled: 1, found: Some(v), pre_refuse: false }, lnm: l }
}
pub open spec fn missing(name: Seq<u8>) -> InstallIn {
    InstallIn { name: name, search: Search { repos_enabled: 1, found: None, pre_refuse: false }, lnm: lnm0(name, None, v100(), false) }
}

pub open spec fn i1() -> InstallIn { found(n_h74a(), v100(), lnm0(n_h74a(), None, v100(), false)) }
pub open spec fn i2() -> InstallIn { found(n_h74a(), v100(), lnm0(n_h74a(), Some(v100()), v100(), false)) }
pub open spec fn i3() -> InstallIn { missing(n_nosuch()) }
pub open spec fn i4() -> InstallIn { missing(seq![]) }
// h74c: a class whose superclass does not exist; Compile (index 3) throws.
pub open spec fn i5() -> InstallIn { found(n_h74c(), v100(), Lnm { phase_fail: Some((3nat, Fail::Thrown)), ..lnm0(n_h74c(), None, v100(), false) }) }
// install h74u 1.0.0 over 1.1.0 (downgrade), with and without -DNoTransaction=1.
pub open spec fn i6() -> InstallIn { found(n_h74u(), v100(), lnm0(n_h74u(), Some(v110()), v100(), false)) }
pub open spec fn i7() -> InstallIn { found(n_h74u(), v100(), Lnm { no_tx: true, ..lnm0(n_h74u(), Some(v110()), v100(), false) }) }
pub open spec fn installed0() -> Seq<Seq<u8>> { seq![n_h74a(), n_h74u(), n_h74f()] }
pub open spec fn upd(inst: InstallIn) -> UpdateIn { UpdateIn { permit_downgrade: false, installed: installed0(), inst: inst } }
pub open spec fn u1() -> UpdateIn { upd(found(n_h74u(), v110(), lnm0(n_h74u(), Some(v100()), v110(), true))) }
// h74f 1.1.0: its update step throws in ApplyUpdateSteps (index 5).
pub open spec fn u2() -> UpdateIn { upd(found(n_h74f(), v110(), Lnm { phase_fail: Some((5nat, Fail::Thrown)), ..lnm0(n_h74f(), Some(v100()), v110(), true) })) }
pub open spec fn u3() -> UpdateIn { upd(found(n_h74f(), v110(), Lnm { no_tx: true, phase_fail: Some((5nat, Fail::Thrown)), ..lnm0(n_h74f(), Some(v100()), v110(), true) })) }
pub open spec fn u4() -> UpdateIn { upd(missing(n_nosuch())) }
pub open spec fn u5() -> UpdateIn { UpdateIn { permit_downgrade: true, ..upd(found(n_h74u(), v110(), lnm0(n_h74u(), Some(v110()), v110(), true))) } }

pub open spec fn rec1(rs: Seq<Rec>) -> Rec { rs[0] }
pub open spec fn nph(r: Rec) -> int { r.phases.len() as int }
pub open spec fn code(p: Ph) -> int {
    match p { Ph::Initialize => 1, Ph::Reload => 2, Ph::Validate => 3, Ph::Compile => 4, Ph::Activate => 5,
        Ph::ApplyUpdateSteps => 6, Ph::Clean => 7, _ => 9 }
}
pub open spec fn act(r: Rec) -> int {
    match r.row.action { h::Action::Load => 1, h::Action::Install => 2, h::Action::Uninstall => 3, h::Action::Update => 4 }
}
pub open spec fn ver_is(r: Rec, v: Seq<u8>) -> bool { r.row.version == v }

// ---------------------------------------------------------------- uninstall worlds (uninstall.rs constructors)

// n1: h74s (id 2, class 10) alone. n2: a name (9) that is not installed. c1: h74m (3, class 12) needs h74d (2,
// class 10). n5: h74y (3, class 12) needs h74z (2, class 10) whose installer undo hook fails.
pub open spec fn w_single() -> unin::W {
    unin::base(Map::empty().insert(0nat, unin::ipm_mod()).insert(2nat, unin::mk(2, 20, seq![], seq![10nat], Set::empty(), Some(Set::empty()))),
        Map::empty().insert(0nat, unin::code(0)).insert(10nat, unin::code(2)), set![0nat, 10nat])
}
pub open spec fn w_hookfail() -> unin::W {
    let dep = unin::Module { installer: true, hook_ok: false, ..unin::mk(2, 20, seq![], seq![10nat], Set::empty(), Some(Set::empty())) };
    let top = unin::mk(3, 30, seq![2nat], seq![12nat], set![2nat], Some(set![2nat]));
    unin::base(Map::empty().insert(0nat, unin::ipm_mod()).insert(2nat, dep).insert(3nat, top),
        Map::empty().insert(0nat, unin::code(0)).insert(10nat, unin::code(2)).insert(12nat, unin::code(3)),
        set![0nat, 10nat, 12nat])
}

pub proof fn n1_facts()
    ensures ({
        let w = w_single();
        let o = unin_out(w, 2, false, false, false, 3);
        let rs = cli_uninstall(w, Some(2nat), false, false, false, 3);
        &&& rs.len() == 1 && o.ok && !o.w.mods.contains_key(2) && !o.w.present.contains(10)
        &&& rs[0].row.committed && ok_of(rs[0]) && rs[0].row.version == fix_ver()
        &&& rs[0].phases == seq![PE { name: Ph::Clean, ok: true, ended: true }]
    })
{
    let w = w_single();
    assert(unin::lookup(w, 2) == Some(2nat)) by {
        assert(w.mods.contains_key(2nat) && w.mods[2nat].key == 2);
        assert(forall|x: nat| #![trigger w.mods[x]] w.mods.contains_key(x) && w.mods[x].key == 2 ==> x == 2);
    }
    assert(by_name(w, 2)) by { assert(w.mods.contains_key(2nat) && w.mods[2nat].key == 2); }
    uninstall_one_record(w, 2, false, false, 3);
    uninstall_committed_is_success(w, 2, false, false, false, 3);
    let p = unin::top_p(false, false, false, false);
    let m = w.mods[2];
    reveal_with_fuel(unin::before_clean, 2);
    reveal_with_fuel(unin::clean_res, 2);
    assert(unin::before_clean(w, m.res) == w);
    assert(!exists|x: nat| unin::code_dependent(w, 2, p.cycle, x));
    let p1 = unin::P { cycle: p.cycle.insert(2), ..p };
    assert(!exists|x: nat| unin::code_dependent(w, 2, p1.cycle, x));
    let w3 = unin::clean_code(w, seq![10nat], false);
    assert(unin::code_gone(unin::clean_res(w, seq![10nat]).w, unin::clean_res(w, seq![10nat]).del, false).contains(10));
    assert(unin::after_code(w, m, p) == w3);
    let a = unin::after_deps(w3, m, p1, 3);
    assert(a == unin::Out { w: w3, ok: true, skip: false });
    assert(a.w.present.contains(unin::sd()));
    let e = unin::exec_clean(w, 2, p, 3);
    assert(e == unin::ipm_block(unin::record_delete(a, 2), 2, p));
    assert(e.ok && !e.w.mods.contains_key(2) && !e.w.present.contains(10));
}

pub proof fn n2_facts()
    ensures ({
        let w = w_single();
        let rs = cli_uninstall(w, Some(9nat), false, false, false, 3);
        &&& rs.len() == 1 && !ok_of(rs[0]) && !rs[0].row.committed && rs[0].phases.len() == 0
        &&& rs[0].row.version == placeholder() && rs[0].row.package == pkg_of(9)
    })
{
    let w = w_single();
    assert(!exists|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) && w.mods[m].key == 9) by {
        assert forall|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) implies w.mods[m].key != 9 by {
            assert(m == 0 || m == 2);
        }
    }
    assert(!exists|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) && w.mods[m].ext_key == 9) by {
        assert forall|m: nat| #![trigger w.mods[m]] w.mods.contains_key(m) implies w.mods[m].ext_key != 9 by {
            assert(m == 0 || m == 2);
        }
    }
    assert(unin::lookup(w, 9) is None);
    assert(!by_name(w, 9));
    uninstall_one_record(w, 9, false, false, 3);
    uninstall_committed_is_success(w, 9, false, false, false, 3);
}

// n5: `uninstall h74y -r`: h74y's class goes, then h74z's undo hook fails. Two records, top first, both failed and
// uncommitted, while a change (class 12 deleted) was kept and both module records stay.
pub proof fn n5_facts()
    ensures ({
        let w = w_hookfail();
        let o = unin_out(w, 3, false, true, false, 3);
        let rs = cli_uninstall(w, Some(3nat), false, true, false, 3);
        &&& !o.ok && w.present.contains(12) && !o.w.present.contains(12) && o.w.present.contains(10)
        &&& o.w.mods.contains_key(3) && o.w.mods.contains_key(2)
        &&& rs.len() == 2
        &&& rs[0].row.package == pkg_of(3) && !rs[0].row.committed && !ok_of(rs[0])
        &&& rs[0].phases == seq![PE { name: Ph::Clean, ok: false, ended: true }]
        &&& rs[1].row.package == pkg_of(2) && !rs[1].row.committed && !ok_of(rs[1])
    })
{
    let w = w_hookfail();
    assert(unin::lookup(w, 3) == Some(3nat)) by {
        assert(w.mods.contains_key(3nat) && w.mods[3nat].key == 3);
        assert(forall|x: nat| #![trigger w.mods[x]] w.mods.contains_key(x) && w.mods[x].key == 3 ==> x == 3);
    }
    assert(by_name(w, 3)) by { assert(w.mods.contains_key(3nat) && w.mods[3nat].key == 3); }
    let p = unin::top_p(false, true, false, false);
    reveal_with_fuel(unin::before_clean, 2);
    reveal_with_fuel(unin::clean_res, 2);
    reveal_with_fuel(unin::dep_loop, 2);
    let m = w.mods[3];
    assert(unin::before_clean(w, m.res) == w);
    assert(!exists|x: nat| unin::code_dependent(w, 3, p.cycle, x));
    let p1 = unin::P { cycle: p.cycle.insert(3), ..p };
    assert(!exists|x: nat| unin::code_dependent(w, 3, p1.cycle, x));
    let w3 = unin::clean_code(w, seq![12nat], false);
    assert(unin::code_gone(unin::clean_res(w, seq![12nat]).w, unin::clean_res(w, seq![12nat]).del, false).contains(12));
    assert(unin::after_code(w, m, p) == w3);
    assert(!w3.present.contains(12) && w3.present.contains(10));
    let pd = unin::P { level: 2, nested: true, ..p1 };
    let m2 = w3.mods[2];
    assert(unin::before_clean(w3, m2.res) == w3);
    assert(!exists|x: nat| unin::code_dependent(w3, 2, pd.cycle, x));
    let w2 = unin::W { hooks: w3.hooks.push(2), ..w3 };
    assert(unin::exec_clean(w3, 2, pd, 2) == unin::Out { w: w2, ok: false, skip: false });
    let s2 = unin::dep_step(w3, 2, p1, 2);
    assert(s2 == unin::Out { w: unin::W { hist: w2.hist.push(unin::HRow { m: Some(2nat), ok: false }), ..w2 }, ok: false, skip: false });
    let a = unin::after_deps(w3, m, p1, 3);
    assert(a == unin::dep_loop(w3, seq![2nat], p1, 2));
    assert(unin::dep_loop(w3, seq![2nat], p1, 2) == s2);
    let e = unin::exec_clean(w, 3, p, 3);
    assert(e == s2);
    let o = unin_out(w, 3, false, true, false, 3);
    assert(o.w == unin::W { hist: e.w.hist.push(unin::HRow { m: Some(3nat), ok: false }), ..e.w } && !o.ok);
    assert(w.hist == Seq::<unin::HRow>::empty());
    let hs = appended(w, o);
    assert(hs =~= seq![unin::HRow { m: Some(2nat), ok: false }, unin::HRow { m: Some(3nat), ok: false }]);
    assert(hs.drop_last() =~= seq![unin::HRow { m: Some(2nat), ok: false }]);
    let rs = cli_uninstall(w, Some(3nat), false, true, false, 3);
    assert(rs =~= seq![top_unin(w, 3, false), dep_rec(unin::HRow { m: Some(2nat), ok: false })]);
}

// c1: `h74m clean` (h74m = 3 needs h74d = 2, nothing else does): one uninstall record, for h74d, successful and
// committed; h74d's record is deleted and h74m stays. Uses uninstall.rs's w_recremove world.
pub proof fn c1_facts()
    ensures ({
        let w = unin::w_recremove();
        let o = unin::clean_cmd(w, 3, false, false, 3);
        let rs = cli_clean(w, 3, false, false, 3);
        &&& o.ok && !o.w.mods.contains_key(2) && o.w.mods.contains_key(3)
        &&& rs.len() == 1 && rs[0].row.package == pkg_of(2) && rs[0].row.committed && ok_of(rs[0])
        &&& rs[0].row.action == h::Action::Uninstall && rs[0].phases == seq![PE { name: Ph::Clean, ok: true, ended: true }]
    })
{
    let w = unin::w_recremove();
    assert(unin::lookup(w, 3) == Some(3nat)) by {
        assert(w.mods.contains_key(3nat) && w.mods[3nat].key == 3);
        assert(forall|x: nat| #![trigger w.mods[x]] w.mods.contains_key(x) && w.mods[x].key == 3 ==> x == 3);
    }
    let p = unin::P { level: 0, force: false, recurse: true, global: false, nested: false, full: false, cycle: Set::empty() };
    reveal_with_fuel(unin::before_clean, 2);
    reveal_with_fuel(unin::clean_res, 2);
    reveal_with_fuel(unin::dep_loop, 2);
    let m = w.mods[3];
    assert(unin::before_clean(w, m.res) == w);
    let p1 = unin::P { cycle: p.cycle.insert(3), ..p };
    assert(!unin::check_skip(w, false, 3, p));
    assert(!unin::check_skip(w, false, 3, p1));
    assert(unin::after_code(w, m, p) == w);
    let pd = unin::P { level: 1, nested: true, ..p1 };
    let m2 = w.mods[2];
    assert(unin::before_clean(w, m2.res) == w);
    assert(!exists|x: nat| unin::code_dependent(w, 2, pd.cycle, x)) by {
        assert forall|x: nat| !unin::code_dependent(w, 2, pd.cycle, x) by {
            if unin::code_dependent(w, 2, pd.cycle, x) { assert(x == 3 || x == 0 || x == 2); }
        }
    }
    let pd1 = unin::P { cycle: pd.cycle.insert(2), ..pd };
    assert(!exists|x: nat| unin::code_dependent(w, 2, pd1.cycle, x)) by {
        assert forall|x: nat| !unin::code_dependent(w, 2, pd1.cycle, x) by {
            if unin::code_dependent(w, 2, pd1.cycle, x) { assert(x == 3 || x == 0 || x == 2); }
        }
    }
    let w5 = unin::clean_code(w, seq![10nat], false);
    assert(unin::code_gone(unin::clean_res(w, seq![10nat]).w, unin::clean_res(w, seq![10nat]).del, false).contains(10));
    assert(unin::after_code(w, m2, pd) == w5);
    assert(unin::after_deps(w5, m2, pd1, 2) == unin::Out { w: w5, ok: true, skip: false });
    assert(unin::exec_clean(w, 2, pd, 2) == unin::Out { w: w5, ok: true, skip: false });
    let s2 = unin::dep_step(w, 2, p1, 2);
    assert(s2.w == unin::W { hist: w5.hist.push(unin::HRow { m: Some(2nat), ok: true }), mods: w5.mods.remove(2), ..w5 } && s2.ok);
    let a = unin::after_deps(w, m, p1, 3);
    assert(a == unin::dep_loop(w, seq![2nat], p1, 2));
    assert(unin::dep_loop(w, seq![2nat], p1, 2) == s2);
    let e = unin::exec_clean(w, 3, p, 3);
    assert(e == unin::ipm_block(s2, 3, p));
    assert(e == s2);
    let o = unin::clean_cmd(w, 3, false, false, 3);
    assert(o == e);
    assert(w.hist == Seq::<unin::HRow>::empty());
    assert(appended(w, o) =~= seq![unin::HRow { m: Some(2nat), ok: true }]);
}

// ---------------------------------------------------------------- neighbour theorems (compute)

pub proof fn install_neighbours()
    ensures
        cli_install(i1()).len() == 1 && ok_of(rec1(cli_install(i1()))) && rec1(cli_install(i1())).row.committed && nph(rec1(cli_install(i1()))) == 5,
        cli_install(i2()).len() == 1 && ok_of(rec1(cli_install(i2()))) && nph(rec1(cli_install(i2()))) == 5,
        cli_install(i3()).len() == 1 && !ok_of(rec1(cli_install(i3()))) && !rec1(cli_install(i3())).row.committed
            && nph(rec1(cli_install(i3()))) == 0 && ver_is(rec1(cli_install(i3())), placeholder()),
        cli_install(i4()).len() == 0 && !ok_of(main_install(i4())) && main_install(i4()).row.package.len() == 0,
        cli_install(i5()).len() == 1 && !ok_of(rec1(cli_install(i5()))) && !rec1(cli_install(i5())).row.committed
            && nph(rec1(cli_install(i5()))) == 4 && code(rec1(cli_install(i5())).phases[3].name) == 4
            && !rec1(cli_install(i5())).phases[3].ok && rec1(cli_install(i5())).phases[3].ended,
        !rec1(cli_install(i6())).row.committed && !rec1(cli_install(i7())).row.committed && nph(rec1(cli_install(i7()))) == 0
            && ver_is(rec1(cli_install(i6())), v100().text),
        after_version(Some(v110()), i6(), false) == Some(v110()) && after_version(Some(v110()), i7(), false) == Some(v110()),
{
    assert(cli_install(i1()).len() == 1 && ok_of(rec1(cli_install(i1()))) && rec1(cli_install(i1())).row.committed && nph(rec1(cli_install(i1()))) == 5) by (compute_only);
    assert(cli_install(i2()).len() == 1 && ok_of(rec1(cli_install(i2()))) && nph(rec1(cli_install(i2()))) == 5) by (compute_only);
    assert(cli_install(i3()).len() == 1 && !ok_of(rec1(cli_install(i3()))) && !rec1(cli_install(i3())).row.committed
            && nph(rec1(cli_install(i3()))) == 0 && ver_is(rec1(cli_install(i3())), placeholder())) by (compute_only);
    assert(cli_install(i4()).len() == 0 && !ok_of(main_install(i4())) && main_install(i4()).row.package.len() == 0) by (compute_only);
    assert(cli_install(i5()).len() == 1 && !ok_of(rec1(cli_install(i5()))) && !rec1(cli_install(i5())).row.committed
            && nph(rec1(cli_install(i5()))) == 4 && code(rec1(cli_install(i5())).phases[3].name) == 4
            && !rec1(cli_install(i5())).phases[3].ok && rec1(cli_install(i5())).phases[3].ended) by (compute_only);
    assert(!rec1(cli_install(i6())).row.committed && !rec1(cli_install(i7())).row.committed && nph(rec1(cli_install(i7()))) == 0
            && ver_is(rec1(cli_install(i6())), v100().text)) by (compute_only);
    assert(after_version(Some(v110()), i6(), false) == Some(v110()) && after_version(Some(v110()), i7(), false) == Some(v110())) by (compute_only);
}

pub proof fn update_neighbours()
    ensures
        cli_update(u1()).len() == 1 && ok_of(rec1(cli_update(u1()))) && rec1(cli_update(u1())).row.committed
            && nph(rec1(cli_update(u1()))) == 6 && code(rec1(cli_update(u1())).phases[5].name) == 6,
        cli_update(u2()).len() == 1 && !ok_of(rec1(cli_update(u2()))) && !rec1(cli_update(u2())).row.committed
            && nph(rec1(cli_update(u2()))) == 6 && !rec1(cli_update(u2())).phases[5].ok,
        cli_update(u3()).len() == 1 && !ok_of(rec1(cli_update(u3()))) && rec1(cli_update(u3())).row.committed,
        cli_update(u4()).len() == 0 && cli_update(u5()).len() == 0,
        after_version(Some(v100()), u2().inst, true) == Some(v100()) && after_version(Some(v100()), u3().inst, true) == Some(v110()),
{
    assert(cli_update(u1()).len() == 1 && ok_of(rec1(cli_update(u1()))) && rec1(cli_update(u1())).row.committed
            && nph(rec1(cli_update(u1()))) == 6 && code(rec1(cli_update(u1())).phases[5].name) == 6) by (compute_only);
    assert(cli_update(u2()).len() == 1 && !ok_of(rec1(cli_update(u2()))) && !rec1(cli_update(u2())).row.committed
            && nph(rec1(cli_update(u2()))) == 6 && !rec1(cli_update(u2())).phases[5].ok) by (compute_only);
    assert(cli_update(u3()).len() == 1 && !ok_of(rec1(cli_update(u3()))) && rec1(cli_update(u3())).row.committed) by (compute_only);
    assert(cli_update(u4()).len() == 0 && cli_update(u5()).len() == 0) by (compute_only);
    assert(after_version(Some(v100()), u2().inst, true) == Some(v100()) && after_version(Some(v100()), u3().inst, true) == Some(v110())) by (compute_only);
}

// A developer-mode install that fails in Compile after the TCOMMIT at :1311 reads committed (no native case).
pub proof fn dev_failure_committed()
    ensures ({
        let i = found(n_h74c(), v100(), Lnm { dev: true, phase_fail: Some((3nat, Fail::Thrown)), ..lnm0(n_h74c(), None, v100(), false) });
        rec1(cli_install(i)).row.committed && !ok_of(rec1(cli_install(i)))
    })
{
    assert({
        let i = found(n_h74c(), v100(), Lnm { dev: true, phase_fail: Some((3nat, Fail::Thrown)), ..lnm0(n_h74c(), None, v100(), false) });
        rec1(cli_install(i)).row.committed && !ok_of(rec1(cli_install(i)))
    }) by (compute_only);
}

// ---------------------------------------------------------------- remaining native instances

// While the action runs, the permanent row is the twin saved at init (HistoryTemp.cls:14-21); its phases and result
// reach it only through PersistToTwin at the end (:30-59). The in-progress copy lists the phases started so far.
pub open spec fn twin_while_running(a: h::Action, n: Seq<u8>) -> Rec { init(a, n) }
pub open spec fn running_ok(r: Rec, a: h::Action, n: Seq<u8>) -> bool {
    r.row.action == a && r.row.package == n && !r.row.ended && r.row.status == 0 && !r.row.finalized
}
pub open spec fn pos(ps: Seq<Ph>, p: Ph) -> int
    decreases ps.len()
{
    if ps.len() == 0 { -1 } else if ps.last() == p { ps.len() - 1 } else { pos(ps.drop_last(), p) }
}
pub open spec fn started_through(t: Ph, p: Ph) -> int { pos(run_list(t), p) + 1 }

pub open spec fn n_h74b() -> Seq<u8> { seq![104u8, 55, 52, 98] }
pub open spec fn t1_dep() -> Lnm { lnm0(n_h74b(), None, v100(), false) }
pub open spec fn t1_scoped() -> Seq<(Seq<u8>, Lnm)> { seq![(n_h74b(), t1_dep())] }
pub open spec fn t1_row() -> Rec { dep_load_row(n_h74b(), t1_dep()) }

// A dependency that LoadModuleReference installs with no caller log is labelled install only when the command is
// `install` (Utils/Module.cls:63-68); a phase command's cmd is the phase, so the record is a load record.
pub proof fn dep_load_label(n: Seq<u8>, l: Lnm)
    ensures dep_load_row(n, l).row.action == h::Action::Load
{}

pub proof fn t1_facts()
    ensures
        cmd_rows(Cmd::Phase(Ph::Test), w_single(), 2, 3, t1_scoped()).len() == 1,
        cmd_rows(Cmd::Phase(Ph::Test), w_single(), 2, 3, t1_scoped())[0] == t1_row(),
        act(t1_row()) == 1 && ok_of(t1_row()) && t1_row().row.committed && nph(t1_row()) == 5,
{
    assert(act(t1_row()) == 1 && ok_of(t1_row()) && t1_row().row.committed && nph(t1_row()) == 5) by (compute_only);
    other_commands_no_record(w_single(), 2, 3, t1_scoped());
    assert(!(Ph::Test == Ph::Clean));
    assert(scoped_rows(t1_scoped())[0] == t1_row());
}

// c2: list, `h74a reload`, `h74a compile`, `reload h74a` (the same phase command) and sync, in one session.
pub open spec fn c2_rows() -> int {
    (cmd_rows(Cmd::List, w_single(), 2, 3, seq![]).len() + cmd_rows(Cmd::Phase(Ph::Reload), w_single(), 2, 3, seq![]).len()
    + cmd_rows(Cmd::Phase(Ph::Compile), w_single(), 2, 3, seq![]).len() + cmd_rows(Cmd::Phase(Ph::Reload), w_single(), 2, 3, seq![]).len()
    + cmd_rows(Cmd::Sync, w_single(), 2, 3, seq![]).len()) as int
}

// n4: h74x's Clean fails in a resource processor after its FileCopy target was deleted. uninstall.rs has no
// processor failure, so this record is computed from the observed Clean status (found by name, Clean failed).
pub open spec fn n4_rs() -> Seq<Rec> { seq![top_rec(5, true, true, false)] }

pub open spec fn n1_rs() -> Seq<Rec> { cli_uninstall(w_single(), Some(2nat), false, false, false, 3) }
pub open spec fn n2_rs() -> Seq<Rec> { cli_uninstall(w_single(), Some(9nat), false, false, false, 3) }
pub open spec fn n3_rs() -> Seq<Rec> { cli_uninstall(w_single(), None, false, false, false, 3) }
pub open spec fn n5_rs() -> Seq<Rec> { cli_uninstall(w_hookfail(), Some(3nat), false, true, false, 3) }
pub open spec fn c1_rs() -> Seq<Rec> { cli_clean(unin::w_recremove(), 3, false, false, 3) }

pub proof fn uninstall_fields()
    ensures
        n1_rs().len() == 1 && act(n1_rs()[0]) == 3 && n1_rs()[0].row.package == pkg_of(2) && ver_is(n1_rs()[0], fix_ver())
            && !ver_is(n1_rs()[0], placeholder()) && n1_rs()[0].row.status == 1 && n1_rs()[0].row.committed && n1_rs()[0].row.ended
            && n1_rs()[0].row.finalized && nph(n1_rs()[0]) == 1 && code(n1_rs()[0].phases[0].name) == 7
            && n1_rs()[0].phases[0].ok && n1_rs()[0].phases[0].ended
            && !unin_out(w_single(), 2, false, false, false, 3).w.mods.contains_key(2),
        n2_rs().len() == 1 && act(n2_rs()[0]) == 3 && n2_rs()[0].row.package == pkg_of(9) && ver_is(n2_rs()[0], placeholder())
            && !ver_is(n2_rs()[0], fix_ver()) && n2_rs()[0].row.status == 2 && !n2_rs()[0].row.committed && n2_rs()[0].row.ended
            && n2_rs()[0].row.finalized && nph(n2_rs()[0]) == 0,
        n3_rs().len() == 0,
        n5_rs().len() == 2 && act(n5_rs()[0]) == 3 && n5_rs()[0].row.package == pkg_of(3) && ver_is(n5_rs()[0], fix_ver())
            && n5_rs()[0].row.status == 2 && !n5_rs()[0].row.committed && n5_rs()[0].row.ended && nph(n5_rs()[0]) == 1
            && code(n5_rs()[0].phases[0].name) == 7 && !n5_rs()[0].phases[0].ok && n5_rs()[0].phases[0].ended
            && act(n5_rs()[1]) == 3 && n5_rs()[1].row.package != pkg_of(3) && ver_is(n5_rs()[1], fix_ver())
            && n5_rs()[1].row.status == 2 && !n5_rs()[1].row.committed && n5_rs()[1].row.ended && nph(n5_rs()[1]) == 1
            && code(n5_rs()[1].phases[0].name) == 7 && !n5_rs()[1].phases[0].ok,
        c1_rs().len() == 1 && act(c1_rs()[0]) == 3 && c1_rs()[0].row.package == pkg_of(2) && ver_is(c1_rs()[0], fix_ver())
            && c1_rs()[0].row.status == 1 && c1_rs()[0].row.committed && c1_rs()[0].row.ended && nph(c1_rs()[0]) == 1
            && code(c1_rs()[0].phases[0].name) == 7 && c1_rs()[0].phases[0].ok && c1_rs()[0].phases[0].ended,
        c2_rows() == 0,
        unin_out(w_hookfail(), 3, false, true, false, 3).w.mods.contains_key(3) && unin_out(w_hookfail(), 3, false, true, false, 3).w.mods.contains_key(2)
            && !unin_out(w_hookfail(), 3, false, true, false, 3).w.present.contains(12) && unin_out(w_hookfail(), 3, false, true, false, 3).w.present.contains(10),
        !unin::clean_cmd(unin::w_recremove(), 3, false, false, 3).w.mods.contains_key(2) && unin::clean_cmd(unin::w_recremove(), 3, false, false, 3).w.mods.contains_key(3),
{
    n1_facts(); n2_facts(); n5_facts(); c1_facts();
    uninstall_no_name(w_single(), false, false, false, 3);
    uninstall_committed_is_success(w_single(), 2, false, false, false, 3);
    uninstall_committed_is_success(w_single(), 9, false, false, false, 3);
    uninstall_committed_is_success(w_hookfail(), 3, false, true, false, 3);
    other_commands_no_record(w_single(), 2, 3, seq![]);
    assert(pkg_of(2) != pkg_of(3)) by { assert(pkg_of(2)[0] != pkg_of(3)[0]); }
    assert(fix_ver() != placeholder()) by { assert(fix_ver().len() != placeholder().len()); }
    let d = dep_rec(unin::HRow { m: Some(2nat), ok: false });
    assert(n5_rs()[1] == d);
    let c = dep_rec(unin::HRow { m: Some(2nat), ok: true });
    assert(c1_rs()[0] == c);
}

fn main() {}

} // verus!
