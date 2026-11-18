//! Iteration 75: ORAS archive integrity neighbours for IPM 571e04a1. Hand translation, not ObjectScript verification.
//!
//! The deciding comparison is inherited unchanged from iteration 2 (`registry::verify_layers`), which transcribes
//! `Repo/Oras/PackageService.cls::VerifyModuleHashAndSize` lines 1014-1021: skip every layer whose mediaType is not
//! `application/vnd.oci.image.layer.v1.tar`, then return `manifest_size == size and manifest_hash == expected_hash`
//! for the first tar layer, and False when there is none.
//!
//! The iteration-68 `as_built::observe` takes the size comparison as a fixture bool (`size_matches`). `pull_effects`
//! below computes the status from the manifest layers and the pulled file instead, through `verify_layers`, and then
//! applies the same gates: `Pull` throws when the check fails (lines 826-829), `GetModule` throws the failed status
//! before returning the stream (lines 239-246), and only a returned stream reaches
//! `Utils/Module.cls::LoadModuleFromArchive` (lines 142-172), which replaces the installed root.
//!
//! `pull_tag` transcribes `Pull` lines 785-787: an empty tag is replaced by `GetLatestTag`, which returns the last
//! tag the provider lists (lines 920-923, `registry::last_tag`); a given tag is used as given.
#[path = "../../consolidated/iter2/registry.rs"]
mod registry;

use vstd::prelude::*;

verus! {

use registry::{Layer, verify_layers, last_tag, pull};

pub struct PulledFile {
    pub size: int,
    pub digest: int,
}

pub struct Effects {
    pub archive_released: bool,
    pub loader_entered: bool,
    pub installed_root_replaced: bool,
}

/// One ORAS pull whose provider call, tgz selection and file link succeed (the scope of the iteration-68 native
/// comparison). The installed root is replaced only by the loader, and only when the root exists and is writable.
pub open spec fn pull_effects(manifest: Seq<Layer>, file: PulledFile, existing_writable_root: bool) -> Effects {
    let status = pull(true, true, true, true, verify_layers(manifest, file.size, file.digest));
    Effects {
        archive_released: status.status_ok,
        loader_entered: status.status_ok,
        installed_root_replaced: status.status_ok && existing_writable_root,
    }
}

pub open spec fn pull_tag(tag: Option<int>, provider_tags: Seq<int>) -> Option<int> {
    match tag {
        Some(t) => Some(t),
        None => last_tag(provider_tags),
    }
}

pub open spec fn tar(size: int, digest: int) -> Layer { Layer { tar: true, size, digest } }
pub open spec fn other(size: int, digest: int) -> Layer { Layer { tar: false, size, digest } }

/// repo.oras.integrity#3 and #7: each comparison decides on its own. A first tar layer whose size matches but
/// whose digest differs is refused, and so is the converse; both matching is accepted.
pub proof fn size_and_digest_each_decide()
    ensures
        !verify_layers(seq![tar(10, 7)], 10, 8),
        !verify_layers(seq![tar(10, 7)], 11, 7),
        verify_layers(seq![tar(10, 7)], 10, 7),
        // A non-tar layer carrying the pulled size and digest does not stand in for the tar layer.
        !verify_layers(seq![other(10, 7), tar(10, 8)], 10, 7),
        !verify_layers(seq![other(10, 7), tar(11, 7)], 10, 7),
        // No tar layer at all: refused.
        !verify_layers(seq![other(10, 7)], 10, 7),
        // A later matching tar layer does not rescue a digest-only or size-only mismatch in the first.
        !verify_layers(seq![tar(10, 8), tar(10, 7)], 10, 7),
        !verify_layers(seq![tar(11, 7), tar(10, 7)], 10, 7),
{
    reveal_with_fuel(verify_layers, 3);
}

/// repo.oras.archive-boundary#2: on a writable installed root, a matching archive reaches the loader and replaces
/// the root, while a size-only or digest-only mismatch stops before the loader and leaves the root in place.
pub proof fn failed_check_keeps_installed_root()
    ensures
        pull_effects(seq![tar(10, 7)], PulledFile { size: 10, digest: 7 }, true).loader_entered,
        pull_effects(seq![tar(10, 7)], PulledFile { size: 10, digest: 7 }, true).installed_root_replaced,
        !pull_effects(seq![tar(10, 7)], PulledFile { size: 10, digest: 8 }, true).archive_released,
        !pull_effects(seq![tar(10, 7)], PulledFile { size: 10, digest: 8 }, true).loader_entered,
        !pull_effects(seq![tar(10, 7)], PulledFile { size: 10, digest: 8 }, true).installed_root_replaced,
        !pull_effects(seq![tar(10, 7)], PulledFile { size: 11, digest: 7 }, true).archive_released,
        !pull_effects(seq![tar(10, 7)], PulledFile { size: 11, digest: 7 }, true).loader_entered,
        !pull_effects(seq![tar(10, 7)], PulledFile { size: 11, digest: 7 }, true).installed_root_replaced,
{
    reveal_with_fuel(verify_layers, 2);
}

/// General form: whatever the manifest and root, a refused archive never reaches the loader or the root.
pub proof fn refused_archive_never_replaces_root(manifest: Seq<Layer>, file: PulledFile, root: bool)
    requires !verify_layers(manifest, file.size, file.digest)
    ensures
        !pull_effects(manifest, file, root).loader_entered,
        !pull_effects(manifest, file, root).installed_root_replaced,
{
}

/// repo.oras.integrity#1: without a tag the last provider tag is pulled, unsorted; an explicit tag is used as given
/// whatever the provider lists.
pub proof fn tag_selection_neighbours()
    ensures
        pull_tag(None, seq![2, 1]) == Some(1int),
        pull_tag(None, seq![1, 2]) == Some(2int),
        pull_tag(None, Seq::<int>::empty()) == None::<int>,
        pull_tag(Some(1), seq![1, 2]) == Some(1int),
        pull_tag(Some(3), Seq::<int>::empty()) == Some(3int),
{
}

fn main() {}

} // verus!
