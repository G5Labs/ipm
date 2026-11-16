#[path="../../../../Verus_verification/consolidated/iter62/interopres.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
assert((v(a3(), 0)) == -1);assert((v(a3(), 0)) == -1); // a3.I62A.a
}
}
fn main() {}
