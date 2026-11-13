# G5 worker host image

This Packer build produces the Ubuntu host image for Linux/KVM workers. It carries
Cloud Hypervisor, the pinned guest kernel/initramfs, the host supervisor and a
separate read-only guest runtime. It is distinct from a workspace environment OCI
image such as `g5d-g5dev` and from that environment's derived ext4 template.

## Image and source lifecycle

1. **Platform build:** prepare the host software and the complete guest runtime
   disk once per platform release. Pins live in `infra/guest/versions.env` and
   the CLI version files under `scripts/`.
2. **Environment build:** the regular environment delivery pipeline publishes an
   ext4 template alongside its immutable OCI image. Platform deployment does not
   need customer source. An authorized host prepares a missing custom environment
   with the same template operation before admitting workspaces.
3. **Host preparation:** validate the selected environment, disk capacity and
   runtime before registration. When a scoped source grant is available, prepare
   an optional repository seed in the background. This does not gate registration
   or first Start. Seeds contain committed baselines and Git objects; their scope
   comes from source authorization, not from an arbitrary checkout or workspace
   lifecycle counter.
4. **Workspace Start:** reflink a private sparse ext4 disk. On a valid seed hit,
   trusted guest bootstrap fetches missing Git objects and restores the exact
   admitted branch/receipt vector. Otherwise import the fresh host-restored
   workspace into the generic clone. Start image hooks and the dispatcher only
   after recovery completes. Source, toolchains, home, temporary files, build
   output and Docker data reside on this private workload filesystem.
5. **Stop and recovery:** use the existing Git checkpoint/receipt authority. Reap
   the VMM and passt before releasing disk/slot custody. Never reuse a
   guest-written disk as a template or mount it on the host. New commits and
   branches do not require rebaking the platform or environment image.

The runtime is a separate read-only block device mounted at `/opt/g5`. It includes
`g5d`, Claude Code, Codex, Node, Python, GitHub/Google Cloud/Kubernetes CLIs, Rust
and test/package tools, plus their runtime closures. This makes commands available;
custom environments still need application-specific native headers, linkers and
libraries. Host executables live separately under `/opt/g5-host`; guest changes
cannot replace the supervisor. The guest `g5d` ELF interpreter is pinned to its
platform closure while preserving native executable identity for self-reexec.

## Host layout and isolation

| Path | Purpose |
|---|---|
| `/opt/g5-host/bin` | Host supervisor and Cloud Hypervisor |
| `/opt/g5-host/boot` | Pinned kernel and minimal trusted initramfs |
| `/var/g5d-data/runtime` | Complete platform runtime and `runtime.ext4` |
| `/var/lib/g5d/rootfs/templates` | Pristine environment and scoped seed caches |
| `/var/lib/g5d/rootfs/<vm>` | Exit-proven per-VM disk/config/network custody |
| `/var/g5d-data/cache` | Authorized host caches |
| `/var/g5d-data/lib/workspaces` | Fresh host workspace materializations |
| `/etc/g5/worker-image.json` | Image/runtime version metadata |
| `/etc/g5/host.env` | Host-private provisioning configuration |

The canonical Packer source installs an XFS root with `reflink=1`; build-time
checks verify actual shared extents. A reflink shares initial blocks, not writable
inodes or VM ownership. Full per-slot capacity and template staging headroom remain
required. Sparse allocation is not a disk-capacity guarantee.

The systemd supervisor delegates per-VM cgroups and has narrowly bounded
capabilities, including network-policy setup and termination of its different-UID
children. It does not receive `CAP_SYS_ADMIN`. Cloud Hypervisor runs as a dedicated
non-root identity with seccomp and strict Landlock file rules. Passt runs under its
own identity with an empty-root sandbox, seccomp and enforcing AppArmor. Its only
additional file rule permits owner writes to `net.sock`, `net.sock.repair` and
`pid` within each VM's canonical `net/` directory. UID-scoped nftables rules govern
egress and host-door access; do not flush those rules while guests run.

The raw image uses the BIOS boot path exercised by its build and boot gate. The
presence of an ESP does not establish tested UEFI support. Self-hosted installs
use the trusted worker OCI root under systemd `RootDirectory`, a delegated cgroup
and a private XFS data store; customer environment images are never service roots.

## Building

The canonical builder needs Linux/KVM, Packer with the HashiCorp qemu plugin,
network access to the pinned Ubuntu/GitHub/package sources and sufficient build
storage. macOS has an explicitly acknowledged local process mode; it does not run
this Cloud Hypervisor backend.

```bash
cd infra/packer/g5-worker
packer init .
packer fmt -check .
packer validate -var git_short_sha=$(git rev-parse --short HEAD) .
packer build -var git_short_sha=$(git rev-parse --short HEAD) .
```

See `variables.pkr.hcl` for all inputs. `skip_runtime_cli_install=true` is only for
fixture/template validation and cannot qualify a release. The optional
`googlecompute` source is not the canonical XFS image builder.

Packer invokes `scripts/build-cloud-hypervisor-runtime.sh`, builds `g5d`, installs
the pinned CLI closures and assembles the independent runtime disk with
`scripts/build-runtime-disk.sh`. The latter requires `patchelf`, e2fsprogs, Git and a CA
bundle (`ca-certificates`) for the bootstrap closure, and verifies the patched guest executable before publishing the disk. Provisioner
phase 80 validates XFS, and phase 90 removes build identities/tooling.

### Optional workload cache baked into the host image

`infra/ci/cloudbuild-packer-worker.yaml` can additionally prewarm an environment
cache through `_WORKLOAD_TEMPLATE_IMAGE_REF` (digest-pinned project GAR image) and
`_WORKLOAD_TEMPLATE_DISK_MIB` (matching deployed capacity). Leave the image
reference empty to omit this optional cache. Increase `_WORKER_BUILD_DISK_GB` for
OCI extraction, template capacity and the build working set.

`stage-workload-template` stages the OCI layout; Packer phase 85 invokes
`g5d oci prepare-workload` and removes temporary inputs. Provenance is written to
`/etc/g5/workload-template.json`. This optimization is independent of the normal
environment companion artifact delivery path.

For an explicitly authorized private image build, `_WORKLOAD_TEMPLATE_REPO_CONFIG`
may point to a trusted checkout configuration accepted by
`scripts/prepare-workload-template-input.py`. It exports selected reachable Git
objects, excluding dirty files, local config and hooks. Such an image contains
private source and must remain inside the authorized organization's artifact
access boundary. Ordinary customer onboarding uses host-side authorized seed
preparation instead; it does not require deployment to possess customer code.

## Publication and verification

The `g5-packer-worker-main-qa` trigger is managed by `infra/qa/delivery`. The full
pipeline validates Packer, stages inputs, builds the disk, extracts its runtime,
runs both image gates, then publishes GCS artifacts and signed GAR metadata. It
verifies the signature and SBOM attestation before importing the GCE image and
publishing the release summary. Operator deployment/host rotation remains separate
from a successful build; never replace a VMM underneath a running workspace.

**Layer 1**, `scripts/image-test-gate/boot-and-assert.sh`, boots an ephemeral qcow2
overlay of the raw image under qemu/KVM using a fresh NoCloud SSH identity. Its ten
checks cover service enablement, file-descriptor budget, image metadata, executable
startup, Cloud Hypervisor/kernel JSON version identity, XFS shared extents, the
complete CLI closure, disabled OS auto-upgrades, e2fsprogs and the module catalog.
It also creates the SBOM from the running image.

**Layer 2**, `image-test-gate-cloud-hypervisor`, runs the extracted candidate
runtime/VMM/kernel against the build host's KVM with the strict guest and network
fixtures. It runs outside the layer-1 qemu guest to avoid an additional unsupported
nested-virtualization layer. Inside its first guest it also acts as an inner host
(`scripts/test-guest-nested-kvm.py --vm-exec`): Landlock, the platform the runtime disk
ships under `/opt/g5/platform`, the passt egress policy and a loop-mounted XFS reflink
root are hard checks; the third-level boot through the in-guest `g5d vm-exec` is
report-only unless `G5_NESTED_BOOT_STRICT=1`, because that depth was unreliable on C3
build workers, except that a Landlock or passt failure always fails the gate. These gates complement product chat, terminal, Files,
Git, checkpoint/recovery, Preview and failure qualification; they do not replace it.

| Artifact | Location |
|---|---|
| Disk, tarball, hashes, SBOM, manifest | `gs://${PROJECT_ID}-g5-worker-images/${VERSION}/` |
| Signed metadata and disk pointer | `${AR_REGION}-docker.pkg.dev/${PROJECT_ID}/g5-worker-images/g5-worker:${VERSION}` |
| GCE image | `projects/${PROJECT_ID}/global/images/${VERSION}` |
| Discovery pointer | `gs://${PROJECT_ID}-g5-worker-images/latest.json` |

Verify a release using `scripts/verify-worker-image.sh` with `PROJECT_ID` and
`COSIGN_KEY` set for its publishing environment. The script verifies the OCI
signature and SPDX attestation, then verifies the downloaded disk against the
hash inside the signed pointer. An unsigned GCS checksum sidecar alone is not a
trust anchor. Air-gapped operators can use an exported cosign public key.

If a gate fails, inspect that phase's log before retrying. Missing CLI dependencies
belong in the complete runtime build; a missing/invalid version report belongs in
the VMM/kernel staging path; failed reflinks belong in XFS provisioning. AppArmor,
Landlock or cgroup failures must be resolved without an unconfined fallback. For
publication failures, check the existing image build identity's registry, bucket
and signing permissions rather than weakening a gate.
