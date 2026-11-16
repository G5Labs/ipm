#[path="../../../../Verus_verification/consolidated/iter46/summary.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(sysreq(d()))) == 0);assert((b(sysreq(d()))) == 1); // d.sysreq.heading
}
}
fn main() {}
