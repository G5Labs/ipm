#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_132() {
assert((code(rec1(cli_update(u1())).phases[5].name)) == 6) by (compute_only);assert((code(rec1(cli_update(u1())).phases[5].name)) == 7); // u1.r1.ph6
}
}
fn main() {}
