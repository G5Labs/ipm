#[path="../../../../Verus_verification/consolidated/iter42/initialize.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((i2().world.boot) == 1) by (compute);assert((i2().world.boot) == 0); // i2.boot
}
}
fn main() {}
