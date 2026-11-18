#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_104() {
assert((cli_reinstall(installed0(), missing(n_nosuch())).len()) == 0) by (compute_only);assert((cli_reinstall(installed0(), missing(n_nosuch())).len()) == 1); // r1.rows
}
}
fn main() {}
