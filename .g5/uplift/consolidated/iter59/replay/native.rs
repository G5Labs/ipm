#[path="../../../../Verus_verification/consolidated/iter59/mappings.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // b2.b.map
assert((visible_sys(C(), c2())) == 1) by (compute);assert((visible_sys(C(), c2())) == 1); // c2.c.insys
assert((b((e2()).module)) == 1) by (compute);assert((b((e2()).module)) == 1); // e2.e.module
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s0.a.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s0.a.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s0.a.inuser
assert((visible_sys(A(), none())) == 0) by (compute);assert((visible_sys(A(), none())) == 0); // s0.a.insys
assert((b((a1()).map)) == 1) by (compute);assert((b((a1()).map)) == 1); // a1.a.map
assert((b((a1()).module)) == 1) by (compute);assert((b((a1()).module)) == 1); // a1.a.module
assert((visible_user(a1())) == 1) by (compute);assert((visible_user(a1())) == 1); // a1.a.inuser
assert((visible_sys(A(), a1())) == 0) by (compute);assert((visible_sys(A(), a1())) == 0); // a1.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // a2.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // a2.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // a2.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // a2.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // s1.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // s1.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // s1.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // s1.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // b1.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // b1.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // b1.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // b1.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // b2.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // b2.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // b2.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // b2.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // c1.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // c1.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // c1.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // c1.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // c2.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // c2.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // c2.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // c2.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // d1.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // d1.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // d1.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // d1.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // e1.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // e1.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // e1.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // e1.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // e2.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // e2.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // e2.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // e2.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // e3.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // e3.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // e3.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // e3.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // c3.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // c3.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // c3.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // c3.a.insys
assert((b((a2()).map)) == 1) by (compute);assert((b((a2()).map)) == 1); // c4.a.map
assert((b((a2()).module)) == 0) by (compute);assert((b((a2()).module)) == 0); // c4.a.module
assert((visible_user(a2())) == 0) by (compute);assert((visible_user(a2())) == 0); // c4.a.inuser
assert((visible_sys(A(), a2())) == 0) by (compute);assert((visible_sys(A(), a2())) == 0); // c4.a.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s0.b.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s0.b.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s0.b.inuser
assert((visible_sys(B(), none())) == 0) by (compute);assert((visible_sys(B(), none())) == 0); // s0.b.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // a1.b.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // a1.b.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // a1.b.inuser
assert((visible_sys(B(), none())) == 0) by (compute);assert((visible_sys(B(), none())) == 0); // a1.b.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // a2.b.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // a2.b.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // a2.b.inuser
assert((visible_sys(B(), none())) == 0) by (compute);assert((visible_sys(B(), none())) == 0); // a2.b.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s1.b.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s1.b.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s1.b.inuser
assert((visible_sys(B(), none())) == 0) by (compute);assert((visible_sys(B(), none())) == 0); // s1.b.insys
assert((b((b1()).map)) == 1) by (compute);assert((b((b1()).map)) == 1); // b1.b.map
assert((b((b1()).module)) == 1) by (compute);assert((b((b1()).module)) == 1); // b1.b.module
assert((visible_user(b1())) == 1) by (compute);assert((visible_user(b1())) == 1); // b1.b.inuser
assert((visible_sys(B(), b1())) == 0) by (compute);assert((visible_sys(B(), b1())) == 0); // b1.b.insys
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // b2.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // b2.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // b2.b.insys
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // c1.b.map
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // c1.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // c1.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // c1.b.insys
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // c2.b.map
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // c2.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // c2.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // c2.b.insys
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // d1.b.map
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // d1.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // d1.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // d1.b.insys
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // e1.b.map
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // e1.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // e1.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // e1.b.insys
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // e2.b.map
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // e2.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // e2.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // e2.b.insys
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // e3.b.map
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // e3.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // e3.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // e3.b.insys
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // c3.b.map
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // c3.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // c3.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // c3.b.insys
assert((b((b2()).map)) == 1) by (compute);assert((b((b2()).map)) == 1); // c4.b.map
assert((b((b2()).module)) == 0) by (compute);assert((b((b2()).module)) == 0); // c4.b.module
assert((visible_user(b2())) == 0) by (compute);assert((visible_user(b2())) == 0); // c4.b.inuser
assert((visible_sys(B(), b2())) == 0) by (compute);assert((visible_sys(B(), b2())) == 0); // c4.b.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s0.c.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s0.c.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s0.c.inuser
assert((visible_sys(C(), none())) == 0) by (compute);assert((visible_sys(C(), none())) == 0); // s0.c.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // a1.c.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // a1.c.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // a1.c.inuser
assert((visible_sys(C(), none())) == 0) by (compute);assert((visible_sys(C(), none())) == 0); // a1.c.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // a2.c.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // a2.c.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // a2.c.inuser
assert((visible_sys(C(), none())) == 0) by (compute);assert((visible_sys(C(), none())) == 0); // a2.c.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s1.c.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s1.c.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s1.c.inuser
assert((visible_sys(C(), none())) == 0) by (compute);assert((visible_sys(C(), none())) == 0); // s1.c.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // b1.c.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // b1.c.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // b1.c.inuser
assert((visible_sys(C(), none())) == 0) by (compute);assert((visible_sys(C(), none())) == 0); // b1.c.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // b2.c.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // b2.c.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // b2.c.inuser
assert((visible_sys(C(), none())) == 0) by (compute);assert((visible_sys(C(), none())) == 0); // b2.c.insys
assert((b((c1()).map)) == 0) by (compute);assert((b((c1()).map)) == 0); // c1.c.map
assert((b((c1()).module)) == 1) by (compute);assert((b((c1()).module)) == 1); // c1.c.module
assert((visible_user(c1())) == 1) by (compute);assert((visible_user(c1())) == 1); // c1.c.inuser
assert((visible_sys(C(), c1())) == 1) by (compute);assert((visible_sys(C(), c1())) == 1); // c1.c.insys
assert((b((c2()).map)) == 0) by (compute);assert((b((c2()).map)) == 0); // c2.c.map
assert((b((c2()).module)) == 0) by (compute);assert((b((c2()).module)) == 0); // c2.c.module
assert((visible_user(c2())) == 1) by (compute);assert((visible_user(c2())) == 1); // c2.c.inuser
assert((b((c2()).map)) == 0) by (compute);assert((b((c2()).map)) == 0); // d1.c.map
assert((b((c2()).module)) == 0) by (compute);assert((b((c2()).module)) == 0); // d1.c.module
assert((visible_user(c2())) == 1) by (compute);assert((visible_user(c2())) == 1); // d1.c.inuser
assert((visible_sys(C(), c2())) == 1) by (compute);assert((visible_sys(C(), c2())) == 1); // d1.c.insys
assert((b((c2()).map)) == 0) by (compute);assert((b((c2()).map)) == 0); // e1.c.map
assert((b((c2()).module)) == 0) by (compute);assert((b((c2()).module)) == 0); // e1.c.module
assert((visible_user(c2())) == 1) by (compute);assert((visible_user(c2())) == 1); // e1.c.inuser
assert((visible_sys(C(), c2())) == 1) by (compute);assert((visible_sys(C(), c2())) == 1); // e1.c.insys
assert((b((c2()).map)) == 0) by (compute);assert((b((c2()).map)) == 0); // e2.c.map
assert((b((c2()).module)) == 0) by (compute);assert((b((c2()).module)) == 0); // e2.c.module
assert((visible_user(c2())) == 1) by (compute);assert((visible_user(c2())) == 1); // e2.c.inuser
assert((visible_sys(C(), c2())) == 1) by (compute);assert((visible_sys(C(), c2())) == 1); // e2.c.insys
assert((b((c2()).map)) == 0) by (compute);assert((b((c2()).map)) == 0); // e3.c.map
assert((b((c2()).module)) == 0) by (compute);assert((b((c2()).module)) == 0); // e3.c.module
assert((visible_user(c2())) == 1) by (compute);assert((visible_user(c2())) == 1); // e3.c.inuser
assert((visible_sys(C(), c2())) == 1) by (compute);assert((visible_sys(C(), c2())) == 1); // e3.c.insys
assert((b((c3()).map)) == 0) by (compute);assert((b((c3()).map)) == 0); // c3.c.map
assert((b((c3()).module)) == 1) by (compute);assert((b((c3()).module)) == 1); // c3.c.module
assert((visible_user(c3())) == 1) by (compute);assert((visible_user(c3())) == 1); // c3.c.inuser
assert((visible_sys(C(), c3())) == 1) by (compute);assert((visible_sys(C(), c3())) == 1); // c3.c.insys
assert((b((c4()).map)) == 0) by (compute);assert((b((c4()).map)) == 0); // c4.c.map
assert((b((c4()).module)) == 0) by (compute);assert((b((c4()).module)) == 0); // c4.c.module
assert((visible_user(c4())) == 0) by (compute);assert((visible_user(c4())) == 0); // c4.c.inuser
assert((visible_sys(C(), c4())) == 0) by (compute);assert((visible_sys(C(), c4())) == 0); // c4.c.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s0.d.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s0.d.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s0.d.inuser
assert((visible_sys(D(), none())) == 0) by (compute);assert((visible_sys(D(), none())) == 0); // s0.d.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // a1.d.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // a1.d.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // a1.d.inuser
assert((visible_sys(D(), none())) == 0) by (compute);assert((visible_sys(D(), none())) == 0); // a1.d.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // a2.d.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // a2.d.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // a2.d.inuser
assert((visible_sys(D(), none())) == 0) by (compute);assert((visible_sys(D(), none())) == 0); // a2.d.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s1.d.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s1.d.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s1.d.inuser
assert((visible_sys(D(), none())) == 0) by (compute);assert((visible_sys(D(), none())) == 0); // s1.d.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // b1.d.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // b1.d.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // b1.d.inuser
assert((visible_sys(D(), none())) == 0) by (compute);assert((visible_sys(D(), none())) == 0); // b1.d.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // b2.d.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // b2.d.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // b2.d.inuser
assert((visible_sys(D(), none())) == 0) by (compute);assert((visible_sys(D(), none())) == 0); // b2.d.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // c1.d.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // c1.d.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // c1.d.inuser
assert((visible_sys(D(), none())) == 0) by (compute);assert((visible_sys(D(), none())) == 0); // c1.d.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // c2.d.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // c2.d.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // c2.d.inuser
assert((visible_sys(D(), none())) == 0) by (compute);assert((visible_sys(D(), none())) == 0); // c2.d.insys
assert((b((d1()).map)) == 0) by (compute);assert((b((d1()).map)) == 0); // d1.d.map
assert((b((d1()).module)) == 1) by (compute);assert((b((d1()).module)) == 1); // d1.d.module
assert((visible_user(d1())) == 1) by (compute);assert((visible_user(d1())) == 1); // d1.d.inuser
assert((visible_sys(D(), d1())) == 0) by (compute);assert((visible_sys(D(), d1())) == 0); // d1.d.insys
assert((b((d1()).map)) == 0) by (compute);assert((b((d1()).map)) == 0); // e1.d.map
assert((b((d1()).module)) == 1) by (compute);assert((b((d1()).module)) == 1); // e1.d.module
assert((visible_user(d1())) == 1) by (compute);assert((visible_user(d1())) == 1); // e1.d.inuser
assert((visible_sys(D(), d1())) == 0) by (compute);assert((visible_sys(D(), d1())) == 0); // e1.d.insys
assert((b((d1()).map)) == 0) by (compute);assert((b((d1()).map)) == 0); // e2.d.map
assert((b((d1()).module)) == 1) by (compute);assert((b((d1()).module)) == 1); // e2.d.module
assert((visible_user(d1())) == 1) by (compute);assert((visible_user(d1())) == 1); // e2.d.inuser
assert((visible_sys(D(), d1())) == 0) by (compute);assert((visible_sys(D(), d1())) == 0); // e2.d.insys
assert((b((d1()).map)) == 0) by (compute);assert((b((d1()).map)) == 0); // e3.d.map
assert((b((d1()).module)) == 1) by (compute);assert((b((d1()).module)) == 1); // e3.d.module
assert((visible_user(d1())) == 1) by (compute);assert((visible_user(d1())) == 1); // e3.d.inuser
assert((visible_sys(D(), d1())) == 0) by (compute);assert((visible_sys(D(), d1())) == 0); // e3.d.insys
assert((b((d1()).map)) == 0) by (compute);assert((b((d1()).map)) == 0); // c3.d.map
assert((b((d1()).module)) == 1) by (compute);assert((b((d1()).module)) == 1); // c3.d.module
assert((visible_user(d1())) == 1) by (compute);assert((visible_user(d1())) == 1); // c3.d.inuser
assert((visible_sys(D(), d1())) == 0) by (compute);assert((visible_sys(D(), d1())) == 0); // c3.d.insys
assert((b((d1()).map)) == 0) by (compute);assert((b((d1()).map)) == 0); // c4.d.map
assert((b((d1()).module)) == 1) by (compute);assert((b((d1()).module)) == 1); // c4.d.module
assert((visible_user(d1())) == 1) by (compute);assert((visible_user(d1())) == 1); // c4.d.inuser
assert((visible_sys(D(), d1())) == 0) by (compute);assert((visible_sys(D(), d1())) == 0); // c4.d.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s0.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s0.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s0.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // s0.e.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // a1.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // a1.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // a1.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // a1.e.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // a2.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // a2.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // a2.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // a2.e.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // s1.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // s1.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // s1.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // s1.e.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // b1.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // b1.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // b1.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // b1.e.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // b2.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // b2.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // b2.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // b2.e.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // c1.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // c1.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // c1.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // c1.e.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // c2.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // c2.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // c2.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // c2.e.insys
assert((b((none()).map)) == 0) by (compute);assert((b((none()).map)) == 0); // d1.e.map
assert((b((none()).module)) == 0) by (compute);assert((b((none()).module)) == 0); // d1.e.module
assert((visible_user(none())) == 0) by (compute);assert((visible_user(none())) == 0); // d1.e.inuser
assert((visible_sys(E(), none())) == 0) by (compute);assert((visible_sys(E(), none())) == 0); // d1.e.insys
assert((b((e1()).map)) == 0) by (compute);assert((b((e1()).map)) == 0); // e1.e.map
assert((b((e1()).module)) == 1) by (compute);assert((b((e1()).module)) == 1); // e1.e.module
assert((visible_user(e1())) == 1) by (compute);assert((visible_user(e1())) == 1); // e1.e.inuser
assert((visible_sys(E(), e1())) == 1) by (compute);assert((visible_sys(E(), e1())) == 1); // e1.e.insys
assert((b((e2()).map)) == 0) by (compute);assert((b((e2()).map)) == 0); // e2.e.map
assert((visible_user(e2())) == 1) by (compute);assert((visible_user(e2())) == 1); // e2.e.inuser
assert((visible_sys(E(), e2())) == 1) by (compute);assert((visible_sys(E(), e2())) == 1); // e2.e.insys
assert((b((e3()).map)) == 0) by (compute);assert((b((e3()).map)) == 0); // e3.e.map
assert((b((e3()).module)) == 0) by (compute);assert((b((e3()).module)) == 0); // e3.e.module
assert((visible_user(e3())) == 0) by (compute);assert((visible_user(e3())) == 0); // e3.e.inuser
assert((visible_sys(E(), e3())) == 0) by (compute);assert((visible_sys(E(), e3())) == 0); // e3.e.insys
assert((b((e3()).map)) == 0) by (compute);assert((b((e3()).map)) == 0); // c3.e.map
assert((b((e3()).module)) == 0) by (compute);assert((b((e3()).module)) == 0); // c3.e.module
assert((visible_user(e3())) == 0) by (compute);assert((visible_user(e3())) == 0); // c3.e.inuser
assert((visible_sys(E(), e3())) == 0) by (compute);assert((visible_sys(E(), e3())) == 0); // c3.e.insys
assert((b((e3()).map)) == 0) by (compute);assert((b((e3()).map)) == 0); // c4.e.map
assert((b((e3()).module)) == 0) by (compute);assert((b((e3()).module)) == 0); // c4.e.module
assert((visible_user(e3())) == 0) by (compute);assert((visible_user(e3())) == 0); // c4.e.inuser
assert((visible_sys(E(), e3())) == 0) by (compute);assert((visible_sys(E(), e3())) == 0); // c4.e.insys
}
}
fn main() {}
