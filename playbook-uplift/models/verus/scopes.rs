// L1/L2 for IPM resource scopes and packaging inclusion (module 08, ipm 571e04a1).
//
// L2 transcribes:
//   - Lifecycle/Base.cls GetCompletePhasesForOne (#L165-L189): the complete phase list of each command;
//   - Storage/ResourceReference.cls IsInScope (#L258-L282): a scoped resource is in scope only if its phase is in the list;
//   - ResourceProcessor/Default/Document.cls OnPhase (#L59-L210): Reload loads in-scope, non-generated resources;
//     Compile compiles in-scope unscoped (or generated) resources and skips scoped ones; a scoped resource is loaded
//     and compiled in its own phase (tInCurrentPhase);
//   - Lifecycle/Base.cls %Export (#L1306-L1316): the export phase list adds Test and Verify when
//     $get(pParams("IncludeTests"), Module.IncludeTests) is true.
// The phase walk is a fold over an unbounded sequence of resources (induction). Preloaded resources and custom phases
// are outside this model (bound S1 in scopes.TRUST notes below).
// PREMISES: none
use vstd::prelude::*;

verus! {

pub enum Scope { NoScope, TestScope, VerifyScope }
pub enum Ph { Initialize, Reload, Validate, Compile, Activate, Test, Package, Verify, Publish }
pub enum Cmd { Load, TestCmd, PackageCmd, VerifyCmd, PublishCmd }

pub struct Res { pub scope: Scope, pub generated: bool }
pub struct St { pub loaded: bool, pub compiled: bool }

pub open spec fn base() -> Seq<Ph> { seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile, Ph::Activate] }

// GetCompletePhasesForOne (the "*" refresh marker is not a phase of any resource and is left out)
pub open spec fn phases(c: Cmd) -> Seq<Ph> {
    match c {
        Cmd::Load => base(),
        Cmd::TestCmd => base().push(Ph::Test),
        Cmd::PackageCmd => base().push(Ph::Package),
        Cmd::VerifyCmd => base().push(Ph::Package).push(Ph::Verify),
        Cmd::PublishCmd => base().push(Ph::Publish),
    }
}

pub open spec fn scope_phase(s: Scope) -> Ph { if s == Scope::TestScope { Ph::Test } else { Ph::Verify } }

// IsInScope
pub open spec fn in_scope(r: Res, l: Seq<Ph>) -> bool { r.scope == Scope::NoScope || l.contains(scope_phase(r.scope)) }
pub open spec fn in_current(r: Res, p: Ph) -> bool { r.scope != Scope::NoScope && scope_phase(r.scope) == p }

// Document.OnPhase for one resource and one phase
pub open spec fn on_phase(r: Res, st: St, p: Ph, l: Seq<Ph>) -> St {
    let loads = (p == Ph::Reload && in_scope(r, l)) || in_current(r, p);
    let st1 = if loads && !r.generated { St { loaded: true, ..st } } else { st };
    // Generated resources are never loaded by IPM; they compile here as usual (Document.cls#L195-L201).
    if p == Ph::Compile && in_scope(r, l) && (r.scope == Scope::NoScope || r.generated) && (st1.loaded || r.generated) {
        St { compiled: true, ..st1 }
    } else if in_current(r, p) && st1.loaded {
        St { compiled: true, ..st1 }                 // its own phase loads and compiles it
    } else { st1 }
}

// The lifecycle walks phases in order and, in each phase, every resource.
pub open spec fn walk(r: Res, st: St, l: Seq<Ph>, i: nat) -> St
    decreases l.len() - i
{
    if i >= l.len() { st } else { walk(r, on_phase(r, st, l[i as int], l), l, i + 1) }
}

pub open spec fn run(rs: Seq<Res>, c: Cmd) -> Seq<St> {
    Seq::new(rs.len(), |j: int| walk(rs[j], St { loaded: false, compiled: false }, phases(c), 0))
}

// %Export inclusion: param is pParams("IncludeTests") if given; manifest is Module.IncludeTests.
pub open spec fn export_phases(c: Cmd, param: Option<bool>, manifest: bool) -> Seq<Ph> {
    let inc = match param { Some(b) => b, None => manifest };
    if inc { phases(c).push(Ph::Test).push(Ph::Verify) } else { phases(c) }
}
pub open spec fn exported(rs: Seq<Res>, c: Cmd, param: Option<bool>, manifest: bool) -> Seq<bool> {
    // ExportSingleModule skips generated resources (Lifecycle/Base.cls#L2047)
    Seq::new(rs.len(), |j: int| in_scope(rs[j], export_phases(c, param, manifest)) && !rs[j].generated)
}

// ---------------------------------------------------------------- L1 (RES statements)

// RES-B1..B4: a resource is compiled when it has no scope or the command runs its scope's phase.
pub open spec fn l1_compiled(r: Res, c: Cmd) -> bool {
    r.scope == Scope::NoScope || phases(c).contains(scope_phase(r.scope))
}
// RES-B1..B4, B11: IPM loads it from source exactly when it is compiled and not generated.
pub open spec fn l1_present(r: Res, c: Cmd) -> bool { !r.generated && l1_compiled(r, c) }
// RES-B13..B15: a package holds scoped resources only when tests are included by the flag or the manifest.
pub open spec fn l1_exported(r: Res, c: Cmd, include: bool) -> bool {
    !r.generated && (r.scope == Scope::NoScope || include || phases(c).contains(scope_phase(r.scope)))
}

// ---------------------------------------------------------------- proofs

pub open spec fn st0() -> St { St { loaded: false, compiled: false } }

proof fn lemma_contains_facts()
    ensures
        phases(Cmd::Load) == seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile, Ph::Activate],
        !phases(Cmd::Load).contains(Ph::Test), !phases(Cmd::Load).contains(Ph::Verify),
        phases(Cmd::TestCmd).contains(Ph::Test), !phases(Cmd::TestCmd).contains(Ph::Verify),
        !phases(Cmd::PackageCmd).contains(Ph::Test), !phases(Cmd::PackageCmd).contains(Ph::Verify),
        !phases(Cmd::VerifyCmd).contains(Ph::Test), phases(Cmd::VerifyCmd).contains(Ph::Verify),
        !phases(Cmd::PublishCmd).contains(Ph::Test), !phases(Cmd::PublishCmd).contains(Ph::Verify),
{
    assert(phases(Cmd::Load) =~= seq![Ph::Initialize, Ph::Reload, Ph::Validate, Ph::Compile, Ph::Activate]);
    assert(phases(Cmd::TestCmd)[5] == Ph::Test);
    assert(phases(Cmd::VerifyCmd)[6] == Ph::Verify);
    let cs = seq![Cmd::Load, Cmd::TestCmd, Cmd::PackageCmd, Cmd::VerifyCmd, Cmd::PublishCmd];
    assert forall|k: int| 0 <= k < phases(Cmd::Load).len() implies phases(Cmd::Load)[k] != Ph::Test && phases(Cmd::Load)[k] != Ph::Verify by {};
    assert forall|k: int| 0 <= k < phases(Cmd::TestCmd).len() implies phases(Cmd::TestCmd)[k] != Ph::Verify by {};
    assert forall|k: int| 0 <= k < phases(Cmd::PackageCmd).len() implies phases(Cmd::PackageCmd)[k] != Ph::Test && phases(Cmd::PackageCmd)[k] != Ph::Verify by {};
    assert forall|k: int| 0 <= k < phases(Cmd::VerifyCmd).len() implies phases(Cmd::VerifyCmd)[k] != Ph::Test by {};
    assert forall|k: int| 0 <= k < phases(Cmd::PublishCmd).len() implies phases(Cmd::PublishCmd)[k] != Ph::Test && phases(Cmd::PublishCmd)[k] != Ph::Verify by {};
}

// One resource, one command: the phase walk compiles it exactly when L1 says it is present (RES-B1..B4, B11).
pub proof fn res_present_one(r: Res, c: Cmd)
    ensures walk(r, st0(), phases(c), 0).compiled == l1_compiled(r, c),
            walk(r, st0(), phases(c), 0).loaded == l1_present(r, c),
{
    lemma_contains_facts();
    reveal_with_fuel(walk, 9);
}

// Every resource of a module of any size (RES-B1..B4, B11).
pub proof fn res_present(rs: Seq<Res>, c: Cmd)
    ensures forall|j: int| 0 <= j < rs.len() ==> (#[trigger] run(rs, c)[j]).compiled == l1_compiled(rs[j], c)
                                                  && run(rs, c)[j].loaded == l1_present(rs[j], c)
{
    assert forall|j: int| 0 <= j < rs.len() implies (#[trigger] run(rs, c)[j]).compiled == l1_compiled(rs[j], c)
                                                  && run(rs, c)[j].loaded == l1_present(rs[j], c) by {
        res_present_one(rs[j], c);
    };
}

// Packaging inclusion (RES-B13..B15): with no explicit parameter, the flag or the manifest decides.
pub open spec fn flag_param(flag: bool) -> Option<bool> { if flag { Some(true) } else { None } }

pub proof fn res_export(rs: Seq<Res>, c: Cmd, manifest: bool, flag: bool)
    requires c == Cmd::PackageCmd || c == Cmd::PublishCmd
    ensures forall|j: int| 0 <= j < rs.len() ==>
        (#[trigger] exported(rs, c, flag_param(flag), manifest)[j]) == l1_exported(rs[j], c, flag || manifest)
{
    lemma_contains_facts();
    let l = export_phases(c, flag_param(flag), manifest);
    assert(l.contains(Ph::Test) <==> (flag || manifest)) by {
        if flag || manifest { assert(l[l.len() - 2] == Ph::Test); }
        else { assert(l == phases(c)); }
    };
    assert(l.contains(Ph::Verify) <==> (flag || manifest)) by {
        if flag || manifest { assert(l[l.len() - 1] == Ph::Verify); }
        else { assert(l == phases(c)); }
    };
}

// The `verify` command's phase list has no Test phase, so a verify never loads test-scoped resources.
pub proof fn res_witness_verify_skips_test_scoped()
    ensures !walk(Res { scope: Scope::TestScope, generated: false }, st0(), phases(Cmd::VerifyCmd), 0).compiled
{
    res_present_one(Res { scope: Scope::TestScope, generated: false }, Cmd::VerifyCmd);
    lemma_contains_facts();
}

// An explicit -DIncludeTests=0 drops tests that the manifest asks for (the -include-tests flag itself only adds).
pub proof fn res_witness_explicit_zero_overrides_manifest()
    ensures !exported(seq![Res { scope: Scope::TestScope, generated: false }], Cmd::PackageCmd, Some(false), true)[0]
{
    lemma_contains_facts();
}

fn main() {}

} // verus!
