// Iteration75 neighbours for ORAS tags, the publishing managers' availability cache and the ORAS availability
// adapter (repo.oras.tag-decode #1 #3, repo.publish-cache #1 #3, repo.oras #5).
//
// Every deciding computation is an inherited definition, imported unchanged and faithful to pin 571e04a1:
// - iter15 oras.rs `tag_for` and iter8 catalog.rs `tag_for`: Oras/PackageService.cls:270-280 TagForReference
//   (tag = $$$Semver2Tag(version), Common.inc:82 replaces "+" with "_"; a deployed reference with an empty
//   PlatformVersion throws; a deployed tag appends $$$OrasTagPlatformSeparator "__" and the platform; a source
//   reference ignores PlatformVersion).
// - iter2 registry.rs `remote_cache`: Remote/PublishManager.cls:12-43 CheckServiceCache (UnavailableNames
//   first, then a cached client is returned with pAvailable = 1 and no Enabled read or IsAvailable call;
//   otherwise Enabled is read and IsAvailable decides which cache the client joins).
// - iter2 registry.rs `oras_cache`: Oras/PublishManager.cls:4-20 CheckServiceCache (no cache: Enabled is
//   read and IsAvailable is called on every call).
// - verus thin_surfaces.rs `oras_available`: Oras/PackageService.cls:93-109 IsAvailable (a transport error
//   gives 0; 2xx gives 1; 401 gives 1 only with a WWW-Authenticate header; anything else gives 0).
#[path = "../../consolidated/iter15/oras.rs"]
mod oras;
#[path = "../../consolidated/iter8/catalog.rs"]
mod catalog;
#[path = "../../consolidated/iter2/registry.rs"]
mod registry;
#[path = "../../consolidated/verus/thin_surfaces.rs"]
mod thin;

use vstd::prelude::*;

verus! {

// tag-decode #1: a source artifact's tag is the version with "+" encoded as "_" and no platform suffix; a
// deployed artifact's tag also carries "__<platform>".
pub proof fn source_tag_is_encoded_version()
    ensures
        oras::tag_for(oras::build_version(), false, seq![]) == Some(oras::encode(oras::build_version())),
        oras::encode(oras::build_version()) != oras::build_version(),
        oras::tag_for(oras::build_version(), true, oras::current())
            == Some(oras::joined(oras::encode(oras::build_version()), oras::current())),
        // A source reference ignores a platform value it happens to carry.
        oras::tag_for(oras::v1(), false, oras::current()) == Some(oras::v1()),
{
    assert(oras::encode(oras::build_version())
        == seq!['1','.','2','.','3','-','a','l','p','h','a','_','b','u','i','l','d','.','7']) by (compute);
    assert(oras::encode(oras::build_version()) != oras::build_version()) by (compute);
    assert(oras::encode(oras::v1()) == oras::v1()) by (compute);
}

// tag-decode #3: only a deployed reference needs a selected platform; a source reference is never refused.
pub proof fn only_deployed_reference_needs_platform()
    ensures
        oras::tag_for(oras::v1(), true, seq![]).is_none(),
        oras::tag_for(oras::v1(), false, seq![]).is_some(),
        oras::tag_for(oras::v1(), true, oras::current()).is_some(),
        catalog::tag_for(catalog::Artifact { version: 1, deployed: true, platforms: set![26int] }, None).is_none(),
        catalog::tag_for(catalog::Artifact { version: 1, deployed: true, platforms: set![26int] }, Some(26))
            == Some(catalog::Tag::Deployed(1, 26)),
        catalog::tag_for(catalog::Artifact { version: 1, deployed: false, platforms: Set::empty() }, None)
            == Some(catalog::Tag::Source(1)),
{
    assert(oras::tag_for(oras::v1(), true, seq![]).is_none()) by (compute);
    assert(oras::tag_for(oras::v1(), false, seq![]).is_some()) by (compute);
    assert(oras::tag_for(oras::v1(), true, oras::current()).is_some()) by (compute);
}

// publish-cache #1: a retained available Remote client is reused without reading Enabled or probing again,
// even when the repository is now disabled and the provider would now answer unavailable. A manager with no
// retained client probes an enabled repository, and that probe decides.
pub proof fn retained_remote_client_is_not_reprobed()
    ensures ({
        let kept = registry::ServiceState { available_cached: true, unavailable_cached: false };
        let fresh = registry::ServiceState { available_cached: false, unavailable_cached: false };
        &&& registry::remote_cache(kept, false, true, false).available
        &&& !registry::remote_cache(kept, false, true, false).provider_called
        &&& !registry::remote_cache(kept, true, true, true).provider_called
        &&& registry::remote_cache(fresh, true, true, true).provider_called
        &&& registry::remote_cache(fresh, true, true, true).available
        &&& !registry::remote_cache(fresh, true, true, false).available
        &&& registry::remote_cache(fresh, true, true, false).state.unavailable_cached
        &&& !registry::remote_cache(fresh, false, true, true).provider_called
    })
{}

// publish-cache #3: the ORAS manager keeps nothing between calls, so each call's Enabled and probe decide.
// With Enabled set, an available probe gives an available manager and an unavailable probe on the next call
// gives an unavailable one; the Remote manager's retained client stays available in the same sequence.
pub proof fn oras_manager_probes_each_call()
    ensures
        registry::oras_cache(true, true),
        !registry::oras_cache(true, false),
        !registry::oras_cache(false, true),
        ({
            let first = registry::remote_cache(
                registry::ServiceState { available_cached: false, unavailable_cached: false }, true, true, true);
            registry::remote_cache(first.state, true, true, false).available
        }),
{}

// repo.oras #5: IsAvailable's five cases, through the exec adapter's contract.
pub fn oras_available_neighbors() {
    let ok200 = thin::oras_available(true, 200, false);
    assert(ok200);
    let ok299 = thin::oras_available(true, 299, false);
    assert(ok299);
    let ok401h = thin::oras_available(true, 401, true);
    assert(ok401h);
    let no401 = thin::oras_available(true, 401, false);
    assert(!no401);
    let no403h = thin::oras_available(true, 403, true);
    assert(!no403h);
    let no300 = thin::oras_available(true, 300, true);
    assert(!no300);
    let notransport = thin::oras_available(false, 200, true);
    assert(!notransport);
}

} // verus!

fn main() {}
