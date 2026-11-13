//! Argument contract for the Cloud Hypervisor `g5d vm-exec` supervisor.

use clap::Args;

/// Arguments for the hidden `g5d vm-exec` runtime and smoke-test entry point.
///
/// The concrete isolation backend spawns this command as a child process. With
/// `--report-version`, it reports the VMM and platform kernel metadata
/// without booting a VM; otherwise it boots the supplied rootfs and propagates
/// the guest command's exit status.
#[derive(Args, Debug, Clone)]
pub struct VmExecArgs {
    /// Path to the rootfs directory (extracted OCI image layer).
    ///
    /// Required when actually booting a VM; omitted in `--report-version` mode.
    #[arg(long, required_unless_present = "report_version")]
    pub rootfs: Option<String>,
    /// Number of vCPUs for the microVM.
    #[arg(long, default_value = "2")]
    pub cpus: u8,
    /// RAM in MiB for the microVM.
    #[arg(long, default_value = "512")]
    pub mem_mib: u32,
    /// Volume mounts in host:guest[:ro|:rw] format. Access defaults to read-write.
    #[arg(long, short = 'V')]
    pub volumes: Vec<String>,
    /// Passt host:guest port forwarding, isolated on this VM's loopback address.
    #[arg(long = "port-map", short = 'p')]
    pub port_map: Vec<String>,
    /// Working directory inside the guest.
    #[arg(long)]
    pub workdir: Option<String>,
    /// Environment variables in KEY=VALUE format.
    #[arg(long, short = 'e')]
    pub env: Vec<String>,
    /// Print VMM version and kernel digest without booting a VM.
    #[arg(long)]
    pub report_version: bool,
    /// Total private workload filesystem capacity in MiB, including Docker and builds.
    #[arg(long = "scratch-disk-mib")]
    pub scratch_disk_mib: Option<u32>,
    /// Enter a populated ext4 workload root (Linux, backend-owned OCI clone only).
    /// The scratch size is total filesystem capacity; /work is seed input only.
    #[arg(long)]
    pub block_workload: bool,
    /// Expose hardware virtualization to the guest (`--cpus nested=on`); the
    /// platform kernel must have KVM built in (`g5.architecture.microvm_backend.nested`).
    #[arg(long)]
    pub nested_virtualization: bool,
    /// Internal pre-registration preparation; no guest is booted.
    #[arg(long, hide = true, conflicts_with_all = ["block_workload", "report_version"])]
    pub prepare_block_template: bool,
    /// Prepare and pin a private generic workload before the dispatcher timer.
    #[arg(long, hide = true, conflicts_with_all = ["prepare_block_template", "report_version"])]
    pub prepare_private_workload: bool,
    /// Full published capacity of all slots that must remain free.
    #[arg(long, hide = true, requires = "prepare_block_template")]
    pub block_template_reserve_mib: Option<u64>,
    /// Delivery-only committed bundle manifest; never supplied by a VM.
    #[arg(long, hide = true, requires = "prepare_block_template")]
    pub block_template_repo_seed: Option<std::path::PathBuf>,
    /// Leave a verified seed staged for the host to reauthorize before publication.
    #[arg(long, hide = true, requires = "block_template_repo_seed")]
    pub block_template_seed_stage_only: bool,
    /// Additional guest /etc/hosts entries in HOST=IP format for trusted routing.
    #[arg(long = "guest-host-alias")]
    pub guest_host_alias: Vec<String>,
    /// Command and arguments to execute inside the VM.
    ///
    /// Required when actually booting a VM; omitted in `--report-version` mode.
    #[arg(trailing_var_arg = true)]
    pub cmd: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: VmExecArgs,
    }

    #[test]
    fn template_preparation_does_not_change_normal_boot_argument_contract() {
        let normal = Cli::try_parse_from(["vm-exec", "--rootfs", "/rootfs", "--", "true"]).unwrap();
        assert!(!normal.args.prepare_block_template);
        assert_eq!(normal.args.block_template_reserve_mib, None);
        let prepare = Cli::try_parse_from([
            "vm-exec",
            "--rootfs",
            "/template/rootfs",
            "--prepare-block-template",
            "--scratch-disk-mib",
            "32768",
            "--block-template-reserve-mib",
            "65536",
        ])
        .unwrap();
        let private = Cli::try_parse_from([
            "vm-exec",
            "--rootfs",
            "/owned/rootfs",
            "--prepare-private-workload",
            "--scratch-disk-mib",
            "32768",
        ])
        .unwrap();
        assert!(private.args.prepare_private_workload);
        assert!(!private.args.prepare_block_template);
        assert!(prepare.args.prepare_block_template);
        assert_eq!(prepare.args.block_template_reserve_mib, Some(65536));
        assert!(Cli::try_parse_from([
            "vm-exec",
            "--rootfs",
            "/rootfs",
            "--block-template-reserve-mib",
            "32"
        ])
        .is_err());
        assert!(Cli::try_parse_from([
            "vm-exec",
            "--rootfs",
            "/rootfs",
            "--block-workload",
            "--prepare-block-template"
        ])
        .is_err());
    }
}
