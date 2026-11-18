#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_102() {
assert((b(rec1(cli_install(i5())).phases[3].ended)) == 1) by (compute_only);assert((b(rec1(cli_install(i5())).phases[3].ended)) == 1); // i5.r1.ph4.ended
}
}
fn main() {}
