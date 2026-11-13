# Retained workspace roots

## Human request

On 2026-10-08 (UTC), after a QA workspace VM crashed and its host was retired, the user asked:

> what happened if user stopped the VM? does /work survive and still sit on host?

After the agent said a stopped workspace's `/work` stays on the host and makes the next restore faster, the user asked:

> are you super sure about it? say, if VM1 is attached to Workspace W1, when VM1 stops but it its /work stay on host, will VM2 pick it up to attach to workspace W2? that seems like a serious issue as VM2 for W2 basically see files of VM1 for W1

The agent found that roots are per workspace and never mounted into another workspace's VM, and that no Start reuses a kept root at all. The user then wrote:

> yes, exactly, please trace this behavior via git history, to see if it is a day-1 behavior or a regression introduced by some PRs

The trace showed reuse existed only in PR #117's early commits and was removed before merge by a security fix (7577fb45), while retention, placement preference and purge stayed. The user then invoked `/build-to-ship` with the agent-written brief "Resolve the dead "retained root" feature on worker hosts", which asks for options before coding (at least A: implement the designed warm cache safely; B: stop retaining roots), says "Done when the user has chosen and the chosen design is recorded in the ontology; implementation only after the user says to implement", and "Do not push or open a PR unless the user asks".

Shown the options comparison in the [plan](../../pr_ontology_diff_design/retained-roots/ontology-plan.md), the user chose **"B: stop keeping roots (Recommended)"** and, for how far to go, **"Implement and ship"** (implement, test, Codex CLI approval, PR, CI and bot reviews, merge, cleanup).

## Agent interpretation

- The user's concern is isolation and dead state: a stopped workspace's files should not linger on a host where they serve no purpose, and nothing from one workspace may reach another.
- The decision is between building the cache the designs describe and removing retention. The user prefers the cleanest design with minimal state.
- Isolation already holds today and must keep holding: each VM mounts only `<workspaces_root>/<its workspace_id>`, rebuilt at every Start.
- Not in this request: worker-host boot-disk sizing (G5Labs/G5-Lean#344), Docker scratch-disk retention, and the host Git repository cache shared per repository URL.
