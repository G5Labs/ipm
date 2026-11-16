#[path="../../../../Verus_verification/consolidated/iter51/toplevel.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_1() {
reveal_with_fuel(is_dep, 3); assert(((if label_full(M { name: 3, external: true, has_build: true }, true) { 1int } else { 0int })) == 1);assert(((if label_full(M { name: 3, external: true, has_build: true }, true) { 1int } else { 0int })) == 0); // i2.listed.extfull
}
}
fn main() {}
