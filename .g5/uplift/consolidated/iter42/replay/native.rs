#[path="../../../../Verus_verification/consolidated/iter42/initialize.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(b1().ok)) == 1) by (compute);assert((b(b1().ok)) == 1); // b1.ok
assert((i2().world.boot) == 1) by (compute);assert((i2().world.boot) == 1); // i2.boot
assert((b(ordinary_compile_ok(true))) == 0) by (compute);assert((b(ordinary_compile_ok(true))) == 0); // c1.ok
assert((b(setup().ok)) == 1) by (compute);assert((b(setup().ok)) == 1); // setup.ok
assert((b(setup().skipped)) == 0) by (compute);assert((b(setup().skipped)) == 0); // setup.skip
assert((setup().world.boot) == 1) by (compute);assert((setup().world.boot) == 1); // setup.boot
assert((b(i1().ok)) == 1) by (compute);assert((b(i1().ok)) == 1); // i1.ok
assert((b(i1().skipped)) == 1) by (compute);assert((b(i1().skipped)) == 1); // i1.skip
assert((i1().world.boot) == 1) by (compute);assert((i1().world.boot) == 1); // i1.boot
assert((b(i2().ok)) == 1) by (compute);assert((b(i2().ok)) == 1); // i2.ok
assert((b(i2().skipped)) == 1) by (compute);assert((b(i2().skipped)) == 1); // i2.skip
assert((b(i3().ok)) == 1) by (compute);assert((b(i3().ok)) == 1); // i3.ok
assert((b(i3().skipped)) == 0) by (compute);assert((b(i3().skipped)) == 0); // i3.skip
assert((i3().world.boot) == 2) by (compute);assert((i3().world.boot) == 2); // i3.boot
assert((b(i4().ok)) == 1) by (compute);assert((b(i4().ok)) == 1); // i4.ok
assert((b(i4().skipped)) == 0) by (compute);assert((b(i4().skipped)) == 0); // i4.skip
assert((i4().world.boot) == 3) by (compute);assert((i4().world.boot) == 3); // i4.boot
assert((b(b1().skipped)) == 0) by (compute);assert((b(b1().skipped)) == 0); // b1.skip
assert((b1().world.boot) == 3) by (compute);assert((b1().world.boot) == 3); // b1.boot
assert((b(b1().world.broken_compiled)) == 0) by (compute);assert((b(b1().world.broken_compiled)) == 0); // b1.bbroken
}
}
fn main() {}
