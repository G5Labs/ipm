#[path="../../../../Verus_verification/consolidated/iter33/dependents.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(listed(w0(), false, A(), 0, 6))) == 1) by (compute);assert((b(listed(w0(), false, A(), 0, 6))) == 1); // l0.list.i33g
}
}
fn main() {}
