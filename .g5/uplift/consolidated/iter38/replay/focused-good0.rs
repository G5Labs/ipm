#[path="../../../../Verus_verification/consolidated/iter38/render.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(summary(rec_z()).shows_unset)) == 1) by (compute);assert((b(summary(rec_z()).shows_unset)) == 1); // find.z.vunset
}
}
fn main() {}
