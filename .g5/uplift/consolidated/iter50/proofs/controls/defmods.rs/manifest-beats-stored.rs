#![allow(non_snake_case)]
// Iteration50: namespace default modifiers and how commands receive them.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter50/bindings.json):
//   Main.cls::ProcessDefaultModifiers           exactly one of -set/-get/-delete; -set stores the command's data
//   Main.cls::Get/Set/DeleteNamespaceDefaultModifiers   merge into / kill the stored array (^%IPM.DefaultModifiers)
//   Main.cls::AccumulateDefaultModifierList     -get renders every stored node as -D<dotted.name>=<value>
//   Main.cls::Shell dispatcher                  stored defaults merged into "data" first, parsed command over them
//   Storage/Module.cls::ExecutePhases           module <Default> values under command data
//
// Keys are small ids (1 = Mode, 2 = Other.Sub, 3 = Verbose); values are small codes. Premises
// (native-qualified): the command parser refuses modifiers a command does not declare; the global is
// shared across namespaces (owned elsewhere, P:F-REPO-5) and not re-modelled.
use vstd::prelude::*;

verus! {

pub type Store = Seq<Option<int>>;   // index = key id, length 4 (index 0 unused)

pub open spec fn empty() -> Store { seq![None, None, None, None] }

// Command data after the dispatcher merge: stored first, then the command's own -D values over them.
pub open spec fn merged(stored: Store, cmd: Store) -> Store {
    Seq::new(4, |k: int| if cmd[k].is_some() { cmd[k] } else { stored[k] })
}

pub struct Cmd { pub set: bool, pub get: bool, pub delete: bool, pub unknown_modifier: bool, pub data: Store }
pub struct Out { pub ok: bool, pub stored: Store }

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }

pub open spec fn default_modifiers(stored: Store, c: Cmd) -> Out {
    if c.unknown_modifier { Out { ok: false, stored: stored } }
    else if b(c.set) + b(c.get) + b(c.delete) != 1 { Out { ok: false, stored: stored } }
    else if c.set { Out { ok: true, stored: merged(stored, c.data) } }
    else if c.delete { Out { ok: true, stored: empty() } }
    else { Out { ok: true, stored: stored } }
}

// Value a module's code sees for key k: command data (stored defaults under command line) over the
// module's own <Default>.
pub open spec fn effective(stored: Store, cmd: Store, manifest: Option<int>, k: int) -> Option<int> {
    let d = merged(stored, cmd);
    if manifest.is_some() { manifest } else { d[k] }
}

pub proof fn set_appends_and_overwrites(s: Store, d: Store, k: int)
    requires s.len() == 4, d.len() == 4, 0 <= k < 4
    ensures default_modifiers(s, Cmd { set: true, get: false, delete: false, unknown_modifier: false, data: d }).stored[k]
              == (if d[k].is_some() { d[k] } else { s[k] })
{}

pub proof fn command_line_wins(s: Store, c: Store, m: Option<int>, k: int)
    requires s.len() == 4, c.len() == 4, 0 <= k < 4, c[k].is_some()
    ensures effective(s, c, m, k) == c[k]
{}

pub proof fn stored_beats_manifest(s: Store, m: Option<int>, k: int)
    requires s.len() == 4, 0 <= k < 4, s[k].is_some()
    ensures effective(s, empty(), m, k) == s[k]
{}

pub proof fn delete_restores_manifest(s: Store, m: Option<int>, k: int)
    requires s.len() == 4, 0 <= k < 4
    ensures effective(default_modifiers(s, Cmd { set: false, get: false, delete: true, unknown_modifier: false, data: empty() }).stored, empty(), m, k) == m
{}

pub proof fn exactly_one_and_known(s: Store, d: Store)
    ensures !default_modifiers(s, Cmd { set: true, get: true, delete: false, unknown_modifier: false, data: d }).ok,
            !default_modifiers(s, Cmd { set: false, get: false, delete: false, unknown_modifier: false, data: d }).ok,
            !default_modifiers(s, Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: d }).ok,
            default_modifiers(s, Cmd { set: true, get: false, delete: false, unknown_modifier: true, data: d }).stored == s
{}

// Native schedule (values: Mode stored=1, second=2, cmd=3, manifest=4; Other.Sub x=5; Verbose 1=1).
pub open spec fn setc(d: Store) -> Cmd { Cmd { set: true, get: false, delete: false, unknown_modifier: false, data: d } }
pub open spec fn s2() -> Store { default_modifiers(default_modifiers(empty(), setc(seq![None, Some(1int), None, None])).stored, setc(seq![None, None, Some(5int), None])).stored }
pub open spec fn s3() -> Store { default_modifiers(s2(), setc(seq![None, Some(2int), None, None])).stored }
pub open spec fn v0() -> Store { default_modifiers(s3(), setc(seq![None, None, None, Some(1int)])).stored }
pub open spec fn listed(s: Store, k: int, v: int) -> int { b(s[k] == Some(v)) }
pub open spec fn none_listed(s: Store) -> int { b(s[1].is_none() && s[2].is_none() && s[3].is_none()) }
pub open spec fn mode_code(o: Option<int>) -> int { match o { Some(v) => v, None => 0 } }
pub open spec fn verbose_load(stored: Store, cmd: Store) -> int { b(merged(stored, cmd)[3] == Some(1int)) }

pub proof fn schedule()
    ensures
        listed(s2(), 1, 1) == 1, listed(s2(), 2, 5) == 1, listed(s3(), 1, 2) == 1, listed(s3(), 1, 1) == 0,
        mode_code(effective(s3(), empty(), Some(4), 1)) == 2,
        mode_code(effective(s3(), seq![None, Some(3int), None, None], Some(4), 1)) == 3,
        verbose_load(v0(), empty()) == 1, verbose_load(v0(), seq![None, None, None, Some(0int)]) == 0,
        mode_code(effective(empty(), empty(), Some(4), 1)) == 4,
{
    assert(s2() =~= seq![None, Some(1int), Some(5int), None]);
    assert(s3() =~= seq![None, Some(2int), Some(5int), None]);
    assert(v0() =~= seq![None, Some(2int), Some(5int), Some(1int)]);
}

} // verus!

fn main() {}
