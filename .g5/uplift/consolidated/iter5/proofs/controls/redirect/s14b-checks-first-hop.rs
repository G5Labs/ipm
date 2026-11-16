//! Challenge to iteration 4's C:S14: credentials across a redirect chain.
//! Owner: repo.remote.download.redirect in concepts/repositories.g5.md.
//! Source: Repo/Http/PackageService.cls::GetHttpRequest (FollowRedirect=1, headers and Basic set on the request);
//! Repo/Remote/PackageService.cls::GetModule.
use vstd::prelude::*;
verus! {

pub struct Origin { pub host: int, pub port: int }

/// hops[0] is the URL the registry returned; later hops follow redirects automatically.
pub open spec fn as_built_sends(config: Origin, hops: Seq<Origin>, i: int) -> bool {
    0 <= i < hops.len()
}

/// Iteration 4's proposal: check only the returned URL, then let the request follow redirects.
pub open spec fn s14_sends(config: Origin, hops: Seq<Origin>, i: int) -> bool {
    0 <= i < hops.len() && hops[0] == config
}

/// Revised proposal C:S14b: send credentials only on a hop whose origin is the repository's,
/// which requires handling redirects explicitly instead of letting the request repeat its headers.
pub open spec fn s14b_sends(config: Origin, hops: Seq<Origin>, i: int) -> bool {
    0 <= i < hops.len() && hops[0] == config
}

pub open spec fn leaks(sends: spec_fn(int) -> bool, config: Origin, hops: Seq<Origin>) -> bool {
    exists|i: int| 0 <= i < hops.len() && hops[i] != config && sends(i)
}

pub proof fn lemma_s14_insufficient()
    ensures ({
        let local = Origin { host: 1, port: 18501 };
        let other = Origin { host: 2, port: 18502 };
        let hops = seq![local, other];
        leaks(|i: int| s14_sends(local, hops, i), local, hops)
            && !leaks(|i: int| s14b_sends(local, hops, i), local, hops)
            && s14b_sends(local, hops, 0)
    })
{
    let local = Origin { host: 1, port: 18501 };
    let other = Origin { host: 2, port: 18502 };
    let hops = seq![local, other];
    assert(hops[1] != local);
    assert(s14_sends(local, hops, 1));
}

pub proof fn lemma_s14b_never_leaks(config: Origin, hops: Seq<Origin>)
    ensures !leaks(|i: int| s14b_sends(config, hops, i), config, hops)
{
}

pub proof fn redirect_neighbors() {
    let local = Origin { host: 1, port: 18501 };
    let other = Origin { host: 2, port: 18502 };
    // Observed: same-origin URL redirected to another origin; both hops carried credentials.
    assert(as_built_sends(local, seq![local, other], 1));
    // Iteration 4's direct cross-origin URL is still caught by both proposals.
    assert(!s14_sends(local, seq![other], 0)) by { assert(seq![other][0] != local); }
    assert(!s14b_sends(local, seq![other], 0)) by { assert(seq![other][0] != local); }
    lemma_s14_insufficient();
}

}
fn main() {}
