#[path="../../../../Verus_verification/consolidated/iter32/sync.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((s1().not_applied) == 1) by (compute);assert((s1().not_applied) == 1); // s1.notapplied
assert((present(s2().files, 1)) == 0) by (compute);assert((present(s2().files, 1)) == 0); // s2.exists.SyncC.B
assert((value(f1().files, 5)) == 1) by (compute);assert((value(f1().files, 5)) == 1); // f1.value.SyncF.A
assert((position(order(), 3)) == 3) by (compute);assert((position(order(), 3)) == 3); // o.start.tx
assert((position(order(), 5)) == 5) by (compute);assert((position(order(), 5)) == 5); // o.start.ty
assert((present(start(), 0)) == 1) by (compute);assert((present(start(), 0)) == 1); // setup.exists.SyncC.A
assert((value(start(), 0)) == 1) by (compute);assert((value(start(), 0)) == 1); // setup.value.SyncC.A
assert((present(start(), 1)) == 1) by (compute);assert((present(start(), 1)) == 1); // setup.exists.SyncC.B
assert((value(start(), 1)) == 1) by (compute);assert((value(start(), 1)) == 1); // setup.value.SyncC.B
assert((present(start(), 2)) == 1) by (compute);assert((present(start(), 2)) == 1); // setup.exists.SyncC.C
assert((value(start(), 2)) == 1) by (compute);assert((value(start(), 2)) == 1); // setup.value.SyncC.C
assert((present(start(), 3)) == 1) by (compute);assert((present(start(), 3)) == 1); // setup.exists.SyncD.D
assert((value(start(), 3)) == 1) by (compute);assert((value(start(), 3)) == 1); // setup.value.SyncD.D
assert((present(start(), 4)) == 0) by (compute);assert((present(start(), 4)) == 0); // setup.exists.SyncE.E
assert((value(start(), 4)) == -1) by (compute);assert((value(start(), 4)) == -1); // setup.value.SyncE.E
assert((present(start(), 5)) == 1) by (compute);assert((present(start(), 5)) == 1); // setup.exists.SyncF.A
assert((value(start(), 5)) == 1) by (compute);assert((value(start(), 5)) == 1); // setup.value.SyncF.A
assert((present(start(), 6)) == 1) by (compute);assert((present(start(), 6)) == 1); // setup.exists.SyncF.B
assert((value(start(), 6)) == 1) by (compute);assert((value(start(), 6)) == 1); // setup.value.SyncF.B
assert((present(s0().files, 0)) == 1) by (compute);assert((present(s0().files, 0)) == 1); // s0.exists.SyncC.A
assert((value(s0().files, 0)) == 1) by (compute);assert((value(s0().files, 0)) == 1); // s0.value.SyncC.A
assert((present(s0().files, 1)) == 1) by (compute);assert((present(s0().files, 1)) == 1); // s0.exists.SyncC.B
assert((value(s0().files, 1)) == 1) by (compute);assert((value(s0().files, 1)) == 1); // s0.value.SyncC.B
assert((present(s0().files, 2)) == 1) by (compute);assert((present(s0().files, 2)) == 1); // s0.exists.SyncC.C
assert((value(s0().files, 2)) == 1) by (compute);assert((value(s0().files, 2)) == 1); // s0.value.SyncC.C
assert((present(s0().files, 3)) == 1) by (compute);assert((present(s0().files, 3)) == 1); // s0.exists.SyncD.D
assert((value(s0().files, 3)) == 1) by (compute);assert((value(s0().files, 3)) == 1); // s0.value.SyncD.D
assert((present(s0().files, 4)) == 0) by (compute);assert((present(s0().files, 4)) == 0); // s0.exists.SyncE.E
assert((value(s0().files, 4)) == -1) by (compute);assert((value(s0().files, 4)) == -1); // s0.value.SyncE.E
assert((present(s0().files, 5)) == 1) by (compute);assert((present(s0().files, 5)) == 1); // s0.exists.SyncF.A
assert((value(s0().files, 5)) == 1) by (compute);assert((value(s0().files, 5)) == 1); // s0.value.SyncF.A
assert((present(s0().files, 6)) == 1) by (compute);assert((present(s0().files, 6)) == 1); // s0.exists.SyncF.B
assert((value(s0().files, 6)) == 1) by (compute);assert((value(s0().files, 6)) == 1); // s0.value.SyncF.B
assert((b(s0().nothing)) == 1) by (compute);assert((b(s0().nothing)) == 1); // s0.nothing
assert((s0().not_applied) == 0) by (compute);assert((s0().not_applied) == 0); // s0.notapplied
assert((s0().deleted) == 0) by (compute);assert((s0().deleted) == 0); // s0.deleted
assert((s0().updated) == 0) by (compute);assert((s0().updated) == 0); // s0.updated
assert((present(s1().files, 0)) == 1) by (compute);assert((present(s1().files, 0)) == 1); // s1.exists.SyncC.A
assert((value(s1().files, 0)) == 1) by (compute);assert((value(s1().files, 0)) == 1); // s1.value.SyncC.A
assert((present(s1().files, 1)) == 1) by (compute);assert((present(s1().files, 1)) == 1); // s1.exists.SyncC.B
assert((value(s1().files, 1)) == 1) by (compute);assert((value(s1().files, 1)) == 1); // s1.value.SyncC.B
assert((present(s1().files, 2)) == 1) by (compute);assert((present(s1().files, 2)) == 1); // s1.exists.SyncC.C
assert((value(s1().files, 2)) == 1) by (compute);assert((value(s1().files, 2)) == 1); // s1.value.SyncC.C
assert((present(s1().files, 3)) == 1) by (compute);assert((present(s1().files, 3)) == 1); // s1.exists.SyncD.D
assert((value(s1().files, 3)) == 1) by (compute);assert((value(s1().files, 3)) == 1); // s1.value.SyncD.D
assert((present(s1().files, 4)) == 0) by (compute);assert((present(s1().files, 4)) == 0); // s1.exists.SyncE.E
assert((value(s1().files, 4)) == -1) by (compute);assert((value(s1().files, 4)) == -1); // s1.value.SyncE.E
assert((present(s1().files, 5)) == 1) by (compute);assert((present(s1().files, 5)) == 1); // s1.exists.SyncF.A
assert((value(s1().files, 5)) == 1) by (compute);assert((value(s1().files, 5)) == 1); // s1.value.SyncF.A
assert((present(s1().files, 6)) == 1) by (compute);assert((present(s1().files, 6)) == 1); // s1.exists.SyncF.B
assert((value(s1().files, 6)) == 1) by (compute);assert((value(s1().files, 6)) == 1); // s1.value.SyncF.B
assert((b(s1().nothing)) == 1) by (compute);assert((b(s1().nothing)) == 1); // s1.nothing
assert((s1().deleted) == 0) by (compute);assert((s1().deleted) == 0); // s1.deleted
assert((s1().updated) == 0) by (compute);assert((s1().updated) == 0); // s1.updated
assert((present(s2().files, 0)) == 1) by (compute);assert((present(s2().files, 0)) == 1); // s2.exists.SyncC.A
assert((value(s2().files, 0)) == 1) by (compute);assert((value(s2().files, 0)) == 1); // s2.value.SyncC.A
assert((value(s2().files, 1)) == -1) by (compute);assert((value(s2().files, 1)) == -1); // s2.value.SyncC.B
assert((present(s2().files, 2)) == 1) by (compute);assert((present(s2().files, 2)) == 1); // s2.exists.SyncC.C
assert((value(s2().files, 2)) == 1) by (compute);assert((value(s2().files, 2)) == 1); // s2.value.SyncC.C
assert((present(s2().files, 3)) == 1) by (compute);assert((present(s2().files, 3)) == 1); // s2.exists.SyncD.D
assert((value(s2().files, 3)) == 1) by (compute);assert((value(s2().files, 3)) == 1); // s2.value.SyncD.D
assert((present(s2().files, 4)) == 0) by (compute);assert((present(s2().files, 4)) == 0); // s2.exists.SyncE.E
assert((value(s2().files, 4)) == -1) by (compute);assert((value(s2().files, 4)) == -1); // s2.value.SyncE.E
assert((present(s2().files, 5)) == 1) by (compute);assert((present(s2().files, 5)) == 1); // s2.exists.SyncF.A
assert((value(s2().files, 5)) == 1) by (compute);assert((value(s2().files, 5)) == 1); // s2.value.SyncF.A
assert((present(s2().files, 6)) == 1) by (compute);assert((present(s2().files, 6)) == 1); // s2.exists.SyncF.B
assert((value(s2().files, 6)) == 1) by (compute);assert((value(s2().files, 6)) == 1); // s2.value.SyncF.B
assert((b(s2().nothing)) == 0) by (compute);assert((b(s2().nothing)) == 0); // s2.nothing
assert((s2().not_applied) == 0) by (compute);assert((s2().not_applied) == 0); // s2.notapplied
assert((s2().deleted) == 1) by (compute);assert((s2().deleted) == 1); // s2.deleted
assert((s2().updated) == 0) by (compute);assert((s2().updated) == 0); // s2.updated
assert((present(s3().files, 0)) == 1) by (compute);assert((present(s3().files, 0)) == 1); // s3.exists.SyncC.A
assert((value(s3().files, 0)) == 2) by (compute);assert((value(s3().files, 0)) == 2); // s3.value.SyncC.A
assert((present(s3().files, 1)) == 0) by (compute);assert((present(s3().files, 1)) == 0); // s3.exists.SyncC.B
assert((value(s3().files, 1)) == -1) by (compute);assert((value(s3().files, 1)) == -1); // s3.value.SyncC.B
assert((present(s3().files, 2)) == 0) by (compute);assert((present(s3().files, 2)) == 0); // s3.exists.SyncC.C
assert((value(s3().files, 2)) == -1) by (compute);assert((value(s3().files, 2)) == -1); // s3.value.SyncC.C
assert((present(s3().files, 3)) == 1) by (compute);assert((present(s3().files, 3)) == 1); // s3.exists.SyncD.D
assert((value(s3().files, 3)) == 1) by (compute);assert((value(s3().files, 3)) == 1); // s3.value.SyncD.D
assert((present(s3().files, 4)) == 0) by (compute);assert((present(s3().files, 4)) == 0); // s3.exists.SyncE.E
assert((value(s3().files, 4)) == -1) by (compute);assert((value(s3().files, 4)) == -1); // s3.value.SyncE.E
assert((present(s3().files, 5)) == 1) by (compute);assert((present(s3().files, 5)) == 1); // s3.exists.SyncF.A
assert((value(s3().files, 5)) == 1) by (compute);assert((value(s3().files, 5)) == 1); // s3.value.SyncF.A
assert((present(s3().files, 6)) == 1) by (compute);assert((present(s3().files, 6)) == 1); // s3.exists.SyncF.B
assert((value(s3().files, 6)) == 1) by (compute);assert((value(s3().files, 6)) == 1); // s3.value.SyncF.B
assert((b(s3().nothing)) == 0) by (compute);assert((b(s3().nothing)) == 0); // s3.nothing
assert((s3().not_applied) == 0) by (compute);assert((s3().not_applied) == 0); // s3.notapplied
assert((s3().deleted) == 1) by (compute);assert((s3().deleted) == 1); // s3.deleted
assert((s3().updated) == 1) by (compute);assert((s3().updated) == 1); // s3.updated
assert((present(s4().files, 0)) == 1) by (compute);assert((present(s4().files, 0)) == 1); // s4.exists.SyncC.A
assert((value(s4().files, 0)) == 2) by (compute);assert((value(s4().files, 0)) == 2); // s4.value.SyncC.A
assert((present(s4().files, 1)) == 0) by (compute);assert((present(s4().files, 1)) == 0); // s4.exists.SyncC.B
assert((value(s4().files, 1)) == -1) by (compute);assert((value(s4().files, 1)) == -1); // s4.value.SyncC.B
assert((present(s4().files, 2)) == 0) by (compute);assert((present(s4().files, 2)) == 0); // s4.exists.SyncC.C
assert((value(s4().files, 2)) == -1) by (compute);assert((value(s4().files, 2)) == -1); // s4.value.SyncC.C
assert((present(s4().files, 3)) == 0) by (compute);assert((present(s4().files, 3)) == 0); // s4.exists.SyncD.D
assert((value(s4().files, 3)) == -1) by (compute);assert((value(s4().files, 3)) == -1); // s4.value.SyncD.D
assert((present(s4().files, 4)) == 0) by (compute);assert((present(s4().files, 4)) == 0); // s4.exists.SyncE.E
assert((value(s4().files, 4)) == -1) by (compute);assert((value(s4().files, 4)) == -1); // s4.value.SyncE.E
assert((present(s4().files, 5)) == 1) by (compute);assert((present(s4().files, 5)) == 1); // s4.exists.SyncF.A
assert((value(s4().files, 5)) == 1) by (compute);assert((value(s4().files, 5)) == 1); // s4.value.SyncF.A
assert((present(s4().files, 6)) == 1) by (compute);assert((present(s4().files, 6)) == 1); // s4.exists.SyncF.B
assert((value(s4().files, 6)) == 1) by (compute);assert((value(s4().files, 6)) == 1); // s4.value.SyncF.B
assert((b(s4().nothing)) == 0) by (compute);assert((b(s4().nothing)) == 0); // s4.nothing
assert((s4().not_applied) == 0) by (compute);assert((s4().not_applied) == 0); // s4.notapplied
assert((s4().deleted) == 1) by (compute);assert((s4().deleted) == 1); // s4.deleted
assert((s4().updated) == 0) by (compute);assert((s4().updated) == 0); // s4.updated
assert((present(s5().files, 0)) == 1) by (compute);assert((present(s5().files, 0)) == 1); // s5.exists.SyncC.A
assert((value(s5().files, 0)) == 2) by (compute);assert((value(s5().files, 0)) == 2); // s5.value.SyncC.A
assert((present(s5().files, 1)) == 0) by (compute);assert((present(s5().files, 1)) == 0); // s5.exists.SyncC.B
assert((value(s5().files, 1)) == -1) by (compute);assert((value(s5().files, 1)) == -1); // s5.value.SyncC.B
assert((present(s5().files, 2)) == 0) by (compute);assert((present(s5().files, 2)) == 0); // s5.exists.SyncC.C
assert((value(s5().files, 2)) == -1) by (compute);assert((value(s5().files, 2)) == -1); // s5.value.SyncC.C
assert((present(s5().files, 3)) == 0) by (compute);assert((present(s5().files, 3)) == 0); // s5.exists.SyncD.D
assert((value(s5().files, 3)) == -1) by (compute);assert((value(s5().files, 3)) == -1); // s5.value.SyncD.D
assert((present(s5().files, 4)) == 1) by (compute);assert((present(s5().files, 4)) == 1); // s5.exists.SyncE.E
assert((value(s5().files, 4)) == 5) by (compute);assert((value(s5().files, 4)) == 5); // s5.value.SyncE.E
assert((present(s5().files, 5)) == 1) by (compute);assert((present(s5().files, 5)) == 1); // s5.exists.SyncF.A
assert((value(s5().files, 5)) == 1) by (compute);assert((value(s5().files, 5)) == 1); // s5.value.SyncF.A
assert((present(s5().files, 6)) == 1) by (compute);assert((present(s5().files, 6)) == 1); // s5.exists.SyncF.B
assert((value(s5().files, 6)) == 1) by (compute);assert((value(s5().files, 6)) == 1); // s5.value.SyncF.B
assert((b(s5().nothing)) == 0) by (compute);assert((b(s5().nothing)) == 0); // s5.nothing
assert((s5().not_applied) == 0) by (compute);assert((s5().not_applied) == 0); // s5.notapplied
assert((s5().deleted) == 0) by (compute);assert((s5().deleted) == 0); // s5.deleted
assert((s5().updated) == 1) by (compute);assert((s5().updated) == 1); // s5.updated
assert((present(f1().files, 0)) == 1) by (compute);assert((present(f1().files, 0)) == 1); // f1.exists.SyncC.A
assert((value(f1().files, 0)) == 2) by (compute);assert((value(f1().files, 0)) == 2); // f1.value.SyncC.A
assert((present(f1().files, 1)) == 0) by (compute);assert((present(f1().files, 1)) == 0); // f1.exists.SyncC.B
assert((value(f1().files, 1)) == -1) by (compute);assert((value(f1().files, 1)) == -1); // f1.value.SyncC.B
assert((present(f1().files, 2)) == 0) by (compute);assert((present(f1().files, 2)) == 0); // f1.exists.SyncC.C
assert((value(f1().files, 2)) == -1) by (compute);assert((value(f1().files, 2)) == -1); // f1.value.SyncC.C
assert((present(f1().files, 3)) == 0) by (compute);assert((present(f1().files, 3)) == 0); // f1.exists.SyncD.D
assert((value(f1().files, 3)) == -1) by (compute);assert((value(f1().files, 3)) == -1); // f1.value.SyncD.D
assert((present(f1().files, 4)) == 1) by (compute);assert((present(f1().files, 4)) == 1); // f1.exists.SyncE.E
assert((value(f1().files, 4)) == 5) by (compute);assert((value(f1().files, 4)) == 5); // f1.value.SyncE.E
assert((present(f1().files, 5)) == 1) by (compute);assert((present(f1().files, 5)) == 1); // f1.exists.SyncF.A
assert((present(f1().files, 6)) == 1) by (compute);assert((present(f1().files, 6)) == 1); // f1.exists.SyncF.B
assert((value(f1().files, 6)) == 1) by (compute);assert((value(f1().files, 6)) == 1); // f1.value.SyncF.B
assert((b(f1().nothing)) == 1) by (compute);assert((b(f1().nothing)) == 1); // f1.nothing
assert((f1().not_applied) == 0) by (compute);assert((f1().not_applied) == 0); // f1.notapplied
assert((f1().deleted) == 0) by (compute);assert((f1().deleted) == 0); // f1.deleted
assert((f1().updated) == 0) by (compute);assert((f1().updated) == 0); // f1.updated
assert((present(f2().files, 0)) == 1) by (compute);assert((present(f2().files, 0)) == 1); // f2.exists.SyncC.A
assert((value(f2().files, 0)) == 2) by (compute);assert((value(f2().files, 0)) == 2); // f2.value.SyncC.A
assert((present(f2().files, 1)) == 0) by (compute);assert((present(f2().files, 1)) == 0); // f2.exists.SyncC.B
assert((value(f2().files, 1)) == -1) by (compute);assert((value(f2().files, 1)) == -1); // f2.value.SyncC.B
assert((present(f2().files, 2)) == 0) by (compute);assert((present(f2().files, 2)) == 0); // f2.exists.SyncC.C
assert((value(f2().files, 2)) == -1) by (compute);assert((value(f2().files, 2)) == -1); // f2.value.SyncC.C
assert((present(f2().files, 3)) == 0) by (compute);assert((present(f2().files, 3)) == 0); // f2.exists.SyncD.D
assert((value(f2().files, 3)) == -1) by (compute);assert((value(f2().files, 3)) == -1); // f2.value.SyncD.D
assert((present(f2().files, 4)) == 1) by (compute);assert((present(f2().files, 4)) == 1); // f2.exists.SyncE.E
assert((value(f2().files, 4)) == 5) by (compute);assert((value(f2().files, 4)) == 5); // f2.value.SyncE.E
assert((present(f2().files, 5)) == 1) by (compute);assert((present(f2().files, 5)) == 1); // f2.exists.SyncF.A
assert((value(f2().files, 5)) == 1) by (compute);assert((value(f2().files, 5)) == 1); // f2.value.SyncF.A
assert((present(f2().files, 6)) == 1) by (compute);assert((present(f2().files, 6)) == 1); // f2.exists.SyncF.B
assert((value(f2().files, 6)) == 1) by (compute);assert((value(f2().files, 6)) == 1); // f2.value.SyncF.B
assert((b(f2().nothing)) == 1) by (compute);assert((b(f2().nothing)) == 1); // f2.nothing
assert((f2().not_applied) == 0) by (compute);assert((f2().not_applied) == 0); // f2.notapplied
assert((f2().deleted) == 0) by (compute);assert((f2().deleted) == 0); // f2.deleted
assert((f2().updated) == 0) by (compute);assert((f2().updated) == 0); // f2.updated
assert((present(f3_files(), 0)) == 1) by (compute);assert((present(f3_files(), 0)) == 1); // f3.exists.SyncC.A
assert((value(f3_files(), 0)) == 2) by (compute);assert((value(f3_files(), 0)) == 2); // f3.value.SyncC.A
assert((present(f3_files(), 1)) == 0) by (compute);assert((present(f3_files(), 1)) == 0); // f3.exists.SyncC.B
assert((value(f3_files(), 1)) == -1) by (compute);assert((value(f3_files(), 1)) == -1); // f3.value.SyncC.B
assert((present(f3_files(), 2)) == 0) by (compute);assert((present(f3_files(), 2)) == 0); // f3.exists.SyncC.C
assert((value(f3_files(), 2)) == -1) by (compute);assert((value(f3_files(), 2)) == -1); // f3.value.SyncC.C
assert((present(f3_files(), 3)) == 0) by (compute);assert((present(f3_files(), 3)) == 0); // f3.exists.SyncD.D
assert((value(f3_files(), 3)) == -1) by (compute);assert((value(f3_files(), 3)) == -1); // f3.value.SyncD.D
assert((present(f3_files(), 4)) == 1) by (compute);assert((present(f3_files(), 4)) == 1); // f3.exists.SyncE.E
assert((value(f3_files(), 4)) == 5) by (compute);assert((value(f3_files(), 4)) == 5); // f3.value.SyncE.E
assert((present(f3_files(), 5)) == 1) by (compute);assert((present(f3_files(), 5)) == 1); // f3.exists.SyncF.A
assert((value(f3_files(), 5)) == 2) by (compute);assert((value(f3_files(), 5)) == 2); // f3.value.SyncF.A
assert((present(f3_files(), 6)) == 1) by (compute);assert((present(f3_files(), 6)) == 1); // f3.exists.SyncF.B
assert((value(f3_files(), 6)) == 1) by (compute);assert((value(f3_files(), 6)) == 1); // f3.value.SyncF.B
assert((position(rows(), 0)) == 1) by (compute);assert((position(rows(), 0)) == 1); // rows.syncc
assert((position(order(), 0)) == 1) by (compute);assert((position(order(), 0)) == 1); // helper.syncc
assert((position(order(), 0)) == 1) by (compute);assert((position(order(), 0)) == 1); // o.start.syncc
assert((position(rows(), 1)) == 2) by (compute);assert((position(rows(), 1)) == 2); // rows.syncf
assert((position(order(), 1)) == 2) by (compute);assert((position(order(), 1)) == 2); // helper.syncf
assert((position(order(), 1)) == 2) by (compute);assert((position(order(), 1)) == 2); // o.start.syncf
assert((position(rows(), 2)) == 3) by (compute);assert((position(rows(), 2)) == 3); // rows.ox
assert((position(order(), 2)) == 6) by (compute);assert((position(order(), 2)) == 6); // helper.ox
assert((position(order(), 2)) == 6) by (compute);assert((position(order(), 2)) == 6); // o.start.ox
assert((position(rows(), 3)) == 4) by (compute);assert((position(rows(), 3)) == 4); // rows.tx
assert((position(order(), 3)) == 3) by (compute);assert((position(order(), 3)) == 3); // helper.tx
assert((position(rows(), 4)) == 5) by (compute);assert((position(rows(), 4)) == 5); // rows.oy
assert((position(order(), 4)) == 4) by (compute);assert((position(order(), 4)) == 4); // helper.oy
assert((position(order(), 4)) == 4) by (compute);assert((position(order(), 4)) == 4); // o.start.oy
assert((position(rows(), 5)) == 6) by (compute);assert((position(rows(), 5)) == 6); // rows.ty
assert((position(order(), 5)) == 5) by (compute);assert((position(order(), 5)) == 5); // helper.ty
assert((order().len() as int) == 6) by (compute);assert((order().len() as int) == 6); // o.starts
}
}
fn main() {}
