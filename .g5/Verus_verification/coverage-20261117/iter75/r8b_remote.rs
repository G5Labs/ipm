// Iteration75 (repair group R8B): remote unpublish credentials and the artifact download request. IPM 571e04a1.
//
// registry.rs::remote_delete takes `authorized` as an input. The successor `remote_unpublish` computes it with the
// inherited registry.rs::remote_authorized, which transcribes the guard that Repo/Remote/PublishService.cls repeats
// in Publish (:16), CheckUnpublishEnabled (:42) and Unpublish (:60): ((Username="")||(Password=""))&&(Token="")
// throws "authorization required" before any request. Unpublish then succeeds only on status 200 (:63-72).
//
// artifacts.rs::activate is inherited unchanged and faithful for the request: ResourceProcessor/
// ArtifactoryTarball.cls:32-43 OnAfterPhase(Activate) calls DownloadArtifact only when
// <Root>/<resource name>/ does not exist.
#[path="../../consolidated/iter2/registry.rs"]
pub mod reg;
#[path="../../consolidated/iter3/artifacts.rs"]
pub mod art;
use vstd::prelude::*;

verus! {

use reg::{remote_authorized, remote_delete, remote_publish};
use art::{activate, download, Auth, Download};

pub open spec fn remote_unpublish(user: bool, password: bool, token: bool, transport_ok: bool, status: int) -> bool {
    remote_delete(remote_authorized(user, password, token), transport_ok, status)
}

// build.publish.partial-effects#4: deletion needs a user with a password, or a token, and exactly 200.
pub proof fn unpublish_credential_neighbors() {
    assert(remote_unpublish(true, true, false, true, 200));
    assert(remote_unpublish(false, false, true, true, 200));
    assert(!remote_unpublish(true, false, false, true, 200));
    assert(!remote_unpublish(false, true, false, true, 200));
    assert(!remote_unpublish(false, false, false, true, 200));
    assert(!remote_unpublish(true, true, false, true, 204));
    // Publish uses the same guard.
    assert(!remote_publish(false, false, false, true, true, 200));
    assert(remote_publish(false, false, true, true, true, 200));
}

// artifact.local-content#1: an absent folder requests a download; a present one does not and is copied as is.
pub proof fn local_folder_neighbors() {
    let fetched = download(true, Auth::Bearer, Download::Complete);
    assert(activate(true, false, fetched, true).request_download);
    assert(activate(true, false, fetched, true).copy_called);
    assert(!activate(true, true, fetched, true).request_download);
    assert(activate(true, true, fetched, true).copy_called);
    // Outside Activate nothing is requested.
    assert(!activate(false, false, fetched, true).request_download);
}

}
fn main() {}
