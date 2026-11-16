#[path="../../../../Verus_verification/consolidated/iter35/listing.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((flat_pos(mods(), all(), 9, 2)) == 2) by (compute);assert((flat_pos(mods(), all(), 9, 2)) == 2); // l1.pos.i35pb
assert((vcode(mods(), cut(), 1)) == 9) by (compute);assert((vcode(mods(), cut(), 1)) == 9); // t2.version.i35a
assert((flat_pos(mods(), all(), 10, 0)) == 11) by (compute);assert((flat_pos(mods(), all(), 10, 0)) == 11); // l0.pos.i35n
assert((1int) == 1) by (compute);assert((1int) == 1); // l0.ok
assert((flat_pos(mods(), all(), 0, 0)) == 1) by (compute);assert((flat_pos(mods(), all(), 0, 0)) == 1); // l0.pos.zpm
assert((vcode(mods(), all(), 0)) == 8) by (compute);assert((vcode(mods(), all(), 0)) == 8); // l0.version.zpm
assert((b(mods()[0].dev)) == 0) by (compute);assert((b(mods()[0].dev)) == 0); // l0.dev.zpm
assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0) by (compute);assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0); // l0.label.zpm
assert((flat_pos(mods(), all(), 1, 0)) == 2) by (compute);assert((flat_pos(mods(), all(), 1, 0)) == 2); // l0.pos.i35a
assert((vcode(mods(), all(), 1)) == 1) by (compute);assert((vcode(mods(), all(), 1)) == 1); // l0.version.i35a
assert((b(mods()[1].dev)) == 0) by (compute);assert((b(mods()[1].dev)) == 0); // l0.dev.i35a
assert((b(mods()[1].label != mods()[1].name && !mods()[1].ipm)) == 0) by (compute);assert((b(mods()[1].label != mods()[1].name && !mods()[1].ipm)) == 0); // l0.label.i35a
assert((flat_pos(mods(), all(), 2, 0)) == 3) by (compute);assert((flat_pos(mods(), all(), 2, 0)) == 3); // l0.pos.i35b
assert((vcode(mods(), all(), 2)) == 1) by (compute);assert((vcode(mods(), all(), 2)) == 1); // l0.version.i35b
assert((b(mods()[2].dev)) == 0) by (compute);assert((b(mods()[2].dev)) == 0); // l0.dev.i35b
assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0) by (compute);assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0); // l0.label.i35b
assert((flat_pos(mods(), all(), 3, 0)) == 4) by (compute);assert((flat_pos(mods(), all(), 3, 0)) == 4); // l0.pos.i35c
assert((vcode(mods(), all(), 3)) == 1) by (compute);assert((vcode(mods(), all(), 3)) == 1); // l0.version.i35c
assert((b(mods()[3].dev)) == 0) by (compute);assert((b(mods()[3].dev)) == 0); // l0.dev.i35c
assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0) by (compute);assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0); // l0.label.i35c
assert((flat_pos(mods(), all(), 4, 0)) == 5) by (compute);assert((flat_pos(mods(), all(), 4, 0)) == 5); // l0.pos.i35d
assert((vcode(mods(), all(), 4)) == 1) by (compute);assert((vcode(mods(), all(), 4)) == 1); // l0.version.i35d
assert((b(mods()[4].dev)) == 0) by (compute);assert((b(mods()[4].dev)) == 0); // l0.dev.i35d
assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0) by (compute);assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0); // l0.label.i35d
assert((flat_pos(mods(), all(), 5, 0)) == 6) by (compute);assert((flat_pos(mods(), all(), 5, 0)) == 6); // l0.pos.i35e
assert((vcode(mods(), all(), 5)) == 1) by (compute);assert((vcode(mods(), all(), 5)) == 1); // l0.version.i35e
assert((b(mods()[5].dev)) == 1) by (compute);assert((b(mods()[5].dev)) == 1); // l0.dev.i35e
assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0) by (compute);assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0); // l0.label.i35e
assert((flat_pos(mods(), all(), 8, 0)) == 7) by (compute);assert((flat_pos(mods(), all(), 8, 0)) == 7); // l0.pos.i35pa
assert((vcode(mods(), all(), 8)) == 4) by (compute);assert((vcode(mods(), all(), 8)) == 4); // l0.version.i35pa
assert((b(mods()[8].dev)) == 0) by (compute);assert((b(mods()[8].dev)) == 0); // l0.dev.i35pa
assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0) by (compute);assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0); // l0.label.i35pa
assert((flat_pos(mods(), all(), 9, 0)) == 8) by (compute);assert((flat_pos(mods(), all(), 9, 0)) == 8); // l0.pos.i35pb
assert((vcode(mods(), all(), 9)) == 5) by (compute);assert((vcode(mods(), all(), 9)) == 5); // l0.version.i35pb
assert((b(mods()[9].dev)) == 0) by (compute);assert((b(mods()[9].dev)) == 0); // l0.dev.i35pb
assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0) by (compute);assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0); // l0.label.i35pb
assert((flat_pos(mods(), all(), 6, 0)) == 9) by (compute);assert((flat_pos(mods(), all(), 6, 0)) == 9); // l0.pos.i35x
assert((vcode(mods(), all(), 6)) == 2) by (compute);assert((vcode(mods(), all(), 6)) == 2); // l0.version.i35x
assert((b(mods()[6].dev)) == 0) by (compute);assert((b(mods()[6].dev)) == 0); // l0.dev.i35x
assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0) by (compute);assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0); // l0.label.i35x
assert((flat_pos(mods(), all(), 7, 0)) == 10) by (compute);assert((flat_pos(mods(), all(), 7, 0)) == 10); // l0.pos.i35y
assert((vcode(mods(), all(), 7)) == 3) by (compute);assert((vcode(mods(), all(), 7)) == 3); // l0.version.i35y
assert((b(mods()[7].dev)) == 0) by (compute);assert((b(mods()[7].dev)) == 0); // l0.dev.i35y
assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0) by (compute);assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0); // l0.label.i35y
assert((vcode(mods(), all(), 10)) == 1) by (compute);assert((vcode(mods(), all(), 10)) == 1); // l0.version.i35n
assert((b(mods()[10].dev)) == 0) by (compute);assert((b(mods()[10].dev)) == 0); // l0.dev.i35n
assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1) by (compute);assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1); // l0.label.i35n
assert(((if all() == all() { 11int } else { 10int })) == 11) by (compute);assert(((if all() == all() { 11int } else { 10int })) == 11); // l0.rows
assert((1int) == 1) by (compute);assert((1int) == 1); // l1.ok
assert((flat_pos(mods(), all(), 0, 2)) == 1) by (compute);assert((flat_pos(mods(), all(), 0, 2)) == 1); // l1.pos.zpm
assert((vcode(mods(), all(), 0)) == 8) by (compute);assert((vcode(mods(), all(), 0)) == 8); // l1.version.zpm
assert((b(mods()[0].dev)) == 0) by (compute);assert((b(mods()[0].dev)) == 0); // l1.dev.zpm
assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0) by (compute);assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0); // l1.label.zpm
assert((vcode(mods(), all(), 9)) == 5) by (compute);assert((vcode(mods(), all(), 9)) == 5); // l1.version.i35pb
assert((b(mods()[9].dev)) == 0) by (compute);assert((b(mods()[9].dev)) == 0); // l1.dev.i35pb
assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0) by (compute);assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0); // l1.label.i35pb
assert((flat_pos(mods(), all(), 8, 2)) == 3) by (compute);assert((flat_pos(mods(), all(), 8, 2)) == 3); // l1.pos.i35pa
assert((vcode(mods(), all(), 8)) == 4) by (compute);assert((vcode(mods(), all(), 8)) == 4); // l1.version.i35pa
assert((b(mods()[8].dev)) == 0) by (compute);assert((b(mods()[8].dev)) == 0); // l1.dev.i35pa
assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0) by (compute);assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0); // l1.label.i35pa
assert((flat_pos(mods(), all(), 10, 2)) == 4) by (compute);assert((flat_pos(mods(), all(), 10, 2)) == 4); // l1.pos.i35n
assert((vcode(mods(), all(), 10)) == 1) by (compute);assert((vcode(mods(), all(), 10)) == 1); // l1.version.i35n
assert((b(mods()[10].dev)) == 0) by (compute);assert((b(mods()[10].dev)) == 0); // l1.dev.i35n
assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1) by (compute);assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1); // l1.label.i35n
assert((flat_pos(mods(), all(), 1, 2)) == 5) by (compute);assert((flat_pos(mods(), all(), 1, 2)) == 5); // l1.pos.i35a
assert((vcode(mods(), all(), 1)) == 1) by (compute);assert((vcode(mods(), all(), 1)) == 1); // l1.version.i35a
assert((b(mods()[1].dev)) == 0) by (compute);assert((b(mods()[1].dev)) == 0); // l1.dev.i35a
assert((b(mods()[1].label != mods()[1].name && !mods()[1].ipm)) == 0) by (compute);assert((b(mods()[1].label != mods()[1].name && !mods()[1].ipm)) == 0); // l1.label.i35a
assert((flat_pos(mods(), all(), 2, 2)) == 6) by (compute);assert((flat_pos(mods(), all(), 2, 2)) == 6); // l1.pos.i35b
assert((vcode(mods(), all(), 2)) == 1) by (compute);assert((vcode(mods(), all(), 2)) == 1); // l1.version.i35b
assert((b(mods()[2].dev)) == 0) by (compute);assert((b(mods()[2].dev)) == 0); // l1.dev.i35b
assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0) by (compute);assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0); // l1.label.i35b
assert((flat_pos(mods(), all(), 3, 2)) == 7) by (compute);assert((flat_pos(mods(), all(), 3, 2)) == 7); // l1.pos.i35c
assert((vcode(mods(), all(), 3)) == 1) by (compute);assert((vcode(mods(), all(), 3)) == 1); // l1.version.i35c
assert((b(mods()[3].dev)) == 0) by (compute);assert((b(mods()[3].dev)) == 0); // l1.dev.i35c
assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0) by (compute);assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0); // l1.label.i35c
assert((flat_pos(mods(), all(), 4, 2)) == 8) by (compute);assert((flat_pos(mods(), all(), 4, 2)) == 8); // l1.pos.i35d
assert((vcode(mods(), all(), 4)) == 1) by (compute);assert((vcode(mods(), all(), 4)) == 1); // l1.version.i35d
assert((b(mods()[4].dev)) == 0) by (compute);assert((b(mods()[4].dev)) == 0); // l1.dev.i35d
assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0) by (compute);assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0); // l1.label.i35d
assert((flat_pos(mods(), all(), 5, 2)) == 9) by (compute);assert((flat_pos(mods(), all(), 5, 2)) == 9); // l1.pos.i35e
assert((vcode(mods(), all(), 5)) == 1) by (compute);assert((vcode(mods(), all(), 5)) == 1); // l1.version.i35e
assert((b(mods()[5].dev)) == 1) by (compute);assert((b(mods()[5].dev)) == 1); // l1.dev.i35e
assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0) by (compute);assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0); // l1.label.i35e
assert((flat_pos(mods(), all(), 7, 2)) == 10) by (compute);assert((flat_pos(mods(), all(), 7, 2)) == 10); // l1.pos.i35y
assert((vcode(mods(), all(), 7)) == 3) by (compute);assert((vcode(mods(), all(), 7)) == 3); // l1.version.i35y
assert((b(mods()[7].dev)) == 0) by (compute);assert((b(mods()[7].dev)) == 0); // l1.dev.i35y
assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0) by (compute);assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0); // l1.label.i35y
assert((flat_pos(mods(), all(), 6, 2)) == 11) by (compute);assert((flat_pos(mods(), all(), 6, 2)) == 11); // l1.pos.i35x
assert((vcode(mods(), all(), 6)) == 2) by (compute);assert((vcode(mods(), all(), 6)) == 2); // l1.version.i35x
assert((b(mods()[6].dev)) == 0) by (compute);assert((b(mods()[6].dev)) == 0); // l1.dev.i35x
assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0) by (compute);assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0); // l1.label.i35x
assert(((if all() == all() { 11int } else { 10int })) == 11) by (compute);assert(((if all() == all() { 11int } else { 10int })) == 11); // l1.rows
assert((1int) == 1) by (compute);assert((1int) == 1); // l2.ok
assert((flat_pos(mods(), all(), 0, 1)) == 1) by (compute);assert((flat_pos(mods(), all(), 0, 1)) == 1); // l2.pos.zpm
assert((vcode(mods(), all(), 0)) == 8) by (compute);assert((vcode(mods(), all(), 0)) == 8); // l2.version.zpm
assert((b(mods()[0].dev)) == 0) by (compute);assert((b(mods()[0].dev)) == 0); // l2.dev.zpm
assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0) by (compute);assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0); // l2.label.zpm
assert((flat_pos(mods(), all(), 10, 1)) == 2) by (compute);assert((flat_pos(mods(), all(), 10, 1)) == 2); // l2.pos.i35n
assert((vcode(mods(), all(), 10)) == 1) by (compute);assert((vcode(mods(), all(), 10)) == 1); // l2.version.i35n
assert((b(mods()[10].dev)) == 0) by (compute);assert((b(mods()[10].dev)) == 0); // l2.dev.i35n
assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1) by (compute);assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1); // l2.label.i35n
assert((flat_pos(mods(), all(), 7, 1)) == 3) by (compute);assert((flat_pos(mods(), all(), 7, 1)) == 3); // l2.pos.i35y
assert((vcode(mods(), all(), 7)) == 3) by (compute);assert((vcode(mods(), all(), 7)) == 3); // l2.version.i35y
assert((b(mods()[7].dev)) == 0) by (compute);assert((b(mods()[7].dev)) == 0); // l2.dev.i35y
assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0) by (compute);assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0); // l2.label.i35y
assert((flat_pos(mods(), all(), 6, 1)) == 4) by (compute);assert((flat_pos(mods(), all(), 6, 1)) == 4); // l2.pos.i35x
assert((vcode(mods(), all(), 6)) == 2) by (compute);assert((vcode(mods(), all(), 6)) == 2); // l2.version.i35x
assert((b(mods()[6].dev)) == 0) by (compute);assert((b(mods()[6].dev)) == 0); // l2.dev.i35x
assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0) by (compute);assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0); // l2.label.i35x
assert((flat_pos(mods(), all(), 9, 1)) == 5) by (compute);assert((flat_pos(mods(), all(), 9, 1)) == 5); // l2.pos.i35pb
assert((vcode(mods(), all(), 9)) == 5) by (compute);assert((vcode(mods(), all(), 9)) == 5); // l2.version.i35pb
assert((b(mods()[9].dev)) == 0) by (compute);assert((b(mods()[9].dev)) == 0); // l2.dev.i35pb
assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0) by (compute);assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0); // l2.label.i35pb
assert((flat_pos(mods(), all(), 8, 1)) == 6) by (compute);assert((flat_pos(mods(), all(), 8, 1)) == 6); // l2.pos.i35pa
assert((vcode(mods(), all(), 8)) == 4) by (compute);assert((vcode(mods(), all(), 8)) == 4); // l2.version.i35pa
assert((b(mods()[8].dev)) == 0) by (compute);assert((b(mods()[8].dev)) == 0); // l2.dev.i35pa
assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0) by (compute);assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0); // l2.label.i35pa
assert((flat_pos(mods(), all(), 5, 1)) == 7) by (compute);assert((flat_pos(mods(), all(), 5, 1)) == 7); // l2.pos.i35e
assert((vcode(mods(), all(), 5)) == 1) by (compute);assert((vcode(mods(), all(), 5)) == 1); // l2.version.i35e
assert((b(mods()[5].dev)) == 1) by (compute);assert((b(mods()[5].dev)) == 1); // l2.dev.i35e
assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0) by (compute);assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0); // l2.label.i35e
assert((flat_pos(mods(), all(), 4, 1)) == 8) by (compute);assert((flat_pos(mods(), all(), 4, 1)) == 8); // l2.pos.i35d
assert((vcode(mods(), all(), 4)) == 1) by (compute);assert((vcode(mods(), all(), 4)) == 1); // l2.version.i35d
assert((b(mods()[4].dev)) == 0) by (compute);assert((b(mods()[4].dev)) == 0); // l2.dev.i35d
assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0) by (compute);assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0); // l2.label.i35d
assert((flat_pos(mods(), all(), 3, 1)) == 9) by (compute);assert((flat_pos(mods(), all(), 3, 1)) == 9); // l2.pos.i35c
assert((vcode(mods(), all(), 3)) == 1) by (compute);assert((vcode(mods(), all(), 3)) == 1); // l2.version.i35c
assert((b(mods()[3].dev)) == 0) by (compute);assert((b(mods()[3].dev)) == 0); // l2.dev.i35c
assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0) by (compute);assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0); // l2.label.i35c
assert((flat_pos(mods(), all(), 2, 1)) == 10) by (compute);assert((flat_pos(mods(), all(), 2, 1)) == 10); // l2.pos.i35b
assert((vcode(mods(), all(), 2)) == 1) by (compute);assert((vcode(mods(), all(), 2)) == 1); // l2.version.i35b
assert((b(mods()[2].dev)) == 0) by (compute);assert((b(mods()[2].dev)) == 0); // l2.dev.i35b
assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0) by (compute);assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0); // l2.label.i35b
assert((flat_pos(mods(), all(), 1, 1)) == 11) by (compute);assert((flat_pos(mods(), all(), 1, 1)) == 11); // l2.pos.i35a
assert((vcode(mods(), all(), 1)) == 1) by (compute);assert((vcode(mods(), all(), 1)) == 1); // l2.version.i35a
assert((b(mods()[1].dev)) == 0) by (compute);assert((b(mods()[1].dev)) == 0); // l2.dev.i35a
assert((b(mods()[1].label != mods()[1].name && !mods()[1].ipm)) == 0) by (compute);assert((b(mods()[1].label != mods()[1].name && !mods()[1].ipm)) == 0); // l2.label.i35a
assert(((if all() == all() { 11int } else { 10int })) == 11) by (compute);assert(((if all() == all() { 11int } else { 10int })) == 11); // l2.rows
assert((1int) == 1) by (compute);assert((1int) == 1); // l4.ok
assert((flat_pos(mods(), all(), 0, 0)) == 1) by (compute);assert((flat_pos(mods(), all(), 0, 0)) == 1); // l4.pos.zpm
assert((vcode(mods(), all(), 0)) == 8) by (compute);assert((vcode(mods(), all(), 0)) == 8); // l4.version.zpm
assert((b(mods()[0].dev)) == 0) by (compute);assert((b(mods()[0].dev)) == 0); // l4.dev.zpm
assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0) by (compute);assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0); // l4.label.zpm
assert((flat_pos(mods(), all(), 1, 0)) == 2) by (compute);assert((flat_pos(mods(), all(), 1, 0)) == 2); // l4.pos.i35a
assert((vcode(mods(), all(), 1)) == 1) by (compute);assert((vcode(mods(), all(), 1)) == 1); // l4.version.i35a
assert((b(mods()[1].dev)) == 0) by (compute);assert((b(mods()[1].dev)) == 0); // l4.dev.i35a
assert((b(mods()[1].label != mods()[1].name && !mods()[1].ipm)) == 0) by (compute);assert((b(mods()[1].label != mods()[1].name && !mods()[1].ipm)) == 0); // l4.label.i35a
assert((flat_pos(mods(), all(), 2, 0)) == 3) by (compute);assert((flat_pos(mods(), all(), 2, 0)) == 3); // l4.pos.i35b
assert((vcode(mods(), all(), 2)) == 1) by (compute);assert((vcode(mods(), all(), 2)) == 1); // l4.version.i35b
assert((b(mods()[2].dev)) == 0) by (compute);assert((b(mods()[2].dev)) == 0); // l4.dev.i35b
assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0) by (compute);assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0); // l4.label.i35b
assert((flat_pos(mods(), all(), 3, 0)) == 4) by (compute);assert((flat_pos(mods(), all(), 3, 0)) == 4); // l4.pos.i35c
assert((vcode(mods(), all(), 3)) == 1) by (compute);assert((vcode(mods(), all(), 3)) == 1); // l4.version.i35c
assert((b(mods()[3].dev)) == 0) by (compute);assert((b(mods()[3].dev)) == 0); // l4.dev.i35c
assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0) by (compute);assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0); // l4.label.i35c
assert((flat_pos(mods(), all(), 4, 0)) == 5) by (compute);assert((flat_pos(mods(), all(), 4, 0)) == 5); // l4.pos.i35d
assert((vcode(mods(), all(), 4)) == 1) by (compute);assert((vcode(mods(), all(), 4)) == 1); // l4.version.i35d
assert((b(mods()[4].dev)) == 0) by (compute);assert((b(mods()[4].dev)) == 0); // l4.dev.i35d
assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0) by (compute);assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0); // l4.label.i35d
assert((flat_pos(mods(), all(), 5, 0)) == 6) by (compute);assert((flat_pos(mods(), all(), 5, 0)) == 6); // l4.pos.i35e
assert((vcode(mods(), all(), 5)) == 1) by (compute);assert((vcode(mods(), all(), 5)) == 1); // l4.version.i35e
assert((b(mods()[5].dev)) == 1) by (compute);assert((b(mods()[5].dev)) == 1); // l4.dev.i35e
assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0) by (compute);assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0); // l4.label.i35e
assert((flat_pos(mods(), all(), 8, 0)) == 7) by (compute);assert((flat_pos(mods(), all(), 8, 0)) == 7); // l4.pos.i35pa
assert((vcode(mods(), all(), 8)) == 4) by (compute);assert((vcode(mods(), all(), 8)) == 4); // l4.version.i35pa
assert((b(mods()[8].dev)) == 0) by (compute);assert((b(mods()[8].dev)) == 0); // l4.dev.i35pa
assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0) by (compute);assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0); // l4.label.i35pa
assert((flat_pos(mods(), all(), 9, 0)) == 8) by (compute);assert((flat_pos(mods(), all(), 9, 0)) == 8); // l4.pos.i35pb
assert((vcode(mods(), all(), 9)) == 5) by (compute);assert((vcode(mods(), all(), 9)) == 5); // l4.version.i35pb
assert((b(mods()[9].dev)) == 0) by (compute);assert((b(mods()[9].dev)) == 0); // l4.dev.i35pb
assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0) by (compute);assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0); // l4.label.i35pb
assert((flat_pos(mods(), all(), 6, 0)) == 9) by (compute);assert((flat_pos(mods(), all(), 6, 0)) == 9); // l4.pos.i35x
assert((vcode(mods(), all(), 6)) == 2) by (compute);assert((vcode(mods(), all(), 6)) == 2); // l4.version.i35x
assert((b(mods()[6].dev)) == 0) by (compute);assert((b(mods()[6].dev)) == 0); // l4.dev.i35x
assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0) by (compute);assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0); // l4.label.i35x
assert((flat_pos(mods(), all(), 7, 0)) == 10) by (compute);assert((flat_pos(mods(), all(), 7, 0)) == 10); // l4.pos.i35y
assert((vcode(mods(), all(), 7)) == 3) by (compute);assert((vcode(mods(), all(), 7)) == 3); // l4.version.i35y
assert((b(mods()[7].dev)) == 0) by (compute);assert((b(mods()[7].dev)) == 0); // l4.dev.i35y
assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0) by (compute);assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0); // l4.label.i35y
assert((flat_pos(mods(), all(), 10, 0)) == 11) by (compute);assert((flat_pos(mods(), all(), 10, 0)) == 11); // l4.pos.i35n
assert((vcode(mods(), all(), 10)) == 1) by (compute);assert((vcode(mods(), all(), 10)) == 1); // l4.version.i35n
assert((b(mods()[10].dev)) == 0) by (compute);assert((b(mods()[10].dev)) == 0); // l4.dev.i35n
assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1) by (compute);assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1); // l4.label.i35n
assert(((if all() == all() { 11int } else { 10int })) == 11) by (compute);assert(((if all() == all() { 11int } else { 10int })) == 11); // l4.rows
assert((1int) == 1) by (compute);assert((1int) == 1); // t0.ok
assert((first_pos(tree(mods(), all()), 3)) == 1) by (compute);assert((first_pos(tree(mods(), all()), 3)) == 1); // t0.pos.i35c
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 3))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 3))) == 0); // t0.depth.i35c
assert((vcode(mods(), all(), 3)) == 1) by (compute);assert((vcode(mods(), all(), 3)) == 1); // t0.version.i35c
assert((first_pos(tree(mods(), all()), 2)) == 2) by (compute);assert((first_pos(tree(mods(), all()), 2)) == 2); // t0.pos.i35b
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 2))) == 1) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 2))) == 1); // t0.depth.i35b
assert((vcode(mods(), all(), 2)) == 1) by (compute);assert((vcode(mods(), all(), 2)) == 1); // t0.version.i35b
assert((first_pos(tree(mods(), all()), 1)) == 3) by (compute);assert((first_pos(tree(mods(), all()), 1)) == 3); // t0.pos.i35a
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 1))) == 2) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 1))) == 2); // t0.depth.i35a
assert((vcode(mods(), all(), 1)) == 1) by (compute);assert((vcode(mods(), all(), 1)) == 1); // t0.version.i35a
assert((first_pos(tree(mods(), all()), 4)) == 4) by (compute);assert((first_pos(tree(mods(), all()), 4)) == 4); // t0.pos.i35d
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 4))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 4))) == 0); // t0.depth.i35d
assert((vcode(mods(), all(), 4)) == 1) by (compute);assert((vcode(mods(), all(), 4)) == 1); // t0.version.i35d
assert((depth_at(tree(mods(), all()), second_pos(tree(mods(), all()), 1))) == 1) by (compute);assert((depth_at(tree(mods(), all()), second_pos(tree(mods(), all()), 1))) == 1); // t0.depth2.i35a
assert((first_pos(tree(mods(), all()), 5)) == 6) by (compute);assert((first_pos(tree(mods(), all()), 5)) == 6); // t0.pos.i35e
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 5))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 5))) == 0); // t0.depth.i35e
assert((vcode(mods(), all(), 5)) == 1) by (compute);assert((vcode(mods(), all(), 5)) == 1); // t0.version.i35e
assert((first_pos(tree(mods(), all()), 10)) == 7) by (compute);assert((first_pos(tree(mods(), all()), 10)) == 7); // t0.pos.i35n
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 10))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 10))) == 0); // t0.depth.i35n
assert((vcode(mods(), all(), 10)) == 1) by (compute);assert((vcode(mods(), all(), 10)) == 1); // t0.version.i35n
assert((first_pos(tree(mods(), all()), 8)) == 8) by (compute);assert((first_pos(tree(mods(), all()), 8)) == 8); // t0.pos.i35pa
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 8))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 8))) == 0); // t0.depth.i35pa
assert((vcode(mods(), all(), 8)) == 4) by (compute);assert((vcode(mods(), all(), 8)) == 4); // t0.version.i35pa
assert((first_pos(tree(mods(), all()), 9)) == 9) by (compute);assert((first_pos(tree(mods(), all()), 9)) == 9); // t0.pos.i35pb
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 9))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 9))) == 0); // t0.depth.i35pb
assert((vcode(mods(), all(), 9)) == 5) by (compute);assert((vcode(mods(), all(), 9)) == 5); // t0.version.i35pb
assert((first_pos(tree(mods(), all()), 6)) == 10) by (compute);assert((first_pos(tree(mods(), all()), 6)) == 10); // t0.pos.i35x
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 6))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 6))) == 0); // t0.depth.i35x
assert((vcode(mods(), all(), 6)) == 2) by (compute);assert((vcode(mods(), all(), 6)) == 2); // t0.version.i35x
assert((first_pos(tree(mods(), all()), 7)) == 11) by (compute);assert((first_pos(tree(mods(), all()), 7)) == 11); // t0.pos.i35y
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 7))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 7))) == 0); // t0.depth.i35y
assert((vcode(mods(), all(), 7)) == 3) by (compute);assert((vcode(mods(), all(), 7)) == 3); // t0.version.i35y
assert((first_pos(tree(mods(), all()), 0)) == 12) by (compute);assert((first_pos(tree(mods(), all()), 0)) == 12); // t0.pos.zpm
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 0))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 0))) == 0); // t0.depth.zpm
assert((vcode(mods(), all(), 0)) == 8) by (compute);assert((vcode(mods(), all(), 0)) == 8); // t0.version.zpm
assert((occurrences(tree(mods(), all()), 0)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 0)) == 1); // t0.count.zpm
assert((occurrences(tree(mods(), all()), 1)) == 2) by (compute);assert((occurrences(tree(mods(), all()), 1)) == 2); // t0.count.i35a
assert((occurrences(tree(mods(), all()), 2)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 2)) == 1); // t0.count.i35b
assert((occurrences(tree(mods(), all()), 3)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 3)) == 1); // t0.count.i35c
assert((occurrences(tree(mods(), all()), 4)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 4)) == 1); // t0.count.i35d
assert((occurrences(tree(mods(), all()), 5)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 5)) == 1); // t0.count.i35e
assert((occurrences(tree(mods(), all()), 6)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 6)) == 1); // t0.count.i35x
assert((occurrences(tree(mods(), all()), 7)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 7)) == 1); // t0.count.i35y
assert((occurrences(tree(mods(), all()), 8)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 8)) == 1); // t0.count.i35pa
assert((occurrences(tree(mods(), all()), 9)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 9)) == 1); // t0.count.i35pb
assert((occurrences(tree(mods(), all()), 10)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 10)) == 1); // t0.count.i35n
assert((tree(mods(), all()).len() as int) == 12) by (compute);assert((tree(mods(), all()).len() as int) == 12); // t0.rows
assert((1int) == 1) by (compute);assert((1int) == 1); // t1.ok
assert((first_pos(tree(mods(), all()), 3)) == 1) by (compute);assert((first_pos(tree(mods(), all()), 3)) == 1); // t1.pos.i35c
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 3))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 3))) == 0); // t1.depth.i35c
assert((vcode(mods(), all(), 3)) == 1) by (compute);assert((vcode(mods(), all(), 3)) == 1); // t1.version.i35c
assert((first_pos(tree(mods(), all()), 2)) == 2) by (compute);assert((first_pos(tree(mods(), all()), 2)) == 2); // t1.pos.i35b
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 2))) == 1) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 2))) == 1); // t1.depth.i35b
assert((vcode(mods(), all(), 2)) == 1) by (compute);assert((vcode(mods(), all(), 2)) == 1); // t1.version.i35b
assert((first_pos(tree(mods(), all()), 1)) == 3) by (compute);assert((first_pos(tree(mods(), all()), 1)) == 3); // t1.pos.i35a
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 1))) == 2) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 1))) == 2); // t1.depth.i35a
assert((vcode(mods(), all(), 1)) == 1) by (compute);assert((vcode(mods(), all(), 1)) == 1); // t1.version.i35a
assert((first_pos(tree(mods(), all()), 4)) == 4) by (compute);assert((first_pos(tree(mods(), all()), 4)) == 4); // t1.pos.i35d
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 4))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 4))) == 0); // t1.depth.i35d
assert((vcode(mods(), all(), 4)) == 1) by (compute);assert((vcode(mods(), all(), 4)) == 1); // t1.version.i35d
assert((depth_at(tree(mods(), all()), second_pos(tree(mods(), all()), 1))) == 1) by (compute);assert((depth_at(tree(mods(), all()), second_pos(tree(mods(), all()), 1))) == 1); // t1.depth2.i35a
assert((first_pos(tree(mods(), all()), 5)) == 6) by (compute);assert((first_pos(tree(mods(), all()), 5)) == 6); // t1.pos.i35e
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 5))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 5))) == 0); // t1.depth.i35e
assert((vcode(mods(), all(), 5)) == 1) by (compute);assert((vcode(mods(), all(), 5)) == 1); // t1.version.i35e
assert((first_pos(tree(mods(), all()), 10)) == 7) by (compute);assert((first_pos(tree(mods(), all()), 10)) == 7); // t1.pos.i35n
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 10))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 10))) == 0); // t1.depth.i35n
assert((vcode(mods(), all(), 10)) == 1) by (compute);assert((vcode(mods(), all(), 10)) == 1); // t1.version.i35n
assert((first_pos(tree(mods(), all()), 8)) == 8) by (compute);assert((first_pos(tree(mods(), all()), 8)) == 8); // t1.pos.i35pa
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 8))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 8))) == 0); // t1.depth.i35pa
assert((vcode(mods(), all(), 8)) == 4) by (compute);assert((vcode(mods(), all(), 8)) == 4); // t1.version.i35pa
assert((first_pos(tree(mods(), all()), 9)) == 9) by (compute);assert((first_pos(tree(mods(), all()), 9)) == 9); // t1.pos.i35pb
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 9))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 9))) == 0); // t1.depth.i35pb
assert((vcode(mods(), all(), 9)) == 5) by (compute);assert((vcode(mods(), all(), 9)) == 5); // t1.version.i35pb
assert((first_pos(tree(mods(), all()), 6)) == 10) by (compute);assert((first_pos(tree(mods(), all()), 6)) == 10); // t1.pos.i35x
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 6))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 6))) == 0); // t1.depth.i35x
assert((vcode(mods(), all(), 6)) == 2) by (compute);assert((vcode(mods(), all(), 6)) == 2); // t1.version.i35x
assert((first_pos(tree(mods(), all()), 7)) == 11) by (compute);assert((first_pos(tree(mods(), all()), 7)) == 11); // t1.pos.i35y
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 7))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 7))) == 0); // t1.depth.i35y
assert((vcode(mods(), all(), 7)) == 3) by (compute);assert((vcode(mods(), all(), 7)) == 3); // t1.version.i35y
assert((first_pos(tree(mods(), all()), 0)) == 12) by (compute);assert((first_pos(tree(mods(), all()), 0)) == 12); // t1.pos.zpm
assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 0))) == 0) by (compute);assert((depth_at(tree(mods(), all()), first_pos(tree(mods(), all()), 0))) == 0); // t1.depth.zpm
assert((vcode(mods(), all(), 0)) == 8) by (compute);assert((vcode(mods(), all(), 0)) == 8); // t1.version.zpm
assert((occurrences(tree(mods(), all()), 0)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 0)) == 1); // t1.count.zpm
assert((occurrences(tree(mods(), all()), 1)) == 2) by (compute);assert((occurrences(tree(mods(), all()), 1)) == 2); // t1.count.i35a
assert((occurrences(tree(mods(), all()), 2)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 2)) == 1); // t1.count.i35b
assert((occurrences(tree(mods(), all()), 3)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 3)) == 1); // t1.count.i35c
assert((occurrences(tree(mods(), all()), 4)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 4)) == 1); // t1.count.i35d
assert((occurrences(tree(mods(), all()), 5)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 5)) == 1); // t1.count.i35e
assert((occurrences(tree(mods(), all()), 6)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 6)) == 1); // t1.count.i35x
assert((occurrences(tree(mods(), all()), 7)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 7)) == 1); // t1.count.i35y
assert((occurrences(tree(mods(), all()), 8)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 8)) == 1); // t1.count.i35pa
assert((occurrences(tree(mods(), all()), 9)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 9)) == 1); // t1.count.i35pb
assert((occurrences(tree(mods(), all()), 10)) == 1) by (compute);assert((occurrences(tree(mods(), all()), 10)) == 1); // t1.count.i35n
assert((tree(mods(), all()).len() as int) == 12) by (compute);assert((tree(mods(), all()).len() as int) == 12); // t1.rows
assert((1int) == 1) by (compute);assert((1int) == 1); // t2.ok
assert((first_pos(tree(mods(), cut()), 3)) == 1) by (compute);assert((first_pos(tree(mods(), cut()), 3)) == 1); // t2.pos.i35c
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 3))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 3))) == 0); // t2.depth.i35c
assert((vcode(mods(), cut(), 3)) == 1) by (compute);assert((vcode(mods(), cut(), 3)) == 1); // t2.version.i35c
assert((first_pos(tree(mods(), cut()), 2)) == 2) by (compute);assert((first_pos(tree(mods(), cut()), 2)) == 2); // t2.pos.i35b
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 2))) == 1) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 2))) == 1); // t2.depth.i35b
assert((vcode(mods(), cut(), 2)) == 1) by (compute);assert((vcode(mods(), cut(), 2)) == 1); // t2.version.i35b
assert((first_pos(tree(mods(), cut()), 1)) == 3) by (compute);assert((first_pos(tree(mods(), cut()), 1)) == 3); // t2.pos.i35a
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 1))) == 2) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 1))) == 2); // t2.depth.i35a
assert((first_pos(tree(mods(), cut()), 4)) == 4) by (compute);assert((first_pos(tree(mods(), cut()), 4)) == 4); // t2.pos.i35d
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 4))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 4))) == 0); // t2.depth.i35d
assert((vcode(mods(), cut(), 4)) == 1) by (compute);assert((vcode(mods(), cut(), 4)) == 1); // t2.version.i35d
assert((depth_at(tree(mods(), cut()), second_pos(tree(mods(), cut()), 1))) == 1) by (compute);assert((depth_at(tree(mods(), cut()), second_pos(tree(mods(), cut()), 1))) == 1); // t2.depth2.i35a
assert((first_pos(tree(mods(), cut()), 5)) == 6) by (compute);assert((first_pos(tree(mods(), cut()), 5)) == 6); // t2.pos.i35e
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 5))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 5))) == 0); // t2.depth.i35e
assert((vcode(mods(), cut(), 5)) == 1) by (compute);assert((vcode(mods(), cut(), 5)) == 1); // t2.version.i35e
assert((first_pos(tree(mods(), cut()), 10)) == 7) by (compute);assert((first_pos(tree(mods(), cut()), 10)) == 7); // t2.pos.i35n
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 10))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 10))) == 0); // t2.depth.i35n
assert((vcode(mods(), cut(), 10)) == 1) by (compute);assert((vcode(mods(), cut(), 10)) == 1); // t2.version.i35n
assert((first_pos(tree(mods(), cut()), 8)) == 8) by (compute);assert((first_pos(tree(mods(), cut()), 8)) == 8); // t2.pos.i35pa
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 8))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 8))) == 0); // t2.depth.i35pa
assert((vcode(mods(), cut(), 8)) == 4) by (compute);assert((vcode(mods(), cut(), 8)) == 4); // t2.version.i35pa
assert((first_pos(tree(mods(), cut()), 9)) == 9) by (compute);assert((first_pos(tree(mods(), cut()), 9)) == 9); // t2.pos.i35pb
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 9))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 9))) == 0); // t2.depth.i35pb
assert((vcode(mods(), cut(), 9)) == 5) by (compute);assert((vcode(mods(), cut(), 9)) == 5); // t2.version.i35pb
assert((first_pos(tree(mods(), cut()), 6)) == 10) by (compute);assert((first_pos(tree(mods(), cut()), 6)) == 10); // t2.pos.i35x
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 6))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 6))) == 0); // t2.depth.i35x
assert((vcode(mods(), cut(), 6)) == 2) by (compute);assert((vcode(mods(), cut(), 6)) == 2); // t2.version.i35x
assert((first_pos(tree(mods(), cut()), 7)) == 11) by (compute);assert((first_pos(tree(mods(), cut()), 7)) == 11); // t2.pos.i35y
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 7))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 7))) == 0); // t2.depth.i35y
assert((vcode(mods(), cut(), 7)) == 3) by (compute);assert((vcode(mods(), cut(), 7)) == 3); // t2.version.i35y
assert((first_pos(tree(mods(), cut()), 0)) == 12) by (compute);assert((first_pos(tree(mods(), cut()), 0)) == 12); // t2.pos.zpm
assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 0))) == 0) by (compute);assert((depth_at(tree(mods(), cut()), first_pos(tree(mods(), cut()), 0))) == 0); // t2.depth.zpm
assert((vcode(mods(), cut(), 0)) == 8) by (compute);assert((vcode(mods(), cut(), 0)) == 8); // t2.version.zpm
assert((occurrences(tree(mods(), cut()), 0)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 0)) == 1); // t2.count.zpm
assert((occurrences(tree(mods(), cut()), 1)) == 2) by (compute);assert((occurrences(tree(mods(), cut()), 1)) == 2); // t2.count.i35a
assert((occurrences(tree(mods(), cut()), 2)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 2)) == 1); // t2.count.i35b
assert((occurrences(tree(mods(), cut()), 3)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 3)) == 1); // t2.count.i35c
assert((occurrences(tree(mods(), cut()), 4)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 4)) == 1); // t2.count.i35d
assert((occurrences(tree(mods(), cut()), 5)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 5)) == 1); // t2.count.i35e
assert((occurrences(tree(mods(), cut()), 6)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 6)) == 1); // t2.count.i35x
assert((occurrences(tree(mods(), cut()), 7)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 7)) == 1); // t2.count.i35y
assert((occurrences(tree(mods(), cut()), 8)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 8)) == 1); // t2.count.i35pa
assert((occurrences(tree(mods(), cut()), 9)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 9)) == 1); // t2.count.i35pb
assert((occurrences(tree(mods(), cut()), 10)) == 1) by (compute);assert((occurrences(tree(mods(), cut()), 10)) == 1); // t2.count.i35n
assert((tree(mods(), cut()).len() as int) == 12) by (compute);assert((tree(mods(), cut()).len() as int) == 12); // t2.rows
assert((1int) == 1) by (compute);assert((1int) == 1); // l5.ok
assert((flat_pos(mods(), cut(), 0, 0)) == 1) by (compute);assert((flat_pos(mods(), cut(), 0, 0)) == 1); // l5.pos.zpm
assert((vcode(mods(), cut(), 0)) == 8) by (compute);assert((vcode(mods(), cut(), 0)) == 8); // l5.version.zpm
assert((b(mods()[0].dev)) == 0) by (compute);assert((b(mods()[0].dev)) == 0); // l5.dev.zpm
assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0) by (compute);assert((b(mods()[0].label != mods()[0].name && !mods()[0].ipm)) == 0); // l5.label.zpm
assert((flat_pos(mods(), cut(), 2, 0)) == 2) by (compute);assert((flat_pos(mods(), cut(), 2, 0)) == 2); // l5.pos.i35b
assert((vcode(mods(), cut(), 2)) == 1) by (compute);assert((vcode(mods(), cut(), 2)) == 1); // l5.version.i35b
assert((b(mods()[2].dev)) == 0) by (compute);assert((b(mods()[2].dev)) == 0); // l5.dev.i35b
assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0) by (compute);assert((b(mods()[2].label != mods()[2].name && !mods()[2].ipm)) == 0); // l5.label.i35b
assert((flat_pos(mods(), cut(), 3, 0)) == 3) by (compute);assert((flat_pos(mods(), cut(), 3, 0)) == 3); // l5.pos.i35c
assert((vcode(mods(), cut(), 3)) == 1) by (compute);assert((vcode(mods(), cut(), 3)) == 1); // l5.version.i35c
assert((b(mods()[3].dev)) == 0) by (compute);assert((b(mods()[3].dev)) == 0); // l5.dev.i35c
assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0) by (compute);assert((b(mods()[3].label != mods()[3].name && !mods()[3].ipm)) == 0); // l5.label.i35c
assert((flat_pos(mods(), cut(), 4, 0)) == 4) by (compute);assert((flat_pos(mods(), cut(), 4, 0)) == 4); // l5.pos.i35d
assert((vcode(mods(), cut(), 4)) == 1) by (compute);assert((vcode(mods(), cut(), 4)) == 1); // l5.version.i35d
assert((b(mods()[4].dev)) == 0) by (compute);assert((b(mods()[4].dev)) == 0); // l5.dev.i35d
assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0) by (compute);assert((b(mods()[4].label != mods()[4].name && !mods()[4].ipm)) == 0); // l5.label.i35d
assert((flat_pos(mods(), cut(), 5, 0)) == 5) by (compute);assert((flat_pos(mods(), cut(), 5, 0)) == 5); // l5.pos.i35e
assert((vcode(mods(), cut(), 5)) == 1) by (compute);assert((vcode(mods(), cut(), 5)) == 1); // l5.version.i35e
assert((b(mods()[5].dev)) == 1) by (compute);assert((b(mods()[5].dev)) == 1); // l5.dev.i35e
assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0) by (compute);assert((b(mods()[5].label != mods()[5].name && !mods()[5].ipm)) == 0); // l5.label.i35e
assert((flat_pos(mods(), cut(), 8, 0)) == 6) by (compute);assert((flat_pos(mods(), cut(), 8, 0)) == 6); // l5.pos.i35pa
assert((vcode(mods(), cut(), 8)) == 4) by (compute);assert((vcode(mods(), cut(), 8)) == 4); // l5.version.i35pa
assert((b(mods()[8].dev)) == 0) by (compute);assert((b(mods()[8].dev)) == 0); // l5.dev.i35pa
assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0) by (compute);assert((b(mods()[8].label != mods()[8].name && !mods()[8].ipm)) == 0); // l5.label.i35pa
assert((flat_pos(mods(), cut(), 9, 0)) == 7) by (compute);assert((flat_pos(mods(), cut(), 9, 0)) == 7); // l5.pos.i35pb
assert((vcode(mods(), cut(), 9)) == 5) by (compute);assert((vcode(mods(), cut(), 9)) == 5); // l5.version.i35pb
assert((b(mods()[9].dev)) == 0) by (compute);assert((b(mods()[9].dev)) == 0); // l5.dev.i35pb
assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0) by (compute);assert((b(mods()[9].label != mods()[9].name && !mods()[9].ipm)) == 0); // l5.label.i35pb
assert((flat_pos(mods(), cut(), 6, 0)) == 8) by (compute);assert((flat_pos(mods(), cut(), 6, 0)) == 8); // l5.pos.i35x
assert((vcode(mods(), cut(), 6)) == 2) by (compute);assert((vcode(mods(), cut(), 6)) == 2); // l5.version.i35x
assert((b(mods()[6].dev)) == 0) by (compute);assert((b(mods()[6].dev)) == 0); // l5.dev.i35x
assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0) by (compute);assert((b(mods()[6].label != mods()[6].name && !mods()[6].ipm)) == 0); // l5.label.i35x
assert((flat_pos(mods(), cut(), 7, 0)) == 9) by (compute);assert((flat_pos(mods(), cut(), 7, 0)) == 9); // l5.pos.i35y
assert((vcode(mods(), cut(), 7)) == 3) by (compute);assert((vcode(mods(), cut(), 7)) == 3); // l5.version.i35y
assert((b(mods()[7].dev)) == 0) by (compute);assert((b(mods()[7].dev)) == 0); // l5.dev.i35y
assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0) by (compute);assert((b(mods()[7].label != mods()[7].name && !mods()[7].ipm)) == 0); // l5.label.i35y
assert((flat_pos(mods(), cut(), 10, 0)) == 10) by (compute);assert((flat_pos(mods(), cut(), 10, 0)) == 10); // l5.pos.i35n
assert((vcode(mods(), cut(), 10)) == 1) by (compute);assert((vcode(mods(), cut(), 10)) == 1); // l5.version.i35n
assert((b(mods()[10].dev)) == 0) by (compute);assert((b(mods()[10].dev)) == 0); // l5.dev.i35n
assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1) by (compute);assert((b(mods()[10].label != mods()[10].name && !mods()[10].ipm)) == 1); // l5.label.i35n
assert((flat_pos(mods(), cut(), 1, 0)) == 0) by (compute);assert((flat_pos(mods(), cut(), 1, 0)) == 0); // l5.pos.i35a
assert(((if cut() == all() { 11int } else { 10int })) == 10) by (compute);assert(((if cut() == all() { 11int } else { 10int })) == 10); // l5.rows
}
}
fn main() {}
