# Host and workspace VM disk sizing

The user requested: change `host disk = min(2000, max(200, 252 + 65 × VM slots)) GB`
to `host disk = min(2000, max(200, 120 + 80 × VM slots)) GB` and file a PR.

After PR #792 merged, the user observed a 63G root filesystem and clarified that
the purpose of increasing the per-slot allowance was to give workspace VMs more
disk space: increase their private disks from 64 to 80 GB.

Interpretation: retain the requested host formula and increase managed workspace
disks from 64 GiB to 80 GiB (81920 MiB), using the existing binary capacity units.
The earlier host-only implementation missed this intended outcome. Update matching
workload templates and replacement-host provisioning; do not resize live disks.
