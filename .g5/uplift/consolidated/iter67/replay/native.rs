#[path="../../../../Verus_verification/consolidated/iter67/composite.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(read_only(cfg(true, true, false, true)))) == 0);assert((b(read_only(cfg(true, true, false, true)))) == 0); // r1.ro
assert((b(create_ok(false))) == 0);assert((b(create_ok(false))) == 0); // s1.new
assert((b(editable(cfg(true, false, false, false)))) == 0);assert((b(editable(cfg(true, false, false, false)))) == 0); // g1.editable
assert((b(editable(cfg(true, true, false, false)))) == 1);assert((b(editable(cfg(true, true, false, false)))) == 1); // g2.editable
assert((b(editable(cfg(false, true, false, false)))) == 0);assert((b(editable(cfg(false, true, false, false)))) == 0); // g3.editable
assert((b(read_only(cfg(true, true, true, false)))) == 1);assert((b(read_only(cfg(true, true, true, false)))) == 1); // r2.ro
assert((b(read_only(cfg(true, true, false, false)))) == 0);assert((b(read_only(cfg(true, true, false, false)))) == 0); // r3.ro
assert((status_calls(cfg(true, true, false, false)).0) == 2);assert((status_calls(cfg(true, true, false, false)).0) == 2); // g1.p
assert((status_calls(cfg(true, true, false, false)).1) == 1);assert((status_calls(cfg(true, true, false, false)).1) == 1); // g1.o
assert((status_calls(cfg(true, true, false, false)).0) == 2);assert((status_calls(cfg(true, true, false, false)).0) == 2); // g2.p
assert((status_calls(cfg(true, true, false, false)).1) == 1);assert((status_calls(cfg(true, true, false, false)).1) == 1); // g2.o
assert((status_calls(cfg(true, true, false, false)).0) == 2);assert((status_calls(cfg(true, true, false, false)).0) == 2); // g3.p
assert((status_calls(cfg(true, true, false, false)).1) == 1);assert((status_calls(cfg(true, true, false, false)).1) == 1); // g3.o
assert((ro_calls(cfg(true, true, false, false)).0) == 1);assert((ro_calls(cfg(true, true, false, false)).0) == 1); // r1.p
assert((ro_calls(cfg(true, true, false, false)).1) == 0);assert((ro_calls(cfg(true, true, false, false)).1) == 0); // r1.o
assert((ro_calls(cfg(true, true, false, false)).0) == 1);assert((ro_calls(cfg(true, true, false, false)).0) == 1); // r2.p
assert((ro_calls(cfg(true, true, false, false)).1) == 0);assert((ro_calls(cfg(true, true, false, false)).1) == 0); // r2.o
assert((ro_calls(cfg(true, true, false, false)).0) == 1);assert((ro_calls(cfg(true, true, false, false)).0) == 1); // r3.p
assert((ro_calls(cfg(true, true, false, false)).1) == 0);assert((ro_calls(cfg(true, true, false, false)).1) == 0); // r3.o
assert((per_item(cfg(true, true, false, false)).0 * 6) == 6);assert((per_item(cfg(true, true, false, false)).0 * 6) == 6); // a1.p
assert((per_item(cfg(true, true, false, false)).1) == 0);assert((per_item(cfg(true, true, false, false)).1) == 0); // a1.o
assert((per_item(cfg(true, true, false, false)).0 * 6) == 6);assert((per_item(cfg(true, true, false, false)).0 * 6) == 6); // a2.p
assert((per_item(cfg(true, true, false, false)).1) == 0);assert((per_item(cfg(true, true, false, false)).1) == 0); // a2.o
assert((per_item(cfg(true, true, false, false)).0 * 6) == 6);assert((per_item(cfg(true, true, false, false)).0 * 6) == 6); // a3.p
assert((per_item(cfg(true, true, false, false)).1) == 0);assert((per_item(cfg(true, true, false, false)).1) == 0); // a3.o
assert((user_action(cfg(true, true, false, false), 1, false).0) == 1);assert((user_action(cfg(true, true, false, false), 1, false).0) == 1); // u1.p
assert((user_action(cfg(true, true, false, false), 1, false).1) == 0);assert((user_action(cfg(true, true, false, false), 1, false).1) == 0); // u1.o
assert((user_action(cfg(true, true, false, false), 0, true).0) == 0);assert((user_action(cfg(true, true, false, false), 0, true).0) == 0); // u2.p
assert((user_action(cfg(true, true, false, false), 0, true).1) == 1);assert((user_action(cfg(true, true, false, false), 0, true).1) == 1); // u2.o
assert((user_action(cfg(true, true, false, false), 0, false).0) == 0);assert((user_action(cfg(true, true, false, false), 0, false).0) == 0); // u3.p
assert((user_action(cfg(true, true, false, false), 0, false).1) == 0);assert((user_action(cfg(true, true, false, false), 0, false).1) == 0); // u3.o
assert((b(configure_ok(false))) == 1);assert((b(configure_ok(false))) == 1); // s1.config
assert((b(primary_after_set(1, 0, false) == 1)) == 1);assert((b(primary_after_set(1, 0, false) == 1)) == 1); // e1.primary.p67
}
}
fn main() {}
