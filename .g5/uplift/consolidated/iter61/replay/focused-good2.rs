#[path="../../../../Verus_verification/consolidated/iter61/webappphases.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(desc(s_w2()) == 11)) == 1) by (compute);assert((b(desc(s_w2()) == 11)) == 1); // w2.a.desc.first
}
}
fn main() {}
