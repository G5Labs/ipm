// Iteration75 neighbours for `import <file|dir> [-norecurse] [-nocompile]` (devtools.authoring.import #1, #3).
//
// The deciding computation is the inherited iter49 importcmd.rs `selected`, imported unchanged. It is faithful
// to Main.cls::Import at pin 571e04a1:
//   Main.cls:3334      tRecurse = '$$$HasModifier(pCommandInfo,"norecurse")
//   Main.cls:3352-3358 an existing directory goes to $system.OBJ.ImportDir(tSourceDir,,tLoadFlags,,tRecurse,...)
//   Main.cls:3366-3369 an existing file goes to $system.OBJ.Load
//   Main.cls:3371-3374 otherwise tSC = FileDoesNotExist and $$$ThrowOnError, before any load call
// The recursion itself is ImportDir's behaviour. The iteration-49 native case i2 (`import d/` with items in
// d/sub/, all compiled) observed it at the pin.
#[path = "../../consolidated/iter49/importcmd.rs"]
mod importcmd;

use vstd::prelude::*;
use importcmd::*;

verus! {

// A directory import without -norecurse defines and compiles an item in a subdirectory.
pub proof fn recursive_import_reaches_subdirectory()
    ensures
        defined(C(Source::Dir, true, true, false), I(true, true, false)),
        compiled(C(Source::Dir, true, true, false), I(true, true, false)),
        // The same item with -norecurse is not loaded, so recursion is what the flag decides.
        !defined(C(Source::Dir, false, true, false), I(true, true, false)),
        // -nocompile still defines the subdirectory item but does not compile it.
        defined(C(Source::Dir, true, false, false), I(true, true, false)),
        !compiled(C(Source::Dir, true, false, false), I(true, true, false)),
{}

// A missing path fails before anything is defined, compiled or handed to source control, for any item and
// any flags. An existing file with the same flags defines its item.
pub proof fn missing_path_defines_nothing(recurse: bool, compile: bool, sc: bool, i: Item, items: Seq<Item>)
    ensures
        !defined(C(Source::Missing, recurse, compile, sc), i),
        !compiled(C(Source::Missing, recurse, compile, sc), i),
        !ok(C(Source::Missing, recurse, compile, sc), items),
        user_actions(C(Source::Missing, recurse, compile, sc)) == 0,
        !after_save(C(Source::Missing, recurse, compile, sc), items, i),
        !added(C(Source::Missing, recurse, compile, sc), items, i),
        defined(C(Source::File, recurse, compile, sc), i),
{}

} // verus!

fn main() {}
