#[path="../../../../Verus_verification/consolidated/iter38/render.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(summary(rec_z()).shows_unset)) == 1) by (compute);assert((b(summary(rec_z()).shows_unset)) == 1); // find.z.vunset
assert((b(summary(rec_i()).na)) == 0) by (compute);assert((b(summary(rec_i()).na)) == 0); // find.i.na
assert((b(detail_phases(rec_e())[4].success)) == 0) by (compute);assert((b(detail_phases(rec_e())[4].success)) == 0); // det.e.p5.success
assert((b(summary(rec_a()).shows_unset)) == 0) by (compute);assert((b(summary(rec_a()).shows_unset)) == 0); // find.a.vunset
assert((b(summary(rec_a()).shows_version)) == 1) by (compute);assert((b(summary(rec_a()).shows_version)) == 1); // find.a.vtext
assert((b(summary(rec_a()).success)) == 1) by (compute);assert((b(summary(rec_a()).success)) == 1); // find.a.success
assert((b(summary(rec_a()).committed)) == 1) by (compute);assert((b(summary(rec_a()).committed)) == 1); // find.a.committed
assert((b(summary(rec_a()).uncommitted)) == 0) by (compute);assert((b(summary(rec_a()).uncommitted)) == 0); // find.a.uncommitted
assert((b(summary(rec_a()).na)) == 1) by (compute);assert((b(summary(rec_a()).na)) == 1); // find.a.na
assert((b(summary(rec_a()).nosource)) == 0) by (compute);assert((b(summary(rec_a()).nosource)) == 0); // find.a.nosource
assert((b(summary(rec_a()).duration)) == 1) by (compute);assert((b(summary(rec_a()).duration)) == 1); // find.a.dur
assert((b(detail_unset(rec_a()))) == 0) by (compute);assert((b(detail_unset(rec_a()))) == 0); // det.a.vunset
assert((b(rec_a().ended)) == 1) by (compute);assert((b(rec_a().ended)) == 1); // det.a.endedtext
assert((b(detail_committed(rec_a()))) == 1) by (compute);assert((b(detail_committed(rec_a()))) == 1); // det.a.committed
assert((b(detail_source_na(rec_a()))) == 1) by (compute);assert((b(detail_source_na(rec_a()))) == 1); // det.a.source.na
assert((detail_phases(rec_a())[0].name) == 1) by (compute);assert((detail_phases(rec_a())[0].name) == 1); // det.a.p1.name
assert((b(detail_phases(rec_a())[0].success)) == 1) by (compute);assert((b(detail_phases(rec_a())[0].success)) == 1); // det.a.p1.success
assert((b(detail_phases(rec_a())[0].duration)) == 1) by (compute);assert((b(detail_phases(rec_a())[0].duration)) == 1); // det.a.p1.dur
assert((detail_phases(rec_a())[1].name) == 2) by (compute);assert((detail_phases(rec_a())[1].name) == 2); // det.a.p2.name
assert((b(detail_phases(rec_a())[1].success)) == 1) by (compute);assert((b(detail_phases(rec_a())[1].success)) == 1); // det.a.p2.success
assert((b(detail_phases(rec_a())[1].duration)) == 1) by (compute);assert((b(detail_phases(rec_a())[1].duration)) == 1); // det.a.p2.dur
assert((detail_phases(rec_a())[2].name) == 4) by (compute);assert((detail_phases(rec_a())[2].name) == 4); // det.a.p3.name
assert((b(detail_phases(rec_a())[2].success)) == 1) by (compute);assert((b(detail_phases(rec_a())[2].success)) == 1); // det.a.p3.success
assert((b(detail_phases(rec_a())[2].duration)) == 1) by (compute);assert((b(detail_phases(rec_a())[2].duration)) == 1); // det.a.p3.dur
assert((detail_phases(rec_a())[3].name) == 6) by (compute);assert((detail_phases(rec_a())[3].name) == 6); // det.a.p4.name
assert((b(detail_phases(rec_a())[3].success)) == 1) by (compute);assert((b(detail_phases(rec_a())[3].success)) == 1); // det.a.p4.success
assert((b(detail_phases(rec_a())[3].duration)) == 1) by (compute);assert((b(detail_phases(rec_a())[3].duration)) == 1); // det.a.p4.dur
assert((detail_phases(rec_a())[4].name) == 7) by (compute);assert((detail_phases(rec_a())[4].name) == 7); // det.a.p5.name
assert((b(detail_phases(rec_a())[4].success)) == 1) by (compute);assert((b(detail_phases(rec_a())[4].success)) == 1); // det.a.p5.success
assert((b(detail_phases(rec_a())[4].duration)) == 1) by (compute);assert((b(detail_phases(rec_a())[4].duration)) == 1); // det.a.p5.dur
assert((detail_phases(rec_a()).len() as int) == 5) by (compute);assert((detail_phases(rec_a()).len() as int) == 5); // det.a.phases
assert((b(summary(rec_b()).shows_unset)) == 1) by (compute);assert((b(summary(rec_b()).shows_unset)) == 1); // find.b.vunset
assert((b(summary(rec_b()).shows_version)) == 0) by (compute);assert((b(summary(rec_b()).shows_version)) == 0); // find.b.vtext
assert((b(summary(rec_b()).success)) == 0) by (compute);assert((b(summary(rec_b()).success)) == 0); // find.b.success
assert((b(summary(rec_b()).committed)) == 0) by (compute);assert((b(summary(rec_b()).committed)) == 0); // find.b.committed
assert((b(summary(rec_b()).uncommitted)) == 1) by (compute);assert((b(summary(rec_b()).uncommitted)) == 1); // find.b.uncommitted
assert((b(summary(rec_b()).na)) == 1) by (compute);assert((b(summary(rec_b()).na)) == 1); // find.b.na
assert((b(summary(rec_b()).nosource)) == 0) by (compute);assert((b(summary(rec_b()).nosource)) == 0); // find.b.nosource
assert((b(summary(rec_b()).duration)) == 1) by (compute);assert((b(summary(rec_b()).duration)) == 1); // find.b.dur
assert((b(detail_unset(rec_b()))) == 1) by (compute);assert((b(detail_unset(rec_b()))) == 1); // det.b.vunset
assert((b(rec_b().ended)) == 1) by (compute);assert((b(rec_b().ended)) == 1); // det.b.endedtext
assert((b(detail_committed(rec_b()))) == 0) by (compute);assert((b(detail_committed(rec_b()))) == 0); // det.b.committed
assert((b(detail_source_na(rec_b()))) == 1) by (compute);assert((b(detail_source_na(rec_b()))) == 1); // det.b.source.na
assert((detail_phases(rec_b()).len() as int) == 0) by (compute);assert((detail_phases(rec_b()).len() as int) == 0); // det.b.phases
assert((b(summary(rec_e()).shows_unset)) == 0) by (compute);assert((b(summary(rec_e()).shows_unset)) == 0); // find.e.vunset
assert((b(summary(rec_e()).shows_version)) == 1) by (compute);assert((b(summary(rec_e()).shows_version)) == 1); // find.e.vtext
assert((b(summary(rec_e()).success)) == 0) by (compute);assert((b(summary(rec_e()).success)) == 0); // find.e.success
assert((b(summary(rec_e()).committed)) == 0) by (compute);assert((b(summary(rec_e()).committed)) == 0); // find.e.committed
assert((b(summary(rec_e()).uncommitted)) == 1) by (compute);assert((b(summary(rec_e()).uncommitted)) == 1); // find.e.uncommitted
assert((b(summary(rec_e()).na)) == 1) by (compute);assert((b(summary(rec_e()).na)) == 1); // find.e.na
assert((b(summary(rec_e()).nosource)) == 0) by (compute);assert((b(summary(rec_e()).nosource)) == 0); // find.e.nosource
assert((b(summary(rec_e()).duration)) == 1) by (compute);assert((b(summary(rec_e()).duration)) == 1); // find.e.dur
assert((b(detail_unset(rec_e()))) == 0) by (compute);assert((b(detail_unset(rec_e()))) == 0); // det.e.vunset
assert((b(rec_e().ended)) == 1) by (compute);assert((b(rec_e().ended)) == 1); // det.e.endedtext
assert((b(detail_committed(rec_e()))) == 0) by (compute);assert((b(detail_committed(rec_e()))) == 0); // det.e.committed
assert((b(detail_source_na(rec_e()))) == 1) by (compute);assert((b(detail_source_na(rec_e()))) == 1); // det.e.source.na
assert((detail_phases(rec_e())[0].name) == 1) by (compute);assert((detail_phases(rec_e())[0].name) == 1); // det.e.p1.name
assert((b(detail_phases(rec_e())[0].success)) == 1) by (compute);assert((b(detail_phases(rec_e())[0].success)) == 1); // det.e.p1.success
assert((b(detail_phases(rec_e())[0].duration)) == 1) by (compute);assert((b(detail_phases(rec_e())[0].duration)) == 1); // det.e.p1.dur
assert((detail_phases(rec_e())[1].name) == 2) by (compute);assert((detail_phases(rec_e())[1].name) == 2); // det.e.p2.name
assert((b(detail_phases(rec_e())[1].success)) == 1) by (compute);assert((b(detail_phases(rec_e())[1].success)) == 1); // det.e.p2.success
assert((b(detail_phases(rec_e())[1].duration)) == 1) by (compute);assert((b(detail_phases(rec_e())[1].duration)) == 1); // det.e.p2.dur
assert((detail_phases(rec_e())[2].name) == 4) by (compute);assert((detail_phases(rec_e())[2].name) == 4); // det.e.p3.name
assert((b(detail_phases(rec_e())[2].success)) == 1) by (compute);assert((b(detail_phases(rec_e())[2].success)) == 1); // det.e.p3.success
assert((b(detail_phases(rec_e())[2].duration)) == 1) by (compute);assert((b(detail_phases(rec_e())[2].duration)) == 1); // det.e.p3.dur
assert((detail_phases(rec_e())[3].name) == 6) by (compute);assert((detail_phases(rec_e())[3].name) == 6); // det.e.p4.name
assert((b(detail_phases(rec_e())[3].success)) == 1) by (compute);assert((b(detail_phases(rec_e())[3].success)) == 1); // det.e.p4.success
assert((b(detail_phases(rec_e())[3].duration)) == 1) by (compute);assert((b(detail_phases(rec_e())[3].duration)) == 1); // det.e.p4.dur
assert((detail_phases(rec_e())[4].name) == 7) by (compute);assert((detail_phases(rec_e())[4].name) == 7); // det.e.p5.name
assert((b(detail_phases(rec_e())[4].duration)) == 0) by (compute);assert((b(detail_phases(rec_e())[4].duration)) == 0); // det.e.p5.dur
assert((detail_phases(rec_e()).len() as int) == 5) by (compute);assert((detail_phases(rec_e()).len() as int) == 5); // det.e.phases
assert((b(summary(rec_z()).shows_version)) == 0) by (compute);assert((b(summary(rec_z()).shows_version)) == 0); // find.z.vtext
assert((b(summary(rec_z()).success)) == 1) by (compute);assert((b(summary(rec_z()).success)) == 1); // find.z.success
assert((b(summary(rec_z()).committed)) == 1) by (compute);assert((b(summary(rec_z()).committed)) == 1); // find.z.committed
assert((b(summary(rec_z()).uncommitted)) == 0) by (compute);assert((b(summary(rec_z()).uncommitted)) == 0); // find.z.uncommitted
assert((b(summary(rec_z()).na)) == 1) by (compute);assert((b(summary(rec_z()).na)) == 1); // find.z.na
assert((b(summary(rec_z()).nosource)) == 0) by (compute);assert((b(summary(rec_z()).nosource)) == 0); // find.z.nosource
assert((b(summary(rec_z()).duration)) == 1) by (compute);assert((b(summary(rec_z()).duration)) == 1); // find.z.dur
assert((b(detail_unset(rec_z()))) == 1) by (compute);assert((b(detail_unset(rec_z()))) == 1); // det.z.vunset
assert((b(rec_z().ended)) == 1) by (compute);assert((b(rec_z().ended)) == 1); // det.z.endedtext
assert((b(detail_committed(rec_z()))) == 1) by (compute);assert((b(detail_committed(rec_z()))) == 1); // det.z.committed
assert((b(detail_source_na(rec_z()))) == 1) by (compute);assert((b(detail_source_na(rec_z()))) == 1); // det.z.source.na
assert((detail_phases(rec_z())[0].name) == 1) by (compute);assert((detail_phases(rec_z())[0].name) == 1); // det.z.p1.name
assert((b(detail_phases(rec_z())[0].success)) == 1) by (compute);assert((b(detail_phases(rec_z())[0].success)) == 1); // det.z.p1.success
assert((b(detail_phases(rec_z())[0].duration)) == 1) by (compute);assert((b(detail_phases(rec_z())[0].duration)) == 1); // det.z.p1.dur
assert((detail_phases(rec_z())[1].name) == 2) by (compute);assert((detail_phases(rec_z())[1].name) == 2); // det.z.p2.name
assert((b(detail_phases(rec_z())[1].success)) == 1) by (compute);assert((b(detail_phases(rec_z())[1].success)) == 1); // det.z.p2.success
assert((b(detail_phases(rec_z())[1].duration)) == 1) by (compute);assert((b(detail_phases(rec_z())[1].duration)) == 1); // det.z.p2.dur
assert((detail_phases(rec_z())[2].name) == 4) by (compute);assert((detail_phases(rec_z())[2].name) == 4); // det.z.p3.name
assert((b(detail_phases(rec_z())[2].success)) == 1) by (compute);assert((b(detail_phases(rec_z())[2].success)) == 1); // det.z.p3.success
assert((b(detail_phases(rec_z())[2].duration)) == 1) by (compute);assert((b(detail_phases(rec_z())[2].duration)) == 1); // det.z.p3.dur
assert((detail_phases(rec_z())[3].name) == 6) by (compute);assert((detail_phases(rec_z())[3].name) == 6); // det.z.p4.name
assert((b(detail_phases(rec_z())[3].success)) == 1) by (compute);assert((b(detail_phases(rec_z())[3].success)) == 1); // det.z.p4.success
assert((b(detail_phases(rec_z())[3].duration)) == 1) by (compute);assert((b(detail_phases(rec_z())[3].duration)) == 1); // det.z.p4.dur
assert((detail_phases(rec_z())[4].name) == 7) by (compute);assert((detail_phases(rec_z())[4].name) == 7); // det.z.p5.name
assert((b(detail_phases(rec_z())[4].success)) == 1) by (compute);assert((b(detail_phases(rec_z())[4].success)) == 1); // det.z.p5.success
assert((b(detail_phases(rec_z())[4].duration)) == 1) by (compute);assert((b(detail_phases(rec_z())[4].duration)) == 1); // det.z.p5.dur
assert((detail_phases(rec_z()).len() as int) == 5) by (compute);assert((detail_phases(rec_z()).len() as int) == 5); // det.z.phases
assert((b(summary(rec_i()).shows_unset)) == 0) by (compute);assert((b(summary(rec_i()).shows_unset)) == 0); // find.i.vunset
assert((b(summary(rec_i()).shows_version)) == 1) by (compute);assert((b(summary(rec_i()).shows_version)) == 1); // find.i.vtext
assert((b(summary(rec_i()).success)) == 1) by (compute);assert((b(summary(rec_i()).success)) == 1); // find.i.success
assert((b(summary(rec_i()).committed)) == 1) by (compute);assert((b(summary(rec_i()).committed)) == 1); // find.i.committed
assert((b(summary(rec_i()).uncommitted)) == 0) by (compute);assert((b(summary(rec_i()).uncommitted)) == 0); // find.i.uncommitted
assert((b(summary(rec_i()).nosource)) == 0) by (compute);assert((b(summary(rec_i()).nosource)) == 0); // find.i.nosource
assert((b(summary(rec_i()).duration)) == 1) by (compute);assert((b(summary(rec_i()).duration)) == 1); // find.i.dur
assert((b(detail_unset(rec_i()))) == 0) by (compute);assert((b(detail_unset(rec_i()))) == 0); // det.i.vunset
assert((b(rec_i().ended)) == 1) by (compute);assert((b(rec_i().ended)) == 1); // det.i.endedtext
assert((b(detail_committed(rec_i()))) == 1) by (compute);assert((b(detail_committed(rec_i()))) == 1); // det.i.committed
assert((b(detail_source_na(rec_i()))) == 0) by (compute);assert((b(detail_source_na(rec_i()))) == 0); // det.i.source.na
assert((detail_phases(rec_i())[0].name) == 1) by (compute);assert((detail_phases(rec_i())[0].name) == 1); // det.i.p1.name
assert((b(detail_phases(rec_i())[0].success)) == 1) by (compute);assert((b(detail_phases(rec_i())[0].success)) == 1); // det.i.p1.success
assert((b(detail_phases(rec_i())[0].duration)) == 1) by (compute);assert((b(detail_phases(rec_i())[0].duration)) == 1); // det.i.p1.dur
assert((detail_phases(rec_i())[1].name) == 2) by (compute);assert((detail_phases(rec_i())[1].name) == 2); // det.i.p2.name
assert((b(detail_phases(rec_i())[1].success)) == 1) by (compute);assert((b(detail_phases(rec_i())[1].success)) == 1); // det.i.p2.success
assert((b(detail_phases(rec_i())[1].duration)) == 1) by (compute);assert((b(detail_phases(rec_i())[1].duration)) == 1); // det.i.p2.dur
assert((detail_phases(rec_i())[2].name) == 4) by (compute);assert((detail_phases(rec_i())[2].name) == 4); // det.i.p3.name
assert((b(detail_phases(rec_i())[2].success)) == 1) by (compute);assert((b(detail_phases(rec_i())[2].success)) == 1); // det.i.p3.success
assert((b(detail_phases(rec_i())[2].duration)) == 1) by (compute);assert((b(detail_phases(rec_i())[2].duration)) == 1); // det.i.p3.dur
assert((detail_phases(rec_i())[3].name) == 6) by (compute);assert((detail_phases(rec_i())[3].name) == 6); // det.i.p4.name
assert((b(detail_phases(rec_i())[3].success)) == 1) by (compute);assert((b(detail_phases(rec_i())[3].success)) == 1); // det.i.p4.success
assert((b(detail_phases(rec_i())[3].duration)) == 1) by (compute);assert((b(detail_phases(rec_i())[3].duration)) == 1); // det.i.p4.dur
assert((detail_phases(rec_i())[4].name) == 7) by (compute);assert((detail_phases(rec_i())[4].name) == 7); // det.i.p5.name
assert((b(detail_phases(rec_i())[4].success)) == 1) by (compute);assert((b(detail_phases(rec_i())[4].success)) == 1); // det.i.p5.success
assert((b(detail_phases(rec_i())[4].duration)) == 1) by (compute);assert((b(detail_phases(rec_i())[4].duration)) == 1); // det.i.p5.dur
assert((detail_phases(rec_i()).len() as int) == 5) by (compute);assert((detail_phases(rec_i()).len() as int) == 5); // det.i.phases
assert((b(summary(rec_u()).shows_unset)) == 0) by (compute);assert((b(summary(rec_u()).shows_unset)) == 0); // find.u.vunset
assert((b(summary(rec_u()).shows_version)) == 1) by (compute);assert((b(summary(rec_u()).shows_version)) == 1); // find.u.vtext
assert((b(summary(rec_u()).success)) == 1) by (compute);assert((b(summary(rec_u()).success)) == 1); // find.u.success
assert((b(summary(rec_u()).committed)) == 1) by (compute);assert((b(summary(rec_u()).committed)) == 1); // find.u.committed
assert((b(summary(rec_u()).uncommitted)) == 0) by (compute);assert((b(summary(rec_u()).uncommitted)) == 0); // find.u.uncommitted
assert((b(summary(rec_u()).na)) == 1) by (compute);assert((b(summary(rec_u()).na)) == 1); // find.u.na
assert((b(summary(rec_u()).nosource)) == 0) by (compute);assert((b(summary(rec_u()).nosource)) == 0); // find.u.nosource
assert((b(summary(rec_u()).duration)) == 1) by (compute);assert((b(summary(rec_u()).duration)) == 1); // find.u.dur
assert((b(detail_unset(rec_u()))) == 0) by (compute);assert((b(detail_unset(rec_u()))) == 0); // det.u.vunset
assert((b(rec_u().ended)) == 1) by (compute);assert((b(rec_u().ended)) == 1); // det.u.endedtext
assert((b(detail_committed(rec_u()))) == 1) by (compute);assert((b(detail_committed(rec_u()))) == 1); // det.u.committed
assert((b(detail_source_na(rec_u()))) == 1) by (compute);assert((b(detail_source_na(rec_u()))) == 1); // det.u.source.na
assert((detail_phases(rec_u())[0].name) == 0) by (compute);assert((detail_phases(rec_u())[0].name) == 0); // det.u.p1.name
assert((b(detail_phases(rec_u())[0].success)) == 1) by (compute);assert((b(detail_phases(rec_u())[0].success)) == 1); // det.u.p1.success
assert((b(detail_phases(rec_u())[0].duration)) == 1) by (compute);assert((b(detail_phases(rec_u())[0].duration)) == 1); // det.u.p1.dur
assert((detail_phases(rec_u()).len() as int) == 1) by (compute);assert((detail_phases(rec_u()).len() as int) == 1); // det.u.phases
}
}
fn main() {}
