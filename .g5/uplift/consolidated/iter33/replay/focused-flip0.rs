#[path="../../../../Verus_verification/consolidated/iter33/dependents.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(tree_has(w0(), A(), 3))) == 0) by (compute);assert((b(tree_has(w0(), A(), 3))) == 1); // l0t.count.i33d
}
}
fn main() {}
