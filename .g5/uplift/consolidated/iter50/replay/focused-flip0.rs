#[path="../../../../Verus_verification/consolidated/iter50/defmods.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((mode_code(effective(s3(), seq![None, Some(3int), None, None], Some(4), 1))) == 3) by (compute);assert((mode_code(effective(s3(), seq![None, Some(3int), None, None], Some(4), 1))) == 4); // l2.mode
}
}
fn main() {}
