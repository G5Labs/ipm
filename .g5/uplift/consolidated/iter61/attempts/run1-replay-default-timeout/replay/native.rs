#[path="../../../../Verus_verification/consolidated/iter61/webappphases.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(r_n1().1)) == 0) by (compute);assert((b(r_n1().1)) == 0); // n1.ok
assert((b(desc(s_c1()) == 99)) == 1) by (compute);assert((b(desc(s_c1()) == 99)) == 1); // c1.a.desc.manual
assert((b(desc(s_w2()) == 11)) == 1) by (compute);assert((b(desc(s_w2()) == 11)) == 1); // w2.a.desc.first
assert((b((s_w1()).app.is_some())) == 1) by (compute);assert((b((s_w1()).app.is_some())) == 1); // w1.a.exists
assert((b(desc(s_w1()) == 11)) == 1) by (compute);assert((b(desc(s_w1()) == 11)) == 1); // w1.a.desc.first
assert((b(desc(s_w1()) == 12)) == 0) by (compute);assert((b(desc(s_w1()) == 12)) == 0); // w1.a.desc.second
assert((b(desc(s_w1()) == 99)) == 0) by (compute);assert((b(desc(s_w1()) == 99)) == 0); // w1.a.desc.manual
assert((b(desc(s_w1()) == 13)) == 0) by (compute);assert((b(desc(s_w1()) == 13)) == 0); // w1.a.desc.other
assert((b(timeout(s_w1()) == 900)) == 1) by (compute);assert((b(timeout(s_w1()) == 900)) == 1); // w1.a.timeout900
assert((b((s_w2()).app.is_some())) == 1) by (compute);assert((b((s_w2()).app.is_some())) == 1); // w2.a.exists
assert((b(desc(s_w2()) == 12)) == 0) by (compute);assert((b(desc(s_w2()) == 12)) == 0); // w2.a.desc.second
assert((b(desc(s_w2()) == 99)) == 0) by (compute);assert((b(desc(s_w2()) == 99)) == 0); // w2.a.desc.manual
assert((b(desc(s_w2()) == 13)) == 0) by (compute);assert((b(desc(s_w2()) == 13)) == 0); // w2.a.desc.other
assert((b(timeout(s_w2()) == 900)) == 1) by (compute);assert((b(timeout(s_w2()) == 900)) == 1); // w2.a.timeout900
assert((b((s_w3()).app.is_some())) == 1) by (compute);assert((b((s_w3()).app.is_some())) == 1); // w3.a.exists
assert((b(desc(s_w3()) == 11)) == 0) by (compute);assert((b(desc(s_w3()) == 11)) == 0); // w3.a.desc.first
assert((b(desc(s_w3()) == 12)) == 1) by (compute);assert((b(desc(s_w3()) == 12)) == 1); // w3.a.desc.second
assert((b(desc(s_w3()) == 99)) == 0) by (compute);assert((b(desc(s_w3()) == 99)) == 0); // w3.a.desc.manual
assert((b(desc(s_w3()) == 13)) == 0) by (compute);assert((b(desc(s_w3()) == 13)) == 0); // w3.a.desc.other
assert((b(timeout(s_w3()) == 900)) == 1) by (compute);assert((b(timeout(s_w3()) == 900)) == 1); // w3.a.timeout900
assert((b((s_c1()).app.is_some())) == 1) by (compute);assert((b((s_c1()).app.is_some())) == 1); // c1.a.exists
assert((b(desc(s_c1()) == 11)) == 0) by (compute);assert((b(desc(s_c1()) == 11)) == 0); // c1.a.desc.first
assert((b(desc(s_c1()) == 12)) == 0) by (compute);assert((b(desc(s_c1()) == 12)) == 0); // c1.a.desc.second
assert((b(desc(s_c1()) == 13)) == 0) by (compute);assert((b(desc(s_c1()) == 13)) == 0); // c1.a.desc.other
assert((b(timeout(s_c1()) == 900)) == 1) by (compute);assert((b(timeout(s_c1()) == 900)) == 1); // c1.a.timeout900
assert((b((s_c2()).app.is_some())) == 1) by (compute);assert((b((s_c2()).app.is_some())) == 1); // c2.a.exists
assert((b(desc(s_c2()) == 11)) == 0) by (compute);assert((b(desc(s_c2()) == 11)) == 0); // c2.a.desc.first
assert((b(desc(s_c2()) == 12)) == 1) by (compute);assert((b(desc(s_c2()) == 12)) == 1); // c2.a.desc.second
assert((b(desc(s_c2()) == 99)) == 0) by (compute);assert((b(desc(s_c2()) == 99)) == 0); // c2.a.desc.manual
assert((b(desc(s_c2()) == 13)) == 0) by (compute);assert((b(desc(s_c2()) == 13)) == 0); // c2.a.desc.other
assert((b(timeout(s_c2()) == 900)) == 1) by (compute);assert((b(timeout(s_c2()) == 900)) == 1); // c2.a.timeout900
assert((b((r_n1().0).app.is_some())) == 1) by (compute);assert((b((r_n1().0).app.is_some())) == 1); // n1.a.exists
assert((b(desc(r_n1().0) == 11)) == 0) by (compute);assert((b(desc(r_n1().0) == 11)) == 0); // n1.a.desc.first
assert((b(desc(r_n1().0) == 12)) == 1) by (compute);assert((b(desc(r_n1().0) == 12)) == 1); // n1.a.desc.second
assert((b(desc(r_n1().0) == 99)) == 0) by (compute);assert((b(desc(r_n1().0) == 99)) == 0); // n1.a.desc.manual
assert((b(desc(r_n1().0) == 13)) == 0) by (compute);assert((b(desc(r_n1().0) == 13)) == 0); // n1.a.desc.other
assert((b(timeout(r_n1().0) == 900)) == 1) by (compute);assert((b(timeout(r_n1().0) == 900)) == 1); // n1.a.timeout900
assert((b((s_u1()).app.is_some())) == 0) by (compute);assert((b((s_u1()).app.is_some())) == 0); // u1.a.exists
assert((b(desc(s_u1()) == 11)) == 0) by (compute);assert((b(desc(s_u1()) == 11)) == 0); // u1.a.desc.first
assert((b(desc(s_u1()) == 12)) == 0) by (compute);assert((b(desc(s_u1()) == 12)) == 0); // u1.a.desc.second
assert((b(desc(s_u1()) == 99)) == 0) by (compute);assert((b(desc(s_u1()) == 99)) == 0); // u1.a.desc.manual
assert((b(desc(s_u1()) == 13)) == 0) by (compute);assert((b(desc(s_u1()) == 13)) == 0); // u1.a.desc.other
assert((b(timeout(s_u1()) == 900)) == 0) by (compute);assert((b(timeout(s_u1()) == 900)) == 0); // u1.a.timeout900
}
}
fn main() {}
