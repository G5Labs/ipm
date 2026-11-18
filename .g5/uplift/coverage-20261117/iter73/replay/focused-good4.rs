#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_4() {
assert((b(load_ok(xout(x2(), 902).resources, x2()))) == 1) by (compute_only);assert((b(load_ok(xout(x2(), 902).resources, x2()))) == 1); // x2.load.ok
}
}
fn main() {}
