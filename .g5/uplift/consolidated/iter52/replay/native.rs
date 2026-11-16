#[path="../../../../Verus_verification/consolidated/iter52/record.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((code(first_install(2, false))) == 1);assert((code(first_install(2, false))) == 1); // l1.life
assert((b(load_changed(s0(), 1, 0, 2).old_class_defined)) == 1);assert((b(load_changed(s0(), 1, 0, 2).old_class_defined)) == 1); // c2.r2defined
assert((b(modified_changes(false))) == 1);assert((b(modified_changes(false))) == 1); // c1.modchanged
assert((code(reinstall(2))) == 2);assert((code(reinstall(2))) == 2); // l2.life
assert((code(first_install(5, true))) == 5);assert((code(first_install(5, true))) == 5); // p1.life
assert((code(unknown(3))) == 1);assert((code(unknown(3))) == 1); // n1.life
assert((b(first_install(2, false) == Life::Named(2))) == 0);assert((b(first_install(2, false) == Life::Named(2))) == 0); // l1.custom
assert((b(reinstall(2) == Life::Named(2))) == 1);assert((b(reinstall(2) == Life::Named(2))) == 1); // l2.custom
assert((b(first_install(5, true) == Life::Named(5))) == 1);assert((b(first_install(5, true) == Life::Named(5))) == 1); // p1.custom
assert((s0().record_resources as int) == 2);assert((s0().record_resources as int) == 2); // c0.resources
assert((s0().record_deps as int) == 1);assert((s0().record_deps as int) == 1); // c0.deps
assert((s0().default_value) == 1);assert((s0().default_value) == 1); // c0.greeting
assert((load_changed(s0(), 1, 0, 2).record_resources as int) == 1);assert((load_changed(s0(), 1, 0, 2).record_resources as int) == 1); // c2.resources
assert((load_changed(s0(), 1, 0, 2).record_deps as int) == 0);assert((load_changed(s0(), 1, 0, 2).record_deps as int) == 0); // c2.deps
assert((b(load_changed(s0(), 1, 0, 2).dep_installed)) == 1);assert((b(load_changed(s0(), 1, 0, 2).dep_installed)) == 1); // c2.dep52installed
assert((load_changed(s0(), 1, 0, 2).default_value) == 2);assert((load_changed(s0(), 1, 0, 2).default_value) == 2); // c2.greeting
assert((b(modified_changes(true))) == 1);assert((b(modified_changes(true))) == 1); // c2.modchanged
}
}
fn main() {}
