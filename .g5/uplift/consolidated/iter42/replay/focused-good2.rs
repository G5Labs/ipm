#[path="../../../../Verus_verification/consolidated/iter42/initialize.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(ordinary_compile_ok(true))) == 0) by (compute);assert((b(ordinary_compile_ok(true))) == 0); // c1.ok
}
}
fn main() {}
