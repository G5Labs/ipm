//! `g5d oci` — OCI image operations.
//!
//! `g5d oci materialize <image-ref> <dest>` pulls an OCI image reference and
//! extracts it to a rootfs directory using the SAME in-process extractor the
//! microVM backend uses at task-assignment time (`crate::cli::oci::OciExtractor`,
//! also wired into `CloudHypervisorBackend`). Exposing it as a standalone command lets
//! the image-test gate produce a *customer-grade* base rootfs via the exact
//! prod pull+extract path — for the A2.5 in-microVM self-contained check — and
//! is handy for local debugging of the OCI layer.

use g5_contracts::error::G5Error;

use super::{GlobalFlags, OciArgs, OciSubCommand};
use g5d_foundation::oci::OciExtractor;

/// Dispatch `g5d oci` subcommands.
pub async fn execute(args: &OciArgs, _global: &GlobalFlags) -> Result<(), G5Error> {
    match &args.command {
        OciSubCommand::PrepareWorkload(m) => {
            if !cfg!(target_os = "linux") || m.size_mib == 0 {
                return Err(G5Error::Config(
                    "workload preparation requires Linux and positive capacity".into(),
                ));
            }
            let extractor = OciExtractor::from_env();
            let state = g5d_isolation::isolation::microvm::rootfs_state_dir();
            let templates = state.join("templates");
            let path = std::path::Path::new(&m.image);
            let template = if g5d_foundation::oci::is_oci_layout_dir(path) {
                extractor
                    .prepare_rootfs_template_from_layout(path, &templates)
                    .await?
            } else {
                if !m.image.contains("@sha256:") {
                    return Err(G5Error::Config(
                        "delivery templates require an immutable OCI digest".into(),
                    ));
                }
                extractor
                    .prepare_rootfs_template(&m.image, &templates)
                    .await?
            };
            #[cfg(feature = "cloud-hypervisor")]
            if m.prefer_artifact && !g5d_foundation::oci::is_oci_layout_dir(path) {
                if let Err(error) = g5d_isolation::isolation::workload_artifact::import(
                    &m.image,
                    &template,
                    m.size_mib,
                    u64::from(m.size_mib) * u64::from(m.slots),
                )
                .await
                {
                    tracing::warn!(%error, "delivery template rejected; preparing from verified OCI contents");
                }
            }
            g5d_isolation::isolation::microvm::prepare_workload_templates(
                &template,
                m.size_mib,
                m.slots,
                m.repo_seed_manifest.as_deref(),
            )
            .await
            .map_err(|e| G5Error::Config(format!("delivery template preparation: {e:#}")))?;
            if let Some(destination) = &m.export_artifact {
                #[cfg(feature = "cloud-hypervisor")]
                g5d_isolation::isolation::workload_artifact::export(
                    &m.image,
                    &template,
                    m.size_mib,
                    destination,
                )
                .map_err(|e| G5Error::Config(format!("export workload template: {e:#}")))?;
                #[cfg(not(feature = "cloud-hypervisor"))]
                {
                    let _ = destination;
                    return Err(G5Error::Config(
                        "template export requires Cloud Hypervisor support".into(),
                    ));
                }
            }
            println!("Prepared workload templates: {}", template.display());
            Ok(())
        }
        OciSubCommand::Materialize(m) => {
            // `from_env()` honors the same cache/registry configuration the
            // microVM backend uses, so the gate exercises the real pull path.
            let extractor = OciExtractor::from_env();
            let rootfs = extractor.materialize_rootfs(&m.image, &m.dest).await?;
            println!(
                "Materialized OCI rootfs: {} -> {}",
                m.image,
                rootfs.display()
            );
            Ok(())
        }
    }
}
