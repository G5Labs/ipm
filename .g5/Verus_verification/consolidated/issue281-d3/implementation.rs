#[path = "../iter68/admission.rs"]
mod admission;
#[path = "../iter15/oras.rs"]
mod oras;

use admission::*;
use oras::{Text, Group, has_char, has_text, metadata_tag, tag_for, encode, decode, joined, replace, replace_cons};
use vstd::prelude::*;

verus! {

// L2 transcription of the protected ORAS retrieval in the #281 D3+ candidate. It revises the #281 iteration70 model
// (../iter70/implementation.rs, kept unchanged) with the first-use transport rule, one shared byte check, and a
// catalog composed with iteration15's version groups, metadata tag and install tag. Cross-invocation verifier state
// is in invocations.rs, repository configuration in configuration.rs.
//
// Identifiers: `registry` is the repo URL's host, `path` the namespace and package, tags are iteration15 Text,
// digests and sizes are those of the bytes as received. tag_id maps a tag to the contract's tag identity.

pub uninterp spec fn tag_id(t: Text) -> int;

pub enum Role { Releases, Targets, Other }

// One row of `notary list <gun> -r targets/releases,targets`.
pub struct Row { pub tag: Text, pub digest: int, pub size: nat, pub role: Role }

// What notary returns to ContentTrust.LookUpSignedTargets.
pub enum Listing {
    // Exit 0. `server` and `root` are the trust server and root notary verified the TUF chain against.
    // `true_rows` are the verified targets and `rows` the rows parsed from notary's text output.
    // `names_safe` is UnsafeTargetName finding no target name with whitespace or a control character.
    Verified { server: int, root: int, rows: Seq<Row>, true_rows: Seq<Row>, names_safe: bool },
    // "does not have trust data": an empty, verified answer.
    NoTrustData,
    // Any other failure, including an unreachable server, a replaced root, a bad signature or no verifier.
    Failed,
}

pub enum Scheme { Https, Http }

// The repo definition's TrustServer (its scheme and canonical URL) and the root pinned for it, if any.
pub struct Config { pub registry: int, pub trust_server: int, pub scheme: Scheme, pub pinned_root: Option<int> }

// The root the trust server holds for the repository, and the root notary received when it contacted it.
pub struct Contact { pub authentic_root: int, pub presented_root: int }

// ---- Verifier ----

// LookUpSignedTargets refuses a trust server that is not https before running notary, as docker does.
pub open spec fn transport_allowed(config: Config) -> bool {
    config.scheme is Https
}

// LookUpSignedTargets runs `notary -d <private directory> -s TrustServer list <gun> ...`.
pub open spec fn invoked_server(config: Config) -> int {
    config.trust_server
}

// Premise on TLS (notary checks the server's certificate chain and host name; the trust server is trusted, as in
// docker): over https the root notary receives is the one the trust server holds.
pub open spec fn tls_authenticated(config: Config, contact: Contact) -> bool {
    config.scheme is Https ==> contact.presented_root == contact.authentic_root
}

// Premise maintained by invocations.rs and this model's first use: a pinned root was accepted from the trust server.
pub open spec fn pins_authentic(config: Config, contact: Contact) -> bool {
    match config.pinned_root { Some(r) => r == contact.authentic_root, None => true }
}

// notary checks against the pinned root, or on first use pins and checks against the root presented.
pub open spec fn checked_root(config: Config, contact: Contact) -> int {
    match config.pinned_root { Some(r) => r, None => contact.presented_root }
}

// Premise on notary (the reference client, not verified here): exit 0 means it verified the listing against the
// server it was given, under checked_root, including thresholds, expiry and rollback checks.
pub open spec fn notary_verified(config: Config, contact: Contact, listing: Listing) -> bool {
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } =>
            server == invoked_server(config) && root == checked_root(config, contact),
        _ => true,
    }
}

// Premise on notary's text output: when no target name contains whitespace, each printed row is one verified target.
pub open spec fn faithful(listing: Listing) -> bool {
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } => names_safe ==> rows == true_rows,
        _ => true,
    }
}

pub open spec fn trusted_role(role: Role) -> bool {
    match role { Role::Releases => true, Role::Targets => true, Role::Other => false }
}

// ParseTargets: the first row for tag whose role is trusted. notary prints one row per tag, chosen by role priority.
pub open spec fn select(rows: Seq<Row>, tag: Text) -> Option<Row>
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

// TrustedTarget: the signed row for tag, if the transport was allowed and notary verified the listing.
pub open spec fn trusted_row(config: Config, listing: Listing, tag: Text) -> Option<Row> {
    if !transport_allowed(config) {
        None
    } else {
        match listing {
            Listing::Verified { server, root, rows, true_rows, names_safe } =>
                if names_safe { select(rows, tag) } else { None },
            _ => None,
        }
    }
}

// The private trust directory starts without a timestamp (invocations.rs), so an unreachable server fails.
pub open spec fn notary_list(reachable: bool, cached_timestamp: bool, answer: Listing, cached: Listing) -> Listing {
    if reachable { answer } else if cached_timestamp { cached } else { Listing::Failed }
}

pub open spec fn implemented_list(reachable: bool, answer: Listing, cached: Listing) -> Listing {
    notary_list(reachable, false, answer, cached)
}

// ---- Bytes ----

pub struct Received { pub digest: int, pub size: nat }

// read_verified, the one size and digest check FetchTrustedManifest and DownloadTrustedBlob both apply.
pub open spec fn bytes_match(expected: Descriptor, received: Received) -> bool {
    received.digest == expected.digest && received.size == expected.size
}

pub open spec fn row_descriptor(row: Row) -> Descriptor {
    Descriptor { digest: row.digest, size: row.size }
}

// The signed manifest as parsed: the layers whose media type is the package layer type.
pub struct Parsed { pub package_layers: Seq<Descriptor> }

// PackageLayer: exactly one package layer, else the signature does not identify one package.
pub open spec fn package_layer(parsed: Parsed) -> Option<Descriptor> {
    if parsed.package_layers.len() == 1 { Some(parsed.package_layers[0]) } else { None }
}

pub enum Outcome { Refused, Stream { package: Descriptor } }

pub struct Effects { pub provider_unpacked: bool, pub stream_returned: bool, pub consumer_unpacks: bool }

// GetTrustedModule: TrustedTarget, TrustedManifest (by signed digest, read_verified), PackageLayer,
// DownloadTrustedBlob (by layer digest, read_verified), then the stream.
pub open spec fn retrieve(
    config: Config, listing: Listing, tag: Text, manifest: Received, parsed: Parsed, blob: Received,
) -> Outcome {
    match trusted_row(config, listing, tag) {
        None => Outcome::Refused,
        Some(row) =>
            if bytes_match(row_descriptor(row), manifest) {
                match package_layer(parsed) {
                    None => Outcome::Refused,
                    Some(layer) =>
                        if bytes_match(layer, blob) {
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

// ---- Catalog: version groups, metadata tag and install tag (iteration15) ----

// What the registry returns when asked for a manifest by digest.
pub struct Registry { pub by_digest: Map<int, Received> }

// The module.xml and deployed annotation parsed from manifest bytes, keyed by the digest of those bytes.
pub struct Annot { pub deployed: bool, pub names: Set<int> }

pub open spec fn annot_of(annots: Map<int, Annot>, digest: int) -> Annot {
    if annots.contains_key(digest) { annots[digest] } else { Annot { deployed: false, names: Set::empty() } }
}

// TrustedManifest: the manifest of tag fetched by its signed digest, kept only if read_verified accepts it.
pub open spec fn signed_manifest(config: Config, listing: Listing, tag: Text, reg: Registry) -> Option<Received> {
    match trusted_row(config, listing, tag) {
        None => None,
        Some(row) =>
            if reg.by_digest.contains_key(row.digest) && bytes_match(row_descriptor(row), reg.by_digest[row.digest]) {
                Some(reg.by_digest[row.digest])
            } else {
                None
            },
    }
}

// Manager.SearchRepositoriesForModule and ModuleInfo.IsCompatibleWithPlatform.
pub open spec fn compatible(deployed: bool, platforms: Seq<Text>, c: Text) -> bool {
    !deployed || has_text(platforms, c)
}

pub open spec fn selected_platform(deployed: bool, platforms: Seq<Text>, c: Text) -> Text {
    if deployed && has_text(platforms, c) { c } else { seq![] }
}

// The tag GetModule and GetModuleManifest request for the row, through TagForReference (iteration15 tag_for).
pub open spec fn install_tag(g: Group, deployed: bool, c: Text) -> Option<Text> {
    if compatible(deployed, g.platforms, c) {
        tag_for(decode(g.encoded), deployed, selected_platform(deployed, g.platforms, c))
    } else {
        None
    }
}

// AddModulesFromTags: the deployed flag comes from the first platform's signed manifest (iteration15
// metadata_tag), and the row's metadata and names from the signed manifest of the artifact install would retrieve.
pub open spec fn checked_tag(g: Group, deployed: bool, c: Text) -> Text {
    match install_tag(g, deployed, c) { Some(t) => t, None => metadata_tag(g) }
}

pub enum CatalogRow {
    Skipped,
    Offered { deployed: bool, platforms: Seq<Text>, checked_tag: Text, checked: Received },
}

// One version group of a protected repository, looked up by name n. package_matches is the repository's own
// package name equalling n, which offers the version without reading module.xml.
pub open spec fn catalog_row(
    config: Config, listing: Listing, reg: Registry, annots: Map<int, Annot>, g: Group, c: Text, n: int,
    package_matches: bool,
) -> CatalogRow {
    match signed_manifest(config, listing, metadata_tag(g), reg) {
        None => CatalogRow::Skipped,
        Some(first) => {
            let deployed = annot_of(annots, first.digest).deployed;
            let t = checked_tag(g, deployed, c);
            match signed_manifest(config, listing, t, reg) {
                None => CatalogRow::Skipped,
                Some(m) =>
                    if annot_of(annots, m.digest).deployed != deployed {
                        CatalogRow::Skipped
                    } else if !package_matches && !annot_of(annots, m.digest).names.contains(n) {
                        CatalogRow::Skipped
                    } else {
                        CatalogRow::Offered { deployed, platforms: g.platforms, checked_tag: t, checked: m }
                    },
            }
        },
    }
}

// ---- Command snapshot and stalled servers ----

// Per shell command snapshot of SignedTargets, keyed by repo name, trust server and repository (GUN).
pub struct Key { pub repo: int, pub server: int, pub gun: int }

// `stalled` holds the (repo name, trust server) pairs whose trust server timed out earlier in the command.
pub struct Cache { pub entries: Map<Key, Listing>, pub stalled: Set<(int, int)> }

pub open spec fn lookup(cache: Cache, key: Key, fresh: Listing, timed_out: bool) -> (Listing, Cache) {
    if cache.entries.contains_key(key) {
        (cache.entries[key], cache)
    } else if cache.stalled.contains((key.repo, key.server)) {
        (Listing::Failed, cache)
    } else {
        let outcome = if timed_out { Listing::Failed } else { fresh };
        (outcome, Cache {
            entries: cache.entries.insert(key, outcome),
            stalled: if timed_out { cache.stalled.insert((key.repo, key.server)) } else { cache.stalled },
        })
    }
}

pub open spec fn key_for(repo: int, config: Config, gun: int) -> Key {
    Key { repo, server: config.trust_server, gun }
}

// ---- Abstraction to the admission contract (L1) ----

pub open spec fn docker_trusts(role: Role) -> bool {
    role is Releases || role is Targets
}

pub open spec fn docker_row(rows: Seq<Row>, tag: Text) -> Option<Row>
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

pub open spec fn single_package(parsed: Parsed) -> Option<Descriptor> {
    if parsed.package_layers.len() == 1 { Some(parsed.package_layers[0]) } else { None }
}

pub open spec fn abs_target(config: Config, path: int, tag: Text) -> Target {
    Target { registry: config.registry, repository_path: path, tag: tag_id(tag) }
}

// The active policy is the configured trust server, the command's snapshot and the trust server's own root.
pub open spec fn abs_policy(config: Config, contact: Contact, snapshot: int) -> Policy {
    Policy { identity: config.trust_server, revision: snapshot, signer: contact.authentic_root }
}

pub open spec fn abs_manifest(manifest: Received, parsed: Parsed) -> Manifest {
    Manifest { digest: manifest.digest, package: single_package(parsed) }
}

pub open spec fn abs_evidence(
    config: Config, contact: Contact, path: int, tag: Text, snapshot: int, listing: Listing,
) -> Verification {
    match listing {
        Listing::Failed => Verification::Unavailable,
        Listing::NoTrustData => Verification::Missing,
        Listing::Verified { server, root, rows, true_rows, names_safe } =>
            if server != config.trust_server || root != contact.authentic_root {
                Verification::Rejected
            } else {
                match docker_row(true_rows, tag) {
                    None => Verification::Missing,
                    Some(row) => Verification::Authorized {
                        target: abs_target(config, path, tag),
                        policy: abs_policy(config, contact, snapshot),
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

pub open spec fn premises(config: Config, contact: Contact, listing: Listing) -> bool {
    &&& faithful(listing)
    &&& notary_verified(config, contact, listing)
    &&& tls_authenticated(config, contact)
    &&& pins_authentic(config, contact)
}

// ---- Theorems ----

proof fn lemma_select_is_docker_row(rows: Seq<Row>, tag: Text)
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

// A listing IPM accepts was verified under the trust server's own root, on first use as on later checks.
pub proof fn accepted_root_is_authentic(config: Config, contact: Contact, listing: Listing, tag: Text)
    requires premises(config, contact, listing)
    ensures
        trusted_row(config, listing, tag) is Some ==> transport_allowed(config) && match listing {
            Listing::Verified { server, root, rows, true_rows, names_safe } => root == contact.authentic_root,
            _ => false,
        },
{}

// A returned stream is admitted by the contract, and nothing is unpacked unless it is.
pub proof fn stream_refines_admission(
    config: Config, contact: Contact, path: int, tag: Text, snapshot: int, listing: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires premises(config, contact, listing)
    ensures
        ({
            let outcome = retrieve(config, listing, tag, manifest, parsed, blob);
            let decision = protected_admission(
                abs_target(config, path, tag), abs_policy(config, contact, snapshot), abs_manifest(manifest, parsed),
                abs_evidence(config, contact, path, tag, snapshot, listing), abs_observed(blob));
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

// A trusted signature on bytes the registry serves unchanged is accepted over https.
pub proof fn admitted_artifact_installs(
    config: Config, contact: Contact, path: int, tag: Text, snapshot: int, listing: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires
        premises(config, contact, listing),
        transport_allowed(config),
        listing is Verified ==> listing->names_safe,
        protected_admission(
            abs_target(config, path, tag), abs_policy(config, contact, snapshot), abs_manifest(manifest, parsed),
            abs_evidence(config, contact, path, tag, snapshot, listing), abs_observed(blob)).may_extract,
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

// Outage, a missing or untrusted signature, a replaced root, a missing verifier and an http trust server all
// refuse before any unpacking.
pub proof fn failures_refuse(
    config: Config, contact: Contact, path: int, tag: Text, snapshot: int, listing: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires
        premises(config, contact, listing),
        !(abs_evidence(config, contact, path, tag, snapshot, listing) is Authorized) || !transport_allowed(config),
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

// With no timestamp in the check's directory, an unreachable trust server refuses, even when valid cached trust
// data would otherwise be used.
pub proof fn outage_refuses(
    config: Config, contact: Contact, path: int, tag: Text, snapshot: int, answer: Listing, cached: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    ensures
        retrieve(config, implemented_list(false, answer, cached), tag, manifest, parsed, blob) is Refused,
        abs_evidence(config, contact, path, tag, snapshot, implemented_list(false, answer, cached)) is Unavailable,
{}

// Only targets/releases and targets signatures are trusted.
pub proof fn only_trusted_roles(config: Config, listing: Listing, tag: Text)
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

// The manifest and the package pass the same size and digest check: bytes that differ from their descriptor in
// size or digest are refused at either step.
pub proof fn one_check_for_manifest_and_package(
    config: Config, listing: Listing, tag: Text, manifest: Received, parsed: Parsed, blob: Received,
)
    ensures
        retrieve(config, listing, tag, manifest, parsed, blob) is Stream ==>
            match (trusted_row(config, listing, tag), package_layer(parsed)) {
                (Some(row), Some(layer)) =>
                    manifest.digest == row.digest && manifest.size == row.size
                    && blob.digest == layer.digest && blob.size == layer.size,
                _ => false,
            },
{}

// A protected repo offers a version, and reads its metadata, only from a manifest the contract authorizes for
// that tag. Whatever the registry lists, an unsigned tag is not offered.
pub proof fn signed_manifest_is_authorized(
    config: Config, contact: Contact, path: int, tag: Text, snapshot: int, listing: Listing, reg: Registry,
)
    requires premises(config, contact, listing)
    ensures
        match signed_manifest(config, listing, tag, reg) {
            Some(read) => match resolve(abs_target(config, path, tag), abs_policy(config, contact, snapshot),
                    Manifest { digest: read.digest, package: None },
                    abs_evidence(config, contact, path, tag, snapshot, listing)) {
                Some(previous) => previous.manifest_digest == read.digest,
                None => false,
            },
            None => true,
        },
{
    match listing {
        Listing::Verified { server, root, rows, true_rows, names_safe } => lemma_select_is_docker_row(rows, tag),
        _ => {},
    }
}

// Tags have no '+', which TagForReference encodes as '_', so decoding and encoding again gives the same text.
pub proof fn encode_decode(s: Text)
    requires !has_char(s, '+')
    ensures encode(decode(s)) == s
    decreases s.len()
{
    reveal_with_fuel(replace, 3);
    if s.len() > 0 {
        assert(s[0] != '+');
        encode_decode(s.skip(1));
        assert(decode(s) == seq![if s[0] == '_' { '+' } else { s[0] }] + decode(s.skip(1)));
        replace_cons(if s[0] == '_' { '+' } else { s[0] }, decode(s.skip(1)), '+', '_');
        assert(encode(decode(s)) == seq![s[0]] + encode(decode(s.skip(1))));
        assert(s == seq![s[0]] + s.skip(1));
    }
}

// An offered row this platform can install was matched on the name, external name and deployed flag of the signed
// manifest of the tag install will request, so the alias belongs to the installed artifact. The current platform
// (CurrentPlatformVersion) is never empty.
pub proof fn catalog_checks_installed_artifact(
    config: Config, listing: Listing, reg: Registry, annots: Map<int, Annot>, g: Group, c: Text, n: int,
    package_matches: bool,
)
    requires
        c.len() > 0,
        catalog_row(config, listing, reg, annots, g, c, n, package_matches) is Offered,
        compatible(catalog_row(config, listing, reg, annots, g, c, n, package_matches)->deployed, g.platforms, c),
    ensures
        ({
            let row = catalog_row(config, listing, reg, annots, g, c, n, package_matches);
            &&& install_tag(g, row->deployed, c) == Some(row->checked_tag)
            &&& signed_manifest(config, listing, row->checked_tag, reg) == Some(row->checked)
            &&& annot_of(annots, row->checked.digest).deployed == row->deployed
            &&& (package_matches || annot_of(annots, row->checked.digest).names.contains(n))
        }),
{}

// Composed with the command snapshot: the package a command installs for an offered row is the one listed by the
// signed manifest whose names and deployed flag the catalog matched.
pub proof fn offered_name_is_installed_artifact(
    cache: Cache, repo: int, config: Config, gun: int, reg: Registry, annots: Map<int, Annot>, g: Group, c: Text,
    n: int, package_matches: bool, first: Listing, second: Listing, first_timed_out: bool, second_timed_out: bool,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires c.len() > 0
    ensures
        ({
            let (catalog_listing, after) = lookup(cache, key_for(repo, config, gun), first, first_timed_out);
            let (payload_listing, _) = lookup(after, key_for(repo, config, gun), second, second_timed_out);
            let row = catalog_row(config, catalog_listing, reg, annots, g, c, n, package_matches);
            (row is Offered && compatible(row->deployed, g.platforms, c)
                && retrieve(config, payload_listing, install_tag(g, row->deployed, c)->0, manifest, parsed, blob) is Stream)
                ==> manifest.digest == row->checked.digest
                    && (package_matches || annot_of(annots, manifest.digest).names.contains(n))
        }),
{
    let (catalog_listing, after) = lookup(cache, key_for(repo, config, gun), first, first_timed_out);
    let row = catalog_row(config, catalog_listing, reg, annots, g, c, n, package_matches);
    if row is Offered && compatible(row->deployed, g.platforms, c) {
        catalog_checks_installed_artifact(config, catalog_listing, reg, annots, g, c, n, package_matches);
    }
}

// Within a command, the listing used for selection and dependency metadata is the one used for the payload,
// whatever a later notary run would have returned, so a retag in between cannot change the manifest consumed.
pub proof fn snapshot_pins_metadata_and_payload(
    cache: Cache, repo: int, config: Config, gun: int, tag: Text,
    first: Listing, second: Listing, first_timed_out: bool, second_timed_out: bool,
    manifest: Received, parsed: Parsed, blob: Received,
)
    ensures
        ({
            let (metadata_listing, after) = lookup(cache, key_for(repo, config, gun), first, first_timed_out);
            let (payload_listing, _) = lookup(after, key_for(repo, config, gun), second, second_timed_out);
            &&& payload_listing == metadata_listing
            &&& (retrieve(config, payload_listing, tag, manifest, parsed, blob) is Stream ==>
                    match trusted_row(config, metadata_listing, tag) {
                        Some(row) => row.digest == manifest.digest,
                        None => false,
                    })
        }),
{
}

// The snapshot is keyed by trust server, so trust data cached for one server never answers for another.
pub proof fn snapshot_keyed_by_server(
    cache: Cache, repo: int, config: Config, other: Config, gun: int, first: Listing, second: Listing,
    first_timed_out: bool,
)
    requires
        other.trust_server != config.trust_server,
        !cache.entries.contains_key(key_for(repo, other, gun)),
        !cache.stalled.contains((repo, other.trust_server)),
    ensures
        lookup(lookup(cache, key_for(repo, config, gun), first, first_timed_out).1,
            key_for(repo, other, gun), second, false).0 == second,
{
}

// A trust server that stalled earlier in the command only refuses.
pub proof fn stalled_server_only_refuses(
    cache: Cache, key: Key, fresh: Listing, timed_out: bool, config: Config, tag: Text,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires
        !cache.entries.contains_key(key),
        cache.stalled.contains((key.repo, key.server)),
    ensures
        lookup(cache, key, fresh, timed_out).0 is Failed,
        retrieve(config, lookup(cache, key, fresh, timed_out).0, tag, manifest, parsed, blob) is Refused,
{
}

// After the trust server times out for one repository, the next repository of that repo on that server fails at
// once, whatever notary would have answered.
pub proof fn timeout_stalls_server(
    cache: Cache, repo: int, server: int, gun: int, other_gun: int, first: Listing, second: Listing,
    second_timed_out: bool,
)
    requires
        gun != other_gun,
        !cache.entries.contains_key(Key { repo, server, gun }),
        !cache.entries.contains_key(Key { repo, server, gun: other_gun }),
    ensures
        ({
            let (_, after) = lookup(cache, Key { repo, server, gun }, first, true);
            lookup(after, Key { repo, server, gun: other_gun }, second, second_timed_out).0 is Failed
        }),
{
}

// The metadata resolution the plan used and the admitted payload agree on the authenticated manifest identity.
pub proof fn plan_and_payload_share_identity(
    config: Config, contact: Contact, path: int, tag: Text, snapshot: int, listing: Listing,
    manifest: Received, parsed: Parsed, blob: Received,
)
    requires
        premises(config, contact, listing),
        retrieve(config, listing, tag, manifest, parsed, blob) is Stream,
    ensures
        match resolve(abs_target(config, path, tag), abs_policy(config, contact, snapshot), abs_manifest(manifest, parsed),
            abs_evidence(config, contact, path, tag, snapshot, listing)) {
            Some(previous) => continue_from_metadata(previous, abs_target(config, path, tag),
                abs_policy(config, contact, snapshot), abs_manifest(manifest, parsed),
                abs_evidence(config, contact, path, tag, snapshot, listing), abs_observed(blob)),
            None => false,
        },
{
    stream_refines_admission(config, contact, path, tag, snapshot, listing, manifest, parsed, blob);
}

// ---- Native-shaped neighbours ----

// Two signed variants of 1.0.0: one for another platform, listed first, whose module.xml gives the external name
// 701, and one for this platform giving 702. The catalog matches on this platform's artifact.
pub open spec fn older() -> Text { oras::bad() }
pub open spec fn here() -> Text { oras::current() }
pub open spec fn two_variants() -> Group { Group { encoded: oras::v1(), platforms: seq![older(), here()] } }
pub open spec fn n_config() -> Config { Config { registry: 1, trust_server: 2, scheme: Scheme::Https, pinned_root: Some(3) } }
pub open spec fn n_listing() -> Listing {
    let rows = seq![
        Row { tag: joined(oras::v1(), older()), digest: 11, size: 100, role: Role::Targets },
        Row { tag: joined(oras::v1(), here()), digest: 12, size: 100, role: Role::Targets },
    ];
    Listing::Verified { server: 2, root: 3, rows, true_rows: rows, names_safe: true }
}
pub open spec fn n_registry() -> Registry {
    Registry { by_digest: map![11int => Received { digest: 11, size: 100 }, 12int => Received { digest: 12, size: 100 }] }
}
pub open spec fn n_annots() -> Map<int, Annot> {
    map![11int => Annot { deployed: true, names: set![701int] }, 12int => Annot { deployed: true, names: set![702int] }]
}

pub proof fn platform_alias_neighbours()
    ensures
        catalog_row(n_config(), n_listing(), n_registry(), n_annots(), two_variants(), here(), 702, false) is Offered,
        catalog_row(n_config(), n_listing(), n_registry(), n_annots(), two_variants(), here(), 701, false) is Skipped,
        catalog_row(n_config(), n_listing(), n_registry(), n_annots(), two_variants(), here(), 0, true) is Offered,
        // The same trust data over http offers nothing.
        catalog_row(Config { scheme: Scheme::Http, ..n_config() }, n_listing(), n_registry(), n_annots(), two_variants(),
            here(), 702, false) is Skipped,
{
    assert(metadata_tag(two_variants()) == joined(oras::v1(), older())) by (compute);
    assert(has_text(two_variants().platforms, here())) by (compute);
    assert(tag_for(decode(oras::v1()), true, here()) == Some(joined(oras::v1(), here()))) by (compute);
    assert(joined(oras::v1(), older()) != joined(oras::v1(), here())) by (compute);
    assert(select(seq![
        Row { tag: joined(oras::v1(), older()), digest: 11, size: 100, role: Role::Targets },
        Row { tag: joined(oras::v1(), here()), digest: 12, size: 100, role: Role::Targets },
    ], joined(oras::v1(), older())) == Some(Row { tag: joined(oras::v1(), older()), digest: 11, size: 100, role: Role::Targets }));
    reveal_with_fuel(select, 3);
}

fn main() {}

} // verus!
