# Preview close and cleanup — one preview at a time

## Human request

On 2026-10-05 the user wrote:

> Now preview process and chat share same VM, so they might interact. So after preview is served, it we should also expose a button to close the preview and running a command to clean up the state inited by preview process, in G5 specifically, it should be just dev down. And if there is a preview process already running, we should not support run one more, the only support next step is to close the preview by just dev down. So we should also support user to provide clean up command if any in the preview profile. You need to look at the code carefully and evaluate the feasibility of the proposal.

After the agent's feasibility assessment, the user wrote: "implement it".

## Agent interpretation

The assessment the user accepted:

- **Close is the preview's graceful stop.** Cancel today SIGKILLs the preview's process group with no grace, so containers started under dockerd survive it. Closing instead gives the preview task a bounded graceful stop: the guest stops the application, then runs the profile's optional `cleanup_command` through the same shell, working directory and environment handling as `command`, and exits. Cleanup also runs when the application exits on its own, so a failed start does not leave state behind.
- **No second preview while one is active.** A start while a preview is queued, running or closing is refused with a conflict; the only offered action is Close. The decision "stop the previous preview before replacing it" is amended accordingly.
- **The browser shows closing.** The preview view carries a `closing` flag so the dialog shows that cleanup is in progress instead of a dead button. A close whose cleanup failed or overran its bound is reported, not hidden.
- **Profile field.** `cleanup_command` is optional in named profiles and the workspace default block, accepted by the API parser, the create-profile route, the YAML renderer, the shared DTOs, the frontend types and the guest config. The G5 repository's `dev-up` profile cleans up with `just dev-down`.
- **No new task kind or command path.** Cleanup rides inside the existing preview task so no new catalog module, spec validation or dispatch ordering is needed.
- **The HTTP gate closes when close begins**, so browser tabs cannot reach a half-torn-down application.
- Out of scope: cleanup on workspace stop or VM loss (docker state is per boot), and any terminal recording.
