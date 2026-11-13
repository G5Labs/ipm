# =============================================================================
# g5-worker.pkr.hcl — Packer template for the signed G5 worker image (rev 7).
#
# This is the rev-7 keystone (see aux_files/future_implementation/microvm_completion_plan.md
# §5.0). The image carries:
#
#   - Ubuntu 24.04 LTS live-server base, installed via autoinstall
#   - XFS root formatted with `mkfs.xfs -m reflink=1` AT INSTALL TIME
#     (not converted at provisioning time — see "XFS root" in README)
#   - Pinned Cloud Hypervisor, trimmed platform kernel and trusted initramfs
#   - KVM kernel modules + libcap-ng0 runtime dep
#   - G5 runtime binaries at /var/g5d-data/runtime/bin/{g5d,claude,codex,node}
#   - g5d-host.service systemd unit (wrapper probes for the real
#     subcommand; ships green until PR-rev7-3 lands the supervisor module)
#   - Pre-created /var/g5d-data/ directory layout (rev-7 §3)
#
# Source-builder strategy:
#   - qemu (default) — boots the Ubuntu 24.04 live-server ISO under qemu,
#     autoinstalls the OS onto XFS root, then runs the provisioner chain.
#     Output is a raw disk image + a GCE-importable .tar.gz + a manifest.
#   - googlecompute — ext4-root only; retained as a parameterized stub for
#     PR-rev7-2's GCE image-import wiring. Production builds use the qemu
#     path; PR-rev7-2 imports the qemu raw disk into GCE.
#
# Usage:
#   packer init  .
#   packer fmt   -check .
#   packer validate -var git_short_sha=$(git rev-parse --short HEAD) .
#   packer build  -var git_short_sha=$(git rev-parse --short HEAD) .
#
# See README.md for full prerequisites and troubleshooting.
# =============================================================================

packer {
  required_version = ">= 1.10.0"

  required_plugins {
    qemu = {
      version = ">= 1.1.0"
      source  = "github.com/hashicorp/qemu"
    }
    googlecompute = {
      version = ">= 1.1.4"
      source  = "github.com/hashicorp/googlecompute"
    }
  }
}

# -----------------------------------------------------------------------------
# Derived locals: image_version follows the prompt's exact shape
# `g5-worker-vYYYYMMDD-HHMM-<git_short_sha>` so artifacts are self-describing
# even when no operator passed -var image_version.
# -----------------------------------------------------------------------------

# Single-source CLI pins (#1342 D-13): the checked-in one-line files under
# scripts/ are the only place the versions live; the variables
# remain explicit overrides for deliberate bump testing. Kept in their own
# locals block so `packer fmt` does not realign the block below.
locals {
  claude_code_version = var.claude_code_version != "" ? var.claude_code_version : trimspace(file("${path.root}/../../../scripts/claude-code-cli-version"))
  codex_version       = var.codex_version != "" ? var.codex_version : trimspace(file("${path.root}/../../../scripts/codex-cli-version"))
}

locals {
  build_timestamp = var.build_timestamp != "" ? var.build_timestamp : formatdate("YYYYMMDD-hhmm", timestamp())
  git_short_sha   = var.git_short_sha != "" ? var.git_short_sha : "devbuild"
  image_version   = var.image_version != "" ? var.image_version : "g5-worker-v${local.build_timestamp}-${local.git_short_sha}"

  # GCE image names: lowercase, [a-z0-9-], <=63 chars. The g5-worker- prefix
  # plus the version already satisfies the rules; no further sanitization.
  gce_image_name = local.image_version

  output_dir = "${path.root}/${var.qemu_output_directory}/${local.image_version}"

  # Per-provisioner env block: shared across all shell scripts so each step
  # can read the build context without re-deriving it.
  provisioner_env = [
    "G5_IMAGE_VERSION=${local.image_version}",
    "G5_IMAGE_FAMILY=${var.image_family}",
    "G5_BUILD_TIMESTAMP=${local.build_timestamp}",
    "G5_GIT_SHORT_SHA=${local.git_short_sha}",
    "RUST_TOOLCHAIN=${var.rust_toolchain}",
    "RUSTUP_VERSION=${var.rustup_version}",
    "RUSTUP_INIT_SHA256=${var.rustup_init_sha256}",
    "JUST_VERSION=${var.just_version}",
    "JUST_SHA256=${var.just_sha256}",
    "CARGO_NEXTEST_VERSION=${var.cargo_nextest_version}",
    "CARGO_NEXTEST_SHA256=${var.cargo_nextest_sha256}",
    "NODE_VERSION=${var.node_version}",
    "NODE_SHA256=${var.node_sha256}",
    "PNPM_VERSION=${var.pnpm_version}",
    "CLAUDE_CODE_VERSION=${local.claude_code_version}",
    "CODEX_VERSION=${local.codex_version}",
    "GH_CLI_VERSION=${var.gh_cli_version}",
    "GH_CLI_SHA256=${var.gh_cli_sha256}",
    "GCLOUD_CLI_VERSION=${var.gcloud_cli_version}",
    "GCLOUD_CLI_SHA256=${var.gcloud_cli_sha256}",
    "KUBECTL_VERSION=${var.kubectl_version}",
    "KUBECTL_SHA256=${var.kubectl_sha256}",
    "SKIP_RUNTIME_CLI_INSTALL=${var.skip_runtime_cli_install ? "1" : "0"}",
    "DEBIAN_FRONTEND=noninteractive",
  ]
}

# -----------------------------------------------------------------------------
# Source: qemu (canonical rev-7 builder — XFS root from install time)
#
# Boots from the Ubuntu 24.04 live-server ISO, drives Ubuntu autoinstall via
# the http/ NoCloud seed, then SSHes in for the rest of the provisioner chain.
# The autoinstall seed (http/user-data) declares root as XFS with
# `mkfs.xfs -m reflink=1` — `cp --reflink=always` works for any path on the
# resulting disk.
#
# Output: a raw disk image suitable for direct distribution to PNC or for
# `gcloud compute images create --source-uri gs://.../disk.raw` (the
# post-processor packages it as .tar.gz with sha256 for that path).
# -----------------------------------------------------------------------------

source "qemu" "g5-worker-raw" {
  vm_name          = "${local.image_version}.raw"
  output_directory = local.output_dir

  iso_url      = var.ubuntu_iso_url
  iso_checksum = var.ubuntu_iso_checksum

  disk_size        = "${var.disk_size_gb}G"
  disk_interface   = "virtio"
  disk_discard     = "unmap"
  disk_compression = false
  format           = "raw"

  accelerator = var.qemu_accelerator
  cpus        = var.build_cpus
  memory      = var.build_memory_mb
  headless    = true

  http_directory = "${path.root}/http"

  ssh_username = "packer"
  ssh_password = "packer"
  # 45m gives headroom for a slow autoinstall (large package set, slow CI
  # worker disk) before Packer gives up waiting for the post-install reboot's
  # sshd. A passing install connects in ~2-3m; the margin only matters on a
  # degraded worker.
  ssh_timeout            = "45m"
  ssh_handshake_attempts = 200

  boot_wait = "5s"
  boot_command = [
    "c<wait>",
    "linux /casper/vmlinuz autoinstall ds=nocloud-net\\;s=http://{{ .HTTPIP }}:{{ .HTTPPort }}/ console=ttyS0,115200 ---<enter><wait5>",
    "initrd /casper/initrd<enter><wait5>",
    "boot<enter>",
  ]

  # Lock the ephemeral packer build user + drop its sudoers AS PART OF shutdown,
  # not in 90-cleanup.sh. The scrub must be the very last privileged act: doing
  # it in the final provisioner would revoke the packer user's sudo before
  # Packer runs this very command (sudo as packer), so shutdown would fail and
  # the build would time out with no artifact. Here the NOPASSWD sudoers is
  # still present, so sudo succeeds; the root shell then locks the user, removes
  # the sudoers file, and powers off — all persisted to the image before halt.
  shutdown_command = "echo 'packer' | sudo -S sh -c 'passwd -l packer; rm -f /etc/sudoers.d/90-packer; shutdown -P now'"

  # Respect var.qemu_accelerator (kvm | hvf | tcg). Packer's `accelerator`
  # setting injects accel as `-machine accel=...`, but our qemuargs `-machine`
  # below REPLACES that whole arg — so without setting accel here, qemu
  # silently falls back to TCG and `-cpu host` then aborts startup (observed:
  # "CPU model 'host' requires KVM or HVF" -> "Qemu failed to start"). Pin
  # accel explicitly to var.qemu_accelerator. -cpu host is valid for kvm/hvf;
  # tcg requires a generic model (use -cpu max).
  qemuargs = concat(
    var.qemu_accelerator == "tcg" ? [["-cpu", "max"]] : [["-cpu", "host"]],
    [["-machine", "type=q35,accel=${var.qemu_accelerator}"]],
    # Capture the in-VM kernel + autoinstall (subiquity) console to a file on
    # the shared /workspace so packer-build can dump it for diagnosis (esp. on a
    # 45-min SSH timeout, where there's otherwise no in-VM signal). Pairs with
    # console=ttyS0 in boot_command above.
    [["-serial", "file:/workspace/serial-console.log"]],
  )
}

# -----------------------------------------------------------------------------
# Source: googlecompute (parameterized stub for PR-rev7-2's GCE-image-import
# wiring; ext4-root only — production builds use the qemu path above).
# -----------------------------------------------------------------------------

source "googlecompute" "g5-worker-gce" {
  project_id              = var.project_id
  zone                    = var.zone
  source_image_project_id = [var.source_image_project]
  source_image_family     = var.source_image_family
  machine_type            = var.machine_type
  disk_size               = var.disk_size_gb
  disk_type               = "pd-ssd"

  image_name        = local.gce_image_name
  image_family      = var.image_family
  image_description = var.image_description
  image_labels = {
    g5_image_version   = replace(local.image_version, ".", "-")
    g5_image_family    = var.image_family
    g5_build_timestamp = local.build_timestamp
    g5_git_short_sha   = local.git_short_sha
  }
  image_storage_locations = var.image_storage_locations

  network               = var.network
  subnetwork            = var.subnetwork
  omit_external_ip      = var.use_internal_ip
  use_internal_ip       = var.use_internal_ip
  service_account_email = var.service_account_email

  enable_nested_virtualization = false

  ssh_username         = var.ssh_username
  wait_to_add_ssh_keys = "20s"

  tags = ["g5-worker-builder"]
}

# -----------------------------------------------------------------------------
# Build: one block, both sources, shared provisioners.
# -----------------------------------------------------------------------------

build {
  name = "g5-worker"

  sources = concat(
    var.build_qemu ? ["source.qemu.g5-worker-raw"] : [],
    var.build_googlecompute ? ["source.googlecompute.g5-worker-gce"] : [],
  )

  # ---- Phase 0: wait for cloud-init / autoinstall to settle ---------------

  provisioner "shell" {
    inline = [
      "set -eu",
      "echo '[g5-worker] waiting for cloud-init to finish'",
      "sudo cloud-init status --wait || true",
      "echo '[g5-worker] kernel: '$(uname -r)",
      "echo '[g5-worker] os-release:'",
      "cat /etc/os-release",
      "echo '[g5-worker] root fs:'",
      "findmnt -no FSTYPE,SOURCE /",
    ]
  }

  # ---- Shared download helper (must precede every artifact-fetching script)

  # Uploaded once and sourced by every provisioner that pulls an upstream
  # artifact, so the whole chain shares one bounded-retry implementation
  # instead of the eleven single-shot curls that let the 2026-08-12 GitHub
  # outage fail the image build at 55-install-gh.sh (G5Labs/G5#1811).
  provisioner "file" {
    source      = "scripts/lib/download-with-retry.sh"
    destination = "/tmp/g5-download-with-retry.sh"
  }

  # Claude Code CLI flag contract (docs/lean-provider-cli-contract.md): the
  # repository-level checker that 40-install-claude.sh runs against the staged
  # binary so a pin bump that lost a runner flag fails the image build.
  provisioner "file" {
    source      = "${var.g5d_source_path}/scripts/check-claude-cli-flags.sh"
    destination = "/tmp/g5-check-claude-cli-flags.sh"
  }

  provisioner "file" {
    source      = "${var.g5d_source_path}/scripts/check-codex-cli-flags.sh"
    destination = "/tmp/g5-check-codex-cli-flags.sh"
  }

  # ---- Phase 1: apt deps (platform kernel + KVM + xfsprogs + nodejs) --------

  provisioner "shell" {
    script           = "scripts/10-apt-deps.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # ---- Phase 3: g5d (built from this worktree) ----------------------------

  # Upload the g5d source as a tarball staged by the packer-build cloudbuild step
  # (excludes the packer output dir holding the in-progress multi-GB raw disk —
  # a raw dir upload copies it into the VM => "No space left on device" — plus
  # .git/target/node_modules; also far faster than scp'ing thousands of files).
  provisioner "file" {
    source      = "/workspace/g5d-src.tar.gz"
    destination = "/tmp/g5d-src.tar.gz"
  }

  provisioner "shell" {
    inline = [
      "mkdir -p /tmp/g5d-src",
      "tar xzf /tmp/g5d-src.tar.gz -C /tmp/g5d-src",
      "rm -f /tmp/g5d-src.tar.gz",
    ]
  }

  provisioner "shell" {
    script           = "scripts/30-build-g5d.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # ---- Phase 4: Claude Code CLI + codex CLI + agent shell tools -----------

  provisioner "shell" {
    script           = "scripts/40-install-claude.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  provisioner "shell" {
    script           = "scripts/50-install-codex.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  provisioner "shell" {
    script           = "scripts/55-install-gh.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  provisioner "shell" {
    script           = "scripts/56-install-gcloud-kubectl.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  provisioner "shell" {
    script           = "scripts/57-install-test-tools.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # Pinned passt for microVM networking (spawned by `vm-exec`, ontology
  # g5.architecture.microvm_backend): Ubuntu 26.04 .deb, sha256-verified — noble's own
  # archive passt predates the flags the D50/D51 network plan requires.
  provisioner "shell" {
    script           = "scripts/58-install-passt.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # ---- Phase 5: /var/g5d-data/ layout + systemd unit + host config --------

  provisioner "file" {
    source      = "files/g5d-host.service"
    destination = "/tmp/g5d-host.service"
  }

  provisioner "file" {
    source      = "files/g5d-host"
    destination = "/tmp/g5d-host"
  }

  provisioner "file" {
    source      = "files/g5d-host.env"
    destination = "/tmp/g5d-host.env"
  }

  provisioner "shell" {
    script           = "scripts/60-layout-dirs.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  provisioner "shell" {
    script           = "scripts/70-systemd.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # ---- Phase 6: write /etc/g5/worker-image.json (capability reporting) ----

  provisioner "shell" {
    script           = "scripts/75-write-image-info.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # ---- Phase 7: configure host (sysctl, kvm modules) + verify XFS reflink

  provisioner "shell" {
    script           = "scripts/80-configure-host.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # Inputs are staged by delivery without credentials. The directory exists
  # even when disabled so Packer validation stays deterministic.
  provisioner "shell" {
    inline = ["mkdir -p /tmp/workload-template-input"]
  }

  provisioner "file" {
    source      = "/workspace/workload-template-input/"
    destination = "/tmp/workload-template-input/"
  }

  provisioner "shell" {
    script = "scripts/85-prepare-workload.sh"
    environment_vars = concat(local.provisioner_env, [
      "G5_WORKLOAD_IMAGE_REF=${var.workload_image_ref}",
      "G5_WORKLOAD_DISK_MIB=${var.workload_disk_mib}",
    ])
    execute_command = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # ---- Phase 8: cleanup, fstrim, finalize ---------------------------------

  provisioner "shell" {
    script           = "scripts/90-cleanup.sh"
    environment_vars = local.provisioner_env
    execute_command  = "echo 'packer' | sudo -S env {{ .Vars }} bash '{{ .Path }}'"
  }

  # ---- Post-processors -----------------------------------------------------
  #
  # 1. Manifest: machine-readable build record consumed by PR-rev7-2's Cloud
  #    Build job to discover what was built without reparsing filenames.
  # 2. shell-local: packages the raw disk as `disk.raw` inside a GCE-importable
  #    .tar.gz plus a .sha256 sidecar. Runs only for the qemu source.
  #    Format follows https://cloud.google.com/compute/docs/import/import-existing-image.

  post-processor "manifest" {
    output     = "${local.output_dir}/manifest.json"
    strip_path = true
    custom_data = {
      image_version        = local.image_version
      image_family         = var.image_family
      build_timestamp      = local.build_timestamp
      git_short_sha        = local.git_short_sha
      rust_toolchain       = var.rust_toolchain
      source_image_project = var.source_image_project
      source_image_family  = var.source_image_family
      zone                 = var.zone
      disk_size_gb         = var.disk_size_gb
      root_filesystem      = "xfs"
      xfs_reflink          = "1"
    }
  }

  post-processor "shell-local" {
    only = ["qemu.g5-worker-raw"]
    inline = [
      "set -eu",
      "echo '[g5-worker] packaging raw disk for GCE import'",
      "cd '${local.output_dir}'",
      "if [ -f '${local.image_version}.raw' ]; then mv '${local.image_version}.raw' disk.raw; fi",
      "tar -czSf '${local.image_version}.tar.gz' disk.raw",
      "sha256sum '${local.image_version}.tar.gz' > '${local.image_version}.tar.gz.sha256'",
      "echo '[g5-worker] artifacts:'",
      "ls -lh disk.raw '${local.image_version}.tar.gz' '${local.image_version}.tar.gz.sha256' manifest.json",
    ]
  }
}
