// Iteration75 (repair group R9): loading or installing the installed version. IPM 571e04a1.
//
// The admission in iter18 reloading.rs is the constant skip_equal_load=false over a literal "1.0.0" comparator.
// The successor `reload75` replaces it with the product guard computed from the installed and the new version:
// Utils/Module.cls:1211-1222 (LoadNewModule) refuses when fromVersion.Follows(toVersion) (a strict downgrade),
// and refuses an install/load without -force or developer mode when toVersion.Follows(fromVersion) and the new
// manifest names an UpdatePackage. Nothing else stands between the guard and the manifest and payload load, so an
// equal version passes and is loaded again. Follows is the inherited semver.rs::follows
// (General/SemanticVersion.cls:123-180; `post` is the SemVerPostRelease setting).
// The successful path reuses reloading.rs::successful_selected_load (module record, payload and history row from
// the selected supplier, under the successful-provider premises stated in reloading.rs).
// Premise: `from` and `to` are the parsed versions of the installed record and the new manifest. The reloading
// world carries only the literal 1.0.0, so the downgrade neighbour is stated for every world.
// The same guard is computed over abstract version ranks in playbook update_steps.rs::refuses.
#[path="../../consolidated/iter18/reloading.rs"]
pub mod rl;
#[path="../../consolidated/verus/semver.rs"]
pub mod sv;
use vstd::prelude::*;

verus! {

use rl::{World, Request, Result, request, alpha, beta, initial, after_a, payload, supplier, installed_version,
    exact_request, successful_selected_load};
use sv::{Version, follows};

pub open spec fn load_guard(from: Option<Version>, to: Version, post: bool, install_or_load: bool, force: bool,
    dev: bool, has_update_package: bool) -> bool {
    match from {
        None => false,
        Some(f) => follows(f, to, post) || (install_or_load && !force && !dev && follows(to, f, post) && has_update_package),
    }
}

pub struct Mode { pub post: bool, pub install_or_load: bool, pub force: bool, pub dev: bool, pub has_update_package: bool }

pub open spec fn reload75(w: World, r: Request, from: Option<Version>, to: Version, m: Mode) -> Result {
    if !exact_request(r) { Result { ok: false, world: w, assessed: false } }
    else {
        match rl::inherited_provenance::select(w, r) {
            None => Result { ok: false, world: w, assessed: true },
            Some(q) => if load_guard(from, to, m.post, m.install_or_load, m.force, m.dev, m.has_update_package) {
                Result { ok: false, world: w, assessed: true }
            } else {
                Result { ok: true, world: successful_selected_load(w, q), assessed: true }
            },
        }
    }
}

pub open spec fn v(major: nat) -> Version { Version { major, minor: 0, patch: 0, pre: seq![], build: seq![] } }
pub open spec fn plain(post: bool) -> Mode {
    Mode { post, install_or_load: true, force: false, dev: false, has_update_package: true }
}

proof fn not_snapshot()
    ensures !sv::snapshot(v(1)), !sv::snapshot(v(2))
{
    assert(v(1).build.map_values(|c: char| sv::lower(c)).len() == 0);
    assert(seq!['s', 'n', 'a', 'p', 's', 'h', 'o', 't'].len() == 8);
}

// An equal version passes the guard under either SemVerPostRelease setting, with an update package and without
// -force or -dev; a strict downgrade is refused; a guarded upgrade is refused only without -force.
pub proof fn guard_neighbors() {
    not_snapshot();
    assert(!load_guard(Some(v(1)), v(1), false, true, false, false, true));
    assert(!load_guard(Some(v(1)), v(1), true, true, false, false, true));
    assert(!load_guard(None, v(1), false, true, false, false, true));
    assert(load_guard(Some(v(2)), v(1), false, true, true, true, false));
    assert(load_guard(Some(v(1)), v(2), false, true, false, false, true));
    assert(!load_guard(Some(v(1)), v(2), false, true, true, false, true));
}

// load.same-version#1: first install from alpha, then the same 1.0.0 from beta succeeds and loads beta's payload
// with beta as the supplier.
pub proof fn same_version_reload_neighbor() {
    not_snapshot();
    let first = reload75(initial(), request(alpha()), None, v(1), plain(false));
    assert(first.ok && first.world == after_a());
    assert(payload(after_a()) == 1);
    let second = reload75(after_a(), request(beta()), Some(v(1)), v(1), plain(false));
    assert(second.ok);
    assert(payload(second.world) == 2);
    assert(supplier(second.world) == beta());
    assert(installed_version(second.world) == rl::inherited_provenance::version());
}

// A refused attempt (strict downgrade) leaves the world as it was.
pub proof fn downgrade_refused(w: World, r: Request, from: Version, to: Version, m: Mode)
    requires exact_request(r), rl::inherited_provenance::select(w, r).is_some(), follows(from, to, m.post)
    ensures !reload75(w, r, Some(from), to, m).ok, reload75(w, r, Some(from), to, m).world == w
{}

pub proof fn downgrade_instance() {
    not_snapshot();
    assert(follows(v(2), v(1), false));
    rl::module_neighbor();
    downgrade_refused(after_a(), request(beta()), v(2), v(1), plain(false));
    assert(payload(reload75(after_a(), request(beta()), Some(v(2)), v(1), plain(false)).world) == 1);
}

}
fn main() {}
