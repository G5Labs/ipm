# Guest cache profile projection (issue #918)

Request: [G5Labs/G5-Lean#918](https://github.com/G5Labs/G5-Lean/issues/918). Fresh
managed guests have no `/opt/g5/cache-profile.env` although host bring-up writes
`/var/g5d-data/runtime/cache-profile.env`: the shared `runtime.ext4` is frozen at image
build, before any host exists, and the microVM attaches that disk read-only, so a later
host-side write into the runtime directory never reaches a guest. The Docker and Rust
launchers in the g5dev image therefore never select the door without a caller override.
The issue asks for a boot/init projection that preserves the immutable shared runtime,
organization isolation, caller overrides and the no-profile fallback, without baking
per-host or per-organization data into shared images.

## Governing commitments

| Commitment | Role here |
| --- | --- |
| [`g5.host.cache-door`](../../ontology/software_architecture/hosts.g5.md#g5.host.cache-door) | VMs reach their host's door by a fixed alias; the profile is how a guest learns the door exists and what it serves |
| [`g5.host.cache-fleet`](../../ontology/software_architecture/hosts.g5.md#g5.host.cache-fleet) | The organization's prefix is per host; it must not enter a shared image |
| [`g5.host.runtime-bundle`](../../ontology/software_architecture/hosts.g5.md#g5.host.runtime-bundle), [`g5.runtime.bundle`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.bundle) | The runtime stays one independent read-only artifact; incompatible closure is rejected, never worked around |
| [`g5.architecture.build_scratch`](../../ontology/software_architecture/hosts.g5.md#g5.architecture.build_scratch) | Per-VM configuration completes during bootstrap before hooks and workload; the trusted init owns the guest mount topology |
| [`g5.runtime.cache.profile`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.cache.profile) (new) | Owns the profile's lifetime: authored per host, projected per boot, never shipped in a shared image |

The earlier [cross-VM cache sharing design](../cross-vm-cache-sharing/ontology-plan.md)
assumed the runtime directory itself is what guests mount. That held for process mode,
where the directory is bind-mounted, and never for the block runtime.

## Decision

The host's profile travels on the per-VM bootstrap input, and the trusted guest init
binds it over a placeholder the runtime image ships, so every consumer keeps reading
`/opt/g5/cache-profile.env` from a read-only runtime mount in both isolation modes.

1. **Host, per boot.** `g5d vm-exec` already builds a private read-only bootstrap disk
   from `config.json` and the workspace archive next to the runtime disk it selects.
   When the runtime directory holds a regular `cache-profile.env` of bounded size, it is
   copied into that input (world-readable, like the data it is). Stop/restart and seeded
   restores take the same path, so a reused guest sees the host it is actually on.
2. **Image, once.** `build-runtime-disk.sh` writes an empty `cache-profile.env` into the
   disk it produces, after populating it, so the shared runtime never carries a host's
   profile even if the building directory had one, and always has the mount point the
   init needs. The runtime directory on the host is not changed, which keeps process mode
   exactly as it was (no file means no profile).
3. **Guest init.** `init.c` binds the bootstrap disk's profile onto the runtime's
   placeholder right after mounting both disks and before any view of `/opt/g5` is
   built, so the workload view and container namespaces inherit it. The bind source is
   on a read-only, `noexec` mount; the runtime stays read-only. A profile that is present
   but cannot be bound (a runtime disk without the placeholder) fails the boot, under the
   rule that an incompatible runtime closure is rejected rather than silently degraded,
   which is the failure mode this issue is about.
4. **Consumers.** The launchers are unchanged except for comments: an empty placeholder
   has no `G5_CACHE_DOOR` line and reads exactly as "no profile". Caller overrides keep
   winning.

### Alternatives considered

- **Move consumers to a per-boot path** such as `/etc/g5/cache-profile.env` written by
  `g5d vm-init`. Requires a g5dev image release in lockstep, diverges from process mode,
  and `vm-init` cannot add mounts to the already-stacked workload view because the guest
  init's binds are private copies.
- **Overlay or writable tmpfs on `/opt/g5`.** Makes the runtime writable by guest root and
  changes the trust posture of the loader path. Rejected.
- **Pass the profile through guest environment.** Platform-reserved keys are deliberately
  not how deployment values reach tools (see the cache sharing design); and the launchers
  read a file.
- **Write the profile into the runtime directory and rebuild the disk per host.** Rebuilding
  a multi-gigabyte disk at host bring-up, or on every profile change, for a 200-byte file.

## Assumptions and limits

- Host `g5d`, the initramfs and `runtime.ext4` come from the same image build; a runtime
  disk predating the placeholder paired with a host that publishes a profile fails the
  boot explicitly. Self-hosted and local staging rebuild the runtime disk with the same
  script, so they move together.
- The profile is non-secret host configuration (door URL, registry prefix, mirror tiers)
  bounded at 64 KiB; it is copied as bytes and consumers parse it as data.
- The ignored dimension remains the cache's effectiveness; the issue leaves the
  performance impact unquantified and this design does not measure it.

## Verification

- `scripts/test-guest-init-mount-roots.py` replays the new bind with and without a
  profile and checks that the workload sees the host profile, that the runtime stays
  read-only and that the placeholder reads as no profile.
- `scripts/test-cloud-hypervisor-ci.sh` boots the real initramfs and a runtime disk built
  by `build-runtime-disk.sh`: with the fixture runtime it publishes a profile and the guest
  compares it byte for byte; with a baked runtime (`G5_CH_TEST_RUNTIME`, read-only in the
  Packer gate) the guest proves the placeholder exists and is empty.
- `scripts/test-cloud-hypervisor-guest.sh` applies the same check to the acceptance guest.
- Unit tests cover the host-side staging rules (absent, present, oversize, non-regular).
- Launcher oracles gain the empty-profile case.
- Real fresh-host acceptance (both launchers selecting the door, a private OCI reuse and
  a seeded same-org fleet hit, stop/restart, disabled cache) remains a QA step on a
  deployed worker image; it is listed in the issue and cannot run from this workspace.
