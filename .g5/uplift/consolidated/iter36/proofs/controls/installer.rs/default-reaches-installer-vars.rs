#![allow(non_snake_case)]
// Iteration36: module installer class callbacks at load and uninstall.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter36/bindings.json):
//   Lifecycle/Base.cls::%Configure          OnInstallComponent(DatabaseName, Verbose, DeveloperMode, IsUpgrade), then
//                                           OnConfigureComponent($namespace, Verbose, vars); vars = params + Configure.Parameters
//                                           + InstallDir + PackageManager.Lifecycle. GetCustomParameters result unused.
//   Storage/Module.cls::ExecutePhases, GetDefaultParameters   <Defaults><Parameter> merged into params, -D overrides
//   Storage/ModuleSetting/Default.cls::GetCustomParameters    <Default> values, -D overrides; read by Invoke ${...}
//   Lifecycle/Base.cls::%Unconfigure, Lifecycle/Module.cls::%Clean   OnUnconfigureComponent then OnUnInstallComponent
// No product writer of Configure.IsUpgrade, Configure.DatabaseName or Configure.Purge exists, so only a
// -D/<Parameter> spelling of those names reaches the callbacks. Premises (native-qualified): a failing
// Activate rolls back the normal-mode load, including the installer's own global writes; $namespace is USER.
use vstd::prelude::*;

verus! {

// Value codes used by the native reader.
pub open spec fn NONE() -> int { 0 }
pub open spec fn PARAM() -> int { 1 }
pub open spec fn CLI() -> int { 2 }
pub open spec fn HELLO() -> int { 3 }
pub open spec fn BYE() -> int { 4 }
pub open spec fn USER() -> int { 5 }

// Callback kinds; ROLLED_BACK marks a record written inside a load that rolled back.
pub open spec fn INSTALL() -> int { 1 }
pub open spec fn CONFIGURE() -> int { 2 }
pub open spec fn UNCONFIGURE() -> int { 3 }
pub open spec fn UNINSTALL() -> int { 4 }
pub open spec fn NOTE() -> int { 5 }
pub open spec fn ROLLED_BACK() -> int { 0 }

pub struct Call { pub kind: int, pub dev: int, pub upgrade: int, pub mode: int, pub greeting: int, pub purge: int, pub db_empty: int, pub ns: int, pub installdir: int, pub lifecycle: int }
pub open spec fn call(kind: int) -> Call { Call { kind, dev: 0, upgrade: 0, mode: 0, greeting: 0, purge: 0, db_empty: 0, ns: 0, installdir: 0, lifecycle: 0 } }

pub struct Module { pub installer: bool, pub note_invoke: bool, pub param_mode: Option<int>, pub default_greeting: Option<int> }
pub struct Cmd { pub load: bool, pub dev: bool, pub d_mode: Option<int>, pub d_greeting: Option<int>, pub d_upgrade: Option<int>, pub fail: bool }

pub open spec fn or(o: Option<int>, d: int) -> int { match o { Some(v) => v, None => d } }
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

// ExecutePhases: module <Parameter> values, then -D values override. <Default> values are not params.
pub open spec fn param_mode(m: Module, c: Cmd) -> int { or(c.d_mode, or(m.param_mode, NONE())) }
pub open spec fn param_greeting(m: Module, c: Cmd) -> int { or(c.d_greeting, or(m.default_greeting, NONE())) }
// GetCustomParameters: <Default> value, replaced by a -D value of the same name.
pub open spec fn custom_greeting(m: Module, c: Cmd) -> int { or(c.d_greeting, or(m.default_greeting, NONE())) }
// Configure.IsUpgrade has no product writer: only a -D spelling sets it.
pub open spec fn upgrade(c: Cmd) -> int { or(c.d_upgrade, 0) }

pub open spec fn install_call(c: Cmd) -> Call { Call { dev: b(c.dev), upgrade: upgrade(c), db_empty: 1, ..call(INSTALL()) } }
pub open spec fn configure_call(m: Module, c: Cmd) -> Call {
    Call { ns: USER(), mode: param_mode(m, c), greeting: param_greeting(m, c), installdir: 1, lifecycle: 1, ..call(CONFIGURE()) }
}
pub open spec fn note_call(m: Module, c: Cmd) -> Call { Call { greeting: custom_greeting(m, c), ..call(NOTE()) } }

pub struct Outcome { pub ok: bool, pub installed: bool, pub calls: Seq<Call> }

// load: Activate runs the nested Configure phase (installer callbacks), then the After-Activate Invoke.
pub open spec fn load(m: Module, c: Cmd, installed_before: bool) -> Outcome {
    if m.installer && c.fail {
        // The refusal fails Configure and Activate; the load's transaction rolls back the record.
        Outcome { ok: false, installed: installed_before, calls: seq![call(ROLLED_BACK())] }
    } else {
        let inst = if m.installer { seq![install_call(c), configure_call(m, c)] } else { seq![] };
        let note = if m.note_invoke { seq![note_call(m, c)] } else { seq![] };
        Outcome { ok: true, installed: true, calls: inst + note }
    }
}

// uninstall: Clean runs Unconfigure first: OnUnconfigureComponent then OnUnInstallComponent.
pub open spec fn uninstall(m: Module) -> Outcome {
    Outcome { ok: true, installed: false, calls: if m.installer { seq![Call { ns: USER(), ..call(UNCONFIGURE()) }, Call { db_empty: 1, ..call(UNINSTALL()) }] } else { seq![] } }
}

pub open spec fn m36a() -> Module { Module { installer: true, note_invoke: true, param_mode: Some(PARAM()), default_greeting: Some(HELLO()) } }
pub open spec fn m36n() -> Module { Module { installer: false, note_invoke: false, param_mode: None, default_greeting: None } }
pub open spec fn plain() -> Cmd { Cmd { load: true, dev: false, d_mode: None, d_greeting: None, d_upgrade: None, fail: false } }

// Native schedule.
pub open spec fn l1() -> Outcome { load(m36a(), plain(), false) }
pub open spec fn l3() -> Outcome { load(m36a(), Cmd { d_mode: Some(CLI()), d_greeting: Some(BYE()), ..plain() }, l1().installed) }
pub open spec fn l5() -> Outcome { load(m36a(), Cmd { d_upgrade: Some(1), ..plain() }, l3().installed) }
pub open spec fn u1() -> Outcome { uninstall(m36a()) }
pub open spec fn l2() -> Outcome { load(m36a(), Cmd { dev: true, ..plain() }, u1().installed) }
pub open spec fn u2() -> Outcome { uninstall(m36a()) }
pub open spec fn e1() -> Outcome { load(m36a(), Cmd { fail: true, ..plain() }, u2().installed) }
pub open spec fn n1() -> Outcome { load(m36n(), plain(), false) }

// A <Default> value reaches Invoke arguments but not the installer's variables; a <Parameter> value
// reaches the installer; -D overrides both.
pub proof fn default_versus_parameter(c: Cmd)
    requires c.d_greeting.is_none(), c.d_mode.is_none()
    ensures
        configure_call(m36a(), c).greeting == NONE(),
        note_call(m36a(), c).greeting == HELLO(),
        configure_call(m36a(), c).mode == PARAM(),
{}

pub proof fn override_reaches_both(c: Cmd, g: int, md: int)
    requires c.d_greeting == Some(g), c.d_mode == Some(md)
    ensures configure_call(m36a(), c).greeting == g, note_call(m36a(), c).greeting == g, configure_call(m36a(), c).mode == md
{}

// IsUpgrade does not depend on whether the module was already installed.
pub proof fn upgrade_ignores_installed_state(m: Module, c: Cmd)
    requires m.installer, !c.fail
    ensures
        load(m, c, true).calls[0].upgrade == load(m, c, false).calls[0].upgrade,
        load(m, c, true).calls[0].upgrade == or(c.d_upgrade, 0),
        load(m, c, true).calls[0].dev == b(c.dev),
{}

// A refused OnInstallComponent stops before OnConfigureComponent and keeps no record or module.
pub proof fn refusal_stops(m: Module, c: Cmd)
    requires m.installer, c.fail
    ensures !load(m, c, false).ok, !load(m, c, false).installed, load(m, c, false).calls.len() == 1, load(m, c, false).calls[0].kind == ROLLED_BACK()
{}

pub proof fn no_installer_no_calls(c: Cmd)
    ensures load(m36n(), c, false).calls.len() == 0, uninstall(m36n()).calls.len() == 0
{}

pub proof fn schedule()
    ensures
        l1().calls.len() == 3, l1().calls[1].greeting == NONE(), l1().calls[2].greeting == HELLO(),
        l3().calls[0].upgrade == 0, l3().calls[1].greeting == BYE(),
        l5().calls[0].upgrade == 1, l2().calls[0].dev == 1,
        u1().calls[0].kind == UNCONFIGURE(), u1().calls[1].kind == UNINSTALL(),
        !e1().ok, !e1().installed, n1().calls.len() == 0,
{
    assert(l1().calls.len() == 3) by (compute);
    assert(l1().calls[1].greeting == NONE()) by (compute);
    assert(l1().calls[2].greeting == HELLO()) by (compute);
    assert(l3().calls[0].upgrade == 0) by (compute);
    assert(l3().calls[1].greeting == BYE()) by (compute);
    assert(l5().calls[0].upgrade == 1) by (compute);
    assert(l2().calls[0].dev == 1) by (compute);
    assert(u1().calls[0].kind == UNCONFIGURE()) by (compute);
    assert(u1().calls[1].kind == UNINSTALL()) by (compute);
    assert(!e1().ok) by (compute);
    assert(!e1().installed) by (compute);
    assert(n1().calls.len() == 0) by (compute);
}

} // verus!

fn main() {}
