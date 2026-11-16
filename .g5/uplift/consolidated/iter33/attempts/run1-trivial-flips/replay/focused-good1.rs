#[path="../../../../Verus_verification/consolidated/iter33/dependents.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(any_warned_from(w0(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), false, 0))) == 0); // l0.warning
}
}
fn main() {}
