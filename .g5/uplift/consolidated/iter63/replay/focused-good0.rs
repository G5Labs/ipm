#[path="../../../../Verus_verification/consolidated/iter63/cleanup.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
assert((b(remains(Kind::DeclaredMapping, false))) == 1);assert((b(remains(Kind::DeclaredMapping, false))) == 1); // x1.map.pkg
}
}
fn main() {}
