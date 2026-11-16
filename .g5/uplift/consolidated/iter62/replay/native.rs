#[path="../../../../Verus_verification/consolidated/iter62/interopres.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((v(a2(), 1)) == -1);assert((v(a2(), 1)) == -1); // a2.I62A.b
assert((v(a3(), 0)) == -1);assert((v(a3(), 0)) == -1); // a3.I62A.a
assert(((if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }) { 1int } else { 0int }) * 9 + (1 - (if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }) { 1int } else { 0int })) * -1) == 9);assert(((if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }) { 1int } else { 0int }) * 9 + (1 - (if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: false, at: 2 }) { 1int } else { 0int })) * -1) == 9); // f1.I62F.k
assert((v(a1(), 0)) == 1);assert((v(a1(), 0)) == 1); // a1.I62A.a
assert((v(a1(), 1)) == 2);assert((v(a1(), 1)) == 2); // a1.I62A.b
assert((v(a2(), 0)) == 10);assert((v(a2(), 0)) == 10); // a2.I62A.a
assert((v(a2(), 2)) == 3);assert((v(a2(), 2)) == 3); // a2.I62A.c
assert((v(a3(), 1)) == -1);assert((v(a3(), 1)) == -1); // a3.I62A.b
assert((v(a3(), 2)) == -1);assert((v(a3(), 2)) == -1); // a3.I62A.c
assert(((if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: true, at: 1 }) { 1int } else { 0int }) * 7 + (1 - (if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: true, at: 1 }) { 1int } else { 0int })) * -1) == 7);assert(((if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: true, at: 1 }) { 1int } else { 0int }) * 7 + (1 - (if candidates_find(Res { declared: false, i14y_exists: false, misc_exists: true, at: 1 }) { 1int } else { 0int })) * -1) == 7); // m1.I62M.k
assert(((if candidates_find(Res { declared: true, i14y_exists: false, misc_exists: false, at: 3 }) { 1int } else { 0int }) * 8 + (1 - (if candidates_find(Res { declared: true, i14y_exists: false, misc_exists: false, at: 3 }) { 1int } else { 0int })) * -1) == 8);assert(((if candidates_find(Res { declared: true, i14y_exists: false, misc_exists: false, at: 3 }) { 1int } else { 0int }) * 8 + (1 - (if candidates_find(Res { declared: true, i14y_exists: false, misc_exists: false, at: 3 }) { 1int } else { 0int })) * -1) == 8); // x1.I62X.k
}
}
fn main() {}
