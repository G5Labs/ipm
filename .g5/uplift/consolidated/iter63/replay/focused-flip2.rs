#[path="../../../../Verus_verification/consolidated/iter63/cleanup.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(remains(Kind::GblAttr, false))) == 0);assert((b(remains(Kind::GblAttr, false))) == 1); // x1.gbl.attr
}
}
fn main() {}
