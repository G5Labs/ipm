#[path="../../../../Verus_verification/consolidated/iter36/installer.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((e1().calls[0].kind) == 0) by (compute);assert((e1().calls[0].kind) == 1); // e1.call1.kind
}
}
fn main() {}
