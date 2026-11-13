# Find why QA microVMs die during builds and fix it

## User instructions

From direct chat on 2026-10-09 and 2026-10-10 (background job), while proving the
cross-VM cache in QA (issue #407 tracks the losses):

- "keep working, the goal is to find the reason why vm crash and fix it, and also prove
  that the whole cache system works with evidence i asked"
- "i want all experiments run a 4vcpu c3-standard-4"
- "you need to monitor the log, since when it crash, the logs are gone"
- "small vm failed, do you have anything to confirm?"

After PR #422 merged, on 2026-10-10:

- "there is a gap in 422 check comments here: https://github.com/G5Labs/G5-Lean/issues/407"

The comment referred to is the reviewer's (zcl-g5, 04:45 UTC) finding that the defect is
the one G5 fixed in G5Labs/G5#2522 / PR #2523 on 2026-09-17, and that #422 lacks that
fix's delivered-frame interrupt, its passt socketpair sizing with the host `wmem_max`
sysctl, and its test for the interrupt.

## Agent interpretation

The losses are one fault: the VM's network stops receiving while it still transmits,
every lease renewal fails, and the host kills the VM (`stale_killed`). Observations on the
4 vCPU / 14 GB shape: traffic both ways drops to zero within a minute at a fixed point of
the `dev-up` build, the host stays calm, the guest reports 13 GB available after the
stall, and passt sleeps in `sock_alloc_send_pskb` (blocked writing a socket the VMM is no
longer reading). Reading libkrun 1.18.1 `src/devices/src/virtio/net/worker.rs` against
those facts gives the mechanism: with event-index suppression negotiated, the device
publishes the guest's kick threshold before a receive burst and never afterwards; when a
burst drains the ring and the next frame waits for buffers, the guest's refill does not
cross the stale threshold, so it does not kick, and the full backend socket raises no
further edge event. Nothing ever resumes rx. Upstream through v1.19.6 is unchanged here.

The fix is a G5 source patch in the libkrun build (the mechanism the repo already uses
for its TSI and virtio-fs patches): publish the stopping position only after processing
stops and re-check for buffers posted meanwhile, with a unit test that fails on upstream
code. Guest-side watchdogs (bouncing the NIC) and larger VMs only mask the fault; the
libkrun 1.19.x releases do not touch this path.

The follow-up ports the three parts named in the review: the patch also signals the
guest for a parked frame that a kick delivers (with a test that fails on upstream), the
host sizes the passt-side socketpair end to 16 MiB before the handoff, and the worker
image raises `net.core.wmem_max` to 16 MiB so libkrun's request for the other direction
holds. The sizing changes how often the window opens; liveness is the libkrun patch.
