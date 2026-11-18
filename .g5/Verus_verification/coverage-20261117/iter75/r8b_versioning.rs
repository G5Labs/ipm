// Iteration75 (repair group R8B): `module-version` applies -prerelease and -build after the bump. IPM 571e04a1.
//
// owners.rs::module_version, ::bump, ::with_pre and ::with_build are inherited unchanged and faithful:
// Main.cls:4031-4049 computes the bumped or parsed version, then WithPrerelease (General/SemanticVersion.cls:245-254,
// keeps Build) and WithBuild (:256-265, keeps Prerelease); WithBump* clear both (:212-243).
// -force skips the "strictly higher" check (Main.cls:4051), so these neighbours do not depend on prerelease order.
#[path="../../consolidated/verus/owners.rs"]
pub mod own;
use vstd::prelude::*;

verus! {

use own::{ModuleState, Request, Error, module_version};
use own::semver::Version;

pub open spec fn v123() -> Version { Version { major: 1, minor: 2, patch: 3, pre: seq![], build: seq![] } }
pub open spec fn st() -> ModuleState { ModuleState { record: v123(), manifest: v123(), disk: v123() } }

pub proof fn pre_and_build_applied() {
    let rc = seq!['r', 'c'];
    let b1 = seq!['b', '1'];
    // patch -prerelease rc
    let a = module_version(st(), Request::Patch, Some(rc), None, true, true, true, true, true, true, 0);
    assert(a.ok);
    assert(a.state.record == Version { major: 1, minor: 2, patch: 4, pre: rc, build: seq![] });
    assert(a.state.manifest == a.state.record);
    // patch -build b1
    let b = module_version(st(), Request::Patch, None, Some(b1), true, true, true, true, true, true, 0);
    assert(b.state.record == Version { major: 1, minor: 2, patch: 4, pre: seq![], build: b1 });
    // minor -prerelease rc -build b1
    let c = module_version(st(), Request::Minor, Some(rc), Some(b1), true, true, true, true, true, true, 0);
    assert(c.state.record == Version { major: 1, minor: 3, patch: 0, pre: rc, build: b1 });
    // neither flag: the plain bump
    let d = module_version(st(), Request::Patch, None, None, true, true, true, true, true, true, 0);
    assert(d.state.record == Version { major: 1, minor: 2, patch: 4, pre: seq![], build: seq![] });
}

}
fn main() {}
