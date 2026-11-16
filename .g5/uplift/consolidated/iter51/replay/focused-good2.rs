#[path="../../../../Verus_verification/consolidated/iter51/toplevel.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observation_2() {
reveal_with_fuel(is_dep, 3); assert(((if shows_build(M { name: 3, external: true, has_build: true }) { 1int } else { 0int })) == 0);assert(((if shows_build(M { name: 3, external: true, has_build: true }) { 1int } else { 0int })) == 0); // i1.listed.extbuild
}
}
fn main() {}
