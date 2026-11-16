#[path="../../../../Verus_verification/consolidated/iter52/record.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(load_changed(s0(), 1, 0, 2).old_class_defined)) == 1);assert((b(load_changed(s0(), 1, 0, 2).old_class_defined)) == 0); // c2.r2defined
}
}
fn main() {}
