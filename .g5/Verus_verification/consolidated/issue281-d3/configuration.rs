// #281 D3+: `repo ... -trust-server <url>` across repository types, and the consumers that carry the stored value.
//
// Source roles (bindings in .g5/uplift/consolidated/issue281-d3/bindings.json):
//   Repo/Definition.cls::Configure            refuses -trust-server for a type without TRUSTSERVERSUPPORTED before
//                                             creating or changing anything; saves the instance last
//   Repo/Definition.cls::TRUSTSERVERSUPPORTED 0 for every type; Repo/Oras/Definition.cls overrides it with 1
//   Repo/Oras/Definition.cls::OnConfigure     normalizes -trust-server before -publish 1 saves another repo
//   Repo/Oras/Definition.cls::NormalizeTrustServer   "" clears; otherwise https only, canonical form
//   Repo/Oras/Definition.cls::GetPackageService, GetPublishService, LockFileMapping and Main.cls copy-from's
//                                             %ConstructClone(1)   carry TrustServer unchanged
//   Repo/Oras/ContentTrust.cls::LookUpSignedTargets  refuses a stored value that is not https before running notary
//
// Repository table: the configured repository and whether another repository holds the default deployment flag.
// URLs are abstract: `canonical` is the canonical form NormalizeTrustServer returns for a valid URL.
use vstd::prelude::*;

verus! {

pub enum Kind { Oras, Remote, Filesystem }
pub enum Scheme { Https, Http, Other }
pub struct Url { pub scheme: Scheme, pub valid: bool, pub canonical: int }

// trust == 0 means no trust server; otherwise the canonical https URL.
pub struct Repo { pub kind: Kind, pub trust: int, pub publish: bool }
pub struct Table { pub target: Option<Repo>, pub other_publishes: bool }

pub enum TrustMod { Absent, Clear, Set { url: Url } }
pub struct Mods { pub kind: Kind, pub trust: TrustMod, pub publish: bool }

pub open spec fn supports_trust(kind: Kind) -> bool { kind is Oras }

// NormalizeTrustServer: a valid https URL in canonical form, else a refusal.
pub open spec fn normalize(url: Url) -> Option<int> {
    if url.scheme is Https && url.valid && url.canonical != 0 { Some(url.canonical) } else { None }
}

// Configure's type check, before any instance is created or saved.
pub open spec fn type_refused(m: Mods) -> bool { !(m.trust is Absent) && !supports_trust(m.kind) }

// OnConfigure's first step for an ORAS repository.
pub open spec fn trust_refused(m: Mods) -> bool {
    match m.trust { TrustMod::Set { url } => normalize(url) is None, _ => false }
}

// -publish 1 takes the default deployment flag from whichever repository had it and saves that repository.
pub open spec fn after_publish(t: Table, m: Mods) -> Table {
    if m.publish { Table { other_publishes: false, ..t } } else { t }
}

pub open spec fn new_trust(t: Table, m: Mods) -> int {
    match m.trust {
        TrustMod::Absent => match t.target { Some(r) => r.trust, None => 0 },
        TrustMod::Clear => 0,
        TrustMod::Set { url } => match normalize(url) { Some(c) => c, None => 0 },
    }
}

// The whole command: (table afterwards, whether it succeeded). A refusal returns the table it was given.
pub open spec fn configure(t: Table, m: Mods) -> (Table, bool) {
    if type_refused(m) {
        (t, false)
    } else if trust_refused(m) {
        (t, false)
    } else {
        let published = after_publish(t, m);
        let stored = if supports_trust(m.kind) { new_trust(t, m) } else { 0 };
        let publish = m.publish || match t.target { Some(r) => r.publish, None => false };
        (Table { target: Some(Repo { kind: m.kind, trust: stored, publish }), ..published }, true)
    }
}

// Consumers of a stored definition.
pub struct Service { pub trust: int }
pub open spec fn package_service(r: Repo) -> Service { Service { trust: r.trust } }
pub open spec fn publish_service(r: Repo) -> Service { Service { trust: r.trust } }
pub open spec fn copied(r: Repo) -> Repo { r }
pub struct LockEntry { pub trust_server: int }
pub open spec fn lock_entry(r: Repo) -> LockEntry { LockEntry { trust_server: r.trust } }

// LookUpSignedTargets' guard on the stored value, e.g. one copied from a definition saved before https was required.
pub open spec fn verification_allowed(stored: Url) -> bool { stored.scheme is Https }

// A refused command changes nothing, including the other repository's default deployment flag.
pub proof fn refusal_changes_nothing(t: Table, m: Mods)
    ensures !configure(t, m).1 ==> configure(t, m).0 == t
{}

// Only an ORAS repository takes -trust-server. Other types refuse it rather than ignore it.
pub proof fn only_oras_takes_trust(t: Table, m: Mods)
    ensures configure(t, m).1 && !(m.trust is Absent) ==> m.kind is Oras
{}

// A trust server that is accepted is stored, in canonical https form, on the configured repository.
pub proof fn set_trust_is_stored_https(t: Table, m: Mods, url: Url)
    requires m.trust == (TrustMod::Set { url })
    ensures
        configure(t, m).1 ==> url.scheme is Https && url.valid
            && configure(t, m).0.target == Some(Repo { kind: m.kind, trust: url.canonical,
                publish: m.publish || match t.target { Some(r) => r.publish, None => false } }),
{}

// An empty value removes the trust server of an ORAS repository, which then behaves as an unprotected one.
pub proof fn clear_removes_trust(t: Table, m: Mods)
    requires m.trust is Clear, m.kind is Oras
    ensures configure(t, m).1, configure(t, m).0.target->0.trust == 0
{}

// An http trust server is refused, and a stored one is not used.
pub proof fn http_refused(t: Table, m: Mods, url: Url)
    requires m.trust == (TrustMod::Set { url }), url.scheme is Http
    ensures !configure(t, m).1, configure(t, m).0 == t, !verification_allowed(url)
{}

// Every consumer of the definition carries the stored trust server unchanged.
pub proof fn consumers_keep_trust(r: Repo)
    ensures
        package_service(r).trust == r.trust,
        publish_service(r).trust == r.trust,
        copied(r).trust == r.trust,
        lock_entry(r).trust_server == r.trust,
{}

// Native-shaped neighbours: an http value with -publish 1 leaves the publisher as it was, and a filesystem
// repository refuses the option without being created.
pub proof fn neighbours()
    ensures
        ({
            let t = Table { target: None, other_publishes: true };
            let m = Mods { kind: Kind::Oras, trust: TrustMod::Set { url: Url { scheme: Scheme::Http, valid: true, canonical: 7 } }, publish: true };
            configure(t, m) == (t, false)
        }),
        ({
            let t = Table { target: None, other_publishes: true };
            let m = Mods { kind: Kind::Filesystem, trust: TrustMod::Set { url: Url { scheme: Scheme::Https, valid: true, canonical: 7 } }, publish: false };
            configure(t, m) == (t, false)
        }),
        ({
            let t = Table { target: None, other_publishes: true };
            let m = Mods { kind: Kind::Oras, trust: TrustMod::Set { url: Url { scheme: Scheme::Https, valid: true, canonical: 7 } }, publish: true };
            configure(t, m) == (Table { target: Some(Repo { kind: Kind::Oras, trust: 7, publish: true }), other_publishes: false }, true)
        }),
{}

} // verus!

fn main() {}
