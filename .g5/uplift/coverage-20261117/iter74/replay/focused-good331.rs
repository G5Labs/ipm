#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_331() {
assert((act(t1_row())) == 1) by (compute_only);assert((act(t1_row())) == 1); // t1.r1.action
}
}
fn main() {}
