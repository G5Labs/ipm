#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_116() {
assert((nph(rec1(cli_update(u1())))) == 6) by (compute_only);assert((nph(rec1(cli_update(u1())))) == 6); // u1.r1.nph
}
}
fn main() {}
