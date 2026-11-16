#[path="../../../../Verus_verification/consolidated/iter66/docphase.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(document(W { d_installed: false, d_offered: true }, 2, true).not_installed)) == 1);assert((b(document(W { d_installed: false, d_offered: true }, 2, true).not_installed)) == 1); // c6.notinstalled
}
}
fn main() {}
