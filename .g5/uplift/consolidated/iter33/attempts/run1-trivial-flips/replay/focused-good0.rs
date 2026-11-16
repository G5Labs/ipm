#[path="../../../../Verus_verification/consolidated/iter33/dependents.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l0.ok
}
}
fn main() {}
