---
name: nested-virtualization-workspace
description: "Workspace 003f1792 (branch cloud-hypervisor-nested-virtualization): nested virtualization for Linux workspace VMs; design confirmed and implemented 2026-10-27, PR #853 open; the strict KVM gate passed; E2E on the final head and QA acceptance from a workspace VM remain."
metadata:
  node_type: memory
  type: project
  originSessionId: d77f3844-bd29-4545-8efc-59bb4bf8597d
  modified: 2026-10-27T03:02:53.617Z
---

Workspace `003f1792-2438-4c99-b200-53bbd2841ee0`, branch `cloud-hypervisor-nested-virtualization`
in both the code and docs repositories. Request: Project Doc `request_investigation.md`
(Kaihui, 2026-10-27; the file has truncated lines in §2–§6). Workflow per the handover:
ontology-planning → owner confirms → implement → ontology-review → `g5d pr create`.

State on 2026-10-27 (later): owner confirmed all five questions as recommended
(`decision: true`); implementation committed and PR G5Labs/G5-Lean#853 opened with
`g5d pr create`. The g5-ci-rust strict KVM gate passed (`69ffbe25`): VMX flag, `/dev/kvm`
0666, unprivileged open with KVM API 12, Docker `--device`, the three-deep boot
(`NESTED_BOOT_PROBE=pass elapsed_s=6.3`) and the switch-off negative control. Remaining:
g5-ci-e2e on the final head (its first failure was the known flaky WorkspaceTabStrip
case), CodeRabbit threads, then QA acceptance from a workspace VM on a newly provisioned
QA host (design §4.3), recorded in the plan. Pushing directly to origin moves the G5
branch but not the GitHub PR head: run `g5d pr update` after every push and watch the new
SHA with `g5d pr checks --watch --fail-fast --head <40-char sha>`.
Local quirk: two pre-existing `cli::host::tests` stat `/var/lib/g5d/rootfs` and fail
with EACCES in this VM; run them with `G5D_ROOTFS_STATE_DIR=/tmp/...` to prove they pass.
- Design: `.g5/pr_ontology_diff_design/nested-virtualization/ontology-plan.md`; new blocks
  `g5.architecture.microvm_backend.nested` (hosts.g5.md) and
  `g5.security.nested-virtualization` (security.g5.md), now human-verified (owner confirmed) with implementation evidence in their metadata.
- Project Doc `nested-virtualization-design.md` + question set
  `questions/nested-virtualization.g5q.json` (ids policy-switch, kvm-amd, dev-kvm-mode,
  ci-nested-boot, implement). Act only on answers with `decision: true`.
- Recommended answers: deployment-level switch `G5_WORKER_NESTED_VIRTUALIZATION` (API) →
  `G5D_NESTED_VIRTUALIZATION=on` in host.env at bring-up, default off, QA first; include
  `CONFIG_KVM_AMD=y`; `/dev/kvm` 0666 root:root from `infra/guest/init.c`; CI hard-gates
  VMX + /dev/kvm + Docker `--device`, nested L3 boot report-only behind
  `G5_NESTED_BOOT_STRICT=1`; `BootSummary` token `nested_virtualization=on|off`; hello
  capability `nested-virtualization:v1`; host refuses the switch when
  `boot/kernel.config` beside the pinned kernel lacks `CONFIG_KVM=y`.

**Why:** The handover requires presenting the design before writing dependent code,
and the four decisions are the owner's to confirm.

**How to apply:** On resume, read the question set with `g5d question list` first; if
confirmed, implement against the plan's §3 and §4 and amend the plan and the two blocks
in place for discoveries. Kernel facts already verified here: the pinned 6.12.112 source
(`/tmp/kcfg` if the VM survived) accepts the four KVM settings on the allnoconfig seed;
`GUEST_KERNEL_SHA256` pins the tarball, so nothing is re-pinned. A fresh VM needs
`sudo apt-get update` before installing flex, bison, bc, libelf-dev and cpio for the
kernel build. See [[rust-and-integration-test-setup]] and [[g5-vm-resets-lose-toolchains]].
