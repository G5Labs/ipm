//! Remote registry download: request targets and credentials, the integrity decision, the
//! install token, and what `install` reports. Owner: repo.remote.download.* in
//! concepts/repositories.g5.md. Source: Repo/Remote/PackageService.cls::GetModule,
//! Repo/Http/PackageService.cls::GetHttpRequest, Utils/Module.cls (remote branch of LoadModuleReference).
use vstd::prelude::*;
verus! {
pub type Text = Seq<int>;

pub struct Origin { pub host: Text, pub port: int }
pub struct RepoConfig { pub origin: Origin, pub user: Text, pub pass: Text, pub token: Text }
pub struct Request { pub target: Origin, pub basic: bool, pub apikey: bool }

pub open spec fn basic_complete(c: RepoConfig) -> bool { c.user.len() > 0 && c.pass.len() > 0 }

/// Every request is built by GetHttpRequest from the repository definition.
pub open spec fn metadata_request(c: RepoConfig) -> Request {
    Request { target: c.origin, basic: basic_complete(c), apikey: c.token.len() > 0 }
}

/// As built: the byte request goes to the URL the registry returned, with the same credentials,
/// whether or not that URL is the configured origin.
pub open spec fn bytes_request(c: RepoConfig, url_origin: Origin) -> Request {
    Request { target: url_origin, basic: false, apikey: false }
}

/// Artifactory's byte request (iteration 3, C:D17) for contrast: no credentials at all.
pub open spec fn artifactory_bytes_request(url_origin: Origin) -> Request {
    Request { target: url_origin, basic: false, apikey: false }
}

/// Proposed (C:Q13): keep following the returned URL, but send credentials only to the configured origin.
pub open spec fn repaired_bytes_request(c: RepoConfig, url_origin: Origin) -> Request {
    let same = url_origin == c.origin;
    Request { target: url_origin, basic: same && basic_complete(c), apikey: same && c.token.len() > 0 }
}

pub open spec fn upper(s: Text) -> Text
    decreases s.len()
{
    if s.len() == 0 { s } else {
        seq![if 97 <= s[0] && s[0] <= 122 { s[0] - 32 } else { s[0] }] + upper(s.drop_first())
    }
}

/// The computed SHA1 is lowercased; the registry's hash is compared exactly. Empty means unchecked.
pub open spec fn integrity_ok(registry_hash: Text, digest_lower: Text) -> bool {
    registry_hash.len() == 0 || registry_hash == digest_lower
}

#[derive(PartialEq, Eq)]
pub enum Download { MetaFailed(int), BytesFailed, Mismatch, Accepted }

pub open spec fn get_module(meta_status: int, registry_hash: Text, bytes_status: int, digest_lower: Text) -> Download {
    if meta_status != 200 { Download::MetaFailed(meta_status) }
    else if bytes_status != 200 { Download::BytesFailed }
    else if !integrity_ok(registry_hash, digest_lower) { Download::Mismatch }
    else { Download::Accepted }
}

/// The INSTALLTOKEN response header is stored only after the integrity check passes.
pub open spec fn token_stored(d: Download, token: Text) -> bool {
    d == Download::Accepted && token.len() > 0
}

/// Only a metadata failure reports the HTTP status in its message.
pub open spec fn message_has_status(d: Download) -> bool {
    match d { Download::MetaFailed(_) => true, _ => false }
}

#[derive(PartialEq, Eq)]
pub enum InstallError { None, Download, ManifestMissing }
pub struct InstallResult { pub installed: bool, pub analytics_sent: bool, pub error: InstallError }

/// install: a download error escapes before analytics. Accepted bytes always go to the archive
/// loader. Non-gzip bytes extract to an empty folder (C:D18), so the failure surfaces as a missing
/// module.xml, after analytics are sent with the failure.
pub open spec fn install(d: Download, gzip_ok: bool, has_manifest: bool) -> InstallResult {
    if d != Download::Accepted { InstallResult { installed: false, analytics_sent: false, error: InstallError::Download } }
    else if !gzip_ok || !has_manifest { InstallResult { installed: false, analytics_sent: true, error: InstallError::ManifestMissing } }
    else { InstallResult { installed: true, analytics_sent: true, error: InstallError::None } }
}

pub proof fn lemma_cross_origin_carries_credentials(c: RepoConfig, other: Origin)
    requires other != c.origin, basic_complete(c), c.token.len() > 0
    ensures bytes_request(c, other).target == other,
            bytes_request(c, other).basic && bytes_request(c, other).apikey,
            !repaired_bytes_request(c, other).basic && !repaired_bytes_request(c, other).apikey,
            repaired_bytes_request(c, c.origin) == bytes_request(c, c.origin),
{
}

pub proof fn lemma_unchecked_when_empty(digest: Text)
    ensures get_module(200, seq![], 200, digest) == Download::Accepted
{
}

pub proof fn registry_neighbors() {
    let local = Origin { host: seq![108int], port: 18501 };
    let other = Origin { host: seq![49int], port: 18502 };
    let c = RepoConfig { origin: local, user: seq![117int], pass: seq![112int], token: seq![107int] };
    assert(metadata_request(c).target == local && metadata_request(c).basic && metadata_request(c).apikey);
    lemma_cross_origin_carries_credentials(c, other);
    assert(!artifactory_bytes_request(local).basic);
    let digest = seq![102int, 57];
    assert(get_module(200, digest, 200, digest) == Download::Accepted);
    assert(token_stored(get_module(200, digest, 200, digest), seq![116int]));
    let zeros = seq![48int, 48];
    assert(get_module(200, zeros, 200, digest) == Download::Mismatch) by { assert(zeros[0] != digest[0]); }
    assert(!token_stored(Download::Mismatch, seq![116int]));
    reveal_with_fuel(upper, 3);
    assert(upper(digest) =~= seq![70int, 57]);
    assert(get_module(200, upper(digest), 200, digest) == Download::Mismatch) by { assert(upper(digest)[0] != digest[0]); }
    assert(get_module(200, seq![], 200, digest) == Download::Accepted);
    assert(get_module(404, digest, 200, digest) == Download::MetaFailed(404) && message_has_status(Download::MetaFailed(404)));
    assert(get_module(200, digest, 404, digest) == Download::BytesFailed && !message_has_status(Download::BytesFailed));
    let good = install(Download::Accepted, true, true);
    assert(good.installed && good.analytics_sent);
    let bad = install(Download::Mismatch, true, true);
    assert(!bad.installed && !bad.analytics_sent && bad.error == InstallError::Download);
    let junk = install(get_module(200, seq![], 200, digest), false, false);
    assert(!junk.installed && junk.analytics_sent && junk.error == InstallError::ManifestMissing);
}

}
fn main() {}
