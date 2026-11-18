#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_287() {
uninstall_fields();assert((b(n5_rs()[0].row.committed)) == 1); // n5.r1.committed
}
}
fn main() {}
