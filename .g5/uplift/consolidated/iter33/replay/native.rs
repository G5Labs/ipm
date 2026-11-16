#[path="../../../../Verus_verification/consolidated/iter33/dependents.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {
reveal_with_fuel(chain_node, 8); reveal_with_fuel(depth_from, 8); reveal_with_fuel(graph_ok_from, 8); reveal_with_fuel(others_ok_from, 8); reveal_with_fuel(tree_has_from, 8); reveal_with_fuel(tree_depth_from, 8); reveal_with_fuel(tree_ver_from, 8); reveal_with_fuel(tree_count_from, 8); reveal_with_fuel(any_warned_from, 8);
assert((b(tree_has(w0(), A(), 3))) == 0) by (compute);assert((b(tree_has(w0(), A(), 3))) == 0); // l0t.count.i33d
assert((b(listed(w0(), false, A(), 0, 6))) == 1) by (compute);assert((b(listed(w0(), false, A(), 0, 6))) == 1); // l0.list.i33g
assert((b(warned(w2(), false, 1))) == 1) by (compute);assert((b(warned(w2(), false, 1))) == 1); // l2.warn.i33b
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l0.ok
assert((b(any_warned_from(w0(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), false, 0))) == 0); // l0.warning
assert((b(listed(w0(), false, A(), 0, 0))) == 0) by (compute);assert((b(listed(w0(), false, A(), 0, 0))) == 0); // l0.list.i33a
assert(((if listed(w0(), false, A(), 0, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 0, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0); // l0.ver.i33a
assert(((if listed(w0(), false, A(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0.server.i33a
assert((b(warned(w0(), false, 0))) == 0) by (compute);assert((b(warned(w0(), false, 0))) == 0); // l0.warn.i33a
assert((b(listed(w0(), false, A(), 0, 1))) == 1) by (compute);assert((b(listed(w0(), false, A(), 0, 1))) == 1); // l0.list.i33b
assert(((if listed(w0(), false, A(), 0, 1) { root_ver(w0(), false, 1) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, A(), 0, 1) { root_ver(w0(), false, 1) } else { 0int })) == 100); // l0.ver.i33b
assert(((if listed(w0(), false, A(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, A(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 1); // l0.server.i33b
assert((b(warned(w0(), false, 1))) == 0) by (compute);assert((b(warned(w0(), false, 1))) == 0); // l0.warn.i33b
assert((b(listed(w0(), false, A(), 0, 2))) == 1) by (compute);assert((b(listed(w0(), false, A(), 0, 2))) == 1); // l0.list.i33c
assert(((if listed(w0(), false, A(), 0, 2) { root_ver(w0(), false, 2) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, A(), 0, 2) { root_ver(w0(), false, 2) } else { 0int })) == 100); // l0.ver.i33c
assert(((if listed(w0(), false, A(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, A(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 1); // l0.server.i33c
assert((b(warned(w0(), false, 2))) == 0) by (compute);assert((b(warned(w0(), false, 2))) == 0); // l0.warn.i33c
assert((b(listed(w0(), false, A(), 0, 3))) == 1) by (compute);assert((b(listed(w0(), false, A(), 0, 3))) == 1); // l0.list.i33d
assert(((if listed(w0(), false, A(), 0, 3) { root_ver(w0(), false, 3) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, A(), 0, 3) { root_ver(w0(), false, 3) } else { 0int })) == 100); // l0.ver.i33d
assert(((if listed(w0(), false, A(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, A(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 1); // l0.server.i33d
assert((b(warned(w0(), false, 3))) == 0) by (compute);assert((b(warned(w0(), false, 3))) == 0); // l0.warn.i33d
assert((b(listed(w0(), false, A(), 0, 4))) == 0) by (compute);assert((b(listed(w0(), false, A(), 0, 4))) == 0); // l0.list.i33e
assert(((if listed(w0(), false, A(), 0, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 0, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0); // l0.ver.i33e
assert(((if listed(w0(), false, A(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0.server.i33e
assert((b(warned(w0(), false, 4))) == 0) by (compute);assert((b(warned(w0(), false, 4))) == 0); // l0.warn.i33e
assert((b(listed(w0(), false, A(), 0, 5))) == 0) by (compute);assert((b(listed(w0(), false, A(), 0, 5))) == 0); // l0.list.i33f
assert(((if listed(w0(), false, A(), 0, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 0, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0); // l0.ver.i33f
assert(((if listed(w0(), false, A(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0.server.i33f
assert((b(warned(w0(), false, 5))) == 0) by (compute);assert((b(warned(w0(), false, 5))) == 0); // l0.warn.i33f
assert(((if listed(w0(), false, A(), 0, 6) { root_ver(w0(), false, 6) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, A(), 0, 6) { root_ver(w0(), false, 6) } else { 0int })) == 100); // l0.ver.i33g
assert(((if listed(w0(), false, A(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, A(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 1); // l0.server.i33g
assert((b(warned(w0(), false, 6))) == 0) by (compute);assert((b(warned(w0(), false, 6))) == 0); // l0.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l0v1.ok
assert((b(any_warned_from(w0(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), false, 0))) == 0); // l0v1.warning
assert((b(listed(w0(), false, A(), 100, 0))) == 0) by (compute);assert((b(listed(w0(), false, A(), 100, 0))) == 0); // l0v1.list.i33a
assert(((if listed(w0(), false, A(), 100, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0); // l0v1.ver.i33a
assert(((if listed(w0(), false, A(), 100, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v1.server.i33a
assert((b(warned(w0(), false, 0))) == 0) by (compute);assert((b(warned(w0(), false, 0))) == 0); // l0v1.warn.i33a
assert((b(listed(w0(), false, A(), 100, 1))) == 0) by (compute);assert((b(listed(w0(), false, A(), 100, 1))) == 0); // l0v1.list.i33b
assert(((if listed(w0(), false, A(), 100, 1) { root_ver(w0(), false, 1) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 1) { root_ver(w0(), false, 1) } else { 0int })) == 0); // l0v1.ver.i33b
assert(((if listed(w0(), false, A(), 100, 1) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 1) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v1.server.i33b
assert((b(warned(w0(), false, 1))) == 0) by (compute);assert((b(warned(w0(), false, 1))) == 0); // l0v1.warn.i33b
assert((b(listed(w0(), false, A(), 100, 2))) == 0) by (compute);assert((b(listed(w0(), false, A(), 100, 2))) == 0); // l0v1.list.i33c
assert(((if listed(w0(), false, A(), 100, 2) { root_ver(w0(), false, 2) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 2) { root_ver(w0(), false, 2) } else { 0int })) == 0); // l0v1.ver.i33c
assert(((if listed(w0(), false, A(), 100, 2) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 2) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v1.server.i33c
assert((b(warned(w0(), false, 2))) == 0) by (compute);assert((b(warned(w0(), false, 2))) == 0); // l0v1.warn.i33c
assert((b(listed(w0(), false, A(), 100, 3))) == 0) by (compute);assert((b(listed(w0(), false, A(), 100, 3))) == 0); // l0v1.list.i33d
assert(((if listed(w0(), false, A(), 100, 3) { root_ver(w0(), false, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 3) { root_ver(w0(), false, 3) } else { 0int })) == 0); // l0v1.ver.i33d
assert(((if listed(w0(), false, A(), 100, 3) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 3) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v1.server.i33d
assert((b(warned(w0(), false, 3))) == 0) by (compute);assert((b(warned(w0(), false, 3))) == 0); // l0v1.warn.i33d
assert((b(listed(w0(), false, A(), 100, 4))) == 0) by (compute);assert((b(listed(w0(), false, A(), 100, 4))) == 0); // l0v1.list.i33e
assert(((if listed(w0(), false, A(), 100, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0); // l0v1.ver.i33e
assert(((if listed(w0(), false, A(), 100, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v1.server.i33e
assert((b(warned(w0(), false, 4))) == 0) by (compute);assert((b(warned(w0(), false, 4))) == 0); // l0v1.warn.i33e
assert((b(listed(w0(), false, A(), 100, 5))) == 0) by (compute);assert((b(listed(w0(), false, A(), 100, 5))) == 0); // l0v1.list.i33f
assert(((if listed(w0(), false, A(), 100, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0); // l0v1.ver.i33f
assert(((if listed(w0(), false, A(), 100, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v1.server.i33f
assert((b(warned(w0(), false, 5))) == 0) by (compute);assert((b(warned(w0(), false, 5))) == 0); // l0v1.warn.i33f
assert((b(listed(w0(), false, A(), 100, 6))) == 0) by (compute);assert((b(listed(w0(), false, A(), 100, 6))) == 0); // l0v1.list.i33g
assert(((if listed(w0(), false, A(), 100, 6) { root_ver(w0(), false, 6) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 6) { root_ver(w0(), false, 6) } else { 0int })) == 0); // l0v1.ver.i33g
assert(((if listed(w0(), false, A(), 100, 6) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 100, 6) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v1.server.i33g
assert((b(warned(w0(), false, 6))) == 0) by (compute);assert((b(warned(w0(), false, 6))) == 0); // l0v1.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l0v2.ok
assert((b(any_warned_from(w0(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), false, 0))) == 0); // l0v2.warning
assert((b(listed(w0(), false, A(), 200, 0))) == 0) by (compute);assert((b(listed(w0(), false, A(), 200, 0))) == 0); // l0v2.list.i33a
assert(((if listed(w0(), false, A(), 200, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 200, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0); // l0v2.ver.i33a
assert(((if listed(w0(), false, A(), 200, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 200, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v2.server.i33a
assert((b(warned(w0(), false, 0))) == 0) by (compute);assert((b(warned(w0(), false, 0))) == 0); // l0v2.warn.i33a
assert((b(listed(w0(), false, A(), 200, 1))) == 1) by (compute);assert((b(listed(w0(), false, A(), 200, 1))) == 1); // l0v2.list.i33b
assert(((if listed(w0(), false, A(), 200, 1) { root_ver(w0(), false, 1) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, A(), 200, 1) { root_ver(w0(), false, 1) } else { 0int })) == 100); // l0v2.ver.i33b
assert(((if listed(w0(), false, A(), 200, 1) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, A(), 200, 1) { if false { 2int } else { 1int } } else { 0int })) == 1); // l0v2.server.i33b
assert((b(warned(w0(), false, 1))) == 0) by (compute);assert((b(warned(w0(), false, 1))) == 0); // l0v2.warn.i33b
assert((b(listed(w0(), false, A(), 200, 2))) == 1) by (compute);assert((b(listed(w0(), false, A(), 200, 2))) == 1); // l0v2.list.i33c
assert(((if listed(w0(), false, A(), 200, 2) { root_ver(w0(), false, 2) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, A(), 200, 2) { root_ver(w0(), false, 2) } else { 0int })) == 100); // l0v2.ver.i33c
assert(((if listed(w0(), false, A(), 200, 2) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, A(), 200, 2) { if false { 2int } else { 1int } } else { 0int })) == 1); // l0v2.server.i33c
assert((b(warned(w0(), false, 2))) == 0) by (compute);assert((b(warned(w0(), false, 2))) == 0); // l0v2.warn.i33c
assert((b(listed(w0(), false, A(), 200, 3))) == 1) by (compute);assert((b(listed(w0(), false, A(), 200, 3))) == 1); // l0v2.list.i33d
assert(((if listed(w0(), false, A(), 200, 3) { root_ver(w0(), false, 3) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, A(), 200, 3) { root_ver(w0(), false, 3) } else { 0int })) == 100); // l0v2.ver.i33d
assert(((if listed(w0(), false, A(), 200, 3) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, A(), 200, 3) { if false { 2int } else { 1int } } else { 0int })) == 1); // l0v2.server.i33d
assert((b(warned(w0(), false, 3))) == 0) by (compute);assert((b(warned(w0(), false, 3))) == 0); // l0v2.warn.i33d
assert((b(listed(w0(), false, A(), 200, 4))) == 0) by (compute);assert((b(listed(w0(), false, A(), 200, 4))) == 0); // l0v2.list.i33e
assert(((if listed(w0(), false, A(), 200, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 200, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0); // l0v2.ver.i33e
assert(((if listed(w0(), false, A(), 200, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 200, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v2.server.i33e
assert((b(warned(w0(), false, 4))) == 0) by (compute);assert((b(warned(w0(), false, 4))) == 0); // l0v2.warn.i33e
assert((b(listed(w0(), false, A(), 200, 5))) == 0) by (compute);assert((b(listed(w0(), false, A(), 200, 5))) == 0); // l0v2.list.i33f
assert(((if listed(w0(), false, A(), 200, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 200, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0); // l0v2.ver.i33f
assert(((if listed(w0(), false, A(), 200, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, A(), 200, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // l0v2.server.i33f
assert((b(warned(w0(), false, 5))) == 0) by (compute);assert((b(warned(w0(), false, 5))) == 0); // l0v2.warn.i33f
assert((b(listed(w0(), false, A(), 200, 6))) == 1) by (compute);assert((b(listed(w0(), false, A(), 200, 6))) == 1); // l0v2.list.i33g
assert(((if listed(w0(), false, A(), 200, 6) { root_ver(w0(), false, 6) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, A(), 200, 6) { root_ver(w0(), false, 6) } else { 0int })) == 100); // l0v2.ver.i33g
assert(((if listed(w0(), false, A(), 200, 6) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, A(), 200, 6) { if false { 2int } else { 1int } } else { 0int })) == 1); // l0v2.server.i33g
assert((b(warned(w0(), false, 6))) == 0) by (compute);assert((b(warned(w0(), false, 6))) == 0); // l0v2.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // lb.ok
assert((b(any_warned_from(w0(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), false, 0))) == 0); // lb.warning
assert((b(listed(w0(), false, B(), 0, 0))) == 0) by (compute);assert((b(listed(w0(), false, B(), 0, 0))) == 0); // lb.list.i33a
assert(((if listed(w0(), false, B(), 0, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0); // lb.ver.i33a
assert(((if listed(w0(), false, B(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // lb.server.i33a
assert((b(warned(w0(), false, 0))) == 0) by (compute);assert((b(warned(w0(), false, 0))) == 0); // lb.warn.i33a
assert((b(listed(w0(), false, B(), 0, 1))) == 0) by (compute);assert((b(listed(w0(), false, B(), 0, 1))) == 0); // lb.list.i33b
assert(((if listed(w0(), false, B(), 0, 1) { root_ver(w0(), false, 1) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 1) { root_ver(w0(), false, 1) } else { 0int })) == 0); // lb.ver.i33b
assert(((if listed(w0(), false, B(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 0); // lb.server.i33b
assert((b(warned(w0(), false, 1))) == 0) by (compute);assert((b(warned(w0(), false, 1))) == 0); // lb.warn.i33b
assert((b(listed(w0(), false, B(), 0, 2))) == 1) by (compute);assert((b(listed(w0(), false, B(), 0, 2))) == 1); // lb.list.i33c
assert(((if listed(w0(), false, B(), 0, 2) { root_ver(w0(), false, 2) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, B(), 0, 2) { root_ver(w0(), false, 2) } else { 0int })) == 100); // lb.ver.i33c
assert(((if listed(w0(), false, B(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, B(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 1); // lb.server.i33c
assert((b(warned(w0(), false, 2))) == 0) by (compute);assert((b(warned(w0(), false, 2))) == 0); // lb.warn.i33c
assert((b(listed(w0(), false, B(), 0, 3))) == 0) by (compute);assert((b(listed(w0(), false, B(), 0, 3))) == 0); // lb.list.i33d
assert(((if listed(w0(), false, B(), 0, 3) { root_ver(w0(), false, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 3) { root_ver(w0(), false, 3) } else { 0int })) == 0); // lb.ver.i33d
assert(((if listed(w0(), false, B(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0); // lb.server.i33d
assert((b(warned(w0(), false, 3))) == 0) by (compute);assert((b(warned(w0(), false, 3))) == 0); // lb.warn.i33d
assert((b(listed(w0(), false, B(), 0, 4))) == 0) by (compute);assert((b(listed(w0(), false, B(), 0, 4))) == 0); // lb.list.i33e
assert(((if listed(w0(), false, B(), 0, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0); // lb.ver.i33e
assert(((if listed(w0(), false, B(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // lb.server.i33e
assert((b(warned(w0(), false, 4))) == 0) by (compute);assert((b(warned(w0(), false, 4))) == 0); // lb.warn.i33e
assert((b(listed(w0(), false, B(), 0, 5))) == 0) by (compute);assert((b(listed(w0(), false, B(), 0, 5))) == 0); // lb.list.i33f
assert(((if listed(w0(), false, B(), 0, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0); // lb.ver.i33f
assert(((if listed(w0(), false, B(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, B(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // lb.server.i33f
assert((b(warned(w0(), false, 5))) == 0) by (compute);assert((b(warned(w0(), false, 5))) == 0); // lb.warn.i33f
assert((b(listed(w0(), false, B(), 0, 6))) == 1) by (compute);assert((b(listed(w0(), false, B(), 0, 6))) == 1); // lb.list.i33g
assert(((if listed(w0(), false, B(), 0, 6) { root_ver(w0(), false, 6) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), false, B(), 0, 6) { root_ver(w0(), false, 6) } else { 0int })) == 100); // lb.ver.i33g
assert(((if listed(w0(), false, B(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w0(), false, B(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 1); // lb.server.i33g
assert((b(warned(w0(), false, 6))) == 0) by (compute);assert((b(warned(w0(), false, 6))) == 0); // lb.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // ld.ok
assert((b(any_warned_from(w0(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), false, 0))) == 0); // ld.warning
assert((b(listed(w0(), false, D(), 0, 0))) == 0) by (compute);assert((b(listed(w0(), false, D(), 0, 0))) == 0); // ld.list.i33a
assert(((if listed(w0(), false, D(), 0, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0); // ld.ver.i33a
assert(((if listed(w0(), false, D(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // ld.server.i33a
assert((b(warned(w0(), false, 0))) == 0) by (compute);assert((b(warned(w0(), false, 0))) == 0); // ld.warn.i33a
assert((b(listed(w0(), false, D(), 0, 1))) == 0) by (compute);assert((b(listed(w0(), false, D(), 0, 1))) == 0); // ld.list.i33b
assert(((if listed(w0(), false, D(), 0, 1) { root_ver(w0(), false, 1) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 1) { root_ver(w0(), false, 1) } else { 0int })) == 0); // ld.ver.i33b
assert(((if listed(w0(), false, D(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 0); // ld.server.i33b
assert((b(warned(w0(), false, 1))) == 0) by (compute);assert((b(warned(w0(), false, 1))) == 0); // ld.warn.i33b
assert((b(listed(w0(), false, D(), 0, 2))) == 0) by (compute);assert((b(listed(w0(), false, D(), 0, 2))) == 0); // ld.list.i33c
assert(((if listed(w0(), false, D(), 0, 2) { root_ver(w0(), false, 2) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 2) { root_ver(w0(), false, 2) } else { 0int })) == 0); // ld.ver.i33c
assert(((if listed(w0(), false, D(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 0); // ld.server.i33c
assert((b(warned(w0(), false, 2))) == 0) by (compute);assert((b(warned(w0(), false, 2))) == 0); // ld.warn.i33c
assert((b(listed(w0(), false, D(), 0, 3))) == 0) by (compute);assert((b(listed(w0(), false, D(), 0, 3))) == 0); // ld.list.i33d
assert(((if listed(w0(), false, D(), 0, 3) { root_ver(w0(), false, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 3) { root_ver(w0(), false, 3) } else { 0int })) == 0); // ld.ver.i33d
assert(((if listed(w0(), false, D(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0); // ld.server.i33d
assert((b(warned(w0(), false, 3))) == 0) by (compute);assert((b(warned(w0(), false, 3))) == 0); // ld.warn.i33d
assert((b(listed(w0(), false, D(), 0, 4))) == 0) by (compute);assert((b(listed(w0(), false, D(), 0, 4))) == 0); // ld.list.i33e
assert(((if listed(w0(), false, D(), 0, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0); // ld.ver.i33e
assert(((if listed(w0(), false, D(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // ld.server.i33e
assert((b(warned(w0(), false, 4))) == 0) by (compute);assert((b(warned(w0(), false, 4))) == 0); // ld.warn.i33e
assert((b(listed(w0(), false, D(), 0, 5))) == 0) by (compute);assert((b(listed(w0(), false, D(), 0, 5))) == 0); // ld.list.i33f
assert(((if listed(w0(), false, D(), 0, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0); // ld.ver.i33f
assert(((if listed(w0(), false, D(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // ld.server.i33f
assert((b(warned(w0(), false, 5))) == 0) by (compute);assert((b(warned(w0(), false, 5))) == 0); // ld.warn.i33f
assert((b(listed(w0(), false, D(), 0, 6))) == 0) by (compute);assert((b(listed(w0(), false, D(), 0, 6))) == 0); // ld.list.i33g
assert(((if listed(w0(), false, D(), 0, 6) { root_ver(w0(), false, 6) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 6) { root_ver(w0(), false, 6) } else { 0int })) == 0); // ld.ver.i33g
assert(((if listed(w0(), false, D(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, D(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 0); // ld.server.i33g
assert((b(warned(w0(), false, 6))) == 0) by (compute);assert((b(warned(w0(), false, 6))) == 0); // ld.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // le.ok
assert((b(any_warned_from(w0(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), false, 0))) == 0); // le.warning
assert((b(listed(w0(), false, E(), 0, 0))) == 0) by (compute);assert((b(listed(w0(), false, E(), 0, 0))) == 0); // le.list.i33a
assert(((if listed(w0(), false, E(), 0, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 0) { root_ver(w0(), false, 0) } else { 0int })) == 0); // le.ver.i33a
assert(((if listed(w0(), false, E(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // le.server.i33a
assert((b(warned(w0(), false, 0))) == 0) by (compute);assert((b(warned(w0(), false, 0))) == 0); // le.warn.i33a
assert((b(listed(w0(), false, E(), 0, 1))) == 0) by (compute);assert((b(listed(w0(), false, E(), 0, 1))) == 0); // le.list.i33b
assert(((if listed(w0(), false, E(), 0, 1) { root_ver(w0(), false, 1) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 1) { root_ver(w0(), false, 1) } else { 0int })) == 0); // le.ver.i33b
assert(((if listed(w0(), false, E(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 0); // le.server.i33b
assert((b(warned(w0(), false, 1))) == 0) by (compute);assert((b(warned(w0(), false, 1))) == 0); // le.warn.i33b
assert((b(listed(w0(), false, E(), 0, 2))) == 0) by (compute);assert((b(listed(w0(), false, E(), 0, 2))) == 0); // le.list.i33c
assert(((if listed(w0(), false, E(), 0, 2) { root_ver(w0(), false, 2) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 2) { root_ver(w0(), false, 2) } else { 0int })) == 0); // le.ver.i33c
assert(((if listed(w0(), false, E(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 0); // le.server.i33c
assert((b(warned(w0(), false, 2))) == 0) by (compute);assert((b(warned(w0(), false, 2))) == 0); // le.warn.i33c
assert((b(listed(w0(), false, E(), 0, 3))) == 0) by (compute);assert((b(listed(w0(), false, E(), 0, 3))) == 0); // le.list.i33d
assert(((if listed(w0(), false, E(), 0, 3) { root_ver(w0(), false, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 3) { root_ver(w0(), false, 3) } else { 0int })) == 0); // le.ver.i33d
assert(((if listed(w0(), false, E(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0); // le.server.i33d
assert((b(warned(w0(), false, 3))) == 0) by (compute);assert((b(warned(w0(), false, 3))) == 0); // le.warn.i33d
assert((b(listed(w0(), false, E(), 0, 4))) == 0) by (compute);assert((b(listed(w0(), false, E(), 0, 4))) == 0); // le.list.i33e
assert(((if listed(w0(), false, E(), 0, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 4) { root_ver(w0(), false, 4) } else { 0int })) == 0); // le.ver.i33e
assert(((if listed(w0(), false, E(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // le.server.i33e
assert((b(warned(w0(), false, 4))) == 0) by (compute);assert((b(warned(w0(), false, 4))) == 0); // le.warn.i33e
assert((b(listed(w0(), false, E(), 0, 5))) == 0) by (compute);assert((b(listed(w0(), false, E(), 0, 5))) == 0); // le.list.i33f
assert(((if listed(w0(), false, E(), 0, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 5) { root_ver(w0(), false, 5) } else { 0int })) == 0); // le.ver.i33f
assert(((if listed(w0(), false, E(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // le.server.i33f
assert((b(warned(w0(), false, 5))) == 0) by (compute);assert((b(warned(w0(), false, 5))) == 0); // le.warn.i33f
assert((b(listed(w0(), false, E(), 0, 6))) == 0) by (compute);assert((b(listed(w0(), false, E(), 0, 6))) == 0); // le.list.i33g
assert(((if listed(w0(), false, E(), 0, 6) { root_ver(w0(), false, 6) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 6) { root_ver(w0(), false, 6) } else { 0int })) == 0); // le.ver.i33g
assert(((if listed(w0(), false, E(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), false, E(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 0); // le.server.i33g
assert((b(warned(w0(), false, 6))) == 0) by (compute);assert((b(warned(w0(), false, 6))) == 0); // le.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // lr.ok
assert((b(any_warned_from(w0(), true, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), true, 0))) == 0); // lr.warning
assert((b(listed(w0(), true, A(), 0, 0))) == 0) by (compute);assert((b(listed(w0(), true, A(), 0, 0))) == 0); // lr.list.i33a
assert(((if listed(w0(), true, A(), 0, 0) { root_ver(w0(), true, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 0) { root_ver(w0(), true, 0) } else { 0int })) == 0); // lr.ver.i33a
assert(((if listed(w0(), true, A(), 0, 0) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 0) { if true { 2int } else { 1int } } else { 0int })) == 0); // lr.server.i33a
assert((b(warned(w0(), true, 0))) == 0) by (compute);assert((b(warned(w0(), true, 0))) == 0); // lr.warn.i33a
assert((b(listed(w0(), true, A(), 0, 1))) == 1) by (compute);assert((b(listed(w0(), true, A(), 0, 1))) == 1); // lr.list.i33b
assert(((if listed(w0(), true, A(), 0, 1) { root_ver(w0(), true, 1) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), true, A(), 0, 1) { root_ver(w0(), true, 1) } else { 0int })) == 100); // lr.ver.i33b
assert(((if listed(w0(), true, A(), 0, 1) { if true { 2int } else { 1int } } else { 0int })) == 2) by (compute);assert(((if listed(w0(), true, A(), 0, 1) { if true { 2int } else { 1int } } else { 0int })) == 2); // lr.server.i33b
assert((b(warned(w0(), true, 1))) == 0) by (compute);assert((b(warned(w0(), true, 1))) == 0); // lr.warn.i33b
assert((b(listed(w0(), true, A(), 0, 2))) == 0) by (compute);assert((b(listed(w0(), true, A(), 0, 2))) == 0); // lr.list.i33c
assert(((if listed(w0(), true, A(), 0, 2) { root_ver(w0(), true, 2) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 2) { root_ver(w0(), true, 2) } else { 0int })) == 0); // lr.ver.i33c
assert(((if listed(w0(), true, A(), 0, 2) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 2) { if true { 2int } else { 1int } } else { 0int })) == 0); // lr.server.i33c
assert((b(warned(w0(), true, 2))) == 0) by (compute);assert((b(warned(w0(), true, 2))) == 0); // lr.warn.i33c
assert((b(listed(w0(), true, A(), 0, 3))) == 0) by (compute);assert((b(listed(w0(), true, A(), 0, 3))) == 0); // lr.list.i33d
assert(((if listed(w0(), true, A(), 0, 3) { root_ver(w0(), true, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 3) { root_ver(w0(), true, 3) } else { 0int })) == 0); // lr.ver.i33d
assert(((if listed(w0(), true, A(), 0, 3) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 3) { if true { 2int } else { 1int } } else { 0int })) == 0); // lr.server.i33d
assert((b(warned(w0(), true, 3))) == 0) by (compute);assert((b(warned(w0(), true, 3))) == 0); // lr.warn.i33d
assert((b(listed(w0(), true, A(), 0, 4))) == 0) by (compute);assert((b(listed(w0(), true, A(), 0, 4))) == 0); // lr.list.i33e
assert(((if listed(w0(), true, A(), 0, 4) { root_ver(w0(), true, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 4) { root_ver(w0(), true, 4) } else { 0int })) == 0); // lr.ver.i33e
assert(((if listed(w0(), true, A(), 0, 4) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 4) { if true { 2int } else { 1int } } else { 0int })) == 0); // lr.server.i33e
assert((b(warned(w0(), true, 4))) == 0) by (compute);assert((b(warned(w0(), true, 4))) == 0); // lr.warn.i33e
assert((b(listed(w0(), true, A(), 0, 5))) == 1) by (compute);assert((b(listed(w0(), true, A(), 0, 5))) == 1); // lr.list.i33f
assert(((if listed(w0(), true, A(), 0, 5) { root_ver(w0(), true, 5) } else { 0int })) == 100) by (compute);assert(((if listed(w0(), true, A(), 0, 5) { root_ver(w0(), true, 5) } else { 0int })) == 100); // lr.ver.i33f
assert(((if listed(w0(), true, A(), 0, 5) { if true { 2int } else { 1int } } else { 0int })) == 2) by (compute);assert(((if listed(w0(), true, A(), 0, 5) { if true { 2int } else { 1int } } else { 0int })) == 2); // lr.server.i33f
assert((b(warned(w0(), true, 5))) == 0) by (compute);assert((b(warned(w0(), true, 5))) == 0); // lr.warn.i33f
assert((b(listed(w0(), true, A(), 0, 6))) == 0) by (compute);assert((b(listed(w0(), true, A(), 0, 6))) == 0); // lr.list.i33g
assert(((if listed(w0(), true, A(), 0, 6) { root_ver(w0(), true, 6) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 6) { root_ver(w0(), true, 6) } else { 0int })) == 0); // lr.ver.i33g
assert(((if listed(w0(), true, A(), 0, 6) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 0, 6) { if true { 2int } else { 1int } } else { 0int })) == 0); // lr.server.i33g
assert((b(warned(w0(), true, 6))) == 0) by (compute);assert((b(warned(w0(), true, 6))) == 0); // lr.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // lrv2.ok
assert((b(any_warned_from(w0(), true, 0))) == 0) by (compute);assert((b(any_warned_from(w0(), true, 0))) == 0); // lrv2.warning
assert((b(listed(w0(), true, A(), 200, 0))) == 0) by (compute);assert((b(listed(w0(), true, A(), 200, 0))) == 0); // lrv2.list.i33a
assert(((if listed(w0(), true, A(), 200, 0) { root_ver(w0(), true, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 0) { root_ver(w0(), true, 0) } else { 0int })) == 0); // lrv2.ver.i33a
assert(((if listed(w0(), true, A(), 200, 0) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 0) { if true { 2int } else { 1int } } else { 0int })) == 0); // lrv2.server.i33a
assert((b(warned(w0(), true, 0))) == 0) by (compute);assert((b(warned(w0(), true, 0))) == 0); // lrv2.warn.i33a
assert((b(listed(w0(), true, A(), 200, 1))) == 0) by (compute);assert((b(listed(w0(), true, A(), 200, 1))) == 0); // lrv2.list.i33b
assert(((if listed(w0(), true, A(), 200, 1) { root_ver(w0(), true, 1) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 1) { root_ver(w0(), true, 1) } else { 0int })) == 0); // lrv2.ver.i33b
assert(((if listed(w0(), true, A(), 200, 1) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 1) { if true { 2int } else { 1int } } else { 0int })) == 0); // lrv2.server.i33b
assert((b(warned(w0(), true, 1))) == 0) by (compute);assert((b(warned(w0(), true, 1))) == 0); // lrv2.warn.i33b
assert((b(listed(w0(), true, A(), 200, 2))) == 0) by (compute);assert((b(listed(w0(), true, A(), 200, 2))) == 0); // lrv2.list.i33c
assert(((if listed(w0(), true, A(), 200, 2) { root_ver(w0(), true, 2) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 2) { root_ver(w0(), true, 2) } else { 0int })) == 0); // lrv2.ver.i33c
assert(((if listed(w0(), true, A(), 200, 2) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 2) { if true { 2int } else { 1int } } else { 0int })) == 0); // lrv2.server.i33c
assert((b(warned(w0(), true, 2))) == 0) by (compute);assert((b(warned(w0(), true, 2))) == 0); // lrv2.warn.i33c
assert((b(listed(w0(), true, A(), 200, 3))) == 0) by (compute);assert((b(listed(w0(), true, A(), 200, 3))) == 0); // lrv2.list.i33d
assert(((if listed(w0(), true, A(), 200, 3) { root_ver(w0(), true, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 3) { root_ver(w0(), true, 3) } else { 0int })) == 0); // lrv2.ver.i33d
assert(((if listed(w0(), true, A(), 200, 3) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 3) { if true { 2int } else { 1int } } else { 0int })) == 0); // lrv2.server.i33d
assert((b(warned(w0(), true, 3))) == 0) by (compute);assert((b(warned(w0(), true, 3))) == 0); // lrv2.warn.i33d
assert((b(listed(w0(), true, A(), 200, 4))) == 0) by (compute);assert((b(listed(w0(), true, A(), 200, 4))) == 0); // lrv2.list.i33e
assert(((if listed(w0(), true, A(), 200, 4) { root_ver(w0(), true, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 4) { root_ver(w0(), true, 4) } else { 0int })) == 0); // lrv2.ver.i33e
assert(((if listed(w0(), true, A(), 200, 4) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 4) { if true { 2int } else { 1int } } else { 0int })) == 0); // lrv2.server.i33e
assert((b(warned(w0(), true, 4))) == 0) by (compute);assert((b(warned(w0(), true, 4))) == 0); // lrv2.warn.i33e
assert((b(listed(w0(), true, A(), 200, 5))) == 0) by (compute);assert((b(listed(w0(), true, A(), 200, 5))) == 0); // lrv2.list.i33f
assert(((if listed(w0(), true, A(), 200, 5) { root_ver(w0(), true, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 5) { root_ver(w0(), true, 5) } else { 0int })) == 0); // lrv2.ver.i33f
assert(((if listed(w0(), true, A(), 200, 5) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 5) { if true { 2int } else { 1int } } else { 0int })) == 0); // lrv2.server.i33f
assert((b(warned(w0(), true, 5))) == 0) by (compute);assert((b(warned(w0(), true, 5))) == 0); // lrv2.warn.i33f
assert((b(listed(w0(), true, A(), 200, 6))) == 0) by (compute);assert((b(listed(w0(), true, A(), 200, 6))) == 0); // lrv2.list.i33g
assert(((if listed(w0(), true, A(), 200, 6) { root_ver(w0(), true, 6) } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 6) { root_ver(w0(), true, 6) } else { 0int })) == 0); // lrv2.ver.i33g
assert(((if listed(w0(), true, A(), 200, 6) { if true { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w0(), true, A(), 200, 6) { if true { 2int } else { 1int } } else { 0int })) == 0); // lrv2.server.i33g
assert((b(warned(w0(), true, 6))) == 0) by (compute);assert((b(warned(w0(), true, 6))) == 0); // lrv2.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l2.ok
assert((b(any_warned_from(w2(), false, 0))) == 1) by (compute);assert((b(any_warned_from(w2(), false, 0))) == 1); // l2.warning
assert((b(listed(w2(), false, A(), 0, 0))) == 0) by (compute);assert((b(listed(w2(), false, A(), 0, 0))) == 0); // l2.list.i33a
assert(((if listed(w2(), false, A(), 0, 0) { root_ver(w2(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 0) { root_ver(w2(), false, 0) } else { 0int })) == 0); // l2.ver.i33a
assert(((if listed(w2(), false, A(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2.server.i33a
assert((b(warned(w2(), false, 0))) == 0) by (compute);assert((b(warned(w2(), false, 0))) == 0); // l2.warn.i33a
assert((b(listed(w2(), false, A(), 0, 1))) == 0) by (compute);assert((b(listed(w2(), false, A(), 0, 1))) == 0); // l2.list.i33b
assert(((if listed(w2(), false, A(), 0, 1) { root_ver(w2(), false, 1) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 1) { root_ver(w2(), false, 1) } else { 0int })) == 0); // l2.ver.i33b
assert(((if listed(w2(), false, A(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2.server.i33b
assert((b(listed(w2(), false, A(), 0, 2))) == 0) by (compute);assert((b(listed(w2(), false, A(), 0, 2))) == 0); // l2.list.i33c
assert(((if listed(w2(), false, A(), 0, 2) { root_ver(w2(), false, 2) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 2) { root_ver(w2(), false, 2) } else { 0int })) == 0); // l2.ver.i33c
assert(((if listed(w2(), false, A(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2.server.i33c
assert((b(warned(w2(), false, 2))) == 1) by (compute);assert((b(warned(w2(), false, 2))) == 1); // l2.warn.i33c
assert((b(listed(w2(), false, A(), 0, 3))) == 0) by (compute);assert((b(listed(w2(), false, A(), 0, 3))) == 0); // l2.list.i33d
assert(((if listed(w2(), false, A(), 0, 3) { root_ver(w2(), false, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 3) { root_ver(w2(), false, 3) } else { 0int })) == 0); // l2.ver.i33d
assert(((if listed(w2(), false, A(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2.server.i33d
assert((b(warned(w2(), false, 3))) == 1) by (compute);assert((b(warned(w2(), false, 3))) == 1); // l2.warn.i33d
assert((b(listed(w2(), false, A(), 0, 4))) == 0) by (compute);assert((b(listed(w2(), false, A(), 0, 4))) == 0); // l2.list.i33e
assert(((if listed(w2(), false, A(), 0, 4) { root_ver(w2(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 4) { root_ver(w2(), false, 4) } else { 0int })) == 0); // l2.ver.i33e
assert(((if listed(w2(), false, A(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2.server.i33e
assert((b(warned(w2(), false, 4))) == 0) by (compute);assert((b(warned(w2(), false, 4))) == 0); // l2.warn.i33e
assert((b(listed(w2(), false, A(), 0, 5))) == 0) by (compute);assert((b(listed(w2(), false, A(), 0, 5))) == 0); // l2.list.i33f
assert(((if listed(w2(), false, A(), 0, 5) { root_ver(w2(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 5) { root_ver(w2(), false, 5) } else { 0int })) == 0); // l2.ver.i33f
assert(((if listed(w2(), false, A(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2.server.i33f
assert((b(warned(w2(), false, 5))) == 0) by (compute);assert((b(warned(w2(), false, 5))) == 0); // l2.warn.i33f
assert((b(listed(w2(), false, A(), 0, 6))) == 0) by (compute);assert((b(listed(w2(), false, A(), 0, 6))) == 0); // l2.list.i33g
assert(((if listed(w2(), false, A(), 0, 6) { root_ver(w2(), false, 6) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 6) { root_ver(w2(), false, 6) } else { 0int })) == 0); // l2.ver.i33g
assert(((if listed(w2(), false, A(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2.server.i33g
assert((b(warned(w2(), false, 6))) == 1) by (compute);assert((b(warned(w2(), false, 6))) == 1); // l2.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l2v1.ok
assert((b(any_warned_from(w2(), false, 0))) == 1) by (compute);assert((b(any_warned_from(w2(), false, 0))) == 1); // l2v1.warning
assert((b(listed(w2(), false, A(), 100, 0))) == 0) by (compute);assert((b(listed(w2(), false, A(), 100, 0))) == 0); // l2v1.list.i33a
assert(((if listed(w2(), false, A(), 100, 0) { root_ver(w2(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 0) { root_ver(w2(), false, 0) } else { 0int })) == 0); // l2v1.ver.i33a
assert(((if listed(w2(), false, A(), 100, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2v1.server.i33a
assert((b(warned(w2(), false, 0))) == 0) by (compute);assert((b(warned(w2(), false, 0))) == 0); // l2v1.warn.i33a
assert((b(listed(w2(), false, A(), 100, 1))) == 0) by (compute);assert((b(listed(w2(), false, A(), 100, 1))) == 0); // l2v1.list.i33b
assert(((if listed(w2(), false, A(), 100, 1) { root_ver(w2(), false, 1) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 1) { root_ver(w2(), false, 1) } else { 0int })) == 0); // l2v1.ver.i33b
assert(((if listed(w2(), false, A(), 100, 1) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 1) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2v1.server.i33b
assert((b(warned(w2(), false, 1))) == 1) by (compute);assert((b(warned(w2(), false, 1))) == 1); // l2v1.warn.i33b
assert((b(listed(w2(), false, A(), 100, 2))) == 0) by (compute);assert((b(listed(w2(), false, A(), 100, 2))) == 0); // l2v1.list.i33c
assert(((if listed(w2(), false, A(), 100, 2) { root_ver(w2(), false, 2) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 2) { root_ver(w2(), false, 2) } else { 0int })) == 0); // l2v1.ver.i33c
assert(((if listed(w2(), false, A(), 100, 2) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 2) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2v1.server.i33c
assert((b(warned(w2(), false, 2))) == 1) by (compute);assert((b(warned(w2(), false, 2))) == 1); // l2v1.warn.i33c
assert((b(listed(w2(), false, A(), 100, 3))) == 0) by (compute);assert((b(listed(w2(), false, A(), 100, 3))) == 0); // l2v1.list.i33d
assert(((if listed(w2(), false, A(), 100, 3) { root_ver(w2(), false, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 3) { root_ver(w2(), false, 3) } else { 0int })) == 0); // l2v1.ver.i33d
assert(((if listed(w2(), false, A(), 100, 3) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 3) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2v1.server.i33d
assert((b(warned(w2(), false, 3))) == 1) by (compute);assert((b(warned(w2(), false, 3))) == 1); // l2v1.warn.i33d
assert((b(listed(w2(), false, A(), 100, 4))) == 0) by (compute);assert((b(listed(w2(), false, A(), 100, 4))) == 0); // l2v1.list.i33e
assert(((if listed(w2(), false, A(), 100, 4) { root_ver(w2(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 4) { root_ver(w2(), false, 4) } else { 0int })) == 0); // l2v1.ver.i33e
assert(((if listed(w2(), false, A(), 100, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2v1.server.i33e
assert((b(warned(w2(), false, 4))) == 0) by (compute);assert((b(warned(w2(), false, 4))) == 0); // l2v1.warn.i33e
assert((b(listed(w2(), false, A(), 100, 5))) == 0) by (compute);assert((b(listed(w2(), false, A(), 100, 5))) == 0); // l2v1.list.i33f
assert(((if listed(w2(), false, A(), 100, 5) { root_ver(w2(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 5) { root_ver(w2(), false, 5) } else { 0int })) == 0); // l2v1.ver.i33f
assert(((if listed(w2(), false, A(), 100, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2v1.server.i33f
assert((b(warned(w2(), false, 5))) == 0) by (compute);assert((b(warned(w2(), false, 5))) == 0); // l2v1.warn.i33f
assert((b(listed(w2(), false, A(), 100, 6))) == 0) by (compute);assert((b(listed(w2(), false, A(), 100, 6))) == 0); // l2v1.list.i33g
assert(((if listed(w2(), false, A(), 100, 6) { root_ver(w2(), false, 6) } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 6) { root_ver(w2(), false, 6) } else { 0int })) == 0); // l2v1.ver.i33g
assert(((if listed(w2(), false, A(), 100, 6) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w2(), false, A(), 100, 6) { if false { 2int } else { 1int } } else { 0int })) == 0); // l2v1.server.i33g
assert((b(warned(w2(), false, 6))) == 1) by (compute);assert((b(warned(w2(), false, 6))) == 1); // l2v1.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l3.ok
assert((b(any_warned_from(w3(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w3(), false, 0))) == 0); // l3.warning
assert((b(listed(w3(), false, A(), 0, 0))) == 0) by (compute);assert((b(listed(w3(), false, A(), 0, 0))) == 0); // l3.list.i33a
assert(((if listed(w3(), false, A(), 0, 0) { root_ver(w3(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 0, 0) { root_ver(w3(), false, 0) } else { 0int })) == 0); // l3.ver.i33a
assert(((if listed(w3(), false, A(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 0, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // l3.server.i33a
assert((b(warned(w3(), false, 0))) == 0) by (compute);assert((b(warned(w3(), false, 0))) == 0); // l3.warn.i33a
assert((b(listed(w3(), false, A(), 0, 1))) == 1) by (compute);assert((b(listed(w3(), false, A(), 0, 1))) == 1); // l3.list.i33b
assert(((if listed(w3(), false, A(), 0, 1) { root_ver(w3(), false, 1) } else { 0int })) == 100) by (compute);assert(((if listed(w3(), false, A(), 0, 1) { root_ver(w3(), false, 1) } else { 0int })) == 100); // l3.ver.i33b
assert(((if listed(w3(), false, A(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w3(), false, A(), 0, 1) { if false { 2int } else { 1int } } else { 0int })) == 1); // l3.server.i33b
assert((b(warned(w3(), false, 1))) == 0) by (compute);assert((b(warned(w3(), false, 1))) == 0); // l3.warn.i33b
assert((b(listed(w3(), false, A(), 0, 2))) == 1) by (compute);assert((b(listed(w3(), false, A(), 0, 2))) == 1); // l3.list.i33c
assert(((if listed(w3(), false, A(), 0, 2) { root_ver(w3(), false, 2) } else { 0int })) == 100) by (compute);assert(((if listed(w3(), false, A(), 0, 2) { root_ver(w3(), false, 2) } else { 0int })) == 100); // l3.ver.i33c
assert(((if listed(w3(), false, A(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w3(), false, A(), 0, 2) { if false { 2int } else { 1int } } else { 0int })) == 1); // l3.server.i33c
assert((b(warned(w3(), false, 2))) == 0) by (compute);assert((b(warned(w3(), false, 2))) == 0); // l3.warn.i33c
assert((b(listed(w3(), false, A(), 0, 3))) == 0) by (compute);assert((b(listed(w3(), false, A(), 0, 3))) == 0); // l3.list.i33d
assert(((if listed(w3(), false, A(), 0, 3) { root_ver(w3(), false, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 0, 3) { root_ver(w3(), false, 3) } else { 0int })) == 0); // l3.ver.i33d
assert(((if listed(w3(), false, A(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 0, 3) { if false { 2int } else { 1int } } else { 0int })) == 0); // l3.server.i33d
assert((b(warned(w3(), false, 3))) == 0) by (compute);assert((b(warned(w3(), false, 3))) == 0); // l3.warn.i33d
assert((b(listed(w3(), false, A(), 0, 4))) == 0) by (compute);assert((b(listed(w3(), false, A(), 0, 4))) == 0); // l3.list.i33e
assert(((if listed(w3(), false, A(), 0, 4) { root_ver(w3(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 0, 4) { root_ver(w3(), false, 4) } else { 0int })) == 0); // l3.ver.i33e
assert(((if listed(w3(), false, A(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 0, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // l3.server.i33e
assert((b(warned(w3(), false, 4))) == 0) by (compute);assert((b(warned(w3(), false, 4))) == 0); // l3.warn.i33e
assert((b(listed(w3(), false, A(), 0, 5))) == 0) by (compute);assert((b(listed(w3(), false, A(), 0, 5))) == 0); // l3.list.i33f
assert(((if listed(w3(), false, A(), 0, 5) { root_ver(w3(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 0, 5) { root_ver(w3(), false, 5) } else { 0int })) == 0); // l3.ver.i33f
assert(((if listed(w3(), false, A(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 0, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // l3.server.i33f
assert((b(warned(w3(), false, 5))) == 0) by (compute);assert((b(warned(w3(), false, 5))) == 0); // l3.warn.i33f
assert((b(listed(w3(), false, A(), 0, 6))) == 1) by (compute);assert((b(listed(w3(), false, A(), 0, 6))) == 1); // l3.list.i33g
assert(((if listed(w3(), false, A(), 0, 6) { root_ver(w3(), false, 6) } else { 0int })) == 100) by (compute);assert(((if listed(w3(), false, A(), 0, 6) { root_ver(w3(), false, 6) } else { 0int })) == 100); // l3.ver.i33g
assert(((if listed(w3(), false, A(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w3(), false, A(), 0, 6) { if false { 2int } else { 1int } } else { 0int })) == 1); // l3.server.i33g
assert((b(warned(w3(), false, 6))) == 0) by (compute);assert((b(warned(w3(), false, 6))) == 0); // l3.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l3v1.ok
assert((b(any_warned_from(w3(), false, 0))) == 0) by (compute);assert((b(any_warned_from(w3(), false, 0))) == 0); // l3v1.warning
assert((b(listed(w3(), false, A(), 100, 0))) == 0) by (compute);assert((b(listed(w3(), false, A(), 100, 0))) == 0); // l3v1.list.i33a
assert(((if listed(w3(), false, A(), 100, 0) { root_ver(w3(), false, 0) } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 100, 0) { root_ver(w3(), false, 0) } else { 0int })) == 0); // l3v1.ver.i33a
assert(((if listed(w3(), false, A(), 100, 0) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 100, 0) { if false { 2int } else { 1int } } else { 0int })) == 0); // l3v1.server.i33a
assert((b(warned(w3(), false, 0))) == 0) by (compute);assert((b(warned(w3(), false, 0))) == 0); // l3v1.warn.i33a
assert((b(listed(w3(), false, A(), 100, 1))) == 1) by (compute);assert((b(listed(w3(), false, A(), 100, 1))) == 1); // l3v1.list.i33b
assert(((if listed(w3(), false, A(), 100, 1) { root_ver(w3(), false, 1) } else { 0int })) == 100) by (compute);assert(((if listed(w3(), false, A(), 100, 1) { root_ver(w3(), false, 1) } else { 0int })) == 100); // l3v1.ver.i33b
assert(((if listed(w3(), false, A(), 100, 1) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w3(), false, A(), 100, 1) { if false { 2int } else { 1int } } else { 0int })) == 1); // l3v1.server.i33b
assert((b(warned(w3(), false, 1))) == 0) by (compute);assert((b(warned(w3(), false, 1))) == 0); // l3v1.warn.i33b
assert((b(listed(w3(), false, A(), 100, 2))) == 1) by (compute);assert((b(listed(w3(), false, A(), 100, 2))) == 1); // l3v1.list.i33c
assert(((if listed(w3(), false, A(), 100, 2) { root_ver(w3(), false, 2) } else { 0int })) == 100) by (compute);assert(((if listed(w3(), false, A(), 100, 2) { root_ver(w3(), false, 2) } else { 0int })) == 100); // l3v1.ver.i33c
assert(((if listed(w3(), false, A(), 100, 2) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w3(), false, A(), 100, 2) { if false { 2int } else { 1int } } else { 0int })) == 1); // l3v1.server.i33c
assert((b(warned(w3(), false, 2))) == 0) by (compute);assert((b(warned(w3(), false, 2))) == 0); // l3v1.warn.i33c
assert((b(listed(w3(), false, A(), 100, 3))) == 0) by (compute);assert((b(listed(w3(), false, A(), 100, 3))) == 0); // l3v1.list.i33d
assert(((if listed(w3(), false, A(), 100, 3) { root_ver(w3(), false, 3) } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 100, 3) { root_ver(w3(), false, 3) } else { 0int })) == 0); // l3v1.ver.i33d
assert(((if listed(w3(), false, A(), 100, 3) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 100, 3) { if false { 2int } else { 1int } } else { 0int })) == 0); // l3v1.server.i33d
assert((b(warned(w3(), false, 3))) == 0) by (compute);assert((b(warned(w3(), false, 3))) == 0); // l3v1.warn.i33d
assert((b(listed(w3(), false, A(), 100, 4))) == 0) by (compute);assert((b(listed(w3(), false, A(), 100, 4))) == 0); // l3v1.list.i33e
assert(((if listed(w3(), false, A(), 100, 4) { root_ver(w3(), false, 4) } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 100, 4) { root_ver(w3(), false, 4) } else { 0int })) == 0); // l3v1.ver.i33e
assert(((if listed(w3(), false, A(), 100, 4) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 100, 4) { if false { 2int } else { 1int } } else { 0int })) == 0); // l3v1.server.i33e
assert((b(warned(w3(), false, 4))) == 0) by (compute);assert((b(warned(w3(), false, 4))) == 0); // l3v1.warn.i33e
assert((b(listed(w3(), false, A(), 100, 5))) == 0) by (compute);assert((b(listed(w3(), false, A(), 100, 5))) == 0); // l3v1.list.i33f
assert(((if listed(w3(), false, A(), 100, 5) { root_ver(w3(), false, 5) } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 100, 5) { root_ver(w3(), false, 5) } else { 0int })) == 0); // l3v1.ver.i33f
assert(((if listed(w3(), false, A(), 100, 5) { if false { 2int } else { 1int } } else { 0int })) == 0) by (compute);assert(((if listed(w3(), false, A(), 100, 5) { if false { 2int } else { 1int } } else { 0int })) == 0); // l3v1.server.i33f
assert((b(warned(w3(), false, 5))) == 0) by (compute);assert((b(warned(w3(), false, 5))) == 0); // l3v1.warn.i33f
assert((b(listed(w3(), false, A(), 100, 6))) == 1) by (compute);assert((b(listed(w3(), false, A(), 100, 6))) == 1); // l3v1.list.i33g
assert(((if listed(w3(), false, A(), 100, 6) { root_ver(w3(), false, 6) } else { 0int })) == 100) by (compute);assert(((if listed(w3(), false, A(), 100, 6) { root_ver(w3(), false, 6) } else { 0int })) == 100); // l3v1.ver.i33g
assert(((if listed(w3(), false, A(), 100, 6) { if false { 2int } else { 1int } } else { 0int })) == 1) by (compute);assert(((if listed(w3(), false, A(), 100, 6) { if false { 2int } else { 1int } } else { 0int })) == 1); // l3v1.server.i33g
assert((b(warned(w3(), false, 6))) == 0) by (compute);assert((b(warned(w3(), false, 6))) == 0); // l3v1.warn.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // l0t.ok
assert((tree_count_from(w0(), A(), 0)) == 3) by (compute);assert((tree_count_from(w0(), A(), 0)) == 3); // l0t.nodes
assert((b(tree_has(w0(), A(), 0))) == 1) by (compute);assert((b(tree_has(w0(), A(), 0))) == 1); // l0t.count.i33a
assert(((if tree_has(w0(), A(), 0) { col(tree_depth(w0(), A(), 0)) } else { 0int })) == 1) by (compute);assert(((if tree_has(w0(), A(), 0) { col(tree_depth(w0(), A(), 0)) } else { 0int })) == 1); // l0t.col.i33a
assert(((if tree_has(w0(), A(), 0) { tree_depth(w0(), A(), 0) + 1 } else { 0int })) == 1) by (compute);assert(((if tree_has(w0(), A(), 0) { tree_depth(w0(), A(), 0) + 1 } else { 0int })) == 1); // l0t.line.i33a
assert(((if tree_has(w0(), A(), 0) { tree_ver(w0(), A(), 0) } else { 0int })) == 200) by (compute);assert(((if tree_has(w0(), A(), 0) { tree_ver(w0(), A(), 0) } else { 0int })) == 200); // l0t.ver.i33a
assert((b(tree_has(w0(), A(), 1))) == 1) by (compute);assert((b(tree_has(w0(), A(), 1))) == 1); // l0t.count.i33b
assert(((if tree_has(w0(), A(), 1) { col(tree_depth(w0(), A(), 1)) } else { 0int })) == 4) by (compute);assert(((if tree_has(w0(), A(), 1) { col(tree_depth(w0(), A(), 1)) } else { 0int })) == 4); // l0t.col.i33b
assert(((if tree_has(w0(), A(), 1) { tree_depth(w0(), A(), 1) + 1 } else { 0int })) == 2) by (compute);assert(((if tree_has(w0(), A(), 1) { tree_depth(w0(), A(), 1) + 1 } else { 0int })) == 2); // l0t.line.i33b
assert(((if tree_has(w0(), A(), 1) { tree_ver(w0(), A(), 1) } else { 0int })) == 100) by (compute);assert(((if tree_has(w0(), A(), 1) { tree_ver(w0(), A(), 1) } else { 0int })) == 100); // l0t.ver.i33b
assert((b(tree_has(w0(), A(), 2))) == 1) by (compute);assert((b(tree_has(w0(), A(), 2))) == 1); // l0t.count.i33c
assert(((if tree_has(w0(), A(), 2) { col(tree_depth(w0(), A(), 2)) } else { 0int })) == 6) by (compute);assert(((if tree_has(w0(), A(), 2) { col(tree_depth(w0(), A(), 2)) } else { 0int })) == 6); // l0t.col.i33c
assert(((if tree_has(w0(), A(), 2) { tree_depth(w0(), A(), 2) + 1 } else { 0int })) == 3) by (compute);assert(((if tree_has(w0(), A(), 2) { tree_depth(w0(), A(), 2) + 1 } else { 0int })) == 3); // l0t.line.i33c
assert(((if tree_has(w0(), A(), 2) { tree_ver(w0(), A(), 2) } else { 0int })) == 100) by (compute);assert(((if tree_has(w0(), A(), 2) { tree_ver(w0(), A(), 2) } else { 0int })) == 100); // l0t.ver.i33c
assert(((if tree_has(w0(), A(), 3) { col(tree_depth(w0(), A(), 3)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 3) { col(tree_depth(w0(), A(), 3)) } else { 0int })) == 0); // l0t.col.i33d
assert(((if tree_has(w0(), A(), 3) { tree_depth(w0(), A(), 3) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 3) { tree_depth(w0(), A(), 3) + 1 } else { 0int })) == 0); // l0t.line.i33d
assert(((if tree_has(w0(), A(), 3) { tree_ver(w0(), A(), 3) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 3) { tree_ver(w0(), A(), 3) } else { 0int })) == 0); // l0t.ver.i33d
assert((b(tree_has(w0(), A(), 4))) == 0) by (compute);assert((b(tree_has(w0(), A(), 4))) == 0); // l0t.count.i33e
assert(((if tree_has(w0(), A(), 4) { col(tree_depth(w0(), A(), 4)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 4) { col(tree_depth(w0(), A(), 4)) } else { 0int })) == 0); // l0t.col.i33e
assert(((if tree_has(w0(), A(), 4) { tree_depth(w0(), A(), 4) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 4) { tree_depth(w0(), A(), 4) + 1 } else { 0int })) == 0); // l0t.line.i33e
assert(((if tree_has(w0(), A(), 4) { tree_ver(w0(), A(), 4) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 4) { tree_ver(w0(), A(), 4) } else { 0int })) == 0); // l0t.ver.i33e
assert((b(tree_has(w0(), A(), 5))) == 0) by (compute);assert((b(tree_has(w0(), A(), 5))) == 0); // l0t.count.i33f
assert(((if tree_has(w0(), A(), 5) { col(tree_depth(w0(), A(), 5)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 5) { col(tree_depth(w0(), A(), 5)) } else { 0int })) == 0); // l0t.col.i33f
assert(((if tree_has(w0(), A(), 5) { tree_depth(w0(), A(), 5) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 5) { tree_depth(w0(), A(), 5) + 1 } else { 0int })) == 0); // l0t.line.i33f
assert(((if tree_has(w0(), A(), 5) { tree_ver(w0(), A(), 5) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 5) { tree_ver(w0(), A(), 5) } else { 0int })) == 0); // l0t.ver.i33f
assert((b(tree_has(w0(), A(), 6))) == 0) by (compute);assert((b(tree_has(w0(), A(), 6))) == 0); // l0t.count.i33g
assert(((if tree_has(w0(), A(), 6) { col(tree_depth(w0(), A(), 6)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 6) { col(tree_depth(w0(), A(), 6)) } else { 0int })) == 0); // l0t.col.i33g
assert(((if tree_has(w0(), A(), 6) { tree_depth(w0(), A(), 6) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 6) { tree_depth(w0(), A(), 6) + 1 } else { 0int })) == 0); // l0t.line.i33g
assert(((if tree_has(w0(), A(), 6) { tree_ver(w0(), A(), 6) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), A(), 6) { tree_ver(w0(), A(), 6) } else { 0int })) == 0); // l0t.ver.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // lbt.ok
assert((tree_count_from(w0(), B(), 0)) == 2) by (compute);assert((tree_count_from(w0(), B(), 0)) == 2); // lbt.nodes
assert((b(tree_has(w0(), B(), 0))) == 0) by (compute);assert((b(tree_has(w0(), B(), 0))) == 0); // lbt.count.i33a
assert(((if tree_has(w0(), B(), 0) { col(tree_depth(w0(), B(), 0)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 0) { col(tree_depth(w0(), B(), 0)) } else { 0int })) == 0); // lbt.col.i33a
assert(((if tree_has(w0(), B(), 0) { tree_depth(w0(), B(), 0) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 0) { tree_depth(w0(), B(), 0) + 1 } else { 0int })) == 0); // lbt.line.i33a
assert(((if tree_has(w0(), B(), 0) { tree_ver(w0(), B(), 0) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 0) { tree_ver(w0(), B(), 0) } else { 0int })) == 0); // lbt.ver.i33a
assert((b(tree_has(w0(), B(), 1))) == 1) by (compute);assert((b(tree_has(w0(), B(), 1))) == 1); // lbt.count.i33b
assert(((if tree_has(w0(), B(), 1) { col(tree_depth(w0(), B(), 1)) } else { 0int })) == 1) by (compute);assert(((if tree_has(w0(), B(), 1) { col(tree_depth(w0(), B(), 1)) } else { 0int })) == 1); // lbt.col.i33b
assert(((if tree_has(w0(), B(), 1) { tree_depth(w0(), B(), 1) + 1 } else { 0int })) == 1) by (compute);assert(((if tree_has(w0(), B(), 1) { tree_depth(w0(), B(), 1) + 1 } else { 0int })) == 1); // lbt.line.i33b
assert(((if tree_has(w0(), B(), 1) { tree_ver(w0(), B(), 1) } else { 0int })) == 100) by (compute);assert(((if tree_has(w0(), B(), 1) { tree_ver(w0(), B(), 1) } else { 0int })) == 100); // lbt.ver.i33b
assert((b(tree_has(w0(), B(), 2))) == 1) by (compute);assert((b(tree_has(w0(), B(), 2))) == 1); // lbt.count.i33c
assert(((if tree_has(w0(), B(), 2) { col(tree_depth(w0(), B(), 2)) } else { 0int })) == 4) by (compute);assert(((if tree_has(w0(), B(), 2) { col(tree_depth(w0(), B(), 2)) } else { 0int })) == 4); // lbt.col.i33c
assert(((if tree_has(w0(), B(), 2) { tree_depth(w0(), B(), 2) + 1 } else { 0int })) == 2) by (compute);assert(((if tree_has(w0(), B(), 2) { tree_depth(w0(), B(), 2) + 1 } else { 0int })) == 2); // lbt.line.i33c
assert(((if tree_has(w0(), B(), 2) { tree_ver(w0(), B(), 2) } else { 0int })) == 100) by (compute);assert(((if tree_has(w0(), B(), 2) { tree_ver(w0(), B(), 2) } else { 0int })) == 100); // lbt.ver.i33c
assert((b(tree_has(w0(), B(), 3))) == 0) by (compute);assert((b(tree_has(w0(), B(), 3))) == 0); // lbt.count.i33d
assert(((if tree_has(w0(), B(), 3) { col(tree_depth(w0(), B(), 3)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 3) { col(tree_depth(w0(), B(), 3)) } else { 0int })) == 0); // lbt.col.i33d
assert(((if tree_has(w0(), B(), 3) { tree_depth(w0(), B(), 3) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 3) { tree_depth(w0(), B(), 3) + 1 } else { 0int })) == 0); // lbt.line.i33d
assert(((if tree_has(w0(), B(), 3) { tree_ver(w0(), B(), 3) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 3) { tree_ver(w0(), B(), 3) } else { 0int })) == 0); // lbt.ver.i33d
assert((b(tree_has(w0(), B(), 4))) == 0) by (compute);assert((b(tree_has(w0(), B(), 4))) == 0); // lbt.count.i33e
assert(((if tree_has(w0(), B(), 4) { col(tree_depth(w0(), B(), 4)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 4) { col(tree_depth(w0(), B(), 4)) } else { 0int })) == 0); // lbt.col.i33e
assert(((if tree_has(w0(), B(), 4) { tree_depth(w0(), B(), 4) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 4) { tree_depth(w0(), B(), 4) + 1 } else { 0int })) == 0); // lbt.line.i33e
assert(((if tree_has(w0(), B(), 4) { tree_ver(w0(), B(), 4) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 4) { tree_ver(w0(), B(), 4) } else { 0int })) == 0); // lbt.ver.i33e
assert((b(tree_has(w0(), B(), 5))) == 0) by (compute);assert((b(tree_has(w0(), B(), 5))) == 0); // lbt.count.i33f
assert(((if tree_has(w0(), B(), 5) { col(tree_depth(w0(), B(), 5)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 5) { col(tree_depth(w0(), B(), 5)) } else { 0int })) == 0); // lbt.col.i33f
assert(((if tree_has(w0(), B(), 5) { tree_depth(w0(), B(), 5) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 5) { tree_depth(w0(), B(), 5) + 1 } else { 0int })) == 0); // lbt.line.i33f
assert(((if tree_has(w0(), B(), 5) { tree_ver(w0(), B(), 5) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 5) { tree_ver(w0(), B(), 5) } else { 0int })) == 0); // lbt.ver.i33f
assert((b(tree_has(w0(), B(), 6))) == 0) by (compute);assert((b(tree_has(w0(), B(), 6))) == 0); // lbt.count.i33g
assert(((if tree_has(w0(), B(), 6) { col(tree_depth(w0(), B(), 6)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 6) { col(tree_depth(w0(), B(), 6)) } else { 0int })) == 0); // lbt.col.i33g
assert(((if tree_has(w0(), B(), 6) { tree_depth(w0(), B(), 6) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 6) { tree_depth(w0(), B(), 6) + 1 } else { 0int })) == 0); // lbt.line.i33g
assert(((if tree_has(w0(), B(), 6) { tree_ver(w0(), B(), 6) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), B(), 6) { tree_ver(w0(), B(), 6) } else { 0int })) == 0); // lbt.ver.i33g
assert((b(true)) == 1) by (compute);assert((b(true)) == 1); // ldt.ok
assert((tree_count_from(w0(), D(), 0)) == 0) by (compute);assert((tree_count_from(w0(), D(), 0)) == 0); // ldt.nodes
assert((b(tree_has(w0(), D(), 0))) == 0) by (compute);assert((b(tree_has(w0(), D(), 0))) == 0); // ldt.count.i33a
assert(((if tree_has(w0(), D(), 0) { col(tree_depth(w0(), D(), 0)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 0) { col(tree_depth(w0(), D(), 0)) } else { 0int })) == 0); // ldt.col.i33a
assert(((if tree_has(w0(), D(), 0) { tree_depth(w0(), D(), 0) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 0) { tree_depth(w0(), D(), 0) + 1 } else { 0int })) == 0); // ldt.line.i33a
assert(((if tree_has(w0(), D(), 0) { tree_ver(w0(), D(), 0) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 0) { tree_ver(w0(), D(), 0) } else { 0int })) == 0); // ldt.ver.i33a
assert((b(tree_has(w0(), D(), 1))) == 0) by (compute);assert((b(tree_has(w0(), D(), 1))) == 0); // ldt.count.i33b
assert(((if tree_has(w0(), D(), 1) { col(tree_depth(w0(), D(), 1)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 1) { col(tree_depth(w0(), D(), 1)) } else { 0int })) == 0); // ldt.col.i33b
assert(((if tree_has(w0(), D(), 1) { tree_depth(w0(), D(), 1) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 1) { tree_depth(w0(), D(), 1) + 1 } else { 0int })) == 0); // ldt.line.i33b
assert(((if tree_has(w0(), D(), 1) { tree_ver(w0(), D(), 1) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 1) { tree_ver(w0(), D(), 1) } else { 0int })) == 0); // ldt.ver.i33b
assert((b(tree_has(w0(), D(), 2))) == 0) by (compute);assert((b(tree_has(w0(), D(), 2))) == 0); // ldt.count.i33c
assert(((if tree_has(w0(), D(), 2) { col(tree_depth(w0(), D(), 2)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 2) { col(tree_depth(w0(), D(), 2)) } else { 0int })) == 0); // ldt.col.i33c
assert(((if tree_has(w0(), D(), 2) { tree_depth(w0(), D(), 2) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 2) { tree_depth(w0(), D(), 2) + 1 } else { 0int })) == 0); // ldt.line.i33c
assert(((if tree_has(w0(), D(), 2) { tree_ver(w0(), D(), 2) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 2) { tree_ver(w0(), D(), 2) } else { 0int })) == 0); // ldt.ver.i33c
assert((b(tree_has(w0(), D(), 3))) == 0) by (compute);assert((b(tree_has(w0(), D(), 3))) == 0); // ldt.count.i33d
assert(((if tree_has(w0(), D(), 3) { col(tree_depth(w0(), D(), 3)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 3) { col(tree_depth(w0(), D(), 3)) } else { 0int })) == 0); // ldt.col.i33d
assert(((if tree_has(w0(), D(), 3) { tree_depth(w0(), D(), 3) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 3) { tree_depth(w0(), D(), 3) + 1 } else { 0int })) == 0); // ldt.line.i33d
assert(((if tree_has(w0(), D(), 3) { tree_ver(w0(), D(), 3) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 3) { tree_ver(w0(), D(), 3) } else { 0int })) == 0); // ldt.ver.i33d
assert((b(tree_has(w0(), D(), 4))) == 0) by (compute);assert((b(tree_has(w0(), D(), 4))) == 0); // ldt.count.i33e
assert(((if tree_has(w0(), D(), 4) { col(tree_depth(w0(), D(), 4)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 4) { col(tree_depth(w0(), D(), 4)) } else { 0int })) == 0); // ldt.col.i33e
assert(((if tree_has(w0(), D(), 4) { tree_depth(w0(), D(), 4) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 4) { tree_depth(w0(), D(), 4) + 1 } else { 0int })) == 0); // ldt.line.i33e
assert(((if tree_has(w0(), D(), 4) { tree_ver(w0(), D(), 4) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 4) { tree_ver(w0(), D(), 4) } else { 0int })) == 0); // ldt.ver.i33e
assert((b(tree_has(w0(), D(), 5))) == 0) by (compute);assert((b(tree_has(w0(), D(), 5))) == 0); // ldt.count.i33f
assert(((if tree_has(w0(), D(), 5) { col(tree_depth(w0(), D(), 5)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 5) { col(tree_depth(w0(), D(), 5)) } else { 0int })) == 0); // ldt.col.i33f
assert(((if tree_has(w0(), D(), 5) { tree_depth(w0(), D(), 5) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 5) { tree_depth(w0(), D(), 5) + 1 } else { 0int })) == 0); // ldt.line.i33f
assert(((if tree_has(w0(), D(), 5) { tree_ver(w0(), D(), 5) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 5) { tree_ver(w0(), D(), 5) } else { 0int })) == 0); // ldt.ver.i33f
assert((b(tree_has(w0(), D(), 6))) == 0) by (compute);assert((b(tree_has(w0(), D(), 6))) == 0); // ldt.count.i33g
assert(((if tree_has(w0(), D(), 6) { col(tree_depth(w0(), D(), 6)) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 6) { col(tree_depth(w0(), D(), 6)) } else { 0int })) == 0); // ldt.col.i33g
assert(((if tree_has(w0(), D(), 6) { tree_depth(w0(), D(), 6) + 1 } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 6) { tree_depth(w0(), D(), 6) + 1 } else { 0int })) == 0); // ldt.line.i33g
assert(((if tree_has(w0(), D(), 6) { tree_ver(w0(), D(), 6) } else { 0int })) == 0) by (compute);assert(((if tree_has(w0(), D(), 6) { tree_ver(w0(), D(), 6) } else { 0int })) == 0); // ldt.ver.i33g
assert((w0().inst[0]) == 200) by (compute);assert((w0().inst[0]) == 200); // ld-g.inst.i33a
assert((w0().inst[1]) == 100) by (compute);assert((w0().inst[1]) == 100); // ld-g.inst.i33b
assert((w0().inst[2]) == 100) by (compute);assert((w0().inst[2]) == 100); // ld-g.inst.i33c
assert((w0().inst[3]) == 100) by (compute);assert((w0().inst[3]) == 100); // ld-g.inst.i33d
assert((w0().inst[4]) == 100) by (compute);assert((w0().inst[4]) == 100); // ld-g.inst.i33e
assert((w0().inst[5]) == 0) by (compute);assert((w0().inst[5]) == 0); // ld-g.inst.i33f
assert((w0().inst[6]) == 100) by (compute);assert((w0().inst[6]) == 100); // ld-g.inst.i33g
assert((w2().inst[0]) == 0) by (compute);assert((w2().inst[0]) == 0); // un-a.inst.i33a
assert((w2().inst[1]) == 100) by (compute);assert((w2().inst[1]) == 100); // un-a.inst.i33b
assert((w2().inst[2]) == 100) by (compute);assert((w2().inst[2]) == 100); // un-a.inst.i33c
assert((w2().inst[3]) == 100) by (compute);assert((w2().inst[3]) == 100); // un-a.inst.i33d
assert((w2().inst[4]) == 100) by (compute);assert((w2().inst[4]) == 100); // un-a.inst.i33e
assert((w2().inst[5]) == 0) by (compute);assert((w2().inst[5]) == 0); // un-a.inst.i33f
assert((w2().inst[6]) == 100) by (compute);assert((w2().inst[6]) == 100); // un-a.inst.i33g
assert((w3().inst[0]) == 0) by (compute);assert((w3().inst[0]) == 0); // un-d.inst.i33a
assert((w3().inst[1]) == 100) by (compute);assert((w3().inst[1]) == 100); // un-d.inst.i33b
assert((w3().inst[2]) == 100) by (compute);assert((w3().inst[2]) == 100); // un-d.inst.i33c
assert((w3().inst[3]) == 0) by (compute);assert((w3().inst[3]) == 0); // un-d.inst.i33d
assert((w3().inst[4]) == 100) by (compute);assert((w3().inst[4]) == 100); // un-d.inst.i33e
assert((w3().inst[5]) == 0) by (compute);assert((w3().inst[5]) == 0); // un-d.inst.i33f
assert((w3().inst[6]) == 100) by (compute);assert((w3().inst[6]) == 100); // un-d.inst.i33g
}
}
fn main() {}
