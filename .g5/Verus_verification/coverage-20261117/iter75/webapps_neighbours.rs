//! Iteration-75 neighbours for generated web-app resources (devtools.generate.webapps#3 and #4).
//!
//! Storage/ModuleTemplate.cls AddWebApps (lines 182-236) decides both:
//! - :203-205 skips IsNameSpaceDefault, Name, NameSpace and Path in the generic property loop, so the
//!   installed namespace and the app-default marker never reach the declaration. The inherited iter8
//!   `generated_props` and `skipped` follow this and are reused.
//! - :220-223 writes Path only when DispatchClass is empty and the old Path is not, as
//!   `$replace(props("Path"), cspDir, "{$cspdir}")`. $replace substitutes every occurrence of the CSP
//!   root, not only a leading one, so the inherited prefix test `manifest_path` is not faithful for a
//!   path that contains the root after its start. `generated_path75` is the corrected successor.
#[path = "../../consolidated/iter8/webapps.rs"]
mod webapps;

use vstd::prelude::*;
use webapps::*;

verus! {

pub enum Tok { Ch(int), CspDir }

/// `$replace(raw, root, "{$cspdir}")`: left to right, non-overlapping, every occurrence.
pub open spec fn replace_root(raw: Seq<int>, root: Seq<int>) -> Seq<Tok>
    decreases raw.len()
{
    if raw.len() == 0 { seq![] }
    else if root.len() > 0 && root.len() <= raw.len() && raw.take(root.len() as int) =~= root {
        seq![Tok::CspDir] + replace_root(raw.skip(root.len() as int), root)
    } else {
        seq![Tok::Ch(raw[0])] + replace_root(raw.drop_first(), root)
    }
}

pub open spec fn lit(raw: Seq<int>) -> Seq<Tok>
    decreases raw.len()
{
    if raw.len() == 0 { seq![] } else { seq![Tok::Ch(raw[0])] + lit(raw.drop_first()) }
}

/// ModuleTemplate.cls:220-223: no Path for a dispatch app or an app without a path.
pub open spec fn generated_path75(dispatch: int, path: Option<Seq<int>>, root: Seq<int>) -> Option<Seq<Tok>> {
    if dispatch != 0 { None } else { match path { Some(p) => if p.len() > 0 { Some(replace_root(p, root)) } else { None }, None => None } }
}

/// devtools.generate.webapps#3: a provider NamespaceDefault and Namespace that differ from the defaults are
/// still left out, while another differing property (Description) is written.
pub proof fn namespace_and_default_marker_left_out() {
    let defaults = map![Key::Authentication => 1int, Key::NamespaceDefault => 0int, Key::Description => 0int];
    let props = map![Key::Authentication => 1int, Key::Namespace => 99int, Key::NamespaceDefault => 1int,
        Key::Description => 5int];
    let p = generated_props(props, defaults, Map::empty());
    assert(!p.contains_key(Key::NamespaceDefault));
    assert(!p.contains_key(Key::Namespace));
    assert(p.contains_key(Key::Description) && p[Key::Description] == 5);
    assert(p.contains_key(Key::Authentication));
}

/// devtools.generate.webapps#4 on concrete paths, root [1,2]:
/// under the root [1,2,3] -> {$cspdir}3; outside it [7,8] -> literal; dispatch -> no Path;
/// as built, a path containing the root after its start [7,1,2,3] -> 7{$cspdir}3.
pub proof fn path_neighbours() {
    let root = seq![1int, 2];
    reveal_with_fuel(replace_root, 6);
    reveal_with_fuel(lit, 4);
    let under = seq![1int, 2, 3];
    assert(under.take(2) =~= root);
    assert(under.skip(2) =~= seq![3int]);
    assert(seq![3int].drop_first() =~= Seq::<int>::empty());
    assert(replace_root(under, root) =~= seq![Tok::CspDir, Tok::Ch(3)]);
    assert(generated_path75(0, Some(under), root) == Some(seq![Tok::CspDir, Tok::Ch(3)]));

    let outside = seq![7int, 8];
    assert(outside.drop_first() =~= seq![8int]);
    assert(seq![8int].drop_first() =~= Seq::<int>::empty());
    assert(!(outside.take(2) =~= root));
    assert(replace_root(outside, root) =~= seq![Tok::Ch(7), Tok::Ch(8)]);
    assert(replace_root(outside, root) =~= lit(outside));
    assert(generated_path75(0, Some(outside), root) == Some(seq![Tok::Ch(7), Tok::Ch(8)]));

    let inner = seq![7int, 1, 2, 3];
    assert(!(inner.take(2) =~= root));
    assert(inner.drop_first() =~= under);
    assert(replace_root(inner, root) =~= seq![Tok::Ch(7), Tok::CspDir, Tok::Ch(3)]);
    assert(generated_path75(0, Some(inner), root) == Some(seq![Tok::Ch(7), Tok::CspDir, Tok::Ch(3)]));

    assert(generated_path75(7, Some(under), root).is_none());
    assert(generated_path75(0, None, root).is_none());
    assert(generated_path75(0, Some(Seq::<int>::empty()), root).is_none());
}

fn main() {}

} // verus!
