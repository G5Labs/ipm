#[path="../../../../Verus_verification/coverage-20261117/iter73/instances.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(load_ok(m2a_out().resources, m2a()))) == 0) by (compute_only);assert((b(load_ok(m2a_out().resources, m2a()))) == 0); // m2a.load.ok
assert((b(load_ok(m2b_out().resources, m2b()))) == 0) by (compute_only);assert((b(load_ok(m2b_out().resources, m2b()))) == 0); // m2b.load.ok
assert((b(load_ok(xout(x1(), 901).resources, x1()))) == 0) by (compute_only);assert((b(load_ok(xout(x1(), 901).resources, x1()))) == 0); // x1.load.ok
assert((b(load_ok(xout(x4(), 904).resources, x4()))) == 0) by (compute_only);assert((b(load_ok(xout(x4(), 904).resources, x4()))) == 0); // x4.load.ok
assert((b(load_ok(xout(x2(), 902).resources, x2()))) == 1) by (compute_only);assert((b(load_ok(xout(x2(), 902).resources, x2()))) == 1); // x2.load.ok
assert((b(has_entry(m3_out().resources, Entry { name: seq![301, 12], ext: PKG, dir: seq![62] }))) == 1) by (compute_only);assert((b(has_entry(m3_out().resources, Entry { name: seq![301, 12], ext: PKG, dir: seq![62] }))) == 1); // m3.Id1.B.PKG@y
assert((b(has_entry(m7_out().resources, Entry { name: seq![701], ext: PKG, dir: seq![66] }))) == 1) by (compute_only);assert((b(has_entry(m7_out().resources, Entry { name: seq![701], ext: PKG, dir: seq![66] }))) == 1); // m7.Im7.PKG@a
assert((b(has_entry(m8_out().resources, Entry { name: seq![801], ext: PKG, dir: seq![68] }))) == 1) by (compute_only);assert((b(has_entry(m8_out().resources, Entry { name: seq![801], ext: PKG, dir: seq![68] }))) == 1); // m8.Im8.PKG@ab
assert((b(loads_class(m3_out().resources, m3(), m3()[0]))) == 0) by (compute_only);assert((b(loads_class(m3_out().resources, m3(), m3()[0]))) == 0); // class.Id1.B.C
assert((b(loads_class(m7_out().resources, m7(), m7()[1]))) == 0) by (compute_only);assert((b(loads_class(m7_out().resources, m7(), m7()[1]))) == 0); // class.Im7.Y
assert((b(has_test(m4_out().tests, ut(seq![65, 64], seq![402])))) == 1) by (compute_only);assert((b(has_test(m4_out().tests, ut(seq![65, 64], seq![402])))) == 1); // m4.unittest
assert((b(one_test(m4_out()) && only_test(m4_out()).processor == UNITTEST)) == 1) by (compute_only);assert((b(one_test(m4_out()) && only_test(m4_out()).processor == UNITTEST)) == 1); // m4.unittest.tag
assert((b(one_test(m4_out()) && only_test(m4_out()).name == seq![65, 64int])) == 1) by (compute_only);assert((b(one_test(m4_out()) && only_test(m4_out()).name == seq![65, 64int])) == 1); // m4.unittest.name
assert((b(one_test(m4_out()) && only_test(m4_out()).package == seq![402int])) == 1) by (compute_only);assert((b(one_test(m4_out()) && only_test(m4_out()).package == seq![402int])) == 1); // m4.unittest.package
assert((b(one_test(m4_out()) && only_test(m4_out()).phase == PHASE_TEST)) == 1) by (compute_only);assert((b(one_test(m4_out()) && only_test(m4_out()).phase == PHASE_TEST)) == 1); // m4.unittest.phase
assert((b(load_ok(m4_out().resources, m4()) && has_test(m4_out().tests, ut(seq![65, 64], seq![402])))) == 1) by (compute_only);assert((b(load_ok(m4_out().resources, m4()) && has_test(m4_out().tests, ut(seq![65, 64], seq![402])))) == 1); // m4.loaded.unittest
assert((b(has_test(m1_out().tests, ut(seq![64], seq![211])))) == 1) by (compute_only);assert((b(has_test(m1_out().tests, ut(seq![64], seq![211])))) == 1); // m1.unittest
assert((b(one_test(m1_out()) && only_test(m1_out()).processor == UNITTEST)) == 1) by (compute_only);assert((b(one_test(m1_out()) && only_test(m1_out()).processor == UNITTEST)) == 1); // m1.unittest.tag
assert((b(one_test(m1_out()) && only_test(m1_out()).name == seq![64int])) == 1) by (compute_only);assert((b(one_test(m1_out()) && only_test(m1_out()).name == seq![64int])) == 1); // m1.unittest.name
assert((b(one_test(m1_out()) && only_test(m1_out()).package == seq![211int])) == 1) by (compute_only);assert((b(one_test(m1_out()) && only_test(m1_out()).package == seq![211int])) == 1); // m1.unittest.package
assert((b(one_test(m1_out()) && only_test(m1_out()).phase == PHASE_TEST)) == 1) by (compute_only);assert((b(one_test(m1_out()) && only_test(m1_out()).phase == PHASE_TEST)) == 1); // m1.unittest.phase
assert((b(load_ok(m1_out().resources, m1()) && has_test(m1_out().tests, ut(seq![64], seq![211])))) == 1) by (compute_only);assert((b(load_ok(m1_out().resources, m1()) && has_test(m1_out().tests, ut(seq![64], seq![211])))) == 1); // m1.loaded.unittest
assert((b(exported(seq![901, 27, 28], MAC).path == seq![2int])) == 1) by (compute_only);assert((b(exported(seq![901, 27, 28], MAC).path == seq![2int])) == 1); // x1.file.Ixa.R.Rtn.flat
assert((b(has_entry(xout(x1(), 901).resources, Entry { name: seq![901, 27, 28], ext: MAC, dir: Seq::<int>::empty() }))) == 1) by (compute_only);assert((b(has_entry(xout(x1(), 901).resources, Entry { name: seq![901, 27, 28], ext: MAC, dir: Seq::<int>::empty() }))) == 1); // x1.Ixa.R.Rtn.MAC
assert((b(m8_disk().backups == seq![1int])) == 1) by (compute_only);assert((b(m8_disk().backups == seq![1int])) == 1); // m8.backup.old
assert((b(has_entry(m1_out().resources, Entry { name: seq![201, 12], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![201, 12], ext: PKG, dir: seq![1] }))) == 1); // m1.Ia1.B.PKG@cls
assert((b(has_name(m1_out().resources, seq![201], PKG))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![201], PKG))) == 0); // m1.Ia1.PKG
assert((b(has_entry(m1_out().resources, Entry { name: seq![202], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![202], ext: PKG, dir: seq![1] }))) == 1); // m1.Ia2.PKG@cls
assert((b(has_name(m1_out().resources, seq![202, 12], PKG))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![202, 12], PKG))) == 0); // m1.Ia2.B.PKG
assert((b(has_entry(m1_out().resources, Entry { name: seq![203, 12], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![203, 12], ext: PKG, dir: seq![1] }))) == 1); // m1.Ia3.B.PKG@cls
assert((b(has_name(m1_out().resources, seq![203], PKG))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![203], PKG))) == 0); // m1.Ia3.PKG
assert((b(has_name(m1_out().resources, seq![203, 12, 13], CLS))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![203, 12, 13], CLS))) == 0); // m1.Ia3.B.C.cls
assert((b(has_entry(m1_out().resources, Entry { name: seq![204], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![204], ext: PKG, dir: seq![1] }))) == 1); // m1.Ia4.PKG@cls
assert((b(has_entry(m1_out().resources, Entry { name: seq![205], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![205], ext: PKG, dir: seq![1] }))) == 1); // m1.Ia5.PKG@cls
assert((b(has_entry(m1_out().resources, Entry { name: seq![206], ext: MAC, dir: seq![2] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![206], ext: MAC, dir: seq![2] }))) == 1); // m1.Ir69.MAC@mac
assert((b(has_entry(m1_out().resources, Entry { name: seq![207], ext: INC, dir: seq![3] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![207], ext: INC, dir: seq![3] }))) == 1); // m1.Ii69.INC@inc
assert((b(has_name(m1_out().resources, seq![208], TXT))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![208], TXT))) == 0); // m1.notes
assert((b(has_name(m1_out().resources, seq![209], PKG) || has_name(m1_out().resources, seq![209, 26], CLS))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![209], PKG) || has_name(m1_out().resources, seq![209, 26], CLS))) == 0); // m1.Ij1
assert((m1_out().tests.len() as int) == 1) by (compute_only);assert((m1_out().tests.len() as int) == 1); // m1.unittest.count
assert((b(has_name(m1_out().resources, seq![212], MAC) || has_test_package(m1_out().tests, seq![212]))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![212], MAC) || has_test_package(m1_out().tests, seq![212]))) == 0); // m1.Ih69
assert((b(has_name(m1_out().resources, seq![213], CLS) || has_test_package(m1_out().tests, seq![213]))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![213], CLS) || has_test_package(m1_out().tests, seq![213]))) == 0); // m1.Inopkg69
assert((m1_out().resources.len() as int) == 7) by (compute_only);assert((m1_out().resources.len() as int) == 7); // m1.resource.count
assert((b(has_entry(m3_out().resources, Entry { name: seq![301, 12], ext: PKG, dir: seq![61] }))) == 0) by (compute_only);assert((b(has_entry(m3_out().resources, Entry { name: seq![301, 12], ext: PKG, dir: seq![61] }))) == 0); // m3.Id1.B.PKG@x
assert((count_name(m3_out().resources, seq![301, 12], PKG)) == 1) by (compute_only);assert((count_name(m3_out().resources, seq![301, 12], PKG)) == 1); // m3.Id1.B.PKG.count
assert((b(has_entry(m3_out().resources, Entry { name: seq![302, 12], ext: PKG, dir: seq![61] }))) == 1) by (compute_only);assert((b(has_entry(m3_out().resources, Entry { name: seq![302, 12], ext: PKG, dir: seq![61] }))) == 1); // m3.Id2.B.PKG@x
assert((b(has_entry(m3_out().resources, Entry { name: seq![302, 14], ext: PKG, dir: seq![62] }))) == 1) by (compute_only);assert((b(has_entry(m3_out().resources, Entry { name: seq![302, 14], ext: PKG, dir: seq![62] }))) == 1); // m3.Id2.D.PKG@y
assert((b(has_name(m3_out().resources, seq![302], PKG))) == 0) by (compute_only);assert((b(has_name(m3_out().resources, seq![302], PKG))) == 0); // m3.Id2.PKG
assert((m3_out().tests.len() as int) == 0) by (compute_only);assert((m3_out().tests.len() as int) == 0); // m3.unittest.count
assert((b(has_entry(m4_out().resources, Entry { name: seq![401], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m4_out().resources, Entry { name: seq![401], ext: PKG, dir: seq![1] }))) == 1); // m4.Ie1.PKG@cls
assert((b(has_name(m4_out().resources, seq![402], PKG) || has_name(m4_out().resources, seq![402, 18], CLS))) == 0) by (compute_only);assert((b(has_name(m4_out().resources, seq![402], PKG) || has_name(m4_out().resources, seq![402, 18], CLS))) == 0); // m4.Ie1T.resource
assert((b(has_name(m4_out().resources, seq![403], MAC) || has_test_package(m4_out().tests, seq![403]))) == 0) by (compute_only);assert((b(has_name(m4_out().resources, seq![403], MAC) || has_test_package(m4_out().tests, seq![403]))) == 0); // m4.Ihelp69
assert((b(has_name(m4_out().resources, seq![404], CLS) || has_test_package(m4_out().tests, seq![404]))) == 0) by (compute_only);assert((b(has_name(m4_out().resources, seq![404], CLS) || has_test_package(m4_out().tests, seq![404]))) == 0); // m4.Inopkg69b
assert((m4_out().resources.len() as int) == 1) by (compute_only);assert((m4_out().resources.len() as int) == 1); // m4.resource.count
assert((b(has_entry(m2a_out().resources, Entry { name: seq![501], ext: CLS, dir: Seq::<int>::empty() }))) == 1) by (compute_only);assert((b(has_entry(m2a_out().resources, Entry { name: seq![501], ext: CLS, dir: Seq::<int>::empty() }))) == 1); // m2a.Ir2a.cls
assert((b(has_entry(m2a_out().resources, Entry { name: seq![502, 21], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m2a_out().resources, Entry { name: seq![502, 21], ext: PKG, dir: seq![1] }))) == 1); // m2a.Ib2a.P.PKG@cls
assert((b(has_entry(m2e_out().resources, Entry { name: seq![504], ext: CLS, dir: Seq::<int>::empty() }))) == 1) by (compute_only);assert((b(has_entry(m2e_out().resources, Entry { name: seq![504], ext: CLS, dir: Seq::<int>::empty() }))) == 1); // m2e.Ir2e.cls
assert((b(has_entry(m2b_out().resources, Entry { name: seq![503, 21], ext: PKG, dir: Seq::<int>::empty() }))) == 1) by (compute_only);assert((b(has_entry(m2b_out().resources, Entry { name: seq![503, 21], ext: PKG, dir: Seq::<int>::empty() }))) == 1); // m2b.Ik2b.P.PKG
assert((b(has_entry(m5_out().resources, Entry { name: seq![601], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m5_out().resources, Entry { name: seq![601], ext: PKG, dir: seq![1] }))) == 1); // m5.Ig1.PKG@cls
assert((b(has_entry(m5_out().resources, Entry { name: seq![602], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(m5_out().resources, Entry { name: seq![602], ext: PKG, dir: seq![1] }))) == 1); // m5.Ig2.PKG@cls
assert((b(has_name(m5_out().resources, seq![603], PKG) || has_name(m5_out().resources, seq![603, 26], CLS))) == 0) by (compute_only);assert((b(has_name(m5_out().resources, seq![603], PKG) || has_name(m5_out().resources, seq![603, 26], CLS))) == 0); // m5.Ig3
assert((b(generate(false, src(), Seq::empty(), none_keys(), Some(seq![64]), true, Seq::empty(), none_keys()).is_some())) == 0) by (compute_only);assert((b(generate(false, src(), Seq::empty(), none_keys(), Some(seq![64]), true, Seq::empty(), none_keys()).is_some())) == 0); // m6.xml.exists
assert((b(generate(false, src(), Seq::empty(), none_keys(), Some(seq![64]), true, Seq::empty(), none_keys()).is_none())) == 1) by (compute_only);assert((b(generate(false, src(), Seq::empty(), none_keys(), Some(seq![64]), true, Seq::empty(), none_keys()).is_none())) == 1); // m6.error
assert((b(load_ok(m1_out().resources, m1()))) == 1) by (compute_only);assert((b(load_ok(m1_out().resources, m1()))) == 1); // m1.load.ok
assert((b(load_ok(m3_out().resources, m3()))) == 1) by (compute_only);assert((b(load_ok(m3_out().resources, m3()))) == 1); // m3.load.ok
assert((b(load_ok(m4_out().resources, m4()))) == 1) by (compute_only);assert((b(load_ok(m4_out().resources, m4()))) == 1); // m4.load.ok
assert((b(load_ok(m2e_out().resources, m2e()))) == 1) by (compute_only);assert((b(load_ok(m2e_out().resources, m2e()))) == 1); // m2e.load.ok
assert((b(loads_class(m1_out().resources, m1(), m1()[0]))) == 1) by (compute_only);assert((b(loads_class(m1_out().resources, m1(), m1()[0]))) == 1); // class.Ia1.B.C
assert((b(loads_class(m1_out().resources, m1(), m1()[3]))) == 1) by (compute_only);assert((b(loads_class(m1_out().resources, m1(), m1()[3]))) == 1); // class.Ia2.D.E
assert((b(loads_class(m1_out().resources, m1(), m1()[4]))) == 1) by (compute_only);assert((b(loads_class(m1_out().resources, m1(), m1()[4]))) == 1); // class.Ia3.B.C
assert((b(loads_class(m1_out().resources, m1(), m1()[6]))) == 1) by (compute_only);assert((b(loads_class(m1_out().resources, m1(), m1()[6]))) == 1); // class.Ia5.Y
assert((b(loads_class(m3_out().resources, m3(), m3()[2]))) == 1) by (compute_only);assert((b(loads_class(m3_out().resources, m3(), m3()[2]))) == 1); // class.Id1.B.E
assert((b(loads_class(m3_out().resources, m3(), m3()[1]))) == 1) by (compute_only);assert((b(loads_class(m3_out().resources, m3(), m3()[1]))) == 1); // class.Id2.B.C
assert((b(loads_class(m3_out().resources, m3(), m3()[3]))) == 1) by (compute_only);assert((b(loads_class(m3_out().resources, m3(), m3()[3]))) == 1); // class.Id2.D.E
assert((b(loads_class(m4_out().resources, m4(), m4()[0]))) == 1) by (compute_only);assert((b(loads_class(m4_out().resources, m4(), m4()[0]))) == 1); // class.Ie1.A
assert((b(loads_class(m2a_out().resources, m2a(), m2a()[0]))) == 0) by (compute_only);assert((b(loads_class(m2a_out().resources, m2a(), m2a()[0]))) == 0); // class.Ir2a
assert((b(loads_class(m2a_out().resources, m2a(), m2a()[1]))) == 0) by (compute_only);assert((b(loads_class(m2a_out().resources, m2a(), m2a()[1]))) == 0); // class.Ib2a.P.Q
assert((b(loads_class(m2e_out().resources, m2e(), m2e()[0]))) == 1) by (compute_only);assert((b(loads_class(m2e_out().resources, m2e(), m2e()[0]))) == 1); // class.Ir2e
assert((b(loads_class(m2b_out().resources, m2b(), m2b()[0]))) == 0) by (compute_only);assert((b(loads_class(m2b_out().resources, m2b(), m2b()[0]))) == 0); // class.Ik2b.P.Q
assert((b(has_entry(m7_out().resources, Entry { name: seq![701], ext: PKG, dir: seq![66, 67] }))) == 0) by (compute_only);assert((b(has_entry(m7_out().resources, Entry { name: seq![701], ext: PKG, dir: seq![66, 67] }))) == 0); // m7.Im7.PKG@a/b
assert((count_name(m7_out().resources, seq![701], PKG)) == 1) by (compute_only);assert((count_name(m7_out().resources, seq![701], PKG)) == 1); // m7.Im7.PKG.count
assert((b(load_ok(m7_out().resources, m7()))) == 1) by (compute_only);assert((b(load_ok(m7_out().resources, m7()))) == 1); // m7.load.ok
assert((b(loads_class(m7_out().resources, m7(), m7()[0]))) == 1) by (compute_only);assert((b(loads_class(m7_out().resources, m7(), m7()[0]))) == 1); // class.Im7.X
assert((b(has_entry(m8_out().resources, Entry { name: seq![801], ext: PKG, dir: seq![66] }))) == 0) by (compute_only);assert((b(has_entry(m8_out().resources, Entry { name: seq![801], ext: PKG, dir: seq![66] }))) == 0); // m8.Im8.PKG@a
assert((count_name(m8_out().resources, seq![801], PKG)) == 1) by (compute_only);assert((count_name(m8_out().resources, seq![801], PKG)) == 1); // m8.Im8.PKG.count
assert((b(load_ok(m8_out().resources, m8()))) == 1) by (compute_only);assert((b(load_ok(m8_out().resources, m8()))) == 1); // m8.load.ok
assert((b(loads_class(m8_out().resources, m8(), m8()[0]))) == 0) by (compute_only);assert((b(loads_class(m8_out().resources, m8(), m8()[0]))) == 0); // class.Im8.X
assert((b(loads_class(m8_out().resources, m8(), m8()[1]))) == 1) by (compute_only);assert((b(loads_class(m8_out().resources, m8(), m8()[1]))) == 1); // class.Im8.Y
assert((m8_disk().backups.len() as int) == 1) by (compute_only);assert((m8_disk().backups.len() as int) == 1); // m8.backup.count
assert((b(m8_disk().module_xml == Some(1int))) == 0) by (compute_only);assert((b(m8_disk().module_xml == Some(1int))) == 0); // m8.xml.old
assert((m7_disk().backups.len() as int) == 0) by (compute_only);assert((m7_disk().backups.len() as int) == 0); // m7.backup.count
assert((b(m7_disk().backups == seq![1int])) == 0) by (compute_only);assert((b(m7_disk().backups == seq![1int])) == 0); // m7.backup.old
assert((b(m7_disk().module_xml == Some(1int))) == 0) by (compute_only);assert((b(m7_disk().module_xml == Some(1int))) == 0); // m7.xml.old
assert((b(exported(seq![901, 21, 22], CLS).path == seq![1int, 901, 21])) == 1) by (compute_only);assert((b(exported(seq![901, 21, 22], CLS).path == seq![1int, 901, 21])) == 1); // x1.file.Ixa.P.Q
assert((b(exported(seq![901, 27, 28], MAC).path == seq![2int, 901, 27])) == 0) by (compute_only);assert((b(exported(seq![901, 27, 28], MAC).path == seq![2int, 901, 27])) == 0); // x1.file.Ixa.R.Rtn.nested
assert((b(has_entry(xout(x1(), 901).resources, Entry { name: seq![901, 21], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(xout(x1(), 901).resources, Entry { name: seq![901, 21], ext: PKG, dir: seq![1] }))) == 1); // x1.Ixa.P.PKG@cls
assert((xout(x1(), 901).resources.len() as int) == 2) by (compute_only);assert((xout(x1(), 901).resources.len() as int) == 2); // x1.resource.count
assert((b(loads_class(xout(x1(), 901).resources, x1(), x1()[0]))) == 0) by (compute_only);assert((b(loads_class(xout(x1(), 901).resources, x1(), x1()[0]))) == 0); // after.Ixa.P.Q
assert((b(loads_class(xout(x1(), 901).resources, x1(), x1()[1]))) == 0) by (compute_only);assert((b(loads_class(xout(x1(), 901).resources, x1(), x1()[1]))) == 0); // after.rtn.Ixa.R.Rtn.MAC
assert((b(exported(seq![902, 21, 22], CLS).path == seq![1int, 902, 21])) == 1) by (compute_only);assert((b(exported(seq![902, 21, 22], CLS).path == seq![1int, 902, 21])) == 1); // x2.file.Ixb.P.Q
assert((b(exported(seq![905], MAC).path == seq![2int])) == 1) by (compute_only);assert((b(exported(seq![905], MAC).path == seq![2int])) == 1); // x2.file.IxbRtn
assert((b(has_entry(xout(x2(), 902).resources, Entry { name: seq![902, 21], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(xout(x2(), 902).resources, Entry { name: seq![902, 21], ext: PKG, dir: seq![1] }))) == 1); // x2.Ixb.P.PKG@cls
assert((b(has_entry(xout(x2(), 902).resources, Entry { name: seq![905], ext: MAC, dir: seq![2] }))) == 1) by (compute_only);assert((b(has_entry(xout(x2(), 902).resources, Entry { name: seq![905], ext: MAC, dir: seq![2] }))) == 1); // x2.IxbRtn.MAC@mac
assert((xout(x2(), 902).resources.len() as int) == 2) by (compute_only);assert((xout(x2(), 902).resources.len() as int) == 2); // x2.resource.count
assert((b(loads_class(xout(x2(), 902).resources, x2(), x2()[0]))) == 1) by (compute_only);assert((b(loads_class(xout(x2(), 902).resources, x2(), x2()[0]))) == 1); // after.Ixb.P.Q
assert((b(loads_class(xout(x2(), 902).resources, x2(), x2()[1]))) == 1) by (compute_only);assert((b(loads_class(xout(x2(), 902).resources, x2(), x2()[1]))) == 1); // after.rtn.IxbRtn.MAC
assert((b(exported(seq![903, 21, 22], CLS).path == seq![1int, 903, 21])) == 1) by (compute_only);assert((b(exported(seq![903, 21, 22], CLS).path == seq![1int, 903, 21])) == 1); // x3.file.Ixc.P.Q
assert((b(exported(seq![906], INC).path == seq![3int])) == 1) by (compute_only);assert((b(exported(seq![906], INC).path == seq![3int])) == 1); // x3.file.IxcInc
assert((b(has_entry(xout(x3(), 903).resources, Entry { name: seq![903, 21], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(xout(x3(), 903).resources, Entry { name: seq![903, 21], ext: PKG, dir: seq![1] }))) == 1); // x3.Ixc.P.PKG@cls
assert((b(has_entry(xout(x3(), 903).resources, Entry { name: seq![906], ext: INC, dir: seq![3] }))) == 1) by (compute_only);assert((b(has_entry(xout(x3(), 903).resources, Entry { name: seq![906], ext: INC, dir: seq![3] }))) == 1); // x3.IxcInc.INC@inc
assert((xout(x3(), 903).resources.len() as int) == 2) by (compute_only);assert((xout(x3(), 903).resources.len() as int) == 2); // x3.resource.count
assert((b(load_ok(xout(x3(), 903).resources, x3()))) == 1) by (compute_only);assert((b(load_ok(xout(x3(), 903).resources, x3()))) == 1); // x3.load.ok
assert((b(loads_class(xout(x3(), 903).resources, x3(), x3()[0]))) == 1) by (compute_only);assert((b(loads_class(xout(x3(), 903).resources, x3(), x3()[0]))) == 1); // after.Ixc.P.Q
assert((b(loads_class(xout(x3(), 903).resources, x3(), x3()[1]))) == 1) by (compute_only);assert((b(loads_class(xout(x3(), 903).resources, x3(), x3()[1]))) == 1); // after.rtn.IxcInc.INC
assert((b(exported(seq![904, 21, 22], CLS).path == seq![1int, 904, 21])) == 1) by (compute_only);assert((b(exported(seq![904, 21, 22], CLS).path == seq![1int, 904, 21])) == 1); // x4.file.Ixd.P.Q
assert((b(exported(seq![904, 30, 29], INC).path == seq![3int])) == 1) by (compute_only);assert((b(exported(seq![904, 30, 29], INC).path == seq![3int])) == 1); // x4.file.Ixd.I.Inc.flat
assert((b(exported(seq![904, 30, 29], INC).path == seq![3int, 904, 30])) == 0) by (compute_only);assert((b(exported(seq![904, 30, 29], INC).path == seq![3int, 904, 30])) == 0); // x4.file.Ixd.I.Inc.nested
assert((b(has_entry(xout(x4(), 904).resources, Entry { name: seq![904, 21], ext: PKG, dir: seq![1] }))) == 1) by (compute_only);assert((b(has_entry(xout(x4(), 904).resources, Entry { name: seq![904, 21], ext: PKG, dir: seq![1] }))) == 1); // x4.Ixd.P.PKG@cls
assert((b(has_entry(xout(x4(), 904).resources, Entry { name: seq![904, 30, 29], ext: INC, dir: Seq::<int>::empty() }))) == 1) by (compute_only);assert((b(has_entry(xout(x4(), 904).resources, Entry { name: seq![904, 30, 29], ext: INC, dir: Seq::<int>::empty() }))) == 1); // x4.Ixd.I.Inc.INC
assert((xout(x4(), 904).resources.len() as int) == 2) by (compute_only);assert((xout(x4(), 904).resources.len() as int) == 2); // x4.resource.count
assert((b(loads_class(xout(x4(), 904).resources, x4(), x4()[0]))) == 0) by (compute_only);assert((b(loads_class(xout(x4(), 904).resources, x4(), x4()[0]))) == 0); // after.Ixd.P.Q
assert((b(loads_class(xout(x4(), 904).resources, x4(), x4()[1]))) == 0) by (compute_only);assert((b(loads_class(xout(x4(), 904).resources, x4(), x4()[1]))) == 0); // after.rtn.Ixd.I.Inc.INC
}
}
fn main() {}
