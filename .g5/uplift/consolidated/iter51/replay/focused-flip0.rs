#[path="../../../../Verus_verification/consolidated/iter51/toplevel.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_0() {
reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 2, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0);assert(((if listed(M { name: 2, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 1); // i1.listed.mixed
}
}
fn main() {}
