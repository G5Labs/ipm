#[path="../../../../Verus_verification/coverage-20261117/iter69/grouping.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(loads_class(m3_out().resources, m3(), m3()[0]))) == 0) by (compute_only);assert((b(loads_class(m3_out().resources, m3(), m3()[0]))) == 0); // class.Id1.B.C
}
}
fn main() {}
