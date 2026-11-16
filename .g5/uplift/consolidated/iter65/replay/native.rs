#[path="../../../../Verus_verification/consolidated/iter65/moderefresh.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

reveal_with_fuel(value, 6); assert((value(3, true, 4)) == 3);assert((value(3, true, 4)) == 3); // c.D.gbl
reveal_with_fuel(value, 6); assert((value(4, true, 3)) == 2);assert((value(4, true, 3)) == 2); // b2.D.filecopy
reveal_with_fuel(value, 6); assert((value(1, false, 2)) == 2);assert((value(1, false, 2)) == 2); // b1.N.inc
reveal_with_fuel(value, 6); assert((value(0, true, 0)) == 1);assert((value(0, true, 0)) == 1); // s0.D.cls
reveal_with_fuel(value, 6); assert((value(1, true, 0)) == 1);assert((value(1, true, 0)) == 1); // s0.D.inc
reveal_with_fuel(value, 6); assert((value(2, true, 0)) == 1);assert((value(2, true, 0)) == 1); // s0.D.mac
reveal_with_fuel(value, 6); assert((value(3, true, 0)) == 1);assert((value(3, true, 0)) == 1); // s0.D.gbl
reveal_with_fuel(value, 6); assert((value(4, true, 0)) == 1);assert((value(4, true, 0)) == 1); // s0.D.filecopy
reveal_with_fuel(value, 6); assert((value(5, true, 0)) == 1);assert((value(5, true, 0)) == 1); // s0.D.webapp
reveal_with_fuel(value, 6); assert((value(6, true, 0)) == 1);assert((value(6, true, 0)) == 1); // s0.D.loc
reveal_with_fuel(value, 6); assert((value(7, true, 0)) == 1);assert((value(7, true, 0)) == 1); // s0.D.dfi
reveal_with_fuel(value, 6); assert((value(0, false, 0)) == 1);assert((value(0, false, 0)) == 1); // s0.N.cls
reveal_with_fuel(value, 6); assert((value(1, false, 0)) == 1);assert((value(1, false, 0)) == 1); // s0.N.inc
reveal_with_fuel(value, 6); assert((value(2, false, 0)) == 1);assert((value(2, false, 0)) == 1); // s0.N.mac
reveal_with_fuel(value, 6); assert((value(3, false, 0)) == 1);assert((value(3, false, 0)) == 1); // s0.N.gbl
reveal_with_fuel(value, 6); assert((value(4, false, 0)) == 1);assert((value(4, false, 0)) == 1); // s0.N.filecopy
reveal_with_fuel(value, 6); assert((value(5, false, 0)) == 1);assert((value(5, false, 0)) == 1); // s0.N.webapp
reveal_with_fuel(value, 6); assert((value(6, false, 0)) == 1);assert((value(6, false, 0)) == 1); // s0.N.loc
reveal_with_fuel(value, 6); assert((value(7, false, 0)) == 1);assert((value(7, false, 0)) == 1); // s0.N.dfi
reveal_with_fuel(value, 6); assert((value(0, true, 1)) == 2);assert((value(0, true, 1)) == 2); // a.D.cls
reveal_with_fuel(value, 6); assert((value(1, true, 1)) == 2);assert((value(1, true, 1)) == 2); // a.D.inc
reveal_with_fuel(value, 6); assert((value(2, true, 1)) == 2);assert((value(2, true, 1)) == 2); // a.D.mac
reveal_with_fuel(value, 6); assert((value(3, true, 1)) == 2);assert((value(3, true, 1)) == 2); // a.D.gbl
reveal_with_fuel(value, 6); assert((value(4, true, 1)) == 2);assert((value(4, true, 1)) == 2); // a.D.filecopy
reveal_with_fuel(value, 6); assert((value(5, true, 1)) == 2);assert((value(5, true, 1)) == 2); // a.D.webapp
reveal_with_fuel(value, 6); assert((value(6, true, 1)) == 2);assert((value(6, true, 1)) == 2); // a.D.loc
reveal_with_fuel(value, 6); assert((value(7, true, 1)) == 2);assert((value(7, true, 1)) == 2); // a.D.dfi
reveal_with_fuel(value, 6); assert((value(0, false, 1)) == 2);assert((value(0, false, 1)) == 2); // a.N.cls
reveal_with_fuel(value, 6); assert((value(1, false, 1)) == 2);assert((value(1, false, 1)) == 2); // a.N.inc
reveal_with_fuel(value, 6); assert((value(2, false, 1)) == 2);assert((value(2, false, 1)) == 2); // a.N.mac
reveal_with_fuel(value, 6); assert((value(3, false, 1)) == 2);assert((value(3, false, 1)) == 2); // a.N.gbl
reveal_with_fuel(value, 6); assert((value(4, false, 1)) == 2);assert((value(4, false, 1)) == 2); // a.N.filecopy
reveal_with_fuel(value, 6); assert((value(5, false, 1)) == 2);assert((value(5, false, 1)) == 2); // a.N.webapp
reveal_with_fuel(value, 6); assert((value(6, false, 1)) == 2);assert((value(6, false, 1)) == 2); // a.N.loc
reveal_with_fuel(value, 6); assert((value(7, false, 1)) == 2);assert((value(7, false, 1)) == 2); // a.N.dfi
reveal_with_fuel(value, 6); assert((value(0, true, 2)) == 2);assert((value(0, true, 2)) == 2); // b1.D.cls
reveal_with_fuel(value, 6); assert((value(1, true, 2)) == 3);assert((value(1, true, 2)) == 3); // b1.D.inc
reveal_with_fuel(value, 6); assert((value(2, true, 2)) == 2);assert((value(2, true, 2)) == 2); // b1.D.mac
reveal_with_fuel(value, 6); assert((value(3, true, 2)) == 3);assert((value(3, true, 2)) == 3); // b1.D.gbl
reveal_with_fuel(value, 6); assert((value(4, true, 2)) == 2);assert((value(4, true, 2)) == 2); // b1.D.filecopy
reveal_with_fuel(value, 6); assert((value(5, true, 2)) == 2);assert((value(5, true, 2)) == 2); // b1.D.webapp
reveal_with_fuel(value, 6); assert((value(6, true, 2)) == 2);assert((value(6, true, 2)) == 2); // b1.D.loc
reveal_with_fuel(value, 6); assert((value(7, true, 2)) == 3);assert((value(7, true, 2)) == 3); // b1.D.dfi
reveal_with_fuel(value, 6); assert((value(0, false, 2)) == 2);assert((value(0, false, 2)) == 2); // b1.N.cls
reveal_with_fuel(value, 6); assert((value(2, false, 2)) == 2);assert((value(2, false, 2)) == 2); // b1.N.mac
reveal_with_fuel(value, 6); assert((value(3, false, 2)) == 2);assert((value(3, false, 2)) == 2); // b1.N.gbl
reveal_with_fuel(value, 6); assert((value(4, false, 2)) == 2);assert((value(4, false, 2)) == 2); // b1.N.filecopy
reveal_with_fuel(value, 6); assert((value(5, false, 2)) == 2);assert((value(5, false, 2)) == 2); // b1.N.webapp
reveal_with_fuel(value, 6); assert((value(6, false, 2)) == 2);assert((value(6, false, 2)) == 2); // b1.N.loc
reveal_with_fuel(value, 6); assert((value(7, false, 2)) == 2);assert((value(7, false, 2)) == 2); // b1.N.dfi
reveal_with_fuel(value, 6); assert((value(0, true, 2)) == 2);assert((value(0, true, 2)) == 2); // b1x.D.cls
reveal_with_fuel(value, 6); assert((value(1, true, 2)) == 3);assert((value(1, true, 2)) == 3); // b1x.D.inc
reveal_with_fuel(value, 6); assert((value(2, true, 2)) == 2);assert((value(2, true, 2)) == 2); // b1x.D.mac
reveal_with_fuel(value, 6); assert((value(3, true, 2)) == 3);assert((value(3, true, 2)) == 3); // b1x.D.gbl
reveal_with_fuel(value, 6); assert((value(4, true, 2)) == 2);assert((value(4, true, 2)) == 2); // b1x.D.filecopy
reveal_with_fuel(value, 6); assert((value(5, true, 2)) == 2);assert((value(5, true, 2)) == 2); // b1x.D.webapp
reveal_with_fuel(value, 6); assert((value(6, true, 2)) == 2);assert((value(6, true, 2)) == 2); // b1x.D.loc
reveal_with_fuel(value, 6); assert((value(7, true, 2)) == 3);assert((value(7, true, 2)) == 3); // b1x.D.dfi
reveal_with_fuel(value, 6); assert((value(0, false, 2)) == 2);assert((value(0, false, 2)) == 2); // b1x.N.cls
reveal_with_fuel(value, 6); assert((value(1, false, 2)) == 2);assert((value(1, false, 2)) == 2); // b1x.N.inc
reveal_with_fuel(value, 6); assert((value(2, false, 2)) == 2);assert((value(2, false, 2)) == 2); // b1x.N.mac
reveal_with_fuel(value, 6); assert((value(3, false, 2)) == 2);assert((value(3, false, 2)) == 2); // b1x.N.gbl
reveal_with_fuel(value, 6); assert((value(4, false, 2)) == 2);assert((value(4, false, 2)) == 2); // b1x.N.filecopy
reveal_with_fuel(value, 6); assert((value(5, false, 2)) == 2);assert((value(5, false, 2)) == 2); // b1x.N.webapp
reveal_with_fuel(value, 6); assert((value(6, false, 2)) == 2);assert((value(6, false, 2)) == 2); // b1x.N.loc
reveal_with_fuel(value, 6); assert((value(7, false, 2)) == 2);assert((value(7, false, 2)) == 2); // b1x.N.dfi
reveal_with_fuel(value, 6); assert((value(0, true, 3)) == 3);assert((value(0, true, 3)) == 3); // b2.D.cls
reveal_with_fuel(value, 6); assert((value(1, true, 3)) == 3);assert((value(1, true, 3)) == 3); // b2.D.inc
reveal_with_fuel(value, 6); assert((value(2, true, 3)) == 3);assert((value(2, true, 3)) == 3); // b2.D.mac
reveal_with_fuel(value, 6); assert((value(3, true, 3)) == 3);assert((value(3, true, 3)) == 3); // b2.D.gbl
reveal_with_fuel(value, 6); assert((value(5, true, 3)) == 2);assert((value(5, true, 3)) == 2); // b2.D.webapp
reveal_with_fuel(value, 6); assert((value(6, true, 3)) == 3);assert((value(6, true, 3)) == 3); // b2.D.loc
reveal_with_fuel(value, 6); assert((value(7, true, 3)) == 3);assert((value(7, true, 3)) == 3); // b2.D.dfi
reveal_with_fuel(value, 6); assert((value(0, false, 3)) == 2);assert((value(0, false, 3)) == 2); // b2.N.cls
reveal_with_fuel(value, 6); assert((value(1, false, 3)) == 2);assert((value(1, false, 3)) == 2); // b2.N.inc
reveal_with_fuel(value, 6); assert((value(2, false, 3)) == 2);assert((value(2, false, 3)) == 2); // b2.N.mac
reveal_with_fuel(value, 6); assert((value(3, false, 3)) == 2);assert((value(3, false, 3)) == 2); // b2.N.gbl
reveal_with_fuel(value, 6); assert((value(4, false, 3)) == 2);assert((value(4, false, 3)) == 2); // b2.N.filecopy
reveal_with_fuel(value, 6); assert((value(5, false, 3)) == 2);assert((value(5, false, 3)) == 2); // b2.N.webapp
reveal_with_fuel(value, 6); assert((value(6, false, 3)) == 2);assert((value(6, false, 3)) == 2); // b2.N.loc
reveal_with_fuel(value, 6); assert((value(7, false, 3)) == 2);assert((value(7, false, 3)) == 2); // b2.N.dfi
reveal_with_fuel(value, 6); assert((value(0, true, 4)) == 4);assert((value(0, true, 4)) == 4); // c.D.cls
reveal_with_fuel(value, 6); assert((value(1, true, 4)) == 4);assert((value(1, true, 4)) == 4); // c.D.inc
reveal_with_fuel(value, 6); assert((value(2, true, 4)) == 4);assert((value(2, true, 4)) == 4); // c.D.mac
reveal_with_fuel(value, 6); assert((value(4, true, 4)) == 2);assert((value(4, true, 4)) == 2); // c.D.filecopy
reveal_with_fuel(value, 6); assert((value(5, true, 4)) == 2);assert((value(5, true, 4)) == 2); // c.D.webapp
reveal_with_fuel(value, 6); assert((value(6, true, 4)) == 4);assert((value(6, true, 4)) == 4); // c.D.loc
reveal_with_fuel(value, 6); assert((value(7, true, 4)) == 4);assert((value(7, true, 4)) == 4); // c.D.dfi
reveal_with_fuel(value, 6); assert((value(0, false, 4)) == 2);assert((value(0, false, 4)) == 2); // c.N.cls
reveal_with_fuel(value, 6); assert((value(1, false, 4)) == 2);assert((value(1, false, 4)) == 2); // c.N.inc
reveal_with_fuel(value, 6); assert((value(2, false, 4)) == 2);assert((value(2, false, 4)) == 2); // c.N.mac
reveal_with_fuel(value, 6); assert((value(3, false, 4)) == 2);assert((value(3, false, 4)) == 2); // c.N.gbl
reveal_with_fuel(value, 6); assert((value(4, false, 4)) == 2);assert((value(4, false, 4)) == 2); // c.N.filecopy
reveal_with_fuel(value, 6); assert((value(5, false, 4)) == 2);assert((value(5, false, 4)) == 2); // c.N.webapp
reveal_with_fuel(value, 6); assert((value(6, false, 4)) == 2);assert((value(6, false, 4)) == 2); // c.N.loc
reveal_with_fuel(value, 6); assert((value(7, false, 4)) == 2);assert((value(7, false, 4)) == 2); // c.N.dfi
reveal_with_fuel(value, 6); assert((value(0, true, 5)) == 5);assert((value(0, true, 5)) == 5); // v.D.cls
reveal_with_fuel(value, 6); assert((value(1, true, 5)) == 5);assert((value(1, true, 5)) == 5); // v.D.inc
reveal_with_fuel(value, 6); assert((value(2, true, 5)) == 5);assert((value(2, true, 5)) == 5); // v.D.mac
reveal_with_fuel(value, 6); assert((value(3, true, 5)) == 3);assert((value(3, true, 5)) == 3); // v.D.gbl
reveal_with_fuel(value, 6); assert((value(4, true, 5)) == 2);assert((value(4, true, 5)) == 2); // v.D.filecopy
reveal_with_fuel(value, 6); assert((value(5, true, 5)) == 2);assert((value(5, true, 5)) == 2); // v.D.webapp
reveal_with_fuel(value, 6); assert((value(6, true, 5)) == 5);assert((value(6, true, 5)) == 5); // v.D.loc
reveal_with_fuel(value, 6); assert((value(7, true, 5)) == 5);assert((value(7, true, 5)) == 5); // v.D.dfi
reveal_with_fuel(value, 6); assert(((if command_ok(true, 0) { 1int } else { 0int })) == 1);assert(((if command_ok(true, 0) { 1int } else { 0int })) == 1); // s0.D.ok
reveal_with_fuel(value, 6); assert(((if command_ok(false, 0) { 1int } else { 0int })) == 1);assert(((if command_ok(false, 0) { 1int } else { 0int })) == 1); // s0.N.ok
reveal_with_fuel(value, 6); assert(((if command_ok(true, 0) { 1int } else { 0int })) == 1);assert(((if command_ok(true, 0) { 1int } else { 0int })) == 1); // a.D.ok
reveal_with_fuel(value, 6); assert(((if command_ok(false, 0) { 1int } else { 0int })) == 1);assert(((if command_ok(false, 0) { 1int } else { 0int })) == 1); // a.N.ok
reveal_with_fuel(value, 6); assert(((if command_ok(true, 1) { 1int } else { 0int })) == 1);assert(((if command_ok(true, 1) { 1int } else { 0int })) == 1); // b1.D.ok
reveal_with_fuel(value, 6); assert(((if command_ok(false, 1) { 1int } else { 0int })) == 1);assert(((if command_ok(false, 1) { 1int } else { 0int })) == 1); // b1.N.ok
reveal_with_fuel(value, 6); assert(((if command_ok(true, 2) { 1int } else { 0int })) == 1);assert(((if command_ok(true, 2) { 1int } else { 0int })) == 1); // b2.D.ok
reveal_with_fuel(value, 6); assert(((if command_ok(false, 2) { 1int } else { 0int })) == 1);assert(((if command_ok(false, 2) { 1int } else { 0int })) == 1); // b2.N.ok
reveal_with_fuel(value, 6); assert(((if command_ok(true, 3) { 1int } else { 0int })) == 1);assert(((if command_ok(true, 3) { 1int } else { 0int })) == 1); // c.D.ok
reveal_with_fuel(value, 6); assert(((if command_ok(false, 3) { 1int } else { 0int })) == 0);assert(((if command_ok(false, 3) { 1int } else { 0int })) == 0); // c.N.ok
}
}
fn main() {}
