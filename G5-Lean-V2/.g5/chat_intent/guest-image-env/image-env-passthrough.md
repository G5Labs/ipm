# Pass the image's declared environment through to agents, terminals and previews

## User instructions

From direct chat on 2026-10-08 (background job), after PR #386 merged:

- "merge the pr and continue the work"

The work being continued is the plan in the shared doc "QA Guest VM Error Summary and
Fix Plan" (Claude Docs artifact `8f24337e-3db4-4e70-a0d2-f3fec8b3639f`), whose Phase 2
reads: "The host writes the image's env key names into the spec as one variable; the
dispatcher copies those keys from its own environment, skipping the reserved set;
modules, terminals and previews inherit them in one place." Earlier instructions in the
same conversation are recorded in
[warm-toolchain.md](../g5dev-warm-toolchain/warm-toolchain.md).

## Agent interpretation

Phase 1 (PR #386) made the g5dev image's Rust toolchain and browsers work without any
environment reaching the agent. Phase 2 is the durable fix for the class: the runtime
forwards the customer image's declared environment (OCI config `Env`, as merged with the
VM spec at boot) to every agent turn, terminal and preview, under the same reserved-key
policy that already governs user settings, so a future image tool that depends on its
`ENV` does not need a launcher.

Decisions taken as the minimal mechanism:

- The host already merges image and spec env and sources it into the dispatcher; only
  the knowledge of which keys came from the image is missing. One reserved variable,
  `G5D_IMAGE_ENV_KEYS`, carries the surviving image key names; no new file or mount.
- Filtering reuses `is_exportable_task_env_key` (platform identity, runtime and
  provider-home keys stay runtime-owned) plus the native loader keys, which the
  workspace CLI block already scopes to G5 processes; runtime-owned keys win on a
  collision.
- The Phase 1 launchers stay: hosts that predate this runtime still boot the image.

Rollout: a dispatcher change ships inside the worker bundle, so merging triggers the
`g5-packer-worker-main-qa` bake, the deploy waits for and pins that image, and QA's
idle-host rotation replaces the running hosts.
