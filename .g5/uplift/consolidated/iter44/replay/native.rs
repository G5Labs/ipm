#[path="../../../../Verus_verification/consolidated/iter44/comments.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(after(c(4), true, stored(4)))) == 0) by (compute);assert((b(after(c(4), true, stored(4)))) == 0); // s1.c4.count
assert((b(arrange(false).ok)) == 0) by (compute);assert((b(arrange(false).ok)) == 0); // a1.ok
assert((b(after(c(1), true, stored(1)))) == 1) by (compute);assert((b(after(c(1), true, stored(1)))) == 1); // s1.c1.count
assert((b(stored(0))) == 0) by (compute);assert((b(stored(0))) == 0); // load.c0.count
assert((b(after(c(0), false, stored(0)))) == 0) by (compute);assert((b(after(c(0), false, stored(0)))) == 0); // a1.c0.count
assert((b(after(c(0), false, stored(0)))) == 0) by (compute);assert((b(after(c(0), false, stored(0)))) == 0); // a2.c0.count
assert((b(after(c(0), true, stored(0)))) == 0) by (compute);assert((b(after(c(0), true, stored(0)))) == 0); // s1.c0.count
assert((b(after(c(0), true, stored(0)))) == 0) by (compute);assert((b(after(c(0), true, stored(0)))) == 0); // s2.c0.count
assert((b(stored(1))) == 1) by (compute);assert((b(stored(1))) == 1); // load.c1.count
assert((b(after(c(1), false, stored(1)))) == 1) by (compute);assert((b(after(c(1), false, stored(1)))) == 1); // a1.c1.count
assert((b(after(c(1), false, stored(1)))) == 1) by (compute);assert((b(after(c(1), false, stored(1)))) == 1); // a2.c1.count
assert((b(after(c(1), true, stored(1)))) == 1) by (compute);assert((b(after(c(1), true, stored(1)))) == 1); // s2.c1.count
assert((b(stored(2))) == 1) by (compute);assert((b(stored(2))) == 1); // load.c2.count
assert((b(after(c(2), false, stored(2)))) == 1) by (compute);assert((b(after(c(2), false, stored(2)))) == 1); // a1.c2.count
assert((b(after(c(2), false, stored(2)))) == 1) by (compute);assert((b(after(c(2), false, stored(2)))) == 1); // a2.c2.count
assert((b(after(c(2), true, stored(2)))) == 1) by (compute);assert((b(after(c(2), true, stored(2)))) == 1); // s1.c2.count
assert((b(after(c(2), true, stored(2)))) == 1) by (compute);assert((b(after(c(2), true, stored(2)))) == 1); // s2.c2.count
assert((b(stored(3))) == 1) by (compute);assert((b(stored(3))) == 1); // load.c3.count
assert((b(after(c(3), false, stored(3)))) == 1) by (compute);assert((b(after(c(3), false, stored(3)))) == 1); // a1.c3.count
assert((b(after(c(3), false, stored(3)))) == 1) by (compute);assert((b(after(c(3), false, stored(3)))) == 1); // a2.c3.count
assert((b(after(c(3), true, stored(3)))) == 1) by (compute);assert((b(after(c(3), true, stored(3)))) == 1); // s1.c3.count
assert((b(after(c(3), true, stored(3)))) == 1) by (compute);assert((b(after(c(3), true, stored(3)))) == 1); // s2.c3.count
assert((b(stored(4))) == 1) by (compute);assert((b(stored(4))) == 1); // load.c4.count
assert((b(after(c(4), false, stored(4)))) == 1) by (compute);assert((b(after(c(4), false, stored(4)))) == 1); // a1.c4.count
assert((b(after(c(4), false, stored(4)))) == 1) by (compute);assert((b(after(c(4), false, stored(4)))) == 1); // a2.c4.count
assert((b(after(c(4), true, stored(4)))) == 0) by (compute);assert((b(after(c(4), true, stored(4)))) == 0); // s2.c4.count
assert((b(stored(5))) == 1) by (compute);assert((b(stored(5))) == 1); // load.c5.count
assert((b(after(c(5), false, stored(5)))) == 1) by (compute);assert((b(after(c(5), false, stored(5)))) == 1); // a1.c5.count
assert((b(after(c(5), false, stored(5)))) == 1) by (compute);assert((b(after(c(5), false, stored(5)))) == 1); // a2.c5.count
assert((b(after(c(5), true, stored(5)))) == 0) by (compute);assert((b(after(c(5), true, stored(5)))) == 0); // s1.c5.count
assert((b(after(c(5), true, stored(5)))) == 0) by (compute);assert((b(after(c(5), true, stored(5)))) == 0); // s2.c5.count
assert((b(stored(6))) == 1) by (compute);assert((b(stored(6))) == 1); // load.c6.count
assert((b(after(c(6), false, stored(6)))) == 1) by (compute);assert((b(after(c(6), false, stored(6)))) == 1); // a1.c6.count
assert((b(after(c(6), false, stored(6)))) == 1) by (compute);assert((b(after(c(6), false, stored(6)))) == 1); // a2.c6.count
assert((b(after(c(6), true, stored(6)))) == 0) by (compute);assert((b(after(c(6), true, stored(6)))) == 0); // s1.c6.count
assert((b(after(c(6), true, stored(6)))) == 0) by (compute);assert((b(after(c(6), true, stored(6)))) == 0); // s2.c6.count
assert((b(arrange(false).ok)) == 0) by (compute);assert((b(arrange(false).ok)) == 0); // a2.ok
assert((b(arrange(true).ok)) == 1) by (compute);assert((b(arrange(true).ok)) == 1); // s1.ok
assert((b(arrange(true).ok)) == 1) by (compute);assert((b(arrange(true).ok)) == 1); // s2.ok
assert((b(arrange(false).ok)) == 0) by (compute);assert((b(arrange(false).ok)) == 0); // n1.ok
assert((b(arrange(true).ok)) == 1) by (compute);assert((b(arrange(true).ok)) == 1); // s3.ok
assert((b(z_first(false))) == 1) by (compute);assert((b(z_first(false))) == 1); // a1.zfirst
assert((b(z_first(true))) == 0) by (compute);assert((b(z_first(true))) == 0); // s1.zfirst
}
}
fn main() {}
