#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(load_ok(m2a_out().resources, m2a()))) == 0) by (compute_only);assert((b(load_ok(m2a_out().resources, m2a()))) == 0); // m2a.load.ok
}
}
fn main() {}
