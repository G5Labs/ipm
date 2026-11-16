#[path="../../../../Verus_verification/consolidated/iter62/interopres.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((v(a2(), 1)) == -1);assert((v(a2(), 1)) == -1); // a2.I62A.b
}
}
fn main() {}
