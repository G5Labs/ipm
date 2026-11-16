#[path="../../../../Verus_verification/consolidated/iter41/settings.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(c5().ok)) == 1) by (compute);assert((b(c5().ok)) == 0); // c5.ok
}
}
fn main() {}
