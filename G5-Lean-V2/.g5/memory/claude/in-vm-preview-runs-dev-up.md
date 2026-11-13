---
name: in-vm-preview-runs-dev-up
description: "A workspace preview runs inside this VM as `g5d preview-serve` → `just dev-up` (compose project g5-preview-vm, ports 17500/15433/13000) built from /work; the agent's token cannot start/stop it through the API."
metadata:
  node_type: memory
  type: reference
  originSessionId: 99601a1b-60ff-4f95-b5aa-34184a878380
  modified: 2026-10-25T07:12:24.132Z
---

Seen 2026-10-25 (issue #820): the owner started a preview mid-task. In the VM it is
`/opt/g5/bin/g5d preview-serve` → `/bin/sh -c '... G5_COMPOSE_PROJECT=g5-preview-${G5D_PROCESS_INCARNATION:-vm} ... just dev-up && sleep forever'`,
sharing the VM's one dockerd and network namespace. Ports come from its environment
(`/proc/<sh pid>/environ`): API 17500, Postgres 15433, edge 13000, OAuth stub 19876 — so a
native test stack on 5432/7500/5174 does not collide, but it competes for the 4 CPUs while
the preview builds (base images, then `compose build oauth-stub g5-api edge`, ~15+ min cold).

- The preview builds from the `/work` checkout as it is at build time; a push changes nothing
  in a running preview.
- `GET|POST /api/v1/workspaces/:id/preview` (and `/preview/cancel`) are owner-session only;
  the VM's workspace token is refused on `/api/v1`, and there is no restart endpoint
  (`workspace_preview.rs`, `task_lifecycle.rs`). Only the owner can close/start it from the UI.
- An agent can still refresh a running full-stack preview in place (2026-10-26, #828): copy
  `G5_PORT_*|G5_IMAGE_TAG|G5D_HOST_MAX_MICROVMS|PORT|G5D_PROCESS_INCARNATION` from the
  preview sh's `/proc/<pid>/environ`, export the profile's own PATH/G5_COMPOSE_* /isolation vars,
  and run `just dev-up && node scripts/seed-source-control-preview.mjs` under `setsid -f`
  (so turn teardown cannot kill it). It withdraws the edge, rebuilds only changed images, keeps
  the host if g5d is unchanged and preserves seeded examples; preview-serve's sleep loop is
  untouched, so the owner's Close still cleans up. ~5 min for a frontend-only change.
- `ps -eo pid,etime,args --forest | sed -n '/preview-serve/,$p'` shows whether one runs and
  which build step it is on.

Related: [[native-stack-browser-e2e]], [[pkill-f-kills-own-shell]].
