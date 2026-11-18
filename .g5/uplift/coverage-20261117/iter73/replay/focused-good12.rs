#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_12() {
assert((b(one_test(m4_out()) && only_test(m4_out()).name == seq![65, 64int])) == 1) by (compute_only);assert((b(one_test(m4_out()) && only_test(m4_out()).name == seq![65, 64int])) == 1); // m4.unittest.name
}
}
fn main() {}
