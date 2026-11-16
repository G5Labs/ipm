#[path="../../../../Verus_verification/consolidated/iter38/render.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(summary(rec_i()).na)) == 0) by (compute);assert((b(summary(rec_i()).na)) == 1); // find.i.na
}
}
fn main() {}
