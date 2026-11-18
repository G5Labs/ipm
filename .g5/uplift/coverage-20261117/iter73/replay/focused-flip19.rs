#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_19() {
assert((b(one_test(m1_out()) && only_test(m1_out()).package == seq![211int])) == 1) by (compute_only);assert((b(one_test(m1_out()) && only_test(m1_out()).package == seq![211int])) == 0); // m1.unittest.package
}
}
fn main() {}
