#[path="../../../../Verus_verification/consolidated/iter50/defmods.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
assert((b(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).ok)) == 0) by (compute);assert((b(default_modifiers(v0(), Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: empty() }).ok)) == 1); // f1.ok
}
}
fn main() {}
