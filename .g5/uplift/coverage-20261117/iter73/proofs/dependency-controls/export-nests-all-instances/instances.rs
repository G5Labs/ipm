// Iteration73 instances: neighbouring source trees and literal transcriptions of the native fixture trees (native/),
// evaluated against the rules in grouping.rs. Native replay (replay/native.rs) imports this file.
#[path = "grouping.rs"]
mod grouping;
pub use grouping::*;
use vstd::prelude::*;

verus! {

// Segment identifiers. Directories: cls 1 (= CLS), mac 2 (= MAC), inc 3 (= INC), x 61, y 62, flat 63, tests 64,
// src 65, a 66, b 67, ab 68. Names: B 12, C 13, D 14, E 15, X 16, Y 17, U 18, V 19, W 20, P 21, Q 22, A 23,
// Good 24, Bad 25, Junk 26, R 27, Rtn 28, Inc 29, I 30. Top-level packages and package-less items: 101.. (neighbours),
// Ia1..Ia5 201..205, Ir69 206, Ii69 207, notes 208, Ij1 209, It1 211, Ih69 212, Inopkg69 213, Id1 301, Id2 302,
// Ie1 401, Ie1T 402, Ihelp69 403, Inopkg69b 404, Ir2a 501, Ib2a 502, Ik2b 503, Ir2e 504, Ig1..Ig3 601..603,
// Im7 701, Im8 801, Ixa..Ixd 901..904, IxbRtn 905, IxcInc 906.
pub open spec fn f(path: Seq<int>, name: Seq<int>, ext: int, listed: bool) -> File { File { path, name, ext, listed } }
pub open spec fn cls(path: Seq<int>, name: Seq<int>) -> File { f(path, name, CLS, true) }
pub open spec fn pkg(name: Seq<int>, dir: Seq<int>) -> Entry { Entry { name, ext: PKG, dir } }
pub open spec fn src() -> Seq<int> { seq![65] }
// Source listing with the tests folder answered as `tests` (outside src) or `src/tests` (inside).
pub open spec fn ex_out() -> Excl { Excl { base: src(), tests: Some(seq![64]) } }
pub open spec fn ex_in() -> Excl { Excl { base: src(), tests: Some(seq![65, 64]) } }
pub open spec fn none_keys() -> Seq<(Seq<int>, int)> { Seq::empty() }

pub open spec fn ut(name: Seq<int>, package: Seq<int>) -> TestEntry { TestEntry { name, processor: UNITTEST, package, phase: PHASE_TEST } }
pub open spec fn has_test_package(ts: Seq<TestEntry>, p: Seq<int>) -> bool
    decreases ts.len()
{
    if ts.len() == 0 { false } else { ts[ts.len() - 1].package == p || has_test_package(ts.subrange(0, ts.len() - 1), p) }
}
// The single UnitTest entry of a manifest, when there is exactly one.
pub open spec fn only_test(m: Manifest) -> TestEntry { m.tests[0] }
pub open spec fn one_test(m: Manifest) -> bool { m.tests.len() == 1 }

// ---------------------------------------------------------------------------------------------------
// Neighbours (not native).
// N1: A.B.C + A.B.D in one directory -> A.B.PKG. N2: A.X + E.Y -> A.PKG and E.PKG. N3: A.B.C alone -> A.B.PKG.
// N4: A.B.C + A.D.E -> A.PKG. N5: the same package in x and y. N5b: A.B in x, A.D in y.
pub open spec fn n1() -> Seq<File> { seq![cls(seq![1, 101, 2], seq![101, 2, 3]), cls(seq![1, 101, 2], seq![101, 2, 4])] }
pub open spec fn n2() -> Seq<File> { seq![cls(seq![1, 104], seq![104, 7]), cls(seq![1, 105], seq![105, 8])] }
pub open spec fn n3() -> Seq<File> { seq![cls(seq![1, 103, 2], seq![103, 2, 3])] }
pub open spec fn n4() -> Seq<File> { seq![cls(seq![1, 102, 2], seq![102, 2, 3]), cls(seq![1, 102, 4], seq![102, 4, 5])] }
pub open spec fn n5() -> Seq<File> { seq![cls(seq![61, 111, 2], seq![111, 2, 3]), cls(seq![62, 111, 2], seq![111, 2, 5])] }
pub open spec fn n5_keys() -> Seq<(Seq<int>, int)> { seq![(seq![61], 111), (seq![62], 111)] }
pub open spec fn n5b() -> Seq<File> { seq![cls(seq![61, 112, 2], seq![112, 2, 3]), cls(seq![62, 112, 4], seq![112, 4, 5])] }
pub open spec fn n5b_keys() -> Seq<(Seq<int>, int)> { seq![(seq![61], 112), (seq![62], 112)] }
pub open spec fn root_cls() -> File { cls(Seq::empty(), seq![121]) }
pub open spec fn flat_cls() -> File { cls(seq![63], seq![122, 2, 3]) }
pub open spec fn rtn() -> File { f(seq![2], seq![131], MAC, true) }
pub open spec fn inc() -> File { f(seq![3], seq![132], INC, true) }
pub open spec fn txt() -> File { f(Seq::empty(), seq![133], TXT, true) }
pub open spec fn unlisted() -> File { f(seq![1, 134], seq![134, 6], CLS, false) }
pub open spec fn under_tests_cls() -> File { f(seq![64, 135], seq![135, 2], CLS, true) }

pub proof fn grouping_names()
    ensures
        group(n1(), ex_out(), seq![1], 101) == Some(seq![101int, 2]),
        group(n2(), ex_out(), seq![1], 104) == Some(seq![104int]) && group(n2(), ex_out(), seq![1], 105) == Some(seq![105int]),
        group(n3(), ex_out(), seq![1], 103) == Some(seq![103int, 2]),
        group(n4(), ex_out(), seq![1], 102) == Some(seq![102int]),
        group(n5b(), ex_out(), seq![61], 112) == Some(seq![112int, 2]) && group(n5b(), ex_out(), seq![62], 112) == Some(seq![112int, 4]),
{
    assert(group(n1(), ex_out(), seq![1], 101) == Some(seq![101int, 2])) by (compute_only);
    assert(group(n2(), ex_out(), seq![1], 104) == Some(seq![104int])) by (compute_only);
    assert(group(n2(), ex_out(), seq![1], 105) == Some(seq![105int])) by (compute_only);
    assert(group(n3(), ex_out(), seq![1], 103) == Some(seq![103int, 2])) by (compute_only);
    assert(group(n4(), ex_out(), seq![1], 102) == Some(seq![102int])) by (compute_only);
    assert(group(n5b(), ex_out(), seq![61], 112) == Some(seq![112int, 2])) by (compute_only);
    assert(group(n5b(), ex_out(), seq![62], 112) == Some(seq![112int, 4])) by (compute_only);
}

pub proof fn leftovers_and_skips()
    ensures
        singles(seq![root_cls(), rtn(), inc(), txt(), unlisted(), under_tests_cls(), flat_cls()], ex_in())
            == seq![Entry { name: seq![121], ext: CLS, dir: Seq::empty() }, Entry { name: seq![131], ext: MAC, dir: seq![2] },
                    Entry { name: seq![132], ext: INC, dir: seq![3] }],
        group(seq![under_tests_cls()], ex_in(), seq![64], 135).is_none(),
        group(seq![under_tests_cls()], ex_out(), seq![64], 135).is_none() == false,
        group(seq![unlisted()], ex_out(), seq![1], 134).is_none(),
        directory(flat_cls()) == Seq::<int>::empty(),
{
    assert(singles(seq![root_cls(), rtn(), inc(), txt(), unlisted(), under_tests_cls(), flat_cls()], ex_in())
            == seq![Entry { name: seq![121], ext: CLS, dir: Seq::empty() }, Entry { name: seq![131], ext: MAC, dir: seq![2] },
                    Entry { name: seq![132], ext: INC, dir: seq![3] }]) by (compute_only);
    assert(group(seq![under_tests_cls()], ex_in(), seq![64], 135).is_none()) by (compute_only);
    assert(group(seq![under_tests_cls()], ex_out(), seq![64], 135).is_none() == false) by (compute_only);
    assert(group(seq![unlisted()], ex_out(), seq![1], 134).is_none()) by (compute_only);
    assert(directory(flat_cls()) == Seq::<int>::empty()) by (compute_only);
}

// ---------------------------------------------------------------------------------------------------
// Native fixture trees, in ReadFolder $order of the full path.

// m1: tests folder `tests` at the module root.
pub open spec fn m1() -> Seq<File> {
    seq![cls(seq![1, 201, 12], seq![201, 12, 13]), cls(seq![1, 201, 12], seq![201, 12, 14]),
         cls(seq![1, 202, 12], seq![202, 12, 13]), cls(seq![1, 202, 14], seq![202, 14, 15]),
         cls(seq![1, 203, 12], seq![203, 12, 13]), cls(seq![1, 204], seq![204, 16]),
         cls(seq![1, 205], seq![205, 17]), f(seq![1, 209], seq![209, 26], CLS, false),
         f(seq![3], seq![207], INC, true), f(seq![2], seq![206], MAC, true),
         f(Seq::empty(), seq![208], TXT, true)]
}
pub open spec fn m1_keys() -> Seq<(Seq<int>, int)> { seq![(seq![1], 201), (seq![1], 202), (seq![1], 203), (seq![1], 204), (seq![1], 205)] }
pub open spec fn m1_tests() -> Seq<File> {
    seq![f(Seq::empty(), seq![212], MAC, true), f(Seq::empty(), seq![213], CLS, true),
         cls(seq![211], seq![211, 18]), cls(seq![211, 19], seq![211, 19, 20])]
}
pub open spec fn m1_tkeys() -> Seq<(Seq<int>, int)> { seq![(Seq::<int>::empty(), 211)] }
pub open spec fn m1_out() -> Manifest { generate(true, src(), m1(), m1_keys(), Some(seq![64]), true, m1_tests(), m1_tkeys()).unwrap() }

// m3: Id1.B in x and y; Id2.B in x, Id2.D in y. Tests folder `tests` does not exist.
pub open spec fn m3() -> Seq<File> {
    seq![cls(seq![61, 301, 12], seq![301, 12, 13]), cls(seq![61, 302, 12], seq![302, 12, 13]),
         cls(seq![62, 301, 12], seq![301, 12, 15]), cls(seq![62, 302, 14], seq![302, 14, 15])]
}
pub open spec fn m3_keys() -> Seq<(Seq<int>, int)> { seq![(seq![61], 301), (seq![61], 302), (seq![62], 301), (seq![62], 302)] }
pub open spec fn m3_out() -> Manifest { generate(true, src(), m3(), m3_keys(), Some(seq![64]), false, Seq::empty(), none_keys()).unwrap() }

// m4: tests folder `src/tests`, inside the source folder.
pub open spec fn m4() -> Seq<File> {
    seq![cls(seq![1, 401], seq![401, 23]), cls(seq![64, 402], seq![402, 18]),
         f(seq![64], seq![403], MAC, true), f(seq![64], seq![404], CLS, true)]
}
pub open spec fn m4_keys() -> Seq<(Seq<int>, int)> { seq![(seq![1], 401)] }
pub open spec fn m4_tests() -> Seq<File> {
    seq![cls(seq![402], seq![402, 18]), f(Seq::empty(), seq![403], MAC, true), f(Seq::empty(), seq![404], CLS, true)]
}
pub open spec fn m4_tkeys() -> Seq<(Seq<int>, int)> { seq![(Seq::<int>::empty(), 402)] }
pub open spec fn m4_out() -> Manifest { generate(true, src(), m4(), m4_keys(), Some(seq![65, 64]), true, m4_tests(), m4_tkeys()).unwrap() }

// m2a: root class Ir2a beside cls/Ib2a/P/Q.cls; m2e: root class only; m2b: flat file flat/Ik2b.P.Q.cls.
pub open spec fn m2a() -> Seq<File> { seq![cls(Seq::empty(), seq![501]), cls(seq![1, 502, 21], seq![502, 21, 22])] }
pub open spec fn m2a_keys() -> Seq<(Seq<int>, int)> { seq![(seq![1], 502)] }
pub open spec fn m2a_out() -> Manifest { generate(true, src(), m2a(), m2a_keys(), Some(seq![64]), false, Seq::empty(), none_keys()).unwrap() }
pub open spec fn m2e() -> Seq<File> { seq![cls(Seq::empty(), seq![504])] }
pub open spec fn m2e_out() -> Manifest { generate(true, src(), m2e(), none_keys(), Some(seq![64]), false, Seq::empty(), none_keys()).unwrap() }
pub open spec fn m2b() -> Seq<File> { seq![cls(seq![63], seq![503, 21, 22])] }
pub open spec fn m2b_keys() -> Seq<(Seq<int>, int)> { seq![(Seq::<int>::empty(), 503)] }
pub open spec fn m2b_out() -> Manifest { generate(true, src(), m2b(), m2b_keys(), Some(seq![64]), false, Seq::empty(), none_keys()).unwrap() }

// m5: Ig1 good, Ig2 class header with a broken body (listed), Ig3 without a header (not listed).
pub open spec fn m5() -> Seq<File> {
    seq![cls(seq![1, 601], seq![601, 24]), cls(seq![1, 602], seq![602, 25]), f(seq![1, 603], seq![603, 26], CLS, false)]
}
pub open spec fn m5_keys() -> Seq<(Seq<int>, int)> { seq![(seq![1], 601), (seq![1], 602)] }
pub open spec fn m5_out() -> Manifest { generate(true, src(), m5(), m5_keys(), Some(seq![64]), false, Seq::empty(), none_keys()).unwrap() }

// m7: Im7 in a (Im7.X) and in a/b (Im7.Y). m8: Im8 in a (Im8.X) and in ab (Im8.Y); m8 starts with a module.xml.
pub open spec fn m7() -> Seq<File> { seq![cls(seq![66, 701], seq![701, 16]), cls(seq![66, 67, 701], seq![701, 17])] }
pub open spec fn m7_keys() -> Seq<(Seq<int>, int)> { seq![(seq![66, 67], 701), (seq![66], 701)] }
pub open spec fn m7_out() -> Manifest { generate(true, src(), m7(), m7_keys(), Some(seq![64]), false, Seq::empty(), none_keys()).unwrap() }
pub open spec fn m8() -> Seq<File> { seq![cls(seq![66, 801], seq![801, 16]), cls(seq![68, 801], seq![801, 17])] }
pub open spec fn m8_keys() -> Seq<(Seq<int>, int)> { seq![(seq![66], 801), (seq![68], 801)] }
pub open spec fn m8_out() -> Manifest { generate(true, src(), m8(), m8_keys(), Some(seq![64]), false, Seq::empty(), none_keys()).unwrap() }
// module.xml contents: 0 none, 1 the fixture's old file, 2 the generated file.
pub open spec fn m8_disk() -> Disk { save_file(Disk { module_xml: Some(1), backups: Seq::empty() }, 2) }
pub open spec fn m7_disk() -> Disk { save_file(Disk { module_xml: None, backups: Seq::empty() }, 2) }

// x1..x4: `generate -export 00000,<prefix>` of one class and one routine or include file.
pub open spec fn x1() -> Seq<File> { seq![exported(seq![901, 21, 22], CLS), exported(seq![901, 27, 28], MAC)] }
pub open spec fn x2() -> Seq<File> { seq![exported(seq![902, 21, 22], CLS), exported(seq![905], MAC)] }
pub open spec fn x3() -> Seq<File> { seq![exported(seq![903, 21, 22], CLS), exported(seq![906], INC)] }
pub open spec fn x4() -> Seq<File> { seq![exported(seq![904, 21, 22], CLS), exported(seq![904, 30, 29], INC)] }
pub open spec fn xk(t: int) -> Seq<(Seq<int>, int)> { seq![(seq![1], t)] }
pub open spec fn xout(s: Seq<File>, t: int) -> Manifest { generate(true, src(), s, xk(t), Some(seq![64]), false, Seq::empty(), none_keys()).unwrap() }

// The key lists above are the product's: complete, used, distinct and in $order.
pub proof fn native_keys_ok()
    ensures keys_ok(m1(), ex_out(), m1_keys()), keys_ok(m1_tests(), Excl { base: seq![64], tests: None }, m1_tkeys()),
        keys_ok(m3(), ex_out(), m3_keys()), keys_ok(m4(), ex_in(), m4_keys()),
        keys_ok(m4_tests(), Excl { base: seq![65, 64], tests: None }, m4_tkeys()), keys_ok(m2a(), ex_out(), m2a_keys()),
        keys_ok(m2e(), ex_out(), none_keys()), keys_ok(m2b(), ex_out(), m2b_keys()), keys_ok(m5(), ex_out(), m5_keys()),
        keys_ok(m7(), ex_out(), m7_keys()), keys_ok(m8(), ex_out(), m8_keys()),
        keys_ok(x1(), ex_out(), xk(901)), keys_ok(x2(), ex_out(), xk(902)), keys_ok(x3(), ex_out(), xk(903)), keys_ok(x4(), ex_out(), xk(904)),
        keys_sorted(m1(), ex_out(), m1_keys()), keys_sorted(m3(), ex_out(), m3_keys()), keys_sorted(m5(), ex_out(), m5_keys()),
        keys_sorted(m7(), ex_out(), m7_keys()), keys_sorted(m8(), ex_out(), m8_keys()),
{
    assert(keys_ok(m1(), ex_out(), m1_keys())) by (compute_only);
    assert(keys_ok(m1_tests(), Excl { base: seq![64], tests: None }, m1_tkeys())) by (compute_only);
    assert(keys_ok(m3(), ex_out(), m3_keys())) by (compute_only);
    assert(keys_ok(m4(), ex_in(), m4_keys())) by (compute_only);
    assert(keys_ok(m4_tests(), Excl { base: seq![65, 64], tests: None }, m4_tkeys())) by (compute_only);
    assert(keys_ok(m2a(), ex_out(), m2a_keys())) by (compute_only);
    assert(keys_ok(m2e(), ex_out(), none_keys())) by (compute_only);
    assert(keys_ok(m2b(), ex_out(), m2b_keys())) by (compute_only);
    assert(keys_ok(m5(), ex_out(), m5_keys())) by (compute_only);
    assert(keys_ok(m7(), ex_out(), m7_keys())) by (compute_only);
    assert(keys_ok(m8(), ex_out(), m8_keys())) by (compute_only);
    assert(keys_ok(x1(), ex_out(), xk(901))) by (compute_only);
    assert(keys_ok(x2(), ex_out(), xk(902))) by (compute_only);
    assert(keys_ok(x3(), ex_out(), xk(903))) by (compute_only);
    assert(keys_ok(x4(), ex_out(), xk(904))) by (compute_only);
    assert(keys_sorted(m1(), ex_out(), m1_keys())) by (compute_only);
    assert(keys_sorted(m3(), ex_out(), m3_keys())) by (compute_only);
    assert(keys_sorted(m5(), ex_out(), m5_keys())) by (compute_only);
    assert(keys_sorted(m7(), ex_out(), m7_keys())) by (compute_only);
    assert(keys_sorted(m8(), ex_out(), m8_keys())) by (compute_only);
}

// C3 on the native trees: the general theorem decides the loser from the key strings; m7 and m8 order the same
// pair of directory names differently because of the ":" separator.
pub proof fn collapse_winners()
    ensures
        !has_entry(m3_out().resources, pkg(seq![301, 12], seq![61])),
        !has_entry(m7_out().resources, pkg(seq![701], seq![66, 67])),
        !has_entry(m8_out().resources, pkg(seq![801], seq![66])),
{
    native_keys_ok();
    assert(directory(m3()[0]) == seq![61int] && directory(m3()[2]) == seq![62int]) by (compute_only);
    assert(group(m3(), ex_out(), seq![61], 301) == group(m3(), ex_out(), seq![62], 301)) by (compute_only);
    assert(lex_lt(order_key(m3(), ex_out(), (seq![61], 301)), order_key(m3(), ex_out(), (seq![62], 301)))) by (compute_only);
    assert(kept(m3()[0], ex_out()) && kept(m3()[2], ex_out())) by (compute_only);
    assert(m3()[0].name[0] == 301 && m3()[2].name[0] == 301) by (compute_only);
    collapse_earlier_key_loses(m3(), ex_out(), m3_keys(), 0, 2);
    assert(m3_out().resources == out_of(m3(), ex_out(), m3_keys()));
    assert(pkg_entry(m3(), ex_out(), (seq![61], 301)) == pkg(seq![301, 12], seq![61])) by (compute_only);

    assert(directory(m7()[0]) == seq![66int] && directory(m7()[1]) == seq![66int, 67]) by (compute_only);
    assert(group(m7(), ex_out(), seq![66, 67], 701) == group(m7(), ex_out(), seq![66], 701)) by (compute_only);
    assert(lex_lt(order_key(m7(), ex_out(), (seq![66, 67], 701)), order_key(m7(), ex_out(), (seq![66], 701)))) by (compute_only);
    assert(kept(m7()[0], ex_out()) && kept(m7()[1], ex_out())) by (compute_only);
    assert(m7()[0].name[0] == 701 && m7()[1].name[0] == 701) by (compute_only);
    collapse_earlier_key_loses(m7(), ex_out(), m7_keys(), 1, 0);
    assert(m7_out().resources == out_of(m7(), ex_out(), m7_keys()));
    assert(pkg_entry(m7(), ex_out(), (seq![66, 67], 701)) == pkg(seq![701], seq![66, 67])) by (compute_only);

    assert(directory(m8()[0]) == seq![66int] && directory(m8()[1]) == seq![68int]) by (compute_only);
    assert(group(m8(), ex_out(), seq![66], 801) == group(m8(), ex_out(), seq![68], 801)) by (compute_only);
    assert(lex_lt(order_key(m8(), ex_out(), (seq![66], 801)), order_key(m8(), ex_out(), (seq![68], 801)))) by (compute_only);
    assert(kept(m8()[0], ex_out()) && kept(m8()[1], ex_out())) by (compute_only);
    assert(m8()[0].name[0] == 801 && m8()[1].name[0] == 801) by (compute_only);
    collapse_earlier_key_loses(m8(), ex_out(), m8_keys(), 0, 1);
    assert(m8_out().resources == out_of(m8(), ex_out(), m8_keys()));
    assert(pkg_entry(m8(), ex_out(), (seq![66], 801)) == pkg(seq![801], seq![66])) by (compute_only);
}

// C5 on the native trees: the general export theorem rejects x1 and x4; x2 and x3 load.
pub proof fn export_round_trips()
    ensures !load_ok(xout(x1(), 901).resources, x1()), !load_ok(xout(x4(), 904).resources, x4()),
        load_ok(xout(x2(), 902).resources, x2()), load_ok(xout(x3(), 903).resources, x3()),
{
    native_keys_ok();
    assert(kept(x1()[1], ex_out()) && x1()[1] == exported(x1()[1].name, x1()[1].ext)) by (compute_only);
    assert(x1()[0].name != x1()[1].name && x1()[1].ext == MAC && x1()[1].name.len() == 3) by (compute_only);
    export_dotted_item_fails_load(x1(), ex_out(), xk(901), 1);
    assert(xout(x1(), 901).resources == out_of(x1(), ex_out(), xk(901)));
    assert(kept(x4()[1], ex_out()) && x4()[1] == exported(x4()[1].name, x4()[1].ext)) by (compute_only);
    assert(x4()[0].name != x4()[1].name && x4()[1].ext == INC && x4()[1].name.len() == 3) by (compute_only);
    export_dotted_item_fails_load(x4(), ex_out(), xk(904), 1);
    assert(xout(x4(), 904).resources == out_of(x4(), ex_out(), xk(904)));
    assert(load_ok(xout(x2(), 902).resources, x2())) by (compute_only);
    assert(load_ok(xout(x3(), 903).resources, x3())) by (compute_only);
}

} // verus!

fn main() {}
