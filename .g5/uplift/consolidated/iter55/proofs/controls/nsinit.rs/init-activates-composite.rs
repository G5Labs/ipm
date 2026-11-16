#![allow(non_snake_case)]
// Iteration55: `namespace`/`zn`, `exec`/`cos` and `init -noprompt` decisions.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter55/bindings.json):
//   Main.cls::Namespace, IsIPMEnabled   exact name: switch only when %IPM.Main.Shell is visible there; otherwise print
//                                       "IPM is not enabled" and return OK; an unknown namespace raises "Failed to switch".
//                                       Shell does not restore $namespace, so a switch persists for the caller.
//   Main.cls::ExecuteCOS                `new $namespace` then xecute: data effects persist, a namespace change does not;
//                                       runtime and syntax errors fail the command.
//   Main.cls::Init (-noprompt)          dangling source-control class disabled; a plain existing class is stored as the
//                                       Composite's primary but stays the active namespace class; -extension X makes the
//                                       Composite active with primary X.
// Premises (native-qualified): namespace existence and class visibility as observed; the Studio source-control
// setting and Composite storage APIs.
use vstd::prelude::*;

verus! {

pub enum Ns { Known { ipm: bool }, Unknown }
pub struct Zn { pub ok: bool, pub switched: bool, pub not_enabled_msg: bool }
pub open spec fn zn(target: Ns, is_current: bool) -> Zn {
    match target {
        Ns::Unknown => Zn { ok: false, switched: false, not_enabled_msg: false },
        Ns::Known { ipm } => if ipm { Zn { ok: true, switched: !is_current, not_enabled_msg: false } }
                             else { Zn { ok: true, switched: false, not_enabled_msg: true } },
    }
}

pub enum Expr { SetGlobal, SetNamespace, RuntimeError, SyntaxError }
pub struct Exec { pub ok: bool, pub global_set: bool, pub namespace_changed: bool }
pub open spec fn exec(e: Expr) -> Exec {
    match e {
        Expr::SetGlobal => Exec { ok: true, global_set: true, namespace_changed: false },
        Expr::SetNamespace => Exec { ok: true, global_set: false, namespace_changed: false },
        _ => Exec { ok: false, global_set: false, namespace_changed: false },
    }
}

pub enum Sc { Empty, Plain, Dangling, Composite }
pub struct Init { pub active: Sc, pub stored_primary_plain: bool, pub claims_enabled: bool }
// stored: whether the Composite already stores the plain class as primary.
pub open spec fn init_quiet(old: Sc, stored: bool, extension_plain: bool) -> Init {
    let cleaned = match old { Sc::Dangling => Sc::Empty, _ => old };
    let wrapped = match cleaned { Sc::Plain => true, _ => false };
    let stored2 = stored || wrapped || extension_plain;
    if extension_plain { Init { active: Sc::Composite, stored_primary_plain: stored2, claims_enabled: wrapped } }
    else { Init { active: if wrapped { Sc::Composite } else { cleaned }, stored_primary_plain: stored2, claims_enabled: wrapped } }
}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub proof fn zn_neighbors()
    ensures zn(Ns::Known { ipm: false }, false).ok, !zn(Ns::Known { ipm: false }, false).switched,
            zn(Ns::Known { ipm: false }, false).not_enabled_msg,
            !zn(Ns::Unknown, false).ok, !zn(Ns::Unknown, false).switched,
            zn(Ns::Known { ipm: true }, false).switched, !zn(Ns::Known { ipm: true }, true).switched
{}

pub proof fn exec_neighbors()
    ensures exec(Expr::SetGlobal).global_set, exec(Expr::SetNamespace).ok, !exec(Expr::SetNamespace).namespace_changed,
            !exec(Expr::RuntimeError).ok, !exec(Expr::SyntaxError).ok
{}

// As built, init -noprompt reports enabling the package manager extension around a plain class, but the plain
// class stays active, so the Composite (and IPM's hooks) is not in effect; -extension makes it active.
pub proof fn init_neighbors()
    ensures init_quiet(Sc::Empty, false, false).active == Sc::Empty,
            init_quiet(Sc::Plain, false, false).active == Sc::Plain,
            init_quiet(Sc::Plain, false, false).stored_primary_plain,
            init_quiet(Sc::Plain, false, false).claims_enabled,
            init_quiet(Sc::Plain, true, true).active == Sc::Composite,
            init_quiet(Sc::Dangling, true, false).active == Sc::Empty
{}

} // verus!

fn main() {}
