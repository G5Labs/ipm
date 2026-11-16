#[path="../../../../Verus_verification/consolidated/iter49/importcmd.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {

reveal_with_fuel(load_fails, 3); assert((b(defined(i3(), I(true, true, false)))) == 0);assert((b(defined(i3(), I(true, true, false)))) == 0); // i3.defined.I49N.Sub
reveal_with_fuel(load_fails, 3); assert((b(ok(i5(), i5items()))) == 0);assert((b(ok(i5(), i5items()))) == 0); // i5.ok
reveal_with_fuel(load_fails, 3); assert((b(added(i7(), i7items(), I(false, true, false)))) == 1);assert((b(added(i7(), i7items(), I(false, true, false)))) == 1); // i7.add.new
reveal_with_fuel(load_fails, 3); assert((b(ok(C(Source::File, true, true, false), seq![I(false, true, false), I(false, true, false)]))) == 1);assert((b(ok(C(Source::File, true, true, false), seq![I(false, true, false), I(false, true, false)]))) == 1); // i1.ok
reveal_with_fuel(load_fails, 3); assert((b(!ok(C(Source::File, true, true, false), seq![I(false, true, false), I(false, true, false)]))) == 0);assert((b(!ok(C(Source::File, true, true, false), seq![I(false, true, false), I(false, true, false)]))) == 0); // i1.errimport
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(C(Source::File, true, true, false), seq![I(false, true, false), I(false, true, false)]))) == 0);assert((b(loaded_line(C(Source::File, true, true, false), seq![I(false, true, false), I(false, true, false)]))) == 0); // i1.loadedline
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(C(Source::File, true, true, false), seq![I(false, true, false), I(false, true, false)]))) == 0);assert((b(loaded_line(C(Source::File, true, true, false), seq![I(false, true, false), I(false, true, false)]))) == 0); // i1.exporting
reveal_with_fuel(load_fails, 3); assert((b(defined(C(Source::File, true, true, false), I(false, true, false)))) == 1);assert((b(defined(C(Source::File, true, true, false), I(false, true, false)))) == 1); // i1.defined.I49X.A
reveal_with_fuel(load_fails, 3); assert(((if compiled(C(Source::File, true, true, false), I(false, true, false)) { 1int } else { -1int })) == 1);assert(((if compiled(C(Source::File, true, true, false), I(false, true, false)) { 1int } else { -1int })) == 1); // i1.value.I49X.A
reveal_with_fuel(load_fails, 3); assert((b(defined(C(Source::File, true, true, false), I(false, true, false)))) == 1);assert((b(defined(C(Source::File, true, true, false), I(false, true, false)))) == 1); // i1.defined.I49X.B
reveal_with_fuel(load_fails, 3); assert(((if compiled(C(Source::File, true, true, false), I(false, true, false)) { 2int } else { -1int })) == 2);assert(((if compiled(C(Source::File, true, true, false), I(false, true, false)) { 2int } else { -1int })) == 2); // i1.value.I49X.B
reveal_with_fuel(load_fails, 3); assert((b(ok(C(Source::Dir, true, true, false), seq![I(false, true, false), I(true, true, false)]))) == 1);assert((b(ok(C(Source::Dir, true, true, false), seq![I(false, true, false), I(true, true, false)]))) == 1); // i2.ok
reveal_with_fuel(load_fails, 3); assert((b(!ok(C(Source::Dir, true, true, false), seq![I(false, true, false), I(true, true, false)]))) == 0);assert((b(!ok(C(Source::Dir, true, true, false), seq![I(false, true, false), I(true, true, false)]))) == 0); // i2.errimport
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(C(Source::Dir, true, true, false), seq![I(false, true, false), I(true, true, false)]))) == 0);assert((b(loaded_line(C(Source::Dir, true, true, false), seq![I(false, true, false), I(true, true, false)]))) == 0); // i2.loadedline
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(C(Source::Dir, true, true, false), seq![I(false, true, false), I(true, true, false)]))) == 0);assert((b(loaded_line(C(Source::Dir, true, true, false), seq![I(false, true, false), I(true, true, false)]))) == 0); // i2.exporting
reveal_with_fuel(load_fails, 3); assert((b(defined(C(Source::Dir, true, true, false), I(false, true, false)))) == 1);assert((b(defined(C(Source::Dir, true, true, false), I(false, true, false)))) == 1); // i2.defined.I49D.Root
reveal_with_fuel(load_fails, 3); assert(((if compiled(C(Source::Dir, true, true, false), I(false, true, false)) { 3int } else { -1int })) == 3);assert(((if compiled(C(Source::Dir, true, true, false), I(false, true, false)) { 3int } else { -1int })) == 3); // i2.value.I49D.Root
reveal_with_fuel(load_fails, 3); assert((b(defined(C(Source::Dir, true, true, false), I(false, true, false)))) == 1);assert((b(defined(C(Source::Dir, true, true, false), I(false, true, false)))) == 1); // i2.defined.I49D.RootX
reveal_with_fuel(load_fails, 3); assert(((if compiled(C(Source::Dir, true, true, false), I(false, true, false)) { 6int } else { -1int })) == 6);assert(((if compiled(C(Source::Dir, true, true, false), I(false, true, false)) { 6int } else { -1int })) == 6); // i2.value.I49D.RootX
reveal_with_fuel(load_fails, 3); assert((b(defined(C(Source::Dir, true, true, false), I(true, true, false)))) == 1);assert((b(defined(C(Source::Dir, true, true, false), I(true, true, false)))) == 1); // i2.defined.I49D.Sub
reveal_with_fuel(load_fails, 3); assert(((if compiled(C(Source::Dir, true, true, false), I(true, true, false)) { 4int } else { -1int })) == 4);assert(((if compiled(C(Source::Dir, true, true, false), I(true, true, false)) { 4int } else { -1int })) == 4); // i2.value.I49D.Sub
reveal_with_fuel(load_fails, 3); assert((b(defined(C(Source::Dir, true, true, false), I(true, true, false)))) == 1);assert((b(defined(C(Source::Dir, true, true, false), I(true, true, false)))) == 1); // i2.defined.I49D.SubX
reveal_with_fuel(load_fails, 3); assert(((if compiled(C(Source::Dir, true, true, false), I(true, true, false)) { 5int } else { -1int })) == 5);assert(((if compiled(C(Source::Dir, true, true, false), I(true, true, false)) { 5int } else { -1int })) == 5); // i2.value.I49D.SubX
reveal_with_fuel(load_fails, 3); assert((b(ok(i3(), seq![I(false, true, false), I(true, true, false)]))) == 1);assert((b(ok(i3(), seq![I(false, true, false), I(true, true, false)]))) == 1); // i3.ok
reveal_with_fuel(load_fails, 3); assert((b(!ok(i3(), seq![I(false, true, false), I(true, true, false)]))) == 0);assert((b(!ok(i3(), seq![I(false, true, false), I(true, true, false)]))) == 0); // i3.errimport
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(i3(), seq![I(false, true, false), I(true, true, false)]))) == 0);assert((b(loaded_line(i3(), seq![I(false, true, false), I(true, true, false)]))) == 0); // i3.loadedline
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(i3(), seq![I(false, true, false), I(true, true, false)]))) == 0);assert((b(loaded_line(i3(), seq![I(false, true, false), I(true, true, false)]))) == 0); // i3.exporting
reveal_with_fuel(load_fails, 3); assert((b(defined(i3(), I(false, true, false)))) == 1);assert((b(defined(i3(), I(false, true, false)))) == 1); // i3.defined.I49N.Root
reveal_with_fuel(load_fails, 3); assert(((if compiled(i3(), I(false, true, false)) { 3int } else { -1int })) == 3);assert(((if compiled(i3(), I(false, true, false)) { 3int } else { -1int })) == 3); // i3.value.I49N.Root
reveal_with_fuel(load_fails, 3); assert((b(defined(i3(), I(false, true, false)))) == 1);assert((b(defined(i3(), I(false, true, false)))) == 1); // i3.defined.I49N.RootX
reveal_with_fuel(load_fails, 3); assert(((if compiled(i3(), I(false, true, false)) { 6int } else { -1int })) == 6);assert(((if compiled(i3(), I(false, true, false)) { 6int } else { -1int })) == 6); // i3.value.I49N.RootX
reveal_with_fuel(load_fails, 3); assert(((if compiled(i3(), I(true, true, false)) { 4int } else { -1int })) == -1);assert(((if compiled(i3(), I(true, true, false)) { 4int } else { -1int })) == -1); // i3.value.I49N.Sub
reveal_with_fuel(load_fails, 3); assert((b(defined(i3(), I(true, true, false)))) == 0);assert((b(defined(i3(), I(true, true, false)))) == 0); // i3.defined.I49N.SubX
reveal_with_fuel(load_fails, 3); assert(((if compiled(i3(), I(true, true, false)) { 5int } else { -1int })) == -1);assert(((if compiled(i3(), I(true, true, false)) { 5int } else { -1int })) == -1); // i3.value.I49N.SubX
reveal_with_fuel(load_fails, 3); assert((b(ok(i4(), seq![I(false, true, false)]))) == 1);assert((b(ok(i4(), seq![I(false, true, false)]))) == 1); // i4.ok
reveal_with_fuel(load_fails, 3); assert((b(!ok(i4(), seq![I(false, true, false)]))) == 0);assert((b(!ok(i4(), seq![I(false, true, false)]))) == 0); // i4.errimport
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(i4(), seq![I(false, true, false)]))) == 0);assert((b(loaded_line(i4(), seq![I(false, true, false)]))) == 0); // i4.loadedline
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(i4(), seq![I(false, true, false)]))) == 0);assert((b(loaded_line(i4(), seq![I(false, true, false)]))) == 0); // i4.exporting
reveal_with_fuel(load_fails, 3); assert((b(defined(i4(), I(false, true, false)))) == 1);assert((b(defined(i4(), I(false, true, false)))) == 1); // i4.defined.I49C.A
reveal_with_fuel(load_fails, 3); assert(((if compiled(i4(), I(false, true, false)) { 7int } else { -1int })) == -1);assert(((if compiled(i4(), I(false, true, false)) { 7int } else { -1int })) == -1); // i4.value.I49C.A
reveal_with_fuel(load_fails, 3); assert((b(defined(i4(), I(false, true, false)))) == 1);assert((b(defined(i4(), I(false, true, false)))) == 1); // i4.defined.I49C.AX
reveal_with_fuel(load_fails, 3); assert(((if compiled(i4(), I(false, true, false)) { 8int } else { -1int })) == -1);assert(((if compiled(i4(), I(false, true, false)) { 8int } else { -1int })) == -1); // i4.value.I49C.AX
reveal_with_fuel(load_fails, 3); assert((b(!ok(i5(), i5items()))) == 1);assert((b(!ok(i5(), i5items()))) == 1); // i5.errimport
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(i5(), i5items()))) == 0);assert((b(loaded_line(i5(), i5items()))) == 0); // i5.loadedline
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(i5(), i5items()))) == 0);assert((b(loaded_line(i5(), i5items()))) == 0); // i5.exporting
reveal_with_fuel(load_fails, 3); assert((b(defined(i5(), I(false, true, false)))) == 1);assert((b(defined(i5(), I(false, true, false)))) == 1); // i5.defined.I49E.Good
reveal_with_fuel(load_fails, 3); assert(((if compiled(i5(), I(false, true, false)) { 9int } else { -1int })) == 9);assert(((if compiled(i5(), I(false, true, false)) { 9int } else { -1int })) == 9); // i5.value.I49E.Good
reveal_with_fuel(load_fails, 3); assert((b(defined(i5(), I(false, false, false)))) == 1);assert((b(defined(i5(), I(false, false, false)))) == 1); // i5.defined.I49E.Bad
reveal_with_fuel(load_fails, 3); assert(((if compiled(i5(), I(false, false, false)) { 0int } else { -1int })) == -1);assert(((if compiled(i5(), I(false, false, false)) { 0int } else { -1int })) == -1); // i5.value.I49E.Bad
reveal_with_fuel(load_fails, 3); assert((b(ok(C(Source::Missing, true, true, false), seq![]))) == 0);assert((b(ok(C(Source::Missing, true, true, false), seq![]))) == 0); // i6.ok
reveal_with_fuel(load_fails, 3); assert((b(!ok(C(Source::Missing, true, true, false), seq![]))) == 1);assert((b(!ok(C(Source::Missing, true, true, false), seq![]))) == 1); // i6.errimport
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(C(Source::Missing, true, true, false), seq![]))) == 0);assert((b(loaded_line(C(Source::Missing, true, true, false), seq![]))) == 0); // i6.loadedline
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(C(Source::Missing, true, true, false), seq![]))) == 0);assert((b(loaded_line(C(Source::Missing, true, true, false), seq![]))) == 0); // i6.exporting
reveal_with_fuel(load_fails, 3); assert((b(ok(i7(), i7items()))) == 1);assert((b(ok(i7(), i7items()))) == 1); // i7.ok
reveal_with_fuel(load_fails, 3); assert((b(!ok(i7(), i7items()))) == 0);assert((b(!ok(i7(), i7items()))) == 0); // i7.errimport
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(i7(), i7items()))) == 1);assert((b(loaded_line(i7(), i7items()))) == 1); // i7.loadedline
reveal_with_fuel(load_fails, 3); assert((b(loaded_line(i7(), i7items()))) == 1);assert((b(loaded_line(i7(), i7items()))) == 1); // i7.exporting
reveal_with_fuel(load_fails, 3); assert((b(defined(i7(), I(false, true, true)))) == 1);assert((b(defined(i7(), I(false, true, true)))) == 1); // i7.defined.I49S.Ctl
reveal_with_fuel(load_fails, 3); assert(((if compiled(i7(), I(false, true, true)) { 10int } else { -1int })) == 10);assert(((if compiled(i7(), I(false, true, true)) { 10int } else { -1int })) == 10); // i7.value.I49S.Ctl
reveal_with_fuel(load_fails, 3); assert((b(defined(i7(), I(false, true, false)))) == 1);assert((b(defined(i7(), I(false, true, false)))) == 1); // i7.defined.I49S.New
reveal_with_fuel(load_fails, 3); assert(((if compiled(i7(), I(false, true, false)) { 11int } else { -1int })) == 11);assert(((if compiled(i7(), I(false, true, false)) { 11int } else { -1int })) == 11); // i7.value.I49S.New
reveal_with_fuel(load_fails, 3); assert((user_actions(i7())) == 1);assert((user_actions(i7())) == 1); // i7.useraction
reveal_with_fuel(load_fails, 3); assert((b(after_save(i7(), i7items(), I(false, true, true)))) == 1);assert((b(after_save(i7(), i7items(), I(false, true, true)))) == 1); // i7.aftersave.ctl
reveal_with_fuel(load_fails, 3); assert((b(added(i7(), i7items(), I(false, true, true)))) == 0);assert((b(added(i7(), i7items(), I(false, true, true)))) == 0); // i7.add.ctl
reveal_with_fuel(load_fails, 3); assert((b(after_save(i7(), i7items(), I(false, true, false)))) == 0);assert((b(after_save(i7(), i7items(), I(false, true, false)))) == 0); // i7.aftersave.new
}
}
fn main() {}
