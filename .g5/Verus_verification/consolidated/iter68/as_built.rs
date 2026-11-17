#[path = "../iter2/registry.rs"]
mod registry;

use vstd::prelude::*;

verus! {

pub struct Trace {
    pub metadata_manifest: int,
    pub pull_manifest: int,
    pub hash_manifest: int,
    pub blob: int,
    pub declared_blob: int,
    pub size_matches: bool,
    pub provider_extracts: bool,
}

pub struct Effects {
    pub provider_extracted: bool,
    pub archive_released: bool,
    pub loader_entered: bool,
    pub installed_root_replaced: bool,
}

pub open spec fn observe(t: Trace, existing_writable_root: bool) -> Effects {
    // Scoped to a successful catalog lookup, provider pull, archive selection and file link.
    let status = registry::pull(true, true, true, true,
        t.size_matches && t.blob == t.declared_blob);
    Effects {
        provider_extracted: t.provider_extracts,
        archive_released: status.status_ok,
        loader_entered: status.status_ok,
        // The modeled replacement has no further filesystem failure.
        installed_root_replaced: status.status_ok && existing_writable_root,
    }
}

pub open spec fn rejected_archive() -> Trace {
    Trace {
        metadata_manifest: 1, pull_manifest: 1, hash_manifest: 1,
        blob: 2, declared_blob: 2, size_matches: false, provider_extracts: true,
    }
}

pub open spec fn changed_tag() -> Trace {
    Trace {
        metadata_manifest: 1, pull_manifest: 2, hash_manifest: 2,
        blob: 2, declared_blob: 2, size_matches: true, provider_extracts: false,
    }
}

pub proof fn rejection_stops_loader_after_provider_effect()
    ensures
        observe(rejected_archive(), true).provider_extracted,
        !observe(rejected_archive(), true).archive_released,
        !observe(rejected_archive(), true).installed_root_replaced,
{}

pub proof fn hash_acceptance_does_not_require_metadata_identity()
    ensures
        changed_tag().metadata_manifest != changed_tag().pull_manifest,
        observe(changed_tag(), false).archive_released,
        observe(changed_tag(), false).loader_entered,
{}

fn main() {}

} // verus!
