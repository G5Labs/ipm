#[path="../../../../Verus_verification/consolidated/iter31/installed.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
resolution_neighbor();reference_neighbor();directory_neighbor();assert((code(s6_load20().world.b)) == 201); // s6-load20.b
}
}
fn main() {}
