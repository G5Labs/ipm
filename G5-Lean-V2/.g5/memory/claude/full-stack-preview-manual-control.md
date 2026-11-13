---
name: full-stack-preview-manual-control
description: "Driving the full-stack preview's stack from the shell needs the profile's exports; slot count, worker restart and DB access recipes."
metadata:
  node_type: memory
  type: reference
  originSessionId: f36ce41d-5fb8-43f1-a3cb-3ef6d7d981fa
  modified: 2026-10-24T15:42:23.679Z
---

The `full-stack` preview profile runs `just dev-up` under its own stack identity. A plain `just dev-down` from the shell stops the shared worker host (both stacks use the checkout's `.dev/g5d-host`) but tears down the default stack, leaving `g5-preview-vm-*` up with no worker. Export the profile's env first:

`export PATH="/usr/local/bin:$PATH" G5_COMPOSE_PROJECT=g5-preview-vm G5_COMPOSE_VOLUMES=g5-preview-vm G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 G5_IMAGE_TAG=preview G5_PORT_EDGE=13000 G5_PORT_API=17500 G5_PORT_POSTGRES=15433 G5_PORT_OAUTH_STUB=19876`

- No in-VM command closes/reopens a preview; restart pieces with `just worker-host-stop && just worker-host-start` or `just dev-down && just dev-up` (the latter drops the preview DB). `preview-serve` keeps proxying 13000 across a manual dev-up.
- Process-mode hosts default to 1 slot; `G5D_HOST_MAX_MICROVMS=2` in the checkout's gitignored `.env` raises it. Slot count is not in the launch fingerprint, so `worker-host-start` alone won't apply it — stop first.
- Preview DB: `docker exec g5-preview-vm-postgres psql -U g5 -d g5_dev`. Child dispatcher logs: `.dev/g5d-host/state/process-vms/<vm_id>/dispatch.log`; a task's failure reason is `tasks.outcome`.
- Run integration tests against a separate unlabeled Postgres on 5432, never the preview DB. See [[rust-and-integration-test-setup]], [[process-isolation-host-mode]].
