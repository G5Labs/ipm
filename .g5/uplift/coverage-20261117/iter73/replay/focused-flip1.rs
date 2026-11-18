#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(load_ok(m2b_out().resources, m2b()))) == 0) by (compute_only);assert((b(load_ok(m2b_out().resources, m2b()))) == 1); // m2b.load.ok
}
}
fn main() {}
