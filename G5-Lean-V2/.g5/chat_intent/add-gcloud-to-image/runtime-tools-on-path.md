# Make the runtime bundle's CLI tools reachable by name

## User instructions

From direct chat on 2026-10-06/07, branch `add-gcloud-to-image`:

- "do you see gcloud in your PATH?" — then, after inspection, "I am not talking about
  this VM, I am looking for a PR to make it permanent for future".
- "as I do see a few other cli tools such as gcloud, npm, just, gh, g5d etc in folder
  `/opt/g5/bin`, wondering 1/ why are they not in PATH".
- "how to make other cli tools under folder `/opt/g5/bin/` available to PATH?"

The requested outcome: the runtime bundle's CLI tools are reachable by name in future
workspace VMs, permanently, via a PR — not a per-VM workaround.

Stated constraints carried into the design from the same conversation: do not shadow the
base image's own toolchain, and do not expose the bundle's shared libraries to
repository commands.

## Agent interpretation

The tools are already shipped; only their reachability is missing. The bundle staging
contract already declares these exact tools as being present *for agent bash*
(`scripts/stage-local-worker-runtime.sh:15-21` — "gh — GitHub CLI for agent bash when
GH_TOKEN is provided", "gcloud — … for agent bash when cloud credentials are provided",
"kubectl — … for agent bash", "cargo/rustc — Rust unit-test toolchain for agent bash",
"npm/pnpm — client Vitest package managers for agent bash"), as does the worker image
(`infra/packer/g5-worker/variables.pkr.hcl:333,350` — "for in-microVM agent bash").
The repository harness also already instructs agents to use `gh`
(`AGENTS.md`, workspace pull requests section), and `.g5/memory/claude/` carries a note
about installing `gcloud` by hand because it was not found.

So this is read as closing an implementation gap against existing bundle intent, not as
reversing the decision to keep bundle executables off PATH. That decision's two
substantive constraints — image tools keep winning, and G5 loader paths stay scoped to
G5 processes — are preserved rather than amended.

Observed in this VM during the investigation, and treated as design input:

- Every executable in `/opt/g5/bin` runs correctly with `LD_LIBRARY_PATH` unset, so no
  loader exposure is required to make them usable.
- The bundle's wrapper scripts resolve siblings through `dirname "$0"` without
  `readlink -f`, so a symlink into another directory breaks them (`gcloud` looks for
  `google-cloud-sdk/` beside the symlink). Reaching them through the bundle directory
  itself, or through a forwarder, is required.
- Only `gcloud`, `gh`, `kubectl`, `gke-gcloud-auth-plugin` and `cargo-nextest` are
  actually missing from PATH; the g5dev image already supplies `node`, `npm`, `npx`,
  `cargo`, `rustc`, `just`, `python3` and `corepack`.
