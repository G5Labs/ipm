#[path="../../../../Verus_verification/consolidated/iter40/unittests.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(d0().t_compiled)) == 0) by (compute);assert((b(d0().t_compiled)) == 0); // d0.testscompiled
}
}
fn main() {}
