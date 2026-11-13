# Sparse VM disk correction (2026-11-04)

Owner instruction in the workspace chat for PR #1057, superseding the earlier
guidance that each VM's entire disk allowance be physically reserved before boot:

- Production uses sparse VM disks. Preallocation is acceptable only in dedicated
  test or CI fixtures and must not change production behavior.
- Keep only the two admin settings: host disk allowance H (default 120 GiB) and
  VM disk allowance V (default 80 GiB).
- Creating a host: the default and minimum disk is H + V × slots; allow a larger
  explicit disk, reject a smaller one, and preserve existing slot-count rules.
- Starting a VM: the disk admission condition is available host space >= V, with
  no multiplier, fixed headroom or other reservation threshold. Slot, memory and
  other existing admission checks stay independent.
- The VM's virtual disk capacity is V; its physical allocation stays sparse and
  grows with actual writes. Preserve sparse/reflink cloning. Do not unshare or
  eagerly copy the image, preallocate V, require privately allocated extents, or
  reserve an equivalent amount in a sidecar file. A copy-free reserve that still
  consumes V is insufficient.
- The free-space check is an admission-time check. No guarantee that future VM
  writes have reserved storage is requested; do not build a replacement
  reservation system.
- Remove the full-reservation requirement, with its verification and lifecycle
  behavior, from every production path. Preserve existing allocations and retry
  semantics; a setting change must not resize existing disks. Remove code and
  tests whose sole purpose enforced the discarded design; keep the two-setting
  policy and its capacity-error handling. No added settings, unrelated
  refactoring or storage-accounting redesign.
- Production-representative startup tests must exercise the sparse production
  path. Do not mask failures with longer timeouts, retries counted as first-attempt
  success or weaker receipts. Treat any CI fixture-quota correction as a separate issue.

Follow-up answer in the same chat: also remove the XFS project quota that capped
shared host data at H, with its project tagging, private-capacity path, shared
reader leases, pressure-driven reclamation and qualification scripts. H then only
sizes the host disk. Trusted guest Git restore for ordinary boots stays.

The owner's earlier choice of a "copy-free reserve" in the same session is
superseded by this correction.

## Interpretation (agent)

- Start admission compares the free space of the host's workspace filesystem with
  the VM's recorded V once, in the host supervisor. Preparation paths keep the
  original request's removal of their extra margins; optional repository seed
  preparation keeps main's free-space gate, because removing the shared quota
  would otherwise leave it unbounded.
- The cache door's free-space floor follows the Start gate (exactly V), as it
  followed the previous V + 512 MiB gate.
