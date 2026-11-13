# Stale worker hosts: placement, Workers page label, forced removal

Human request, 2026-10-15, on the fresh `main` head `970b28f6`.

## Human instructions

- When a user clicks Start VM and a worker host is stale, the Start today fails on
  that host with a runtime mismatch. Do not place the VM there in the first place:
  regard a stale host's slots as unavailable. If no slot is available, create a new
  host exactly as the existing no-capacity path does.
- On the Workers page, add a tag showing which hosts are stale. Later the same day,
  after seeing a `stale runtime` chip with a tooltip and a `runtime unknown` chip:
  "just show stale chip do not expose more details".
- Removing a host that still runs VMs answers
  `Conflict: worker host … still owns 1 active workspace VM(s); drain it or retry
  with force=true`, but the UI offers no force action. Add one.

## Interpretation (agent)

- "Stale" here is the host's runtime, not its heartbeat: the host's module catalog
  or dispatcher contract no longer matches the control plane's, which the host
  reports as `RuntimeMismatch` when it validates a boot spec. The existing
  `heartbeat_state = 'stale'` (a silent host) is a different, already handled condition.
- The host is the only party that knows its runtime today. The design has each host
  publish its runtime identity on hello, so the control plane can exclude it before
  placement; a host that publishes none receives no placement either, matching the
  existing rule that a host without a published sizing contract receives none.
- "Create a new host" reuses demand-driven capacity unchanged; stale hosts simply stop
  counting as usable supply. No new provisioning path.
- The removal flow keeps the backend's refusal as the authority: the page warns up front
  when the row shows occupancy and sends `force=true` from that confirmation, and when
  the backend still refuses (a VM arrived meanwhile), the page offers "Remove anyway".
