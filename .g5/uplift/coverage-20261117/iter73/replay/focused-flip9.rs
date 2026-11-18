#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_9() {
assert((b(loads_class(m7_out().resources, m7(), m7()[1]))) == 0) by (compute_only);assert((b(loads_class(m7_out().resources, m7(), m7()[1]))) == 1); // class.Im7.Y
}
}
fn main() {}
