# =============================================================================
# variables.pkr.hcl — Inputs to the G5 worker image build.
#
# All variables have safe defaults so `packer build .` works for a developer
# spike. Production builds (Cloud Build, PR-rev7-2) override image_version,
# project_id, and the sources_enabled flags via -var/-var-file.
#
# Variable naming matches the rev-7 PR-rev7-1 prompt exactly:
#   image_version, image_family, source_image_project, source_image_family,
#   zone, disk_size_gb, g5d_source_path.
# =============================================================================

# -----------------------------------------------------------------------------
# Image identity
# -----------------------------------------------------------------------------

variable "image_version" {
  type        = string
  description = "Image version tag (e.g. g5-worker-v20260527-1830-abc1234). Empty = auto-generated from build timestamp + git short SHA."
  default     = ""

  validation {
    condition     = var.image_version == "" || can(regex("^g5-worker-v[0-9]{8}-[0-9]{4}-[a-f0-9]+(-[a-f0-9]{32})?$", var.image_version))
    error_message = "The image_version must match g5-worker-vYYYYMMDD-HHMM-<git-short-sha> with an optional 32-hex build ID suffix, or be empty for auto-generation."
  }
}

variable "image_family" {
  type        = string
  description = "Image family. GCE instance templates pin to family; rolling updates pick the latest member."
  default     = "g5-worker"
}

variable "image_description" {
  type        = string
  description = "Human-readable description baked into the GCE image metadata."
  default     = "G5 worker image (rev 7) — Ubuntu 24.04 + XFS root + Cloud Hypervisor + g5d + Claude Code + codex."
}

# -----------------------------------------------------------------------------
# Source-builder selection
# -----------------------------------------------------------------------------
#
# V1 = qemu (autoinstall, raw disk). The same raw disk is imported into GCE
# in PR-rev7-2 via `gcloud compute images create --source-uri gs://.../disk.raw`
# from the .tar.gz the post-processor emits.
#
# `build_googlecompute = false` by default in this rev: the googlecompute
# Ubuntu cloud-image base is ext4 and rev-7 §5.0 requires XFS root at install
# time. We get XFS root by booting the live-server ISO under qemu and driving
# Ubuntu autoinstall. PR-rev7-2 wires the resulting raw disk into the GCE
# import path; the GCP source remains parameterized for that PR.
# -----------------------------------------------------------------------------

variable "build_qemu" {
  type        = bool
  description = "Build the qemu raw-disk variant (canonical for rev-7; raw disk later imported to GCE / shipped to PNC)."
  default     = true
}

variable "build_googlecompute" {
  type        = bool
  description = "Build a GCE image directly from an ext4 Ubuntu base. WARNING: ext4-root only — rev-7 §5.0 production requires the qemu+autoinstall path. Kept for prototype/sanity builds and PR-rev7-2 wiring."
  default     = false
}

# -----------------------------------------------------------------------------
# googlecompute source inputs (reserved for the GCE import path in PR-rev7-2)
# -----------------------------------------------------------------------------

variable "project_id" {
  type        = string
  description = "GCP project that owns the resulting image. Required when build_googlecompute=true."
  default     = ""
}

variable "zone" {
  type        = string
  description = "Zone for the transient GCE builder instance and (in PR-rev7-2) the image-import job."
  default     = "us-central1-a"
}

variable "source_image_project" {
  type        = string
  description = "Project that owns the base GCE image."
  default     = "ubuntu-os-cloud"
}

variable "source_image_family" {
  type        = string
  description = "Base GCE image family."
  default     = "ubuntu-2404-lts-amd64"
}

variable "machine_type" {
  type        = string
  description = "Builder instance type. The g5d release build is RAM-hungry — n2-standard-8 minimum."
  default     = "n2-standard-8"
}

variable "image_storage_locations" {
  type        = list(string)
  description = "GCS locations to store the resulting GCE image. Empty = default (instance region)."
  default     = []
}

variable "network" {
  type        = string
  description = "VPC name for the GCE builder instance."
  default     = "default"
}

variable "subnetwork" {
  type        = string
  description = "Subnet for the GCE builder instance. Empty = auto."
  default     = ""
}

variable "use_internal_ip" {
  type        = bool
  description = "If true, no external IP on the GCE builder — required in restricted VPCs with Cloud NAT for egress."
  default     = false
}

variable "service_account_email" {
  type        = string
  description = "Service account attached to the GCE builder instance. Empty = Compute default."
  default     = ""
}

variable "ssh_username" {
  type        = string
  description = "SSH username Packer connects as on the GCE builder. Ubuntu cloud images default to 'ubuntu'."
  default     = "ubuntu"
}

# -----------------------------------------------------------------------------
# qemu source inputs
# -----------------------------------------------------------------------------

variable "ubuntu_iso_url" {
  type        = string
  description = "Ubuntu Server 24.04 LTS live-server ISO URL. The autoinstall seed formats / as XFS with reflink=1."
  default     = "https://old-releases.ubuntu.com/releases/24.04.2/ubuntu-24.04.2-live-server-amd64.iso"
}

# Pinned, not fetched: releases.ubuntu.com drops superseded point releases,
# and its 24.04.2/SHA256SUMS stopped listing this ISO, failing validate.
# Source: old-releases.ubuntu.com/releases/24.04.2/SHA256SUMS.
variable "ubuntu_iso_checksum" {
  type        = string
  description = "SHA256 checksum of the ISO, as a Packer checksum expression (e.g. file:URL or sha256:hex)."
  default     = "sha256:d6dab0c3a657988501b4bd76f1297c053df710e06e0c3aece60dead24f270b4d"
}

variable "qemu_output_directory" {
  type        = string
  description = "Directory to write the raw disk artifact, packer-manifest.json, and GCE-importable tar.gz."
  default     = "output"
}

variable "qemu_accelerator" {
  type        = string
  description = "qemu accelerator: kvm (Linux), hvf (macOS), tcg (slow fallback for CI)."
  default     = "kvm"
}

variable "build_cpus" {
  type        = number
  description = "vCPUs for the qemu builder VM (Rust+Cloud Hypervisor compile is CPU-bound)."
  default     = 4
}

variable "build_memory_mb" {
  type        = number
  description = "RAM in MB for the qemu builder VM (Cloud Hypervisor+g5d compile peaks ~6 GB)."
  default     = 8192
}

variable "disk_size_gb" {
  type        = number
  description = "Boot-disk size for the resulting raw image. ~15GB is the working set; budget headroom for cache + runtime growth."
  default     = 30
}

# -----------------------------------------------------------------------------
# G5 binary sourcing
# -----------------------------------------------------------------------------

variable "g5d_source_path" {
  type        = string
  description = "Path on the builder host to the  source tree (relative to packer's CWD when invoked from this directory). Uploaded + built inside the VM."
  default     = "../../.."
}





variable "rust_toolchain" {
  type        = string
  description = "Rust toolchain pin used during image build. Pinned so consecutive rebuilds produce bit-identical binaries and kept aligned with the base-rust tier."
  default     = "1.97.1"
}

variable "rustup_version" {
  type        = string
  description = "rustup-init version used to install the build-time Rust toolchain and stage the guest-visible Rust unit-test toolchain into runtime/bin/rust. The toolchain itself is pinned by rust_toolchain."
  default     = "1.29.0"

  validation {
    condition     = can(regex("^v?[0-9]+\\.[0-9]+\\.[0-9]+$", var.rustup_version))
    error_message = "The rustup_version must be a rustup release version such as 1.29.0 or v1.29.0."
  }
}

variable "rustup_init_sha256" {
  type        = string
  description = "SHA256 of rustup-init for rustup_version and image architecture. Empty = verify against rustup-init.sha256 fetched over TLS. CI SHOULD pin."
  default     = ""
}

variable "just_version" {
  type        = string
  description = "just version to install into runtime/bin/just for in-microVM unit-test recipes."
  default     = "1.55.1"

  validation {
    condition     = can(regex("^v?[0-9]+\\.[0-9]+\\.[0-9]+$", var.just_version))
    error_message = "The just_version must be a just release version such as 1.55.1 or v1.55.1."
  }
}

variable "just_sha256" {
  type        = string
  description = "SHA256 of just-<just_version>-<arch>.tar.gz. Empty = verify against the release SHA256SUMS fetched over TLS. CI SHOULD pin."
  default     = ""
}

variable "cargo_nextest_version" {
  type        = string
  description = "cargo-nextest version to install into runtime/bin/cargo-nextest for Rust unit-test recipes."
  default     = "0.9.138"

  validation {
    condition     = can(regex("^v?[0-9]+\\.[0-9]+\\.[0-9]+$", var.cargo_nextest_version))
    error_message = "The cargo_nextest_version must be a cargo-nextest release version such as 0.9.138 or v0.9.138."
  }
}

variable "cargo_nextest_sha256" {
  type        = string
  description = "SHA256 of cargo-nextest-<cargo_nextest_version>-<arch>.tar.gz. Empty = verify against the release .sha256 file fetched over TLS. CI SHOULD pin."
  default     = ""
}

variable "node_version" {
  type        = string
  description = "Official self-contained Node.js version to install into runtime/bin/node (P5.5). The distro 'node' is a libnode shim that crashes in the microVM guest on externalized builtins (/usr/share/nodejs/...); the nodejs.org self-contained build embeds V8 + builtins. Pin a 22.x LTS so client test tools can run Vite 7+."
  default     = "22.23.4"

  validation {
    condition     = can(regex("^22\\.[0-9]+\\.[0-9]+$", var.node_version))
    error_message = "The node_version must be a 22.x release such as 22.23.4 in MAJOR.MINOR.PATCH form."
  }
}

variable "node_sha256" {
  type        = string
  description = "SHA256 of node-v<node_version>-linux-x64.tar.xz (supply-chain pin). When set, 40-install-claude.sh verifies the download against this exact value. Empty = verify against the official SHASUMS256.txt fetched over TLS from nodejs.org (TOFU). CI SHOULD pin."
  default     = ""
}

variable "pnpm_version" {
  type        = string
  description = "pnpm version vendored into runtime/bin/nodejs for in-microVM client unit-test commands. Pinned so pnpm never depends on Corepack network fetches at first use."
  default     = "11.9.0"

  validation {
    condition     = can(regex("^v?[0-9]+\\.[0-9]+\\.[0-9]+$", var.pnpm_version))
    error_message = "The pnpm_version must be a pnpm release version such as 11.9.0 or v11.9.0."
  }
}

variable "claude_code_version" {
  type        = string
  description = "Claude Code CLI version to install via npm (@anthropic-ai/claude-code). Empty (the default) resolves the single-source pin file scripts/claude-code-cli-version; set explicitly only for deliberate bump testing with worker-image smoke validation."
  default     = ""
}

variable "codex_version" {
  type        = string
  description = "openai/codex CLI version to install via npm (@openai/codex). Empty (the default) resolves the single-source pin file scripts/codex-cli-version; set explicitly only for deliberate bump testing with worker-image smoke validation."
  default     = ""
}

variable "gh_cli_version" {
  type        = string
  description = "GitHub CLI version to install into runtime/bin/gh for in-microVM agent bash. Pinned by default; bump deliberately with worker-image smoke validation."
  default     = "2.95.0"

  validation {
    condition     = can(regex("^v?[0-9]+\\.[0-9]+\\.[0-9]+$", var.gh_cli_version))
    error_message = "The gh_cli_version must be a GitHub CLI release version such as 2.95.0 or v2.95.0."
  }
}

variable "gh_cli_sha256" {
  type        = string
  description = "SHA256 of gh_<gh_cli_version>_linux_<arch>.tar.gz. Empty = verify against the GitHub CLI release checksums fetched over TLS. CI SHOULD pin."
  default     = ""
}

variable "gcloud_cli_version" {
  type        = string
  description = "Google Cloud CLI version to install into runtime/bin/gcloud for in-microVM agent bash. Pinned by default; bump deliberately with worker-image smoke validation."
  default     = "575.0.0"

  validation {
    condition     = can(regex("^v?[0-9]+\\.[0-9]+\\.[0-9]+$", var.gcloud_cli_version))
    error_message = "The gcloud_cli_version must be a Google Cloud CLI release version such as 575.0.0 or v575.0.0."
  }
}

variable "gcloud_cli_sha256" {
  type        = string
  description = "SHA256 of google-cloud-cli-<gcloud_cli_version>-linux-<arch>.tar.gz. Empty = rely on TLS and print the observed SHA256. CI SHOULD pin."
  default     = ""
}

variable "kubectl_version" {
  type        = string
  description = "kubectl version to install into runtime/bin/kubectl for in-microVM agent bash. Pinned by default; bump deliberately with worker-image smoke validation."
  default     = "1.36.2"

  validation {
    condition     = can(regex("^v?[0-9]+\\.[0-9]+\\.[0-9]+$", var.kubectl_version))
    error_message = "The kubectl_version must be a Kubernetes release version such as 1.36.2 or v1.36.2."
  }
}

variable "kubectl_sha256" {
  type        = string
  description = "SHA256 of the kubectl binary for kubectl_version and image architecture. Empty = verify against the Kubernetes release .sha256 file fetched over TLS. CI SHOULD pin."
  default     = ""
}

variable "skip_runtime_cli_install" {
  type        = bool
  description = "When true, skip Claude Code + codex installation. Intended ONLY for offline template validation; production builds must leave this false."
  default     = false
}


# -----------------------------------------------------------------------------
# Build provenance
# -----------------------------------------------------------------------------

variable "build_timestamp" {
  type        = string
  description = "Build timestamp written into image metadata (YYYYMMDD-HHMM). Empty = generated at packer-build time."
  default     = ""
}

variable "git_short_sha" {
  type        = string
  description = "Short git SHA (7 hex chars) embedded in the image version. Pass $(git rev-parse --short HEAD). Empty = 'devbuild' for local-only artifacts."
  default     = ""
}

# Optional rebuildable workload caches, prepared before the image is published.
variable "workload_image_ref" {
  type        = string
  description = "Pinned OCI environment represented by the staged OCI layout; empty disables prebuild."
  default     = ""
  validation {
    condition     = var.workload_image_ref == "" || can(regex("@sha256:[0-9a-f]{64}$", var.workload_image_ref))
    error_message = "The workload_image_ref must be empty or pinned by SHA256 digest."
  }
}

variable "workload_disk_mib" {
  type        = number
  description = "Guest capacity for prebuilt templates; must match the deployed host slot configuration."
  default     = 32768
  validation {
    condition     = var.workload_disk_mib > 0 && floor(var.workload_disk_mib) == var.workload_disk_mib
    error_message = "The workload_disk_mib must be a positive integer."
  }
}
