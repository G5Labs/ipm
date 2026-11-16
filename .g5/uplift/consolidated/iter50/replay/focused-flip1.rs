#[path="../../../../Verus_verification/consolidated/iter50/defmods.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((mode_code(effective(s3(), empty(), Some(4), 1))) == 2) by (compute);assert((mode_code(effective(s3(), empty(), Some(4), 1))) == 3); // l1.mode
}
}
fn main() {}
