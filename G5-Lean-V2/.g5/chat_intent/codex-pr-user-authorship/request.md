# PR authorship

User follow-up: “rebase the branch on main to pick up changes”. Preserve both branches’
compatible intent, reconcile conflicts and update the existing PR.

## Workspace CLI usability follow-up

User reported that an agent chose `gh` for PR creation and that `g5d` was absent from
the terminal PATH, then said “close the gaps” after the proposed fixes: consistent
agent PR instructions, shared command availability, automatic workspace/authentication
context without exposing dispatcher credentials, and terminal/provider verification.
User then asked “why can't the terminal just call g5d directly?” The assistant corrected
the proposed adapter: use direct CLI calls with the same workspace-scoped token as agents,
as already permitted by the MVP guest-token boundary. Host/dispatcher control credentials
and GitHub publication credentials remain outside this terminal context. Implement this
simpler approach in the existing PR design.

User: “Does G5 using the github app create a PR as the user or as itself. Is it possible to create the PR as the logged in user?”

Context: The assistant explained that current App-bound PR creation uses installation
credentials, while GitHub supports user access tokens and G5 already has user grant
infrastructure for issue comments.

User: “make a branch and add this support”

Interpretation: Implement PR creation as the initiating user's connected GitHub account
on a new branch, reusing GitHub App user authorization. Preserve repository restrictions
and show actionable connection failures instead of silently using a bot.

User: “Let's centralize Connect Github somewhere - is it possible to do on login?”

User selected: “Offer Connect GitHub after login and manage it in Account settings
(recommended; keeps existing sign-in).”

Interpretation: Offer optional App authorization after login, preserve the intended
destination, and use one account-level connection for PRs and issue comments. Existing
platform identity OAuth and repository/organization permissions remain separate.

User: “g5d could use a 'pr' command modeled after 'gh pr' for retrieving pr information.”

User selected: “Both: current workspace by default, GitHub by number or URL”.

Interpretation: Add read-only `pr list`, `pr view` and `pr status` commands with human and
JSON output. Workspace context selects G5 PR records by default. Explicit GitHub numbers
or URLs retrieve provider information for a repository attached to that workspace,
through the platform's existing authorized credential boundary.

User: “update JSON compatibility” after reviewing the `gh pr` differences.

Interpretation: Make PR JSON field selection and view/list shapes follow `gh`, with
supported field discovery and validation. Keep workspace selection semantics and other
G5 commands' existing boolean JSON behavior. This does not add jq/template engines or
otherwise implement unavailable provider fields.

User: “let's add the missing pieces. But ensure that the cli commands call into exactly
 the same flow as the UI buttons - eg there's exactly one 'create/update PR' flow”.

Interpretation: Implement `g5d pr create` and `g5d pr update` for owner-bound VM agents
and signed-in CLI users as transport adapters to the existing UI create/update operation.
Keep all preparation, publication, PR reuse and provider synchronization in one owner.
Provide discoverable worker-agent guidance for the new commands. Preserve typed VM
identity, current-incarnation checks, target access, publication readiness and user authorship.

User asked whether PR creation can infer its destination, whether G5 PRs have titles,
and why `view 42 --json id` fails, then instructed: “make these fixes”.

Interpretation: infer the destination through the same default as the UI (parent first,
then eligible Base), preserve explicit target selection, add `-t/--title` through the
shared create/update API, and expose GitHub's node ID under JSON `id`. The current
workspace remains the implicitly discovered source.

User reported that pushing a new commit left the internal PR pinned to its older SHA
and required a separate Update PR before merging. The assistant proposed automatically
refreshing open PRs after successful pushes while preserving version-bound approvals
and exact-commit merge checks. User: “go ahead and make this change”.

Interpretation: make existing open workspace PRs follow successful committed source
branch pushes through the shared publication owner. Preserve explicit PR creation,
current authority, provider failure recovery and merge-time race protection; do not
use automatic refresh to save dirty work, start compute or approve newer commits.


## CI and review follow-up

Human request: “fix ci failures and code review comments. you have gh access outside the sandbox.” Continue the existing PR, preserving the shared publication flow and governing authority.


## Rebase after main adds migrations

Human request: “It looks like main added new migrations? Run a rebase if necessary.”


## Manual retry without new migrations

User: “if the user just re-runs the update PR button, does this correct the PR state? If so, we might not need these pending columns?”

After explanation that ordinary Update PR retries can repair synchronization and a failed
explicit title would need `--title` resubmitted, user: “As long as we provide a good error,
I think that retry is acceptable. Can we remove these migrations?”

Interpretation: remove branch migrations 055/056 and their pending title/refresh/error
columns and dependent machinery. Keep automatic refresh after successful pushes, useful
failure details with the Update PR / `g5d pr update` recovery action, and one shared UI/CLI
flow. Manual retry, including resubmission of an explicit failed title, is acceptable.

## Inspect PR checks from the VM

User: “I also wanted to look up the check state from the PR. Is this reasonable to add
to the PR interface? My goal is to allow the in vm agent to run the push, check ci, read
failures, then fix loop. (allowing the agent to directly read gcloud cloud build is a
separate project)”

Interpretation: design a read-only `g5d pr checks` interface for the current workspace's
provider PRs, with watch/JSON output and the failure evidence available through GitHub.
Protect the loop against results from an earlier pushed commit. Direct Cloud Build
access and fetching full build logs are excluded. User subsequently instructed: “go ahead and add pr checks support”. Implement the saved
design, preserving the existing publication and authorization boundaries.

## Rebase and migration history follow-up

The user requested rebasing the PR onto current main, then asked to squash commits
so the removed PR retry-state migrations do not remain in the PR's intermediate
history. Preserve main's migrations and the final PR behavior.
