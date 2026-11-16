#[path="../../../../Verus_verification/consolidated/iter42/initialize.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(b1().ok)) == 1) by (compute);assert((b(b1().ok)) == 1); // b1.ok
}
}
fn main() {}
