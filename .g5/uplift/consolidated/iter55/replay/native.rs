#[path="../../../../Verus_verification/consolidated/iter55/nsinit.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(match init_quiet(Sc::Plain, false, false).active { Sc::Plain => true, _ => false })) == 1);assert((b(match init_quiet(Sc::Plain, false, false).active { Sc::Plain => true, _ => false })) == 1); // i2.sc.base
assert((b(!zn(Ns::Known { ipm: false }, false).switched)) == 1);assert((b(!zn(Ns::Known { ipm: false }, false).switched)) == 1); // z1.after.user
assert((b(!exec(Expr::SetNamespace).namespace_changed)) == 1);assert((b(!exec(Expr::SetNamespace).namespace_changed)) == 1); // e2.after.user
assert((b(init_quiet(Sc::Plain, false, false).stored_primary_plain)) == 1);assert((b(init_quiet(Sc::Plain, false, false).stored_primary_plain)) == 1); // i2.sc.storedprimarybase
assert((b(init_quiet(Sc::Plain, false, false).claims_enabled)) == 1);assert((b(init_quiet(Sc::Plain, false, false).claims_enabled)) == 1); // i2.msg.enabling
assert((b(match init_quiet(Sc::Plain, false, false).active { Sc::Composite => true, _ => false })) == 0);assert((b(match init_quiet(Sc::Plain, false, false).active { Sc::Composite => true, _ => false })) == 0); // i2.sc.activecomposite
assert((b(match init_quiet(Sc::Plain, true, true).active { Sc::Composite => true, _ => false })) == 1);assert((b(match init_quiet(Sc::Plain, true, true).active { Sc::Composite => true, _ => false })) == 1); // i3.sc.activecomposite
assert((b(match init_quiet(Sc::Empty, false, false).active { Sc::Empty => true, _ => false })) == 1);assert((b(match init_quiet(Sc::Empty, false, false).active { Sc::Empty => true, _ => false })) == 1); // i1.sc.empty
assert((b(match init_quiet(Sc::Dangling, true, false).active { Sc::Empty => true, _ => false })) == 1);assert((b(match init_quiet(Sc::Dangling, true, false).active { Sc::Empty => true, _ => false })) == 1); // i4.sc.empty
assert((b(zn(Ns::Known { ipm: false }, false).ok)) == 1);assert((b(zn(Ns::Known { ipm: false }, false).ok)) == 1); // z1.ok
assert((b(zn(Ns::Known { ipm: false }, false).not_enabled_msg)) == 1);assert((b(zn(Ns::Known { ipm: false }, false).not_enabled_msg)) == 1); // z1.notenabled
assert((b(zn(Ns::Unknown, false).ok)) == 0);assert((b(zn(Ns::Unknown, false).ok)) == 0); // z2.ok
assert((b(!zn(Ns::Unknown, false).switched)) == 1);assert((b(!zn(Ns::Unknown, false).switched)) == 1); // z2.after.user
assert((b(zn(Ns::Known { ipm: false }, false).ok)) == 1);assert((b(zn(Ns::Known { ipm: false }, false).ok)) == 1); // z4.ok
assert((b(zn(Ns::Known { ipm: false }, false).switched)) == 0);assert((b(zn(Ns::Known { ipm: false }, false).switched)) == 0); // z4.after.sys
assert((b(zn(Ns::Known { ipm: true }, true).ok)) == 1);assert((b(zn(Ns::Known { ipm: true }, true).ok)) == 1); // z3.ok
assert((b(!zn(Ns::Known { ipm: true }, true).switched)) == 1);assert((b(!zn(Ns::Known { ipm: true }, true).switched)) == 1); // z3.after.user
assert((b(exec(Expr::SetGlobal).global_set)) == 1);assert((b(exec(Expr::SetGlobal).global_set)) == 1); // e1.global
assert((b(exec(Expr::SetGlobal).ok)) == 1);assert((b(exec(Expr::SetGlobal).ok)) == 1); // e1.ok
assert((b(exec(Expr::RuntimeError).ok)) == 0);assert((b(exec(Expr::RuntimeError).ok)) == 0); // e3.ok
assert((b(exec(Expr::SyntaxError).ok)) == 0);assert((b(exec(Expr::SyntaxError).ok)) == 0); // e4.ok
assert((b(exec(Expr::SetNamespace).ok)) == 1);assert((b(exec(Expr::SetNamespace).ok)) == 1); // e2.ok
}
}
fn main() {}
