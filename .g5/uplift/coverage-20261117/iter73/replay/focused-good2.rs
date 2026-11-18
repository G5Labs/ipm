#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(load_ok(xout(x1(), 901).resources, x1()))) == 0) by (compute_only);assert((b(load_ok(xout(x1(), 901).resources, x1()))) == 0); // x1.load.ok
}
}
fn main() {}
