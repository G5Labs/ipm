#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_243() {
uninstall_fields();assert((b(n1_rs()[0].row.committed)) == 0); // n1.r1.committed
}
}
fn main() {}
