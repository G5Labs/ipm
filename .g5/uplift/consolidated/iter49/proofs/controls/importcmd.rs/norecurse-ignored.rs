#![allow(non_snake_case)]
// Iteration49: the `import <file|dir> [-norecurse] [-nocompile]` command.
//
// Source role at pin 571e04a1: Main.cls::Import. Load flags carry compile ("c") unless -nocompile; a
// directory goes to $system.OBJ.ImportDir with the recurse flag, a file to $system.OBJ.Load, a missing
// path raises FileDoesNotExist; any load error is rethrown as "Error in import: ...". When the namespace
// has a source-control class, a list-only pass first calls UserAction(1, 6, <all items>), the load
// pass follows, and then each loaded item gets OnAfterSave when GetStatus says it is already in source
// control, otherwise AddToSourceControl.
//
// Premises (native-qualified): ImportDir's default wildcard includes .cls and .xml; the loader keeps and
// compiles every good item even when another item in the same load fails to compile.
use vstd::prelude::*;

verus! {

pub struct Item { pub sub: bool, pub good: bool, pub in_sc: bool }
pub enum Source { File, Dir, Missing }
pub struct Cmd { pub source: Source, pub recurse: bool, pub compile: bool, pub sc: bool }

pub open spec fn selected(c: Cmd, i: Item) -> bool {
    match c.source { Source::Missing => false, Source::File => true, Source::Dir => true }
}
pub open spec fn defined(c: Cmd, i: Item) -> bool { selected(c, i) }
pub open spec fn compiled(c: Cmd, i: Item) -> bool { selected(c, i) && c.compile && i.good }

pub open spec fn load_fails(c: Cmd, items: Seq<Item>) -> bool
    decreases items.len()
{
    if items.len() == 0 { false }
    else { load_fails(c, items.drop_last()) || (selected(c, items.last()) && c.compile && !items.last().good) }
}
pub open spec fn ok(c: Cmd, items: Seq<Item>) -> bool {
    match c.source { Source::Missing => false, _ => !load_fails(c, items) }
}
// Source-control calls. The list pass precedes the load, so UserAction happens even if the load fails;
// export calls happen only after a successful load.
pub open spec fn user_actions(c: Cmd) -> int { if c.sc && !(c.source is Missing) { 1 } else { 0 } }
pub open spec fn after_save(c: Cmd, items: Seq<Item>, i: Item) -> bool { c.sc && ok(c, items) && selected(c, i) && i.in_sc }
pub open spec fn added(c: Cmd, items: Seq<Item>, i: Item) -> bool { c.sc && ok(c, items) && selected(c, i) && !i.in_sc }
pub open spec fn loaded_line(c: Cmd, items: Seq<Item>) -> bool { c.sc && ok(c, items) }

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub proof fn norecurse_skips_subdirectories(c: Cmd, i: Item)
    requires c.source is Dir, !c.recurse, i.sub
    ensures !defined(c, i), !compiled(c, i)
{}
pub proof fn nocompile_defines_only(c: Cmd, i: Item)
    requires !c.compile, !(c.source is Missing)
    ensures !compiled(c, i)
{}
pub proof fn one_bad_item_fails_command_but_keeps_good(c: Cmd, good: Item, bad: Item)
    requires c.source is Dir, c.compile, !good.sub, !bad.sub, good.good, !bad.good
    ensures !ok(c, seq![good, bad]), compiled(c, good), defined(c, bad), !compiled(c, bad)
{
    reveal_with_fuel(load_fails, 3);
}
pub proof fn source_control_routing(c: Cmd, ctl: Item, new: Item)
    requires c.sc, c.source is Dir, c.compile, !ctl.sub, !new.sub, ctl.good, new.good, ctl.in_sc, !new.in_sc
    ensures user_actions(c) == 1,
        after_save(c, seq![ctl, new], ctl), !added(c, seq![ctl, new], ctl),
        added(c, seq![ctl, new], new), !after_save(c, seq![ctl, new], new),
        loaded_line(c, seq![ctl, new])
{
    reveal_with_fuel(load_fails, 3);
}
pub proof fn missing_path_fails(c: Cmd, items: Seq<Item>)
    requires c.source is Missing
    ensures !ok(c, items), user_actions(c) == 0
{}

// Native schedule.
pub open spec fn I(sub: bool, good: bool, in_sc: bool) -> Item { Item { sub, good, in_sc } }
pub open spec fn C(source: Source, recurse: bool, compile: bool, sc: bool) -> Cmd { Cmd { source, recurse, compile, sc } }
pub open spec fn i3() -> Cmd { C(Source::Dir, false, true, false) }
pub open spec fn i4() -> Cmd { C(Source::Dir, true, false, false) }
pub open spec fn i5() -> Cmd { C(Source::Dir, true, true, false) }
pub open spec fn i5items() -> Seq<Item> { seq![I(false, true, false), I(false, false, false)] }
pub open spec fn i7() -> Cmd { C(Source::Dir, true, true, true) }
pub open spec fn i7items() -> Seq<Item> { seq![I(false, true, true), I(false, true, false)] }

} // verus!

fn main() {}
