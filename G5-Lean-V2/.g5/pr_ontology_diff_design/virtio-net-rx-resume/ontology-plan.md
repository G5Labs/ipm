# Keep the guest receiving when its receive ring runs empty

Intent: [rx-resume.md](../../chat_intent/virtio-net-rx-resume/rx-resume.md). Issue:
G5Labs/G5-Lean#407.

## Ontology changes

- New `g5.runtime.boot.net-resume` (D) in `software_architecture/runtime.g5.md`: the VMM's
  virtio-net device must publish its stopping position after any receive burst that
  leaves a frame waiting, carried as a libkrun source patch. New because no block owned
  the VMM's device behaviour; the runtime only committed to failing closed on backend
  selection (`g5.runtime.boot.backend`).
- New `g5.runtime.boot.net-sndbuf` (D): the passt-side socketpair end is sized before
  the handoff and the worker image raises `net.core.wmem_max`. Ported from G5 PR #2523
  after the review on issue #407 identified the same defect and this gap.

## Governing ontology

- `g5.runtime.boot.backend`: the microVM backend is libkrun and must not be swapped for an
  unisolated process; the fix therefore has to live in libkrun, not around it.
- `g5.runtime.lease`: a VM whose renewals fail is killed by the host; the device fault
  turns into a VM loss through this contract, which stays as it is.

## High-level design

| Owner | Responsibility |
| --- | --- |
| libkrun virtio-net worker (`net/worker.rs`, patched) | Disable kicks while processing; after `process_rx`, if a frame is deferred, publish the position where consumption stopped and re-process when the guest posted buffers before the publication; when the backend is drained, leave kicks disabled. A retry whose posted head cannot be popped (a malformed ring) parks the frame as upstream did, instead of spinning. |
| `scripts/build-libkrun.sh` | Applies the patch (`patch_libkrun_virtio_net_rx_resume`), exports marker `krun_g5_virtio_net_rx_resume_v1`, runs `make test-g5-virtio-net-rx-resume` in every build. |
| `justfile` host setup | Rebuilds a local library that lacks the marker, as for the other patches. |
| `spawn_passt` (`g5d-isolation/src/isolation/passt.rs`) and the worker image sysctl | Size the passt-side socketpair end to 16 MiB before the handoff and raise `net.core.wmem_max` so libkrun's own request holds (`g5.runtime.boot.net-sndbuf`): fewer full-queue blocks, no liveness role. |

Mechanism: libkrun's net worker reads the backend socket edge-triggered and stops
reading at the first frame that finds no guest buffer; only a guest kick resumes it. With
`VIRTIO_RING_F_EVENT_IDX` the guest kicks only when its new `avail.idx` crosses the
device's `avail_event`. Upstream sets `avail_event` at the start of the readable handler
and then consumes buffers past it, so after a burst that drains the ring the guest's
refill never crosses it. The patch moves the publication after processing and acts on
`enable_notification`'s "buffers already posted" result instead of discarding it.

## Decision notes

- Alternatives rejected: upgrading to libkrun v1.19.6 (no change on this path); a guest
  NIC-bounce watchdog (privileged daemon in the customer VM, masks the fault); larger VM
  shapes (fewer starved refills, same fault). The merge-to-publish tier window and
  kill-time evidence capture are separate follow-ups.
- Acceptance: the unit test fails on upstream `worker.rs` and passes patched; QA proof is
  the previously failing experiment (4 vCPU c3-standard-4, pinned `dev-up` preview)
  completing on the patched worker image, repeated. Met on 2026-10-10: four of four
  runs on the #422 image and one of one on the #425 image reached ready with the VM alive,
  and the host sysctl read back as set (see the `g5.runtime.boot.net-resume` and
  `net-sndbuf` metadata).
- Risk: the Linux `tap` backend and the datagram backend share the worker, so the change
  applies to every libkrun net backend. Flags-mode guests (no event index) keep kicks
  suppressed during processing and after a drain, exactly as upstream; a guest that posts
  an unusable head gets one retry, then the frame stays parked until the guest repairs the
  ring and kicks, or resets the device (upstream parked it too, without the retry).
- Review: Codex (gpt-6-astra) round 1 found the unusable-head spin (P1), the flags-mode
  suppression gap (P2), a bare make target without the `net` feature (P2) and a host-setup
  fixture that no longer isolated the name-safety marker (P3); round 2 found the
  no-progress guard compared a wrapping index (P2, replaced by popping the head and
  undoing it) and an overstated "kicks armed" claim for the parked case (P3). All fixed.
- Lineage found after PR #422 merged: G5 fixed the identical defect on 2026-09-17
  (G5Labs/G5#2522, PR #2523), four days after this repository's baseline was taken. Its
  review on issue #407 listed what #422 lacked: the interrupt for a parked frame that a
  kick delivers (upstream initialises `signal_queue` after that delivery, so the guest
  learned of it only with the next frame), the socketpair sizing and sysctl, and a test
  for that interrupt. All three are ported in the follow-up PR; its Codex round asked for
  a new marker (`v2`) so a library built from #422 is rebuilt by host setup, a best-effort
  wording in the sizing block and the corrected lineage date, all applied.
