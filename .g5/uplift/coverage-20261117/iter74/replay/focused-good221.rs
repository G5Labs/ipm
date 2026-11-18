#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_221() {
assert((b(rec1(cli_install(i7())).row.committed)) == 0) by (compute_only);assert((b(rec1(cli_install(i7())).row.committed)) == 0); // i7.r1.committed
}
}
fn main() {}
