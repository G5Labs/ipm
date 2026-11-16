#[path="../../../../Verus_verification/consolidated/iter47/formats.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((toon_rows(0, true) as int) == 2) by (compute);assert((toon_rows(0, true) as int) == 3); // toonf.rows.TestFThrow
}
}
fn main() {}
