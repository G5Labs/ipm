#[path="../../../../Verus_verification/consolidated/iter67/composite.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(editable(cfg(true, false, false, false)))) == 0);assert((b(editable(cfg(true, false, false, false)))) == 0); // g1.editable
}
}
fn main() {}
