#[path="../../../../Verus_verification/coverage-20261117/iter74/actions.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_175() {
assert((b(rec1(cli_update(u3())).row.committed)) == 1) by (compute_only);assert((b(rec1(cli_update(u3())).row.committed)) == 1); // u3.r1.committed
}
}
fn main() {}
