//! In-process OCI image pull, extract, and content-addressed layer cache.
//!
//! Replaces the rev-5 `skopeo` + `umoci` shell-out path used to prepare a
//! customer's `base_image` rootfs. See ontology `g5/g5d/oci` for the full
//! design contract (D1-D14). The legacy shell-out was deleted in PR #108;
//! this module fills the placeholder `resolve_rootfs` left behind it.
//!
//! DockerHub (PR-rev7-5a), GHCR / `ghcr.io` (PR-rev7-5b), Google Artifact
//! Registry (`*-docker.pkg.dev`, PR-rev7-5c), Amazon ECR
//! (`<acct>.dkr.ecr.<region>.amazonaws.com`, PR-rev7-5d) and Azure ACR
//! (`*.azurecr.io`, PR-rev7-5e) are all supported. Each registry adds a
//! per-registry `CredentialProvider` on top of the shared `DistributionClient`
//! core (D15) so `mint_locked`'s singleflight + evict-stale-and-remint
//! invariants apply uniformly.

pub mod acr;
pub mod auth;
pub mod cache;
pub mod ecr;
pub mod error;
pub mod extract;
pub mod gar;
pub mod layout;
pub mod registry;
mod template;
mod temporary;

pub use cache::LayerCache;
pub use error::OciError;
pub use extract::{MaterializedRootfs, OciExtractor};
pub use layout::is_oci_layout_dir;
pub use registry::{
    detect_registry, parse_image_ref, upstream_client, AcrClient, BlobStream, Digest,
    DistributionClient, DockerHubClient, EcrClient, GarClient, GhcrClient, ImageConfig, ImageRef,
    RawManifest, Reference, Registry, RegistryClient,
};
pub use template::{is_complete as template_is_complete, ROOTFS_DIR as TEMPLATE_ROOTFS_DIR};

#[cfg(test)]
mod acr_tests;
#[cfg(test)]
mod ecr_tests;
#[cfg(test)]
mod end_to_end_tests;
#[cfg(test)]
mod gar_tests;
#[cfg(test)]
mod ghcr_tests;
