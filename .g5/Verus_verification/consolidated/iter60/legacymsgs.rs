#![allow(non_snake_case)]
// Iteration60: the legacy localized-messages processor (ProcessorClass="LegacyLocalizedMessages").
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter60/bindings.json):
//   ResourceProcessor/LegacyLocalizedMessages.cls::OnBeforePhase (Compile)   Import^%occMsgXML replaces each domain in
//       the file; one include per mapped target is generated; with merge=1 the mapped domains' earlier entries are restored
//   ::GetDomainMap, ::GetDomains, ::MergeMessageDomains                       domain map and the merge save/restore
//   ::OnResolveChildren                                                        the include names reported as the
//       resource's children, which Lifecycle/Base.cls::%Clean deletes at uninstall
// includeFiles is "<inc>" (single) or "<dom>:<inc>,<dom>:<inc>..." (mapped). For the mapped form the child name is
// taken as $piece(includeFiles,":",2) of the whole attribute. Includes are small ints; GARBAGE stands for the
// resulting "<inc1>,<dom2>" name, which matches no generated include. Premises (native-qualified): the message
// importer and include generator, and %Clean deleting exactly the resolved children.
use vstd::prelude::*;

verus! {

pub open spec fn GARBAGE() -> int { -1 }
pub enum Spec { Single(int), Mapped(Seq<int>) }   // Mapped: target include per entry, in attribute order

pub open spec fn children(s: Spec) -> Seq<int> {
    match s {
        Spec::Single(i) => seq![i],
        Spec::Mapped(incs) => if incs.len() == 1 { seq![incs[0]] } else if incs.len() == 0 { seq![] } else { seq![GARBAGE()] },
    }
}
pub open spec fn generated(s: Spec) -> Seq<int> { match s { Spec::Single(i) => seq![i], Spec::Mapped(incs) => incs } }

pub open spec fn in_seq(s: Seq<int>, x: int) -> bool
    decreases s.len()
{
    if s.len() == 0 { false } else { s.last() == x || in_seq(s.drop_last(), x) }
}
// After uninstall, a generated include survives unless it is a resolved child.
pub open spec fn survives_uninstall(s: Spec, inc: int) -> bool { in_seq(generated(s), inc) && !in_seq(children(s), inc) }

// Loading a domain: the file's ids replace the domain, except that merge=1 keeps earlier ids not in the file.
pub open spec fn after_load(old_has: bool, in_file: bool, merge: bool) -> bool { in_file || (merge && old_has) }

pub proof fn single_include_cleaned(i: int)
    ensures !survives_uninstall(Spec::Single(i), i)
{
    reveal_with_fuel(in_seq, 2);
}
pub proof fn one_mapped_entry_cleaned(i: int)
    ensures !survives_uninstall(Spec::Mapped(seq![i]), i)
{
    reveal_with_fuel(in_seq, 2);
}
// As built, with two or more mapped entries no generated include is a resolved child, so all survive uninstall.
pub proof fn mapped_includes_leak(a: int, c: int)
    requires a >= 0, c >= 0
    ensures survives_uninstall(Spec::Mapped(seq![a, c]), a), survives_uninstall(Spec::Mapped(seq![a, c]), c)
{
    reveal_with_fuel(in_seq, 3);
    assert(generated(Spec::Mapped(seq![a, c])) =~= seq![a, c]);
    assert(children(Spec::Mapped(seq![a, c])) =~= seq![GARBAGE()]);
}
pub proof fn replace_and_merge()
    ensures !after_load(true, false, false), after_load(true, false, true), after_load(false, true, false)
{}

// Fixture: Inc1 = 1, IncA = 2, IncB = 3.
pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
pub proof fn fixture()
    ensures
        in_seq(children(Spec::Single(1)), 1),
        !in_seq(children(Spec::Mapped(seq![2int, 3])), 2), !in_seq(children(Spec::Mapped(seq![2int, 3])), 3),
        survives_uninstall(Spec::Mapped(seq![2int, 3]), 2), survives_uninstall(Spec::Mapped(seq![2int, 3]), 3),
        !survives_uninstall(Spec::Single(1), 1),
{
    reveal_with_fuel(in_seq, 3);
    assert(children(Spec::Mapped(seq![2int, 3])) =~= seq![GARBAGE()]);
    assert(generated(Spec::Mapped(seq![2int, 3])) =~= seq![2int, 3]);
}

} // verus!

fn main() {}
