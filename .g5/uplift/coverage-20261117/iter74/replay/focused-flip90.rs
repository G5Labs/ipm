#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_90() {
assert((nph(rec1(cli_install(i5())))) == 4) by (compute_only);assert((nph(rec1(cli_install(i5())))) == 5); // i5.r1.nph
}
}
fn main() {}
