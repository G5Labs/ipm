#[path="../../../../Verus_verification/consolidated/iter33/dependents.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(warned(w2(), false, 1))) == 1) by (compute);assert((b(warned(w2(), false, 1))) == 1); // l2.warn.i33b
}
}
fn main() {}
