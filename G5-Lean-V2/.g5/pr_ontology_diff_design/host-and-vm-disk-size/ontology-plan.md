# Host and workspace VM disk sizing

Implement the [requested formula](../../chat_intent/host-and-vm-disk-size/request.md)
through the existing [host disk owner](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.host-disk)
and [VM disk provisioning policy](../../ontology/software_architecture/host_control_plane.g5.md#g5.runtime.placement.vm-disk).
The user authorized implementation and PR publication.

`WorkerHostBootDiskSizing` owns fixed coefficients: 120 GB base, 80 GB per VM
slot, 200 GB automatic floor and 2000 GB cap. The base includes the existing
20 GB cache-door allowance. Remove scratch size from this calculation and its
resolver inputs; publish the same coefficients to the administrator UI. Add Host,
managed capacity, rotation and stockout retry all reuse this owner. Explicit
30–2000 GB overrides remain persisted; automatic sizes stay unpinned and are
recalculated at provisioning/replacement.

The user's follow-up clarifies that workspace VMs must also grow from 64 to
80 GiB. QA, staging and production configure 81920 MiB through the existing
environment setting; generic deployment templates use the same capacity. Keep
the unconfigured/self-hosted default and optional Packer prebake settings intact.
Cloud provisioning persists the resolved capacity and passes it into host startup;
registration publishes it for placement and private workload creation.

Rotation must sample the current valid deployment setting rather than perpetually
copying 64 GiB from the predecessor. If that setting is absent or invalid, retain
the predecessor's capacity to avoid an accidental downgrade. Retry of the same
host retains its persisted capacity, keeping registry and startup aligned.
Reuse existing replacement/drain eligibility and custody ordering; a setting
change alone does not force a rotation. No live disk resize is introduced.
Existing VMs gain the new capacity only when recreated on a host provisioned with
the new setting; restarting on an old host alone is insufficient.

This replaces the prior guarantee of provisioning three extra scratch disks for
source/seed preparation. Template admission, source preparation and Start retain
their existing actual-free-space checks. At 80 GiB per VM, optional seed preparation
requires `(slots + 3) × 80` GiB free, exceeding the automatic formula's allocation;
these hosts use generic startup unless explicitly provisioned with more space.
The provisioning formula is a policy, not proof of admission capacity.
Cap/undersize diagnostics must describe allowances rather than claiming an exact
number of startable VMs from them.

Verification covers fixed coefficients, floor/cap boundaries and overflow, explicit
overrides, serialized cloud requests, API publication/provisioning/rotation/retry,
managed capacity parity and UI defaults. Reuse existing regression tests and update
their expected policy. The archived HostDisk model remains historical evidence;
this change does not authorize a new model run or live cloud qualification.

The follow-up verifies deployment/template capacity agreement across all three
environments, actual rendered Helm settings, 80 GiB API provisioning and startup,
and rotation from a 64 GiB predecessor to an 80 GiB replacement while preserving
the predecessor and normal readiness/drain ordering. Cover absent/invalid rotation
configuration and retained retry/boot-disk overrides. Live 80 GiB guest boot and
fleet rollout remain deployment qualification, not evidence supplied by these tests.
