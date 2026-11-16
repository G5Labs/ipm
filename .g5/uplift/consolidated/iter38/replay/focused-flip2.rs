#[path="../../../../Verus_verification/consolidated/iter38/render.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(detail_phases(rec_e())[4].success)) == 0) by (compute);assert((b(detail_phases(rec_e())[4].success)) == 1); // det.e.p5.success
}
}
fn main() {}
