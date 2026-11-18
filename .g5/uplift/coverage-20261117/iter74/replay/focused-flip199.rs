#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_199() {
assert((cli_update(u5()).len()) == 0) by (compute_only);assert((cli_update(u5()).len()) == 1); // u5.rows
}
}
fn main() {}
