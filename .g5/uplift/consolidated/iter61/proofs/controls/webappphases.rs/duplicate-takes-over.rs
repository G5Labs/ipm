#![allow(non_snake_case)]
// Iteration61: WebApplication processor phases and attributes.
// Source roles at pin 571e04a1: ResourceProcessor/WebApplication.cls::CopyAttributes (NameSpace default,
// every declared attribute evaluated), OnBeforePhase (create/update on Activate and Configure, delete on
// Clean/Unconfigure), CreateOrUpdateWebApp (Modify with declared properties only), NameGet,
// OnGetUniqueName (duplicate guard), OnPhase (Validate). Premises (native-qualified): IRIS application
// names compare case-insensitively and ignore a trailing slash; Security.Applications rejects a
// non-numeric Timeout; the load's transaction rolls back the application; a new application's Timeout
// defaults to 900 (observed in run 1, kept under attempts/).
use vstd::prelude::*;

verus! {

// A property value: 0 = unset; other ints are codes.
pub struct App { pub desc: int, pub timeout: int }
pub struct Decl { pub name: int, pub desc: int, pub timeout: int, pub valid: bool }   // 0 = not declared
pub struct S { pub app: Option<App>, pub owner: int }   // owner = module id holding the name, 0 = none

pub open spec fn apply(a: App, d: Decl) -> App {
    App { desc: if d.desc != 0 { d.desc } else { a.desc }, timeout: if d.timeout != 0 { d.timeout } else { a.timeout } }
}
// install/load of module m declaring d (Activate runs the update).
pub open spec fn load(s: S, m: int, d: Decl) -> (S, bool) {
    if false { (s, false) }                       // duplicate name, case-insensitive
    else if !d.valid { (s, false) }                                      // rejected property, rolled back
    else {
        let base = match s.app { Some(a) => a, None => App { desc: 0, timeout: 900 } };   // IRIS default Timeout for a new application (native premise)
        (S { app: Some(apply(base, d)), owner: m }, true)
    }
}
pub open spec fn compile_only(s: S) -> S { s }
pub open spec fn edit_desc(s: S, v: int) -> S { match s.app { Some(a) => S { app: Some(App { desc: v, ..a }), ..s }, None => s } }
pub open spec fn uninstall(s: S, m: int) -> S { if s.owner == m { S { app: None, owner: 0 } } else { s } }

pub proof fn neighbors()
    ensures
        // removed attribute keeps its old value; changed attribute updates
        load(load(S { app: None, owner: 0 }, 1, Decl { name: 1, desc: 11, timeout: 0, valid: true }).0, 1, Decl { name: 1, desc: 0, timeout: 900, valid: true }).0.app == Some(App { desc: 11, timeout: 900 }),
        // compile -only keeps a manual edit; activate restores the declared value
        compile_only(edit_desc(S { app: Some(App { desc: 12, timeout: 900 }), owner: 1 }, 99)).app == Some(App { desc: 99, timeout: 900 }),
        load(edit_desc(S { app: Some(App { desc: 12, timeout: 900 }), owner: 1 }, 99), 1, Decl { name: 1, desc: 12, timeout: 900, valid: true }).0.app == Some(App { desc: 12, timeout: 900 }),
        // another module with the same (case-folded) name is refused and changes nothing
        !load(S { app: Some(App { desc: 12, timeout: 900 }), owner: 1 }, 2, Decl { name: 1, desc: 13, timeout: 0, valid: true }).1,
        // an invalid property fails with nothing kept
        load(S { app: None, owner: 0 }, 4, Decl { name: 4, desc: 0, timeout: 0, valid: false }).0.app.is_none(),
        // uninstall deletes the owner's application only
        uninstall(S { app: Some(App { desc: 12, timeout: 900 }), owner: 1 }, 1).app.is_none(),
        uninstall(S { app: Some(App { desc: 12, timeout: 900 }), owner: 1 }, 2).app.is_some(),
{}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
// Native schedule (desc codes: first 11, second 12, manual 99, other 13).
pub open spec fn s_w1() -> S { load(S { app: None, owner: 0 }, 1, Decl { name: 1, desc: 11, timeout: 0, valid: true }).0 }
pub open spec fn s_w2() -> S { load(s_w1(), 1, Decl { name: 1, desc: 0, timeout: 900, valid: true }).0 }
pub open spec fn s_w3() -> S { load(s_w2(), 1, Decl { name: 1, desc: 12, timeout: 900, valid: true }).0 }
pub open spec fn s_c1() -> S { compile_only(edit_desc(s_w3(), 99)) }
pub open spec fn s_c2() -> S { load(s_c1(), 1, Decl { name: 1, desc: 12, timeout: 900, valid: true }).0 }
pub open spec fn r_n1() -> (S, bool) { load(s_c2(), 2, Decl { name: 1, desc: 13, timeout: 0, valid: true }) }
pub open spec fn s_u1() -> S { uninstall(r_n1().0, 1) }
pub open spec fn desc(s: S) -> int { match s.app { Some(a) => a.desc, None => 0 } }
pub open spec fn timeout(s: S) -> int { match s.app { Some(a) => a.timeout, None => 0 } }

} // verus!

fn main() {}
