#[path="../../../../Verus_verification/coverage-20261117/iter69/grouping.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

assert((b(has_entry(m1_out().resources, pkg(seq![201, 12], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, pkg(seq![201, 12], seq![1])))) == 1); // m1.Ia1.B.PKG@cls
assert((b(has_entry(m3_out().resources, pkg(seq![301, 12], seq![61])))) == 0) by (compute_only);assert((b(has_entry(m3_out().resources, pkg(seq![301, 12], seq![61])))) == 0); // m3.Id1.B.PKG@x
assert((b(loads_class(m3_out().resources, m3(), m3()[0]))) == 0) by (compute_only);assert((b(loads_class(m3_out().resources, m3(), m3()[0]))) == 0); // class.Id1.B.C
assert((b(has_name(m1_out().resources, seq![201], PKG))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![201], PKG))) == 0); // m1.Ia1.PKG
assert((b(has_entry(m1_out().resources, pkg(seq![202], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, pkg(seq![202], seq![1])))) == 1); // m1.Ia2.PKG@cls
assert((b(has_name(m1_out().resources, seq![202, 12], PKG))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![202, 12], PKG))) == 0); // m1.Ia2.B.PKG
assert((b(has_entry(m1_out().resources, pkg(seq![203, 12], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, pkg(seq![203, 12], seq![1])))) == 1); // m1.Ia3.B.PKG@cls
assert((b(has_name(m1_out().resources, seq![203], PKG))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![203], PKG))) == 0); // m1.Ia3.PKG
assert((b(has_name(m1_out().resources, seq![203, 12, 13], CLS))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![203, 12, 13], CLS))) == 0); // m1.Ia3.B.C.cls
assert((b(has_entry(m1_out().resources, pkg(seq![204], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, pkg(seq![204], seq![1])))) == 1); // m1.Ia4.PKG@cls
assert((b(has_entry(m1_out().resources, pkg(seq![205], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, pkg(seq![205], seq![1])))) == 1); // m1.Ia5.PKG@cls
assert((b(has_entry(m1_out().resources, Entry { name: seq![206], ext: MAC, dir: seq![2] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![206], ext: MAC, dir: seq![2] }))) == 1); // m1.Ir69.MAC@mac
assert((b(has_entry(m1_out().resources, Entry { name: seq![207], ext: INC, dir: seq![3] }))) == 1) by (compute_only);assert((b(has_entry(m1_out().resources, Entry { name: seq![207], ext: INC, dir: seq![3] }))) == 1); // m1.Ii69.INC@inc
assert((b(has_name(m1_out().resources, seq![208], TXT))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![208], TXT))) == 0); // m1.notes
assert((b(has_name(m1_out().resources, seq![209], PKG) || has_name(m1_out().resources, seq![209, 26], CLS))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![209], PKG) || has_name(m1_out().resources, seq![209, 26], CLS))) == 0); // m1.Ij1
assert((b(has_test(m1_out().tests, TestEntry { dir: Seq::empty(), package: seq![211] }))) == 1) by (compute_only);assert((b(has_test(m1_out().tests, TestEntry { dir: Seq::empty(), package: seq![211] }))) == 1); // m1.unittest
assert((m1_out().tests.len() as int) == 1) by (compute_only);assert((m1_out().tests.len() as int) == 1); // m1.unittest.count
assert((b(has_name(m1_out().resources, seq![212], MAC) || has_test(m1_out().tests, TestEntry { dir: Seq::empty(), package: seq![212] }))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![212], MAC) || has_test(m1_out().tests, TestEntry { dir: Seq::empty(), package: seq![212] }))) == 0); // m1.Ih69
assert((b(has_name(m1_out().resources, seq![213], CLS) || has_test(m1_out().tests, TestEntry { dir: Seq::empty(), package: seq![213] }))) == 0) by (compute_only);assert((b(has_name(m1_out().resources, seq![213], CLS) || has_test(m1_out().tests, TestEntry { dir: Seq::empty(), package: seq![213] }))) == 0); // m1.Inopkg69
assert((m1_out().resources.len() as int) == 7) by (compute_only);assert((m1_out().resources.len() as int) == 7); // m1.resource.count
assert((b(has_entry(m3_out().resources, pkg(seq![301, 12], seq![62])))) == 1) by (compute_only);assert((b(has_entry(m3_out().resources, pkg(seq![301, 12], seq![62])))) == 1); // m3.Id1.B.PKG@y
assert((count_name(m3_out().resources, seq![301, 12], PKG)) == 1) by (compute_only);assert((count_name(m3_out().resources, seq![301, 12], PKG)) == 1); // m3.Id1.B.PKG.count
assert((b(has_entry(m3_out().resources, pkg(seq![302, 12], seq![61])))) == 1) by (compute_only);assert((b(has_entry(m3_out().resources, pkg(seq![302, 12], seq![61])))) == 1); // m3.Id2.B.PKG@x
assert((b(has_entry(m3_out().resources, pkg(seq![302, 14], seq![62])))) == 1) by (compute_only);assert((b(has_entry(m3_out().resources, pkg(seq![302, 14], seq![62])))) == 1); // m3.Id2.D.PKG@y
assert((b(has_name(m3_out().resources, seq![302], PKG))) == 0) by (compute_only);assert((b(has_name(m3_out().resources, seq![302], PKG))) == 0); // m3.Id2.PKG
assert((m3_out().tests.len() as int) == 0) by (compute_only);assert((m3_out().tests.len() as int) == 0); // m3.unittest.count
assert((b(has_entry(m4_out().resources, pkg(seq![401], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m4_out().resources, pkg(seq![401], seq![1])))) == 1); // m4.Ie1.PKG@cls
assert((b(has_test(m4_out().tests, TestEntry { dir: Seq::empty(), package: seq![402] }))) == 1) by (compute_only);assert((b(has_test(m4_out().tests, TestEntry { dir: Seq::empty(), package: seq![402] }))) == 1); // m4.unittest
assert((b(has_name(m4_out().resources, seq![402], PKG) || has_name(m4_out().resources, seq![402, 18], CLS))) == 0) by (compute_only);assert((b(has_name(m4_out().resources, seq![402], PKG) || has_name(m4_out().resources, seq![402, 18], CLS))) == 0); // m4.Ie1T.resource
assert((b(has_name(m4_out().resources, seq![403], MAC) || has_test(m4_out().tests, TestEntry { dir: Seq::empty(), package: seq![403] }))) == 0) by (compute_only);assert((b(has_name(m4_out().resources, seq![403], MAC) || has_test(m4_out().tests, TestEntry { dir: Seq::empty(), package: seq![403] }))) == 0); // m4.Ihelp69
assert((b(has_name(m4_out().resources, seq![404], CLS) || has_test(m4_out().tests, TestEntry { dir: Seq::empty(), package: seq![404] }))) == 0) by (compute_only);assert((b(has_name(m4_out().resources, seq![404], CLS) || has_test(m4_out().tests, TestEntry { dir: Seq::empty(), package: seq![404] }))) == 0); // m4.Inopkg69b
assert((m4_out().resources.len() as int) == 1) by (compute_only);assert((m4_out().resources.len() as int) == 1); // m4.resource.count
assert((b(has_entry(m2a_out().resources, Entry { name: seq![501], ext: CLS, dir: Seq::empty() }))) == 1) by (compute_only);assert((b(has_entry(m2a_out().resources, Entry { name: seq![501], ext: CLS, dir: Seq::empty() }))) == 1); // m2a.Ir2a.cls
assert((b(has_entry(m2a_out().resources, pkg(seq![502, 21], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m2a_out().resources, pkg(seq![502, 21], seq![1])))) == 1); // m2a.Ib2a.P.PKG@cls
assert((b(has_entry(m2e_out().resources, Entry { name: seq![504], ext: CLS, dir: Seq::empty() }))) == 1) by (compute_only);assert((b(has_entry(m2e_out().resources, Entry { name: seq![504], ext: CLS, dir: Seq::empty() }))) == 1); // m2e.Ir2e.cls
assert((b(has_entry(m2b_out().resources, pkg(seq![503, 21], Seq::empty())))) == 1) by (compute_only);assert((b(has_entry(m2b_out().resources, pkg(seq![503, 21], Seq::empty())))) == 1); // m2b.Ik2b.P.PKG
assert((b(has_entry(m5_out().resources, pkg(seq![601], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m5_out().resources, pkg(seq![601], seq![1])))) == 1); // m5.Ig1.PKG@cls
assert((b(has_entry(m5_out().resources, pkg(seq![602], seq![1])))) == 1) by (compute_only);assert((b(has_entry(m5_out().resources, pkg(seq![602], seq![1])))) == 1); // m5.Ig2.PKG@cls
assert((b(has_name(m5_out().resources, seq![603], PKG) || has_name(m5_out().resources, seq![603, 26], CLS))) == 0) by (compute_only);assert((b(has_name(m5_out().resources, seq![603], PKG) || has_name(m5_out().resources, seq![603, 26], CLS))) == 0); // m5.Ig3
assert((b(generate(false, Seq::empty(), Seq::empty(), true, Seq::empty(), Seq::empty()).is_some())) == 0) by (compute_only);assert((b(generate(false, Seq::empty(), Seq::empty(), true, Seq::empty(), Seq::empty()).is_some())) == 0); // m6.xml.exists
assert((b(generate(false, Seq::empty(), Seq::empty(), true, Seq::empty(), Seq::empty()).is_none())) == 1) by (compute_only);assert((b(generate(false, Seq::empty(), Seq::empty(), true, Seq::empty(), Seq::empty()).is_none())) == 1); // m6.error
assert((b(load_ok(m1_out().resources, m1()))) == 1) by (compute_only);assert((b(load_ok(m1_out().resources, m1()))) == 1); // m1.load.ok
assert((b(load_ok(m3_out().resources, m3()))) == 1) by (compute_only);assert((b(load_ok(m3_out().resources, m3()))) == 1); // m3.load.ok
assert((b(load_ok(m4_out().resources, m4()))) == 1) by (compute_only);assert((b(load_ok(m4_out().resources, m4()))) == 1); // m4.load.ok
assert((b(load_ok(m2a_out().resources, m2a()))) == 0) by (compute_only);assert((b(load_ok(m2a_out().resources, m2a()))) == 0); // m2a.load.ok
assert((b(load_ok(m2e_out().resources, m2e()))) == 1) by (compute_only);assert((b(load_ok(m2e_out().resources, m2e()))) == 1); // m2e.load.ok
assert((b(load_ok(m2b_out().resources, m2b()))) == 0) by (compute_only);assert((b(load_ok(m2b_out().resources, m2b()))) == 0); // m2b.load.ok
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
}
}
fn main() {}
