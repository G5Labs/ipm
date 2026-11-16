#[path="../../../../Verus_verification/consolidated/iter66/docphase.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((pos(document(full(), 4, true).marks, 3)) == 0);assert((pos(document(full(), 4, true).marks, 3)) == 0); // c4.hook.e66
assert((pos(document(full(), 2, false).marks, 1)) == 0);assert((pos(document(full(), 2, false).marks, 1)) == 0); // c2.hook.d66
assert((b(document(W { d_installed: false, d_offered: true }, 2, true).not_installed)) == 1);assert((b(document(W { d_installed: false, d_offered: true }, 2, true).not_installed)) == 1); // c6.notinstalled
assert((b(document(full(), 2, true).ok)) == 1);assert((b(document(full(), 2, true).ok)) == 1); // c1.ok
assert((b(document(full(), 2, true).not_installed)) == 0);assert((b(document(full(), 2, true).not_installed)) == 0); // c1.notinstalled
assert((document(full(), 2, true).starts) == 2);assert((document(full(), 2, true).starts) == 2); // c1.starts
assert((pos(document(full(), 2, true).marks, 1)) == 1);assert((pos(document(full(), 2, true).marks, 1)) == 1); // c1.hook.d66
assert((pos(document(full(), 2, true).marks, 2)) == 2);assert((pos(document(full(), 2, true).marks, 2)) == 2); // c1.hook.m66
assert((pos(document(full(), 2, true).marks, 3)) == 0);assert((pos(document(full(), 2, true).marks, 3)) == 0); // c1.hook.e66
assert((pos(document(full(), 2, true).marks, 4)) == 0);assert((pos(document(full(), 2, true).marks, 4)) == 0); // c1.hook.f66
assert((b(document(full(), 2, false).ok)) == 1);assert((b(document(full(), 2, false).ok)) == 1); // c2.ok
assert((b(document(full(), 2, false).not_installed)) == 0);assert((b(document(full(), 2, false).not_installed)) == 0); // c2.notinstalled
assert((document(full(), 2, false).starts) == 1);assert((document(full(), 2, false).starts) == 1); // c2.starts
assert((pos(document(full(), 2, false).marks, 2)) == 1);assert((pos(document(full(), 2, false).marks, 2)) == 1); // c2.hook.m66
assert((pos(document(full(), 2, false).marks, 3)) == 0);assert((pos(document(full(), 2, false).marks, 3)) == 0); // c2.hook.e66
assert((pos(document(full(), 2, false).marks, 4)) == 0);assert((pos(document(full(), 2, false).marks, 4)) == 0); // c2.hook.f66
assert((b(document(full(), 5, true).ok)) == 1);assert((b(document(full(), 5, true).ok)) == 1); // c3.ok
assert((b(document(full(), 5, true).not_installed)) == 0);assert((b(document(full(), 5, true).not_installed)) == 0); // c3.notinstalled
assert((document(full(), 5, true).starts) == 1);assert((document(full(), 5, true).starts) == 1); // c3.starts
assert((pos(document(full(), 5, true).marks, 1)) == 0);assert((pos(document(full(), 5, true).marks, 1)) == 0); // c3.hook.d66
assert((pos(document(full(), 5, true).marks, 2)) == 0);assert((pos(document(full(), 5, true).marks, 2)) == 0); // c3.hook.m66
assert((pos(document(full(), 5, true).marks, 3)) == 0);assert((pos(document(full(), 5, true).marks, 3)) == 0); // c3.hook.e66
assert((pos(document(full(), 5, true).marks, 4)) == 0);assert((pos(document(full(), 5, true).marks, 4)) == 0); // c3.hook.f66
assert((b(document(full(), 4, true).ok)) == 0);assert((b(document(full(), 4, true).ok)) == 0); // c4.ok
assert((b(document(full(), 4, true).not_installed)) == 0);assert((b(document(full(), 4, true).not_installed)) == 0); // c4.notinstalled
assert((document(full(), 4, true).starts) == 2);assert((document(full(), 4, true).starts) == 2); // c4.starts
assert((pos(document(full(), 4, true).marks, 1)) == 0);assert((pos(document(full(), 4, true).marks, 1)) == 0); // c4.hook.d66
assert((pos(document(full(), 4, true).marks, 2)) == 0);assert((pos(document(full(), 4, true).marks, 2)) == 0); // c4.hook.m66
assert((pos(document(full(), 4, true).marks, 4)) == 0);assert((pos(document(full(), 4, true).marks, 4)) == 0); // c4.hook.f66
assert((b(document(W { d_installed: false, d_offered: false }, 2, true).ok)) == 0);assert((b(document(W { d_installed: false, d_offered: false }, 2, true).ok)) == 0); // c5.ok
assert((b(document(W { d_installed: false, d_offered: false }, 2, true).not_installed)) == 0);assert((b(document(W { d_installed: false, d_offered: false }, 2, true).not_installed)) == 0); // c5.notinstalled
assert((document(W { d_installed: false, d_offered: false }, 2, true).starts) == 1);assert((document(W { d_installed: false, d_offered: false }, 2, true).starts) == 1); // c5.starts
assert((pos(document(W { d_installed: false, d_offered: false }, 2, true).marks, 1)) == 0);assert((pos(document(W { d_installed: false, d_offered: false }, 2, true).marks, 1)) == 0); // c5.hook.d66
assert((pos(document(W { d_installed: false, d_offered: false }, 2, true).marks, 2)) == 0);assert((pos(document(W { d_installed: false, d_offered: false }, 2, true).marks, 2)) == 0); // c5.hook.m66
assert((pos(document(W { d_installed: false, d_offered: false }, 2, true).marks, 3)) == 0);assert((pos(document(W { d_installed: false, d_offered: false }, 2, true).marks, 3)) == 0); // c5.hook.e66
assert((pos(document(W { d_installed: false, d_offered: false }, 2, true).marks, 4)) == 0);assert((pos(document(W { d_installed: false, d_offered: false }, 2, true).marks, 4)) == 0); // c5.hook.f66
assert((b(document(W { d_installed: false, d_offered: true }, 2, true).ok)) == 0);assert((b(document(W { d_installed: false, d_offered: true }, 2, true).ok)) == 0); // c6.ok
assert((document(W { d_installed: false, d_offered: true }, 2, true).starts) == 1);assert((document(W { d_installed: false, d_offered: true }, 2, true).starts) == 1); // c6.starts
assert((pos(document(W { d_installed: false, d_offered: true }, 2, true).marks, 1)) == 0);assert((pos(document(W { d_installed: false, d_offered: true }, 2, true).marks, 1)) == 0); // c6.hook.d66
assert((pos(document(W { d_installed: false, d_offered: true }, 2, true).marks, 2)) == 0);assert((pos(document(W { d_installed: false, d_offered: true }, 2, true).marks, 2)) == 0); // c6.hook.m66
assert((pos(document(W { d_installed: false, d_offered: true }, 2, true).marks, 3)) == 0);assert((pos(document(W { d_installed: false, d_offered: true }, 2, true).marks, 3)) == 0); // c6.hook.e66
assert((pos(document(W { d_installed: false, d_offered: true }, 2, true).marks, 4)) == 0);assert((pos(document(W { d_installed: false, d_offered: true }, 2, true).marks, 4)) == 0); // c6.hook.f66
}
}
fn main() {}
