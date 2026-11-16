#[path="../../../../Verus_verification/consolidated/iter61/webappphases.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(r_n1().1)) == 0) by (compute);assert((b(r_n1().1)) == 0); // n1.ok
}
}
fn main() {}
