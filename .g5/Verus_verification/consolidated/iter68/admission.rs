use vstd::prelude::*;

verus! {

// Identifiers represent canonical values at the verifier and OCI boundaries.
pub struct Target {
    pub registry: int,
    pub repository_path: int, // Includes the OCI namespace and external package key.
    pub tag: int, // Includes the chosen platform and encoded build label.
}

pub struct Policy {
    pub identity: int,
    pub revision: int,
    pub signer: int,
}

pub struct Descriptor {
    pub digest: int,
    pub size: nat,
}

pub struct Manifest {
    pub digest: int,
    // Project the single package descriptor selected by the accepted manifest parser.
    pub package: Option<Descriptor>,
}

pub enum Verification {
    Authorized { target: Target, policy: Policy, manifest_digest: int, signer: int },
    Missing,
    Rejected,
    Unavailable,
}

pub open spec fn authorized(
    expected_target: Target, active_policy: Policy, manifest: Manifest, evidence: Verification,
) -> bool {
    match evidence {
        Verification::Authorized { target, policy, manifest_digest, signer } =>
            target == expected_target
                && policy == active_policy
                && signer == active_policy.signer
                && manifest_digest == manifest.digest,
        _ => false,
    }
}

pub open spec fn verified_bytes(manifest: Manifest, observed: Descriptor) -> bool {
    manifest.package == Some(observed)
}

pub struct Decision {
    pub use_manifest: bool,
    pub may_extract: bool,
    pub may_replace_root: bool,
}

pub open spec fn protected_admission(
    target: Target, policy: Policy, manifest: Manifest, evidence: Verification,
    observed: Descriptor,
) -> Decision {
    let use_manifest = authorized(target, policy, manifest, evidence);
    let admitted = use_manifest && verified_bytes(manifest, observed);
    Decision { use_manifest, may_extract: admitted, may_replace_root: admitted }
}

pub proof fn extraction_binds_scope_and_bytes(
    target: Target, policy: Policy, manifest: Manifest, evidence: Verification,
    observed: Descriptor,
)
    ensures
        protected_admission(target, policy, manifest, evidence, observed).may_extract
            ==> match evidence {
                Verification::Authorized { target: scope, policy: checked, manifest_digest, signer } =>
                    scope.registry == target.registry
                    && scope.repository_path == target.repository_path
                    && scope.tag == target.tag
                    && checked == policy
                    && signer == policy.signer
                    && manifest_digest == manifest.digest
                    && match manifest.package {
                        Some(layer) => layer.digest == observed.digest && layer.size == observed.size,
                        None => false,
                    },
                _ => false,
            },
        protected_admission(target, policy, manifest, evidence, observed).may_replace_root
            ==> protected_admission(target, policy, manifest, evidence, observed).may_extract,
{}

pub proof fn trust_failure_has_no_consumer_effect(
    target: Target, policy: Policy, manifest: Manifest, evidence: Verification,
    observed: Descriptor,
)
    requires !authorized(target, policy, manifest, evidence)
    ensures
        !protected_admission(target, policy, manifest, evidence, observed).use_manifest,
        !protected_admission(target, policy, manifest, evidence, observed).may_extract,
        !protected_admission(target, policy, manifest, evidence, observed).may_replace_root,
{}

pub proof fn verified_manifest_with_bad_bytes_only_allows_metadata(
    target: Target, policy: Policy, manifest: Manifest, evidence: Verification,
    observed: Descriptor,
)
    requires authorized(target, policy, manifest, evidence), !verified_bytes(manifest, observed)
    ensures
        protected_admission(target, policy, manifest, evidence, observed).use_manifest,
        !protected_admission(target, policy, manifest, evidence, observed).may_extract,
        !protected_admission(target, policy, manifest, evidence, observed).may_replace_root,
{}

pub proof fn authorized_artifact_can_continue(
    target: Target, policy: Policy, manifest: Manifest, evidence: Verification,
    observed: Descriptor,
)
    requires authorized(target, policy, manifest, evidence), verified_bytes(manifest, observed)
    ensures
        protected_admission(target, policy, manifest, evidence, observed).use_manifest,
        protected_admission(target, policy, manifest, evidence, observed).may_extract,
        protected_admission(target, policy, manifest, evidence, observed).may_replace_root,
{}

pub proof fn cached_evidence_cannot_cross_identity(
    old_target: Target, new_target: Target, old_policy: Policy, new_policy: Policy,
    old_manifest: Manifest, new_manifest: Manifest, observed: Descriptor,
)
    requires old_target != new_target || old_policy != new_policy
        || old_manifest.digest != new_manifest.digest,
    ensures
        !protected_admission(new_target, new_policy, new_manifest,
            Verification::Authorized {
                target: old_target, policy: old_policy,
                manifest_digest: old_manifest.digest, signer: old_policy.signer,
            }, observed).use_manifest,
{}

pub struct Resolution {
    pub target: Target,
    pub policy: Policy,
    pub manifest_digest: int,
}

pub open spec fn resolve(
    target: Target, policy: Policy, manifest: Manifest, evidence: Verification,
) -> Option<Resolution> {
    if authorized(target, policy, manifest, evidence) {
        Some(Resolution { target, policy, manifest_digest: manifest.digest })
    } else {
        None
    }
}

pub open spec fn continue_from_metadata(
    previous: Resolution, target: Target, policy: Policy,
    manifest: Manifest, evidence: Verification, observed: Descriptor,
) -> bool {
    previous.target == target
        && previous.policy == policy
        && previous.manifest_digest == manifest.digest
        && protected_admission(target, policy, manifest, evidence, observed).may_extract
}

pub proof fn resolution_preserves_metadata_identity(
    previous_target: Target, previous_policy: Policy, previous_manifest: Manifest,
    previous_evidence: Verification, target: Target, policy: Policy,
    manifest: Manifest, evidence: Verification, observed: Descriptor,
)
    ensures
        match resolve(previous_target, previous_policy, previous_manifest, previous_evidence) {
            Some(previous) =>
                continue_from_metadata(previous, target, policy, manifest, evidence, observed)
                ==> previous_target == target && previous_policy == policy
                    && previous_manifest.digest == manifest.digest,
            None => !authorized(previous_target, previous_policy, previous_manifest, previous_evidence),
        },
{}

pub proof fn authorized_resolution_has_a_success_path(
    target: Target, policy: Policy, manifest: Manifest, evidence: Verification, observed: Descriptor,
)
    requires authorized(target, policy, manifest, evidence), verified_bytes(manifest, observed)
    ensures
        match resolve(target, policy, manifest, evidence) {
            Some(previous) => continue_from_metadata(previous, target, policy, manifest, evidence, observed),
            None => false,
        },
{}

fn main() {}

} // verus!
