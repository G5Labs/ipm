#[path="../../../../Verus_verification/consolidated/iter51/toplevel.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 2, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0);assert(((if listed(M { name: 2, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0); // i1.listed.mixed
reveal_with_fuel(is_dep, 3); assert(((if label_full(M { name: 3, external: true, has_build: true }, true) { 1int } else { 0int })) == 1);assert(((if label_full(M { name: 3, external: true, has_build: true }, true) { 1int } else { 0int })) == 1); // i2.listed.extfull
reveal_with_fuel(is_dep, 3); assert(((if shows_build(M { name: 3, external: true, has_build: true }) { 1int } else { 0int })) == 0);assert(((if shows_build(M { name: 3, external: true, has_build: true }) { 1int } else { 0int })) == 0); // i1.listed.extbuild
reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 4, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 1);assert(((if listed(M { name: 4, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 1); // i1.listed.top51
reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 1, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0);assert(((if listed(M { name: 1, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0); // i1.listed.dep51
reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 3, external: true, has_build: true }, deps()) { 1int } else { 0int })) == 1);assert(((if listed(M { name: 3, external: true, has_build: true }, deps()) { 1int } else { 0int })) == 1); // i1.listed.ext
reveal_with_fuel(is_dep, 3); assert(((if label_full(M { name: 3, external: true, has_build: true }, false) { 1int } else { 0int })) == 0);assert(((if label_full(M { name: 3, external: true, has_build: true }, false) { 1int } else { 0int })) == 0); // i1.listed.extfull
reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 4, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 1);assert(((if listed(M { name: 4, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 1); // i2.listed.top51
reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 1, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0);assert(((if listed(M { name: 1, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0); // i2.listed.dep51
reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 2, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0);assert(((if listed(M { name: 2, external: false, has_build: false }, deps()) { 1int } else { 0int })) == 0); // i2.listed.mixed
reveal_with_fuel(is_dep, 3); assert(((if listed(M { name: 3, external: true, has_build: true }, deps()) { 1int } else { 0int })) == 1);assert(((if listed(M { name: 3, external: true, has_build: true }, deps()) { 1int } else { 0int })) == 1); // i2.listed.ext
}
}
fn main() {}
