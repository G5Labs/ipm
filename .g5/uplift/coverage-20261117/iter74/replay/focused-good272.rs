#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_272() {
assert((b(n4_rs()[0].row.committed)) == 0) by (compute_only);assert((b(n4_rs()[0].row.committed)) == 0); // n4.r1.committed
}
}
fn main() {}
