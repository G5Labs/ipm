#![allow(non_snake_case)]
// Iteration51: `information` lists installed top-level modules.
// Source roles at pin 571e04a1: Main.cls::Information (installed modules minus every name that appears as a
// dependency, compared case-insensitively because stored names are normalized to lowercase; external
// name as label, "External (name)" with -verbose; version without build metadata), BuildIntroMessages,
// GetSystemMode (intro lines). Premise: the module-name datatype stores names in lowercase (native).
use vstd::prelude::*;

verus! {

pub struct M { pub name: int, pub external: bool, pub has_build: bool }
pub open spec fn is_dep(deps: Seq<int>, n: int) -> bool
    decreases deps.len()
{
    if deps.len() == 0 { false } else { deps.last() == n || is_dep(deps.drop_last(), n) }
}
pub open spec fn listed(m: M, deps: Seq<int>) -> bool { !is_dep(deps, m.name) }
pub open spec fn label_full(m: M, verbose: bool) -> bool { m.external }
pub open spec fn shows_build(m: M) -> bool { false }

// Fixture names: 1 dep51, 2 mixeddep51 (stored lowercase), 3 ext51, 4 top51; top51 depends on 1 and 2.
pub open spec fn deps() -> Seq<int> { seq![1, 2] }
pub proof fn neighbors()
    ensures listed(M { name: 4, external: false, has_build: false }, deps()),
            !listed(M { name: 1, external: false, has_build: false }, deps()),
            !listed(M { name: 2, external: false, has_build: false }, deps()),
            listed(M { name: 3, external: true, has_build: true }, deps()),
            label_full(M { name: 3, external: true, has_build: true }, true),
            !label_full(M { name: 3, external: true, has_build: true }, false),
            !shows_build(M { name: 3, external: true, has_build: true })
{
    reveal_with_fuel(is_dep, 3);
}

} // verus!

fn main() {}
