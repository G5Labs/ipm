#[path="../../../../Verus_verification/consolidated/iter33/dependents.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(listed(w0(), false, A(), 0, 0))) == 0) by (compute);assert((b(listed(w0(), false, A(), 0, 0))) == 1); // l0.list.i33a
}
}
fn main() {}
