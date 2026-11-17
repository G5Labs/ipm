#[path = "../iter68/admission.rs"]
mod admission;

use admission::*;
use vstd::prelude::*;

verus! {

// L2 transcription of the protected ORAS retrieval in the #281 implementation.
// Identifiers are canonical values: `registry` is the repo URL's host, `path` the namespace and package,
// `tag` the TagForReference result, and digests and sizes are those of the bytes as received.

pub enum Role { Releases, Targets, Other }

// One row of `notary list <gun> -r targets/releases,targets`.
pub struct Row { pub tag: int, pub digest: int, pub size: nat, pub role: Role }

// What notary returns to ContentTrust.LookUpSignedTargets.
pub enum Listing {
    // Exit 0. Premise: notary verified the TUF chain for the trust server under the pinned root.
    // `true_rows` are the verified targets and `rows` the rows parsed from notary's text output.
    // `names_safe` is UnsafeTargetName finding no target name with whitespace or a control character.
    Verified { server: int, root: int, rows: Seq<Row>, true_rows: Seq<Row>, names_safe: bool },
    // "does not have trust data": an empty, verified answer.
    NoTrustData,
    // Any other failure, including an unreachable server, a replaced root, a bad signature or no verifier.
    Failed,
}

// The repo definition's TrustServer and the trust data notary has pinned for it.
pub struct Config { pub registry: int, pub trust_server: int, pub pinned_root: int }

// Per shell command snapshot of SignedTargets, keyed by repo name, trust server and repository (GUN).
pub struct Key { pub repo: int, pub server: int, pub gun: int }

pub struct Cache { pub entries: Map<Key, Listing> }

// Bytes the registry returned when asked for a digest, as hashed and counted by the Python helpers.
pub struct Received { pub digest: int, pub size: nat }

// The signed manifest as parsed: the layers whose media type is the package layer type.
pub struct Parsed { pub package_layers: Seq<Descriptor> }

pub struct Effects {
    pub provider_unpacked: bool,
    pub stream_returned: bool,
    pub consumer_unpacks: bool,
}

pub open spec fn trusted_role(role: Role) -> bool {
    match role { Role::Releases => true, Role::Targets => true, Role::Other => false }
}

// ParseTargets: the first row for tag whose role is trusted. notary prints one row per tag, chosen by role priority.
pub open spec fn select(rows: Seq<Row>, tag: int) -> Option<Row>
    decreases rows.len(),
{
    if rows.len() == 0 {
        None
    } else if rows[0].tag == tag && trusted_role(rows[0].role) {
        Some(rows[0])
    } else {
        select(rows.subrange(1, rows.len() as int), tag)
    }
}

// LookUpSignedTargets deletes the cached timestamp before running notary. notary falls back to cached trust data
// only while it holds an unexpired cached timestamp, so an unreachable server then yields Failed.
pub open spec fn notary_list(reachable: bool, cached_timestamp: bool, answer: Listing, cached: Listing) -> Listing {
    if reachable { answer } else if cached_timestamp { cached } else { Listing::Failed }
}

pub open spec fn implemented_list(reachable: bool, answer: Listing, cached: Listing) -> Listing {
    notary_list(reachable, false, answer, cached)
}

// SignedTargets: the first outcome for a key within a command is kept for the rest of the command.
pub open spec fn lookup(cache: Cache, key: Key, fresh: Listing) -> (Listing, Cache) {
    if cache.entries.contains_key(key) {
        (cache.entries[key], cache)
    } else {
        (fresh, Cache { entries: cache.entries.insert(key, fresh) })
    }
}

pub open spec fn key_for(repo: int, config: Config, gun: int) -> Key {
    Key { repo, server: config.trust_server, gun }
}

// TrustedTarget: the signed row for tag, if the listing verified one for this trust server and pinned root.
pub open spec fn trusted_row(config: Config, listing: Listing, tag: int) -> Option<Row> {
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } =>
            if server == config.trust_server && root == config.pinned_root && names_safe { select(rows, tag) } else { None },
        _ => None,
    }
}

// Premise on notary's text output: when no target name contains whitespace, each printed row is one verified target.
// A name with a tab or newline could otherwise print as a row of a role that did not sign it.
pub open spec fn faithful(listing: Listing) -> bool {
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } => names_safe ==> rows == true_rows,
        _ => true,
    }
}

// PackageLayer: exactly one package layer, else the signature does not identify one package.
pub open spec fn package_layer(parsed: Parsed) -> Option<Descriptor> {
    if parsed.package_layers.len() == 1 { Some(parsed.package_layers[0]) } else { None }
}

pub enum Outcome { Refused, Stream { package: Descriptor } }

// GetTrustedModule: TrustedTarget, FetchTrustedManifest, PackageLayer, DownloadTrustedBlob, then the stream.
pub open spec fn retrieve(
    config: Config, listing: Listing, tag: int, manifest: Received, parsed: Parsed, blob: Received,
) -> Outcome {
    match trusted_row(config, listing, tag) {
        None => Outcome::Refused,
        Some(row) =>
            if manifest.digest == row.digest && manifest.size == row.size {
                match package_layer(parsed) {
                    None => Outcome::Refused,
                    Some(layer) =>
                        if blob.digest == layer.digest && blob.size == layer.size {
                            Outcome::Stream { package: layer }
                        } else {
                            Outcome::Refused
                        },
                }
            } else {
                Outcome::Refused
            },
    }
}

// The protected path downloads raw bytes only. IPM's loader unpacks the archive only from a returned stream.
pub open spec fn effects(outcome: Outcome) -> Effects {
    let returned = outcome is Stream;
    Effects { provider_unpacked: false, stream_returned: returned, consumer_unpacks: returned }
}

// ---- Abstraction to the admission contract (L1) ----

// Docker's rule, independent of the filter above: docker pull trusts targets/releases, then targets.
pub open spec fn docker_trusts(role: Role) -> bool {
    role is Releases || role is Targets
}

// The signed row docker pull would use for tag: notary's row for it, if its role is one docker trusts.
pub open spec fn docker_row(rows: Seq<Row>, tag: int) -> Option<Row>
    decreases rows.len(),
{
    if rows.len() == 0 {
        None
    } else if rows[0].tag == tag && docker_trusts(rows[0].role) {
        Some(rows[0])
    } else {
        docker_row(rows.subrange(1, rows.len() as int), tag)
    }
}

// The admission contract's single package descriptor: a manifest with no or several package layers has none.
pub open spec fn single_package(parsed: Parsed) -> Option<Descriptor> {
    if parsed.package_layers.len() == 1 { Some(parsed.package_layers[0]) } else { None }
}

pub open spec fn abs_target(config: Config, path: int, tag: int) -> Target {
    Target { registry: config.registry, repository_path: path, tag }
}

// The active policy is the configured trust server, the root pinned for it and the command's snapshot.
pub open spec fn abs_policy(config: Config, snapshot: int) -> Policy {
    Policy { identity: config.trust_server, revision: snapshot, signer: config.pinned_root }
}

pub open spec fn abs_manifest(manifest: Received, parsed: Parsed) -> Manifest {
    Manifest { digest: manifest.digest, package: single_package(parsed) }
}

pub open spec fn abs_evidence(
    config: Config, path: int, tag: int, snapshot: int, listing: Listing,
) -> Verification {
    match listing {
        Listing::Failed => Verification::Unavailable,
        Listing::NoTrustData => Verification::Missing,
        Listing::Verified { server, root, rows, true_rows, names_safe } =>
            if server != config.trust_server || root != config.pinned_root {
                Verification::Rejected
            } else {
                match docker_row(true_rows, tag) {
                    None => Verification::Missing,
                    Some(row) => Verification::Authorized {
                        target: abs_target(config, path, tag),
                        policy: abs_policy(config, snapshot),
                        manifest_digest: row.digest,
                        signer: root,
                    },
                }
            },
    }
}

pub open spec fn abs_observed(blob: Received) -> Descriptor {
    Descriptor { digest: blob.digest, size: blob.size }
}

// ParseTargets' filter selects exactly the row docker would.
proof fn lemma_select_is_docker_row(rows: Seq<Row>, tag: int)
    ensures
        select(rows, tag) == docker_row(rows, tag),
        match select(rows, tag) {
            Some(row) => row.tag == tag && docker_trusts(row.role),
            None => true,
        },
    decreases rows.len(),
{
    if rows.len() > 0 && !(rows[0].tag == tag && trusted_role(rows[0].role)) {
        lemma_select_is_docker_row(rows.subrange(1, rows.len() as int), tag);
    }
}

// A returned stream is admitted by the contract, and nothing is unpacked unless it is.
pub proof fn stream_refines_admission(
    config: Config, path: int, tag: int, snapshot: int, listing: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires
        faithful(listing),
    ensures
        ({
            let outcome = retrieve(config, listing, tag, manifest, parsed, blob);
            let decision = protected_admission(
                abs_target(config, path, tag), abs_policy(config, snapshot), abs_manifest(manifest, parsed),
                abs_evidence(config, path, tag, snapshot, listing), abs_observed(blob));
            &&& (outcome is Stream ==> decision.may_extract && decision.may_replace_root)
            &&& (effects(outcome).provider_unpacked ==> decision.may_extract)
            &&& (effects(outcome).consumer_unpacks ==> decision.may_extract)
            &&& (outcome is Stream ==> outcome->package == abs_observed(blob))
        }),
{
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } => lemma_select_is_docker_row(rows, tag),
        _ => {},
    }
}

// A trusted signature on bytes the registry serves unchanged is accepted. The signer recorded the size of the
// manifest whose digest it signed, so equal digests imply equal sizes (collision resistance of sha256).
// A repository whose trust data has a target name that is not safe to read is refused as a whole.
pub proof fn admitted_artifact_installs(
    config: Config, path: int, tag: int, snapshot: int, listing: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires
        faithful(listing),
        listing is Verified ==> listing->names_safe,
        protected_admission(
            abs_target(config, path, tag), abs_policy(config, snapshot), abs_manifest(manifest, parsed),
            abs_evidence(config, path, tag, snapshot, listing), abs_observed(blob)).may_extract,
        match trusted_row(config, listing, tag) {
            Some(row) => manifest.digest == row.digest ==> manifest.size == row.size,
            None => true,
        },
    ensures
        retrieve(config, listing, tag, manifest, parsed, blob) is Stream,
{
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } => lemma_select_is_docker_row(rows, tag),
        _ => {},
    }
}

// Outage, a missing or untrusted signature, a replaced root and missing verifier all refuse before any unpacking.
pub proof fn failures_refuse(
    config: Config, path: int, tag: int, snapshot: int, listing: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires
        faithful(listing),
        !(abs_evidence(config, path, tag, snapshot, listing) is Authorized),
    ensures
        retrieve(config, listing, tag, manifest, parsed, blob) is Refused,
        !effects(retrieve(config, listing, tag, manifest, parsed, blob)).provider_unpacked,
        !effects(retrieve(config, listing, tag, manifest, parsed, blob)).consumer_unpacks,
{
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } => lemma_select_is_docker_row(rows, tag),
        _ => {},
    }
}

// With the cached timestamp removed, an unreachable trust server always refuses, even when valid cached trust data
// would otherwise be used.
pub proof fn outage_refuses(
    config: Config, path: int, tag: int, snapshot: int, answer: Listing, cached: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    ensures
        retrieve(config, implemented_list(false, answer, cached), tag, manifest, parsed, blob) is Refused,
        abs_evidence(config, path, tag, snapshot, implemented_list(false, answer, cached)) is Unavailable,
{
}

// Only targets/releases and targets signatures are trusted.
pub proof fn only_trusted_roles(config: Config, listing: Listing, tag: int)
    ensures
        match trusted_row(config, listing, tag) {
            Some(row) => docker_trusts(row.role) && row.tag == tag,
            None => true,
        },
{
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } => lemma_select_is_docker_row(rows, tag),
        _ => {},
    }
}

// Within a command, the listing used for selection and dependency metadata is the one used for the payload,
// whatever a later notary run would have returned, so a retag in between cannot change the manifest consumed.
pub proof fn snapshot_pins_metadata_and_payload(
    cache: Cache, repo: int, config: Config, gun: int, tag: int,
    first: Listing, second: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    ensures
        ({
            let (metadata_listing, after) = lookup(cache, key_for(repo, config, gun), first);
            let (payload_listing, _) = lookup(after, key_for(repo, config, gun), second);
            &&& payload_listing == metadata_listing
            &&& (retrieve(config, payload_listing, tag, manifest, parsed, blob) is Stream ==>
                    match trusted_row(config, metadata_listing, tag) {
                        Some(row) => row.digest == manifest.digest,
                        None => false,
                    })
        }),
{
}

// The snapshot is keyed by trust server, so trust data cached for one server never answers for another,
// e.g. after the repo is reconfigured to another trust server within the same process.
pub proof fn snapshot_keyed_by_server(
    cache: Cache, repo: int, config: Config, other: Config, gun: int, first: Listing, second: Listing,
)
    requires
        other.trust_server != config.trust_server,
        !cache.entries.contains_key(key_for(repo, other, gun)),
    ensures
        lookup(lookup(cache, key_for(repo, config, gun), first).1, key_for(repo, other, gun), second).0 == second,
{
}

// The metadata resolution the plan used and the admitted payload agree on the authenticated manifest identity.
pub proof fn plan_and_payload_share_identity(
    config: Config, path: int, tag: int, snapshot: int, listing: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires
        faithful(listing),
        retrieve(config, listing, tag, manifest, parsed, blob) is Stream,
    ensures
        match resolve(abs_target(config, path, tag), abs_policy(config, snapshot), abs_manifest(manifest, parsed),
            abs_evidence(config, path, tag, snapshot, listing)) {
            Some(previous) => continue_from_metadata(previous, abs_target(config, path, tag),
                abs_policy(config, snapshot), abs_manifest(manifest, parsed),
                abs_evidence(config, path, tag, snapshot, listing), abs_observed(blob)),
            None => false,
        },
{
    stream_refines_admission(config, path, tag, snapshot, listing, manifest, parsed, blob);
}

fn main() {}

} // verus!
