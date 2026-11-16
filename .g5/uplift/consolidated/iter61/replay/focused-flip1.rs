#[path="../../../../Verus_verification/consolidated/iter61/webappphases.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((b(desc(s_c1()) == 99)) == 1) by (compute);assert((b(desc(s_c1()) == 99)) == 0); // c1.a.desc.manual
}
}
fn main() {}
