#![allow(non_snake_case)]
// Iteration67: editor-facing routing of IPM's composite source-control extension.
//
// Source roles at pin 571e04a1 (bindings: .g5/uplift/consolidated/iter67/bindings.json):
//   ExtensionBase/Composite.cls  per-item callbacks (OnBeforeSave, OnAfterSave, OnBeforeLoad, OnAfterCompile,
//     OnBeforeDelete, ItemIconState, ...) go to the primary only; GetStatus ANDs Editable over the sub-extension
//     list, which includes the primary; IsReadOnly returns the primary's answer when it is true and otherwise
//     stops at the first false in the loop (the primary), so other extensions are never asked; UserAction
//     routes type 1 to the primary and menu actions to the extension owning the menu; unknown methods go to
//     the primary.
//   ExtensionBase/CompositeMethodOverrides.cls  %OnNew instantiates every stored sub-extension class;
//     SetSubExtensionClasses validates names but stores the raw list; SetPrimaryExtensionClass ignores a
//     name that is not a class; MapMenus/GetMenuExtension map menu names to their extension.
//   ExtensionBase/Utils.cls::CorrectInternalNames  "/A/B.cls" -> "A.B.CLS", extension upper-cased.
// Premise (native-qualified): ObjectScript `&&` short-circuits; %New of a nonexistent class fails.
use vstd::prelude::*;

verus! {

pub struct Ext { pub editable: bool, pub ro: bool, pub owns_menu: bool, pub exists: bool }
pub struct Cfg { pub primary: Ext, pub other: Ext }

// Who receives a per-item callback: (primary, other).
pub open spec fn per_item(c: Cfg) -> (int, int) { (1, 0) }

// GetStatus: primary, then the loop over [primary, other].
pub open spec fn editable(c: Cfg) -> bool { c.primary.editable && c.primary.editable && c.other.editable }
pub open spec fn status_calls(c: Cfg) -> (int, int) { (2, 1) }

// IsReadOnly as built.
pub open spec fn read_only(c: Cfg) -> bool { if c.primary.ro { true } else { false } }
pub open spec fn ro_calls(c: Cfg) -> (int, int) { (1, 0) }

// UserAction: type 1 -> primary; type 0 -> extension owning the menu, else nobody.
pub open spec fn user_action(c: Cfg, menu_type: int, menu_of_other: bool) -> (int, int) {
    if menu_type != 0 { (1, 0) } else if menu_of_other && c.other.owns_menu { (0, 1) } else { (0, 0) }
}

// Configuring others: stored raw, so creation fails when any stored name is not a class.
pub open spec fn configure_ok(others_exist: bool) -> bool { true }
pub open spec fn create_ok(others_exist: bool) -> bool { others_exist }

// Setting an empty primary keeps the previous primary.
pub open spec fn primary_after_set(old: int, new: int, new_is_class: bool) -> int { new }

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
pub open spec fn cfg(pe: bool, oe: bool, pr: bool, or_: bool) -> Cfg {
    Cfg { primary: Ext { editable: pe, ro: pr, owns_menu: false, exists: true }, other: Ext { editable: oe, ro: or_, owns_menu: true, exists: true } }
}

// The as-built inconsistency: another extension can make an item uneditable through GetStatus
// but cannot make it read-only through IsReadOnly.
pub proof fn other_extension_inconsistency()
    ensures !editable(cfg(true, false, false, true)), !read_only(cfg(true, false, false, true))
{}
pub proof fn neighbors()
    ensures editable(cfg(true, true, false, false)), read_only(cfg(true, true, true, false)),
            per_item(cfg(true, true, false, false)) == (1int, 0int),
            user_action(cfg(true, true, false, false), 0, true) == (0int, 1int),
            user_action(cfg(true, true, false, false), 0, false) == (0int, 0int),
            user_action(cfg(true, true, false, false), 1, false) == (1int, 0int)
{}
pub proof fn misconfigured_other_breaks_creation()
    ensures configure_ok(false), !create_ok(false), create_ok(true)
{}
pub proof fn empty_primary_ignored()
    ensures primary_after_set(1, 0, false) == 1
{}

} // verus!

fn main() {}
