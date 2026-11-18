#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_3() {
assert((b(load_ok(xout(x4(), 904).resources, x4()))) == 0) by (compute_only);assert((b(load_ok(xout(x4(), 904).resources, x4()))) == 0); // x4.load.ok
}
}
fn main() {}
