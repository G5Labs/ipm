#[path="../../../../Verus_verification/consolidated/iter36/installer.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((l3().calls[0].upgrade) == 0) by (compute);assert((l3().calls[0].upgrade) == 1); // l3.call1.upgrade
}
}
fn main() {}
