# Git

<a id="g5.git.authority"></a>
[I · g5.git.authority]

the platform internal Git service is the durable authority for repository content and workspace branches  

<a id="g5.git.protocol"></a>
[I · g5.git.protocol]

it exposes a standard git interface which uses smart HTTP to advertise refs, transfer objects, and receive objects (git-upload-pack and git-receive-pack)  

<a id="g5.git.authentication"></a>
[I · g5.git.authentication]

authentication uses HTTP Basic carrying a G5 JWT  

<a id="g5.git.vm-authority"></a>
[I · g5.git.vm-authority]

VM git requests use current workspace-incarnation authority, which is checked against the workspace, repository, and live incarnation  

<a id="g5.git.commit-identity-question"></a>
[D · g5.git.commit-identity-question]

The author and committer text inside a pushed commit is advisory; the recorded push is the authoritative attribution. A push is attributed to the workspace that made it and to the person who owned that workspace at the time of the push, which a later transfer does not change: VM pushes carry incarnation-bound [VM authority](#g5.git.vm-authority) and update only that workspace's branches, or the owner's personal branch through the [personal mount](../features/personal_workspace.g5.md#g5.personal.mount-push), which admission credits to the personal workspace; G5 sets author and committer to the owner for agent work. G5 does not verify the text at receive, so code in a VM, or an owner using Git directly, can commit under another member's name, and views that attribute by commit (blame, intent history) may show that person. The [attribution tier](../concepts/system_invariants.g5.md#g5.security.tier1.attribution) therefore holds for the push record, not the commit text. Reason: the push already carries a verified identity, while verifying commit text would refuse legitimate rebases, cherry-picks and imported history whose authors are not the owner.

<a id="g5.git.query-api"></a>
[I · g5.git.query-api]

the platform exposes an API for querying workspace repository files, file content, commit history, and commit diffs.  

<a id="g5.git.multi-repo-merge"></a>
[I · g5.git.multi-repo-merge]

When merging a workspace with multiple repos into its parent, ensure that the merge is observed atomically within the platform.  For repos without external repositories, this means perform the update to the internal git repos under the appropriate locks.  For external repos, ensure that when the parent pulls from the remote, it pulls across all repos.  

<a id="g5.git.provider-binding"></a>
[I · g5.git.provider-binding]

External synchronization explicitly binds a workspace repository to a provider repository and branch.  

<a id="g5.git.save-publication"></a>
[I · g5.git.save-publication]

Saving to G5 and publishing externally are separate operations with separate outcomes.  

<a id="g5.git.push-policy"></a>
[I · g5.git.push-policy]

Push policy chooses automatic forwarding or an explicit user push.  

<a id="g5.git.pull-policy"></a>
[I · g5.git.pull-policy]

Pull policy chooses automatic imports or an explicit user pull; imports leave live VM files unchanged.  

<a id="g5.git.publication-authority"></a>
[I · g5.git.publication-authority]

Publication requires current authorization and respects the provider’s branch protections.  

<a id="g5.git.credentials"></a>
[I · g5.git.credentials]

See the [server-side publication credential boundary](workspace_vm.g5.md#g5.security.credentials.publication).

<a id="g5.git.branch-creation"></a>
[I · g5.git.branch-creation]

New external branches require explicit creation; synchronization never silently recreates deleted branches.  

<a id="g5.git.sync-state"></a>
[I · g5.git.sync-state]

Synchronization distinguishes local history changes, provider history changes and divergence.  

<a id="g5.git.divergence"></a>
[I · g5.git.divergence]

Divergence pauses synchronization until the owner requests resolution; provider history rewrites require separate approval.  

<a id="g5.git.base-import"></a>
[I · g5.git.base-import]

Base workspace imports provider history and never pushes to the provider except for [explicitly confirmed administrator branch creation](#g5.git.base-create-exception).

<a id="g5.git.base-create-exception"></a>
[D · g5.git.base-create-exception]

[Base never pushes](#g5.git.base-import) to its provider except in one administrator action: when an administrator's [Make Remote](#g5.git.make-remote) on Base finds the provider branch missing, G5 asks them to confirm, and only after that confirmation creates the branch from Base's saved head, acting as that administrator. Synchronization and every other operation on Base never push. Reason: an explicit, confirmed administrator action is the right gate for the one provider write that binds Base.

<a id="g5.git.pr-save"></a>
[I · g5.git.pr-save]

Creating or updating a pull request in the UI saves and pushes pending source work before publishing an exact committed version. `g5d pr` publishes committed work only ([CLI](#g5.git.pr-cli.create-save)).

<a id="g5.git.merge-recovery"></a>
[I · g5.git.merge-recovery]

Merge operations retain their selected versions and reconcile partial completion safely.  

<a id="g5.git.ci-revision"></a>
[I · g5.git.ci-revision]

PR and CI status identify the provider revision observed and disclose missing or stale information.  

<a id="g5.git.check-suites"></a>
[D · g5.git.check-suites]

A queued rerequest cannot reuse earlier completed results as current: runs in a queued suite read pending. Suites themselves are not reported. GitHub creates one on every push for each installed App with checks access, including Apps that never post a run, so an empty suite says nothing about whether a check is coming. Checks that must report are named by [required checks](#g5.git.required-checks).

<a id="g5.git.check-read-settling"></a>
[D · g5.git.check-read-settling]

A check read that could settle the verdict [repeats its observation](#g5.git.check-observation) and compares GitHub's view of the PR when it starts and when it ends. Only a change to the PR itself (its head, state or base branch) means the checks it read belong to another revision; that is reported at once. Anything else that moves meanwhile is GitHub settling around an unchanged head: the base branch advancing, the test merge being recomputed, mergeability finishing or restarting, or a check attempt changing. The read is then repeated, up to three reads in all, and reports current results once nothing moved. If GitHub still has not settled, it says which of these kept moving instead of claiming the PR changed. A read that is pending anyway, including one where GitHub is still computing the test merge, is not repeated. Reason: owner request (2026-10-30); Ship to parent showed "PR revisions changed" when only GitHub's test merge or the base branch had moved. Repeating only reads that could settle keeps [the provider read budget](#g5.git.provider-read-budget).

<a id="g5.git.required-checks"></a>
[D · g5.git.required-checks]

Check reads mark which results the PR's base branch requires, taken from GitHub: its classic branch protection and its active rulesets for that branch. A requirement names a check run or commit status, optionally bound to the GitHub App that must post it. It is met only on the revision GitHub judges: the PR's test-merge commit once that commit has any check or status, otherwise the head. An App-bound requirement matches check runs from that App and commit statuses by name only, because GitHub does not report which App posted a status; App-bound statuses are often created under a plain user account, so refusing them would leave those requirements expected forever. A required check with no result on the judged revision is listed as expected and pending, so a check that has not reported yet is visible by name instead of inferred. Strict up-to-date rules and other ruleset conditions stay GitHub's to enforce. Check output and Prepare to Ship list each repository's required checks apart from the others and say when its base branch requires none.

The SCM provider client reads the required set alongside checks and statuses; GitHub owns both check state and branch requirements. Display and CLI reads may use a bounded in-process cache per repository and base branch, held for up to ten minutes and lost freely; the [merge policy](#g5.git.external-check-policy) reads the set from GitHub for each decision. No database state, queue or server-side monitor is added. A PR the gate admits still merges through GitHub's own merge before G5 applies it, and GitHub enforces its required checks, App bindings included, at that merge. A set that cannot be read makes that repository's checks unavailable; it is never read as "nothing required".

<a id="g5.git.pr-identity"></a>
[I · g5.git.pr-identity]

A G5 pull request identifies a source workspace, a target workspace and the exact repository versions proposed for merge.  

<a id="g5.git.pr-providers"></a>
[I · g5.git.pr-providers]

One G5 pull request may link to provider pull requests across multiple repositories.  

<a id="g5.git.pr-repository"></a>
[D · g5.git.pr-repository]

Expands [g5.git.pr-providers](#g5.git.pr-providers). A G5 pull request has one repository pull request for each member it changes, as [publication](#g5.git.save-publication) decides (the recorded source and target versions differ and the preflight finds the member neither identical nor already merged); an unchanged member has none, except that a GitHub pull request already recorded for it stays. The G5 pull request is the unit of work: it owns its identity, recorded versions, canonical description, approvals, merge admission and lifecycle, and its repository pull requests share that lifecycle, except that closing it leaves its GitHub pull requests open on GitHub, as a close always has. Each repository pull request is served by the [adapter](#g5.git.pr-adapter) its member binds: GitHub when the member has a GitHub link or publication would publish it to GitHub (a GitHub repository tracking a remote branch on both sides), otherwise local. The binding follows the member, not the pull request's creation: a member that becomes published gets its GitHub repository pull request at the next refresh or update, and the local one, which holds no state of its own, ends with it. A lost or failing connection never rebinds a member; it stays an error with its recovery step ([authorization loss](#g5.git.authorization-loss)). The binding is the same whatever the G5 pull request's status: a member publication would have published but never did is GitHub's, unpublished, and shares the G5 pull request's end. Reason: owner request (2026-10-29), so that a pull request is per repository as in `gh` while one workspace's change still ships as one reviewed, atomic unit ([intent](../../chat_intent/pr-adapter/request.md)).

<a id="g5.git.pr-adapter"></a>
[D · g5.git.pr-adapter]

One pull-request service serves every pull-request surface: the UI, `g5d pr`, merge when ready and the agent paths. It keeps the provider-neutral rules: read and write authority, PR selection, the publication decision and adapter binding, the G5 identity of repository pull requests, the [canonical description](#g5.git.pr-description), approvals, [merge admission](#g5.git.pr-recovery-cut), coordination of the atomic merge, merge when ready, overrides, events and announcements. It sends repository pull-request operations (read, list and search, checks, conversation, [attachments](../features/issues.g5.md#g5.attachments), merge state, merge, merge observation, publication, edit, close) to the adapter of each repository pull request, and coordinates a merge by asking each adapter for its effect inside the one admitted merge. The GitHub adapter owns GitHub's protocol and guards: credentials, publication pushes and permission refusals, workflow files, required checks, provider rules and merge state, the acting identity of a [GitHub override](#g5.git.provider-override), comment receipts and the agent search budget. The local adapter has no provider effects: it answers from the G5 pull request and Git (state, recorded heads, the canonical description as its body), reports that no external checks apply, and adds nothing to a merge beyond the G5 cut. Stored per-repository records hold link identity and what G5 decided or must recover (published heads, expected target, observed merge evidence); a single pull request's provider content is read live through its adapter, and a listing that shows a last recorded observation says so. No surface branches on the provider: an operation an adapter does not support fails before any effect with the reason and the alternative, as [unsupported verbs](#g5.git.pr-cli.unsupported) do. The local adapter is not a repository provider, so it sits outside ScmProvider; a GitLab adapter would sit on its ScmProvider client. Reason: the issue design's split ([native adapter](../features/issues.g5.md#g5.issue.native.adapter)) gives one experience and one code path without spreading GitHub's protocol into G5-only work ([intent](../../chat_intent/pr-adapter/request.md)).

<a id="g5.git.pr-local-number"></a>
[D · g5.git.pr-local-number]

Every G5 PR has a number from 1 to 0xffffff, chosen at random when it is created among those its organization has not used, and is named `PR` with that number as six zero-padded lowercase hex digits (for example `PR3fa9c1`) wherever a person or tool sees it, so it never reads as a GitHub number; PRs that existed before numbering got random numbers the same way. Choosing a number holds no lock and uses no sequence, so numbers reveal neither creation order nor how many PRs exist. Only two creations that pick the same number at once meet: the later waits for the earlier to end and, if it was saved, takes another number. This assumes an organization stays far below 16 million G5 PRs; a creation that finds no unused number after 32 tries fails, and outgrowing six digits needs a new decision. `PR` and exactly six hex digits, in any case, selects any G5 PR the reader may [read](#g5.git.pr-reads), in any repository, and its local [repository pull requests](#g5.git.pr-repository) have no number of their own: they are named by their G5 PR. A bare number (`42`, `#42`) always means GitHub's PR; in a repository not on GitHub it names nothing and is refused, pointing to the `PR` form. `g5d pr view --json` keeps `number` for the GitHub PR, as `gh` does, and gives the G5 PR's name as `g5Number`. Reason: owner decisions: "PR numbers that look like GitHub numbers are bad" and "don't hold G5 locks while waiting for GitHub" (2026-10-30); `PR` and six zero-padded hex digits, with random numbers and no sequence (2026-11-02).

<a id="g5.git.pr-local-conversation"></a>
[D · g5.git.pr-local-conversation]

Local repository pull requests carry no conversation. `g5d pr view -c`, `comment` and `resolve` on one fail before any effect, naming the workspace's [comments and reviews](../features/reviews.g5.md#g5.governance.review.comments) as the alternative, so the local adapter keeps no state of its own. A later conversation would need a store keyed by G5 PR and repository and a rule for what happens to it when its member [becomes published](#g5.git.pr-repository). Reason: as for [numbering](#g5.git.pr-local-number).

<a id="g5.git.pr-description"></a>
[D · g5.git.pr-description]

A G5 PR's canonical description is its saved description, else its PR-body Project Doc; a PR with neither has none. Each provider PR's published body is compared with what the existing publication serializer would publish for that repository from the same PR record, including the legacy body it falls back to after GitHub refuses a derived body, after normalizing line endings only: matching, different, or unavailable when the provider body or the expected body cannot be read. Only all-matching publications are shown as matching. The comparison is a read made when shown, with no stored synchronization state, and never writes either side. Provider bodies are shown as GitHub renders them, and the G5 description is rendered through GitHub's Markdown API with the sole provider repository holding a published PR as context (no context when none or several have publications), sanitized, with the local renderer as fallback. Without provider publications the description is read without any provider call. Every surface that shows a stored G5 PR's description takes it from this one read and never resolves the precedence itself (when the read fails, the landing may show the saved text it already has, marked as possibly out of date): the [landing](../features/ui.g5.md#g5.workbench.landing.implementation) and `g5d pr view`, whose `body` is this description and whose text output also says when publications differ or are unchecked. Listings (`g5d pr list`, `--all`, `--workspace`, `status`, and the stored rows of a repository listing) resolve `body` for the PRs they show in one batch read with the same precedence and no provider call, since comparison and rendering are what make a single read costly. Both reads serve any [readable PR](#g5.git.pr-reads). A GitHub PR read by number or URL shows GitHub's body, as `gh pr view` does. Reason: issue #816; GitHub edits and unpublished G5 updates both make the two diverge, and a second resolver would drift from this one.

<a id="g5.git.pr-update"></a>
[I · g5.git.pr-update]

PR creation and update follow [save-before-publication](#g5.git.pr-save).

<a id="g5.git.pr-write-identity"></a>
[D · g5.git.pr-write-identity]

Update continues an existing open G5 pull request and never creates one, even if it closes while source work is saved. Update may create a repository’s first provider PR to recover incomplete initial publication. Replacing a known closed or merged provider PR requires an explicit create-new request, after recorded merge recovery finishes. Report saved G5 and per-repository provider outcomes separately under the [publication rule](#g5.git.save-publication).

<a id="g5.git.pr-refresh"></a>
[I · g5.git.pr-refresh]

Later source pushes refresh open pull requests under the applicable publication policy.  

<a id="g5.git.live-rebase"></a>
[I · g5.git.live-rebase]

A running workspace rebases in its current owner VM, including clean, fast-forward and no-op cases.  

<a id="g5.git.stopped-rebase"></a>
[I · g5.git.stopped-rebase]

A stopped workspace may rebase centrally only when its saved state is clean and fully published.  

<a id="g5.git.lifecycle-rebase"></a>
[I · g5.git.lifecycle-rebase]

Rebase refuses during workspace lifecycle transitions rather than switching execution locations.  

<a id="g5.git.rebase-history"></a>
[I · g5.git.rebase-history]

Rebase incorporates missing G5 commits and preserves unpublished local commits without duplicating existing changes.  

<a id="g5.git.dirty-rebase"></a>
[I · g5.git.dirty-rebase]

Dirty files or unresolved Git state block the affected repository without discarding its work.  

<a id="g5.git.conflicts-rebase"></a>
[I · g5.git.conflicts-rebase]

Replay conflicts pass to a resolution task in the same VM.  

<a id="g5.git.conflicts-rebase.models"></a>
[D · g5.git.conflicts-rebase.models]

When an agent resolves a rebase in which both sides changed TLA+ models, it also makes the affected models agree with the result. It fixes what the combination broke, then makes sure each affected model parses and its declared runs complete, and records those runs and any changed outcome. It does not re-verify the models or bring them up to date with the code; that is a [Base re-verification](../features/ontology_and_harness.g5.md#g5.reverification)'s work.

- A repository without TLA+ models, or a rebase in which one side changed none, skips this.
- Affected models are those whose files either side changed, those that use a changed module, and those linked from a reconciled ontology statement.
- The rule applies to the resolution task and to agents that rebase or merge on their own.
- A G5 rebase that Git replays without a conflict starts no resolution task, so models it combines cleanly stay unchecked until the next Base re-verification. The same holds for models that a merge into a parent combines.

<a id="g5.git.stopped-rebase.resolve-with-agent"></a>
[D · g5.git.stopped-rebase.resolve-with-agent]

While the owner's VM is stopped or failed, a rebase that meets conflicts is not refused for want of a VM: the conflicts dialog's agent choice reads **Start and resolve with agent**, with a note saying it starts the workspace and queues the agent's first message, and one click is one request that records the owner's [Start consent](../features/chat.g5.md#g5.chat.start-and-send) and queues the resolution brief bound to it, so the agent rebases and resolves once the VM is up. While the owner's own Start is already under way the choice stays Resolve with Agent, shows the Start's progress, and the brief joins that Start. A Start that ends before compute is ready withdraws the brief as every Start and send message is withdrawn, the task it leaves no longer counts as the active resolution, and Resolve with Agent can be pressed again. A rebase the stopped workspace could not run anyway refuses before any VM starts: unpublished or dirty saved work ([dirty rebase](#g5.git.dirty-rebase)), Git inputs the agent cannot fix, or someone other than the owner; a clean rebase keeps landing [centrally](#g5.git.stopped-rebase); and a VM found running by the time the Start is recorded takes the [live path](#g5.git.live-rebase). Reason: the owner's request (2026-11-06) that Resolve with Agent start a VM when none is attached, like Start and ship with agent.

<a id="g5.git.publication-head"></a>
[I · g5.git.publication-head]

Publication verifies that the captured G5 branch head has not changed.  

<a id="g5.git.multi-repo-results"></a>
[I · g5.git.multi-repo-results]

Multi-repository results identify completed members and blocked or failed members.  

<a id="g5.git.rebase-completion"></a>
[I · g5.git.rebase-completion]

Rebase completes when G5 accepts the result; external publication follows the workspace’s push policy separately.  

<a id="g5.git.rewrite-approval"></a>
[I · g5.git.rewrite-approval]

A local rewrite needs no extra approval when the provider is unchanged; provider rewrites require separate approval.  

<a id="g5.git.make-remote"></a>
[I · g5.git.make-remote]

"Make Remote" operation explicitly binds a workspace repository to an external branch. Only an organization administrator who owns the workspace may run it (on Base, any administrator, per [base-admin](../features/workspaces.g5.md#g5.governance.pr.base-admin)); the server refuses others before any provider effect.  

<a id="g5.git.make-remote-create"></a>
[I · g5.git.make-remote-create]

A missing external branch is created from the saved G5 head.  

<a id="g5.git.make-remote-import"></a>
[I · g5.git.make-remote-import]

Compatible provider history is imported without overwriting divergent history.  

<a id="g5.git.make-remote-pause"></a>
[I · g5.git.make-remote-pause]

Incompatible histories bind paused until explicitly reconciled, either by agent or by adopting the provider history.  

<a id="g5.git.login-separation"></a>
[I · g5.git.login-separation]

Signing into G5 through GitHub does not grant repository access.  

<a id="g5.git.repository-access"></a>
[I · g5.git.repository-access]

GitHub Repository access uses a connected personal credential or an organization’s GitHub App installation.  

<a id="g5.git.repository-operation"></a>
[I · g5.git.repository-operation]

Each GitHub repository operation requires current authority for the selected repository and action.  

<a id="g5.git.auto-publication"></a>
[I · g5.git.auto-publication]

Automatic publication requires organization GitHub App credentials.  

<a id="g5.git.authorization-loss"></a>
[I · g5.git.authorization-loss]

Missing, expired or revoked authorization requires reconnection; G5 never substitutes another user or the installation bot.  

<a id="g5.git.pr-authorship"></a>
[I · g5.git.pr-authorship]

New GitHub pull requests use the initiating user’s connected GitHub identity.  

<a id="g5.git.server-credentials"></a>
[I · g5.git.server-credentials]

GitHub publication follows the [server-side publication credential boundary](workspace_vm.g5.md#g5.security.credentials.publication).

<a id="g5.git.protections"></a>
[I · g5.git.protections]

GitHub permissions and branch protections remain binding.  

<a id="g5.git.workflow-files"></a>
[D · g5.git.workflow-files]

Whether G5 may publish changes to GitHub Actions workflow files through the organization's GitHub App is the installation's grant, under [binding provider permissions](#g5.git.protections). App Git writes (explicit and PR-source publication, automatic forwarding, merge) request Workflows write only when the binding's recorded installation permissions include it; otherwise they request contents write alone, so ordinary writes never depend on that grant.

<a id="g5.git.workflow-files.no-escalation"></a>
[D · g5.git.workflow-files.no-escalation]

Pushes do not inspect their changed paths to widen the token per push, and do not retry a refusal with a wider one: the server pushes exactly the authorized head, and per-push escalation would add a second provider policy to every write path.

<a id="g5.git.workflow-files.refusal"></a>
[D · g5.git.workflow-files.refusal]

A provider refusal for a missing Workflows permission or workflow scope is an expected, sanitized publication refusal, never an internal error. It names the refused file, the permission and its recovery (an administrator grants and approves it on the installation and refreshes the binding, or the user reconnects with the scope), and the refused branch is left unchanged. During PR creation it is a failed source push under [PR save](#g5.git.pr-save).

<a id="g5.git.receipt.limits"></a>
[D · g5.git.receipt.limits]

Default capture limits are 100 MiB per file and 2 GiB per tree/pack, with oversize failing rather than silently excluding content.

<a id="g5.git.network.lease"></a>
[D · g5.git.network.lease]

Agent profiles require force-with-lease because the server cannot prove that the client incorporated a prior head.

The agent profiles are the [guidance G5 installs](../features/ontology_and_harness.g5.md#g5.harness.guidance-delivery) for workspace agents and the prompts of the tasks it launches: they let an agent replace published commits only with a push leased on the head captured before the rewrite, never with a bare force push.

<a id="g5.architecture.git_owner"></a>
[D · g5.architecture.git_owner]

Git owners expose checked receive-pack, publication and merge operations over the organization-keyed repository store and its [tenancy guard](../features/repositories.g5.md#g5.repo.storage-guard), GitRepo and workspace Git mechanisms, all executed through the Git execution layer. Retained remote integration owns provider forwarding and incoming publication behavior.

<a id="g5.architecture.repository_credentials"></a>
[D · g5.architecture.repository_credentials]

RepositoryCredentials resolves repository and operation authority for Git/governance callers. Binding snapshots, installation IDs, permission mapping, token minting and caches stay private to that service and its provider adapter.

<a id="g5.architecture.governance_owner"></a>
[D · g5.architecture.governance_owner]

Git/governance coordinates publication and merge over exact Git versions through prepare/apply/land operations and separately authorized approval decisions.

<a id="g5.architecture.git_execution"></a>
[D · g5.architecture.git_execution]

g5-scm is the only platform code that opens repositories, links libgit2 or starts the git program. Its public operations use owned Git inputs and outcomes, keeping backend handles, command arguments and environment assembly private. Policy owners compose complete operations without their own libgit2 access; entry adapters call those owners. The native and libgit2 implementations share backend-independent types, and native-only consumers need not link libgit2. Application content consumers retain the authorized reader, which uses this layer without relinquishing source selection, pinning or read authority. Projection maintenance uses its declared typed-reader path. Moving a raw read into g5-scm does not authorize a caller to bypass either owner. This hides the libgit2-versus-native variation (stable abstractions) and gives credentials, environment and ref safety one home. Git that users and agents run interactively in the guest is their own tool (familiar Git), not platform code.

<a id="g5.architecture.scm_provider_client"></a>
[D · g5.architecture.scm_provider_client]

Each repository provider (GitHub, GitLab) has one client behind ScmProvider that owns its provider operations, pagination and error meaning. Pull-request reads and lifecycle, merges, external issues, App installation binding, user repository-access checks, SCM-connection validation, organization/group listing and remote integrations request its typed operations; no caller receives its raw HTTP client or branches on the provider name itself. Clients consume credentials scoped to the requested operation. They own provider protocol and provider-specific errors; account, installation-binding and publication policy remains with their existing owners. Provider selection belongs at the composition boundary, not in each consumer. Two owners stay outside it because their credential and authority differ: OAuth login identity (authorize, token exchange, user and email lookup) in AuthService, and platform bug reports in PlatformIssueGithubClient. Calls that use a stored SCM connection token are provider operations, even when AuthService exposes them.

<a id="g5.git.pr-cli"></a>
[I · g5.git.pr-cli]

the 'g5d pr' CLI surface area and functionality intentionally closely follows 'gh pr' to enable agents to naturally use it  

<a id="g5.git.pr-reads"></a>
[D · g5.git.pr-reads]

Read-only `g5d pr` requests follow `gh pr`'s rule that read access to the code is read access to its pull requests: whoever can open a workspace under the [audience rule](../features/workspaces.g5.md#g5.workspace.audience) can list, view and inspect every G5 PR into or out of it, with its description, history, checks, publications and GitHub conversation. That covers a person and the workspace agent acting as its owner, and it imposes no other read limit. Deleting a workspace does not hide its PRs: whether the source, the target or both are deleted, `g5d pr`, the UI and the API behind both list, view and inspect them as before, and the UI and tools get the same answers ([UI presentation](../features/ui.g5.md#g5.ui.deleted-workspace-prs)), for whoever could open the deleted workspace under the audience rule applied to its retained registry identity and owner, with current active organization membership, and say which side is deleted ([deletion](../features/workspaces.g5.md#g5.workspace.delete.pull-requests)). Answers carry everything G5 or GitHub shows that reader, withholding only what is secret by design (credentials, redacted check output, workspaces the reader cannot open). Writes (comment, resolve, edit, close, merge, create, update) keep their own owners. Reason: owner request, 2026-10-28, so agents can audit PRs across G5; a narrower rule was a permission model the owner does not want.

<a id="g5.git.pr-read-repositories"></a>
[D · g5.git.pr-read-repositories]

Provider pull-request reads (`g5d pr view` and `checks` by number or URL, and [repository listing](#g5.git.pr-list)) select one GitHub repository: a member of any workspace the reader can open ([PR reads](#g5.git.pr-reads)), including, in a personal workspace, a [reference repository](../features/personal_workspace.g5.md#g5.personal.pr-reads). A bare number uses the workspace's only GitHub repository, or else its only repository; otherwise it needs `--repo` or a URL. A URL never selects a repository outside that set. Within the selected repository every PR is readable, whichever workspace published it, under the owner's [read authority](#g5.git.repository-operation) for that repository, because GitHub's read authority is per repository. [Comments and thread resolution](#g5.git.pr-conversation) select only members.

<a id="g5.git.pr-list"></a>
[D · g5.git.pr-list]

`g5d pr list` with no repository option lists the stored G5 PRs whose source or target is the workspace. A repository option (`--repo`, `--head`, `--base`, `--author`, `--label` or `--search`) lists the [selected repository's](#g5.git.pr-read-repositories) [repository pull requests](#g5.git.pr-repository) as `gh pr list` does, from each [adapter](#g5.git.pr-adapter) that holds some. For a GitHub repository, GitHub decides which of its PRs exist and match, their state and their order (newest created first), including PRs opened outside G5; without a GitHub integration that part uses the [local fallback](#g5.git.pr-list.local). The repository's local repository pull requests are listed beside them, each row naming its tracker, all newest created first. Results are capped at 100 per request, and the output says when more exist. G5 merges record no PR number, so finding a branch's GitHub PR needs GitHub's head-branch match, and stored provider state may lag GitHub.

<a id="g5.git.pr-list.search"></a>
[D · g5.git.pr-list.search]

`--author`, `--label` and `--search` use GitHub's search, as `gh` does; `--state`, `--head` and `--base` alone use the repository's plain PR listing. An agent's search counts as one search against the [agent search budget](../features/issues.g5.md#g5.external-issues.cli.search-budget) shared with issue search, because both draw on the same GitHub search allowance. A refused search sends nothing to GitHub. People's searches are not limited by G5.

<a id="g5.git.pr-list.local"></a>
[D · g5.git.pr-list.local]

The local adapter lists a repository's local repository pull requests, and when the selected repository has no GitHub integration its GitHub repository pull requests are listed from G5's stored records instead. It says so when GitHub was expected (a GitHub repository without a usable connection); a repository not on GitHub is listed without a notice ([no GitHub expected](#g5.git.pr-no-github)). That covers a repository not on GitHub, a GitHub repository whose organization has no GitHub App binding and whose owner has no GitHub connection, and a deployment without GitHub. Both list the G5 PRs that change that repository, as [publication](#g5.git.save-publication) decides (the recorded source and target versions differ, and the preflight finds it neither identical nor already merged), and whose source or target workspace the reader can open under the [audience rule](../features/workspaces.g5.md#g5.workspace.audience). A G5 PR records every member, so an unchanged one would otherwise appear under every repository; each row carries the recorded source head as `headRefOid`. The owner accepted this on 2026-10-28 after a preview listing showed a PR under a repository it did not change. `--head` and `--base` match the recorded source and target branches, `--author` matches the PR's creator, `--search` matches every word in the title or saved description, and `--label` matches none of these rows because G5 PRs have no labels; where only such rows can be listed it is refused. An integration that exists but fails (an expired or revoked connection, an inactive or uncovered App binding, a missing permission, a provider error or rate limit) remains an error with its recovery step. Answering from stored records there would hide the reconnection that [authorization loss](#g5.git.authorization-loss) requires.

<a id="g5.git.pr-no-github"></a>
[D · g5.git.pr-no-github]

Where G5 can show that GitHub is not expected, it never reports GitHub data as missing, unavailable, pending or incomplete. A PR expects GitHub when one of its [repository pull requests](#g5.git.pr-repository) binds the GitHub adapter: it has a provider record or a member that [publication](#g5.git.save-publication) would publish to GitHub (a GitHub repository tracking a remote branch on both sides); a repository listing expects it when the [selected repository](#g5.git.pr-read-repositories) is on GitHub. Without that expectation, `g5d pr checks` succeeds and says that no external checks apply (as [merge](#g5.git.external-check-policy) already treats local-only members; in a PR that does expect GitHub, each local repository pull request is reported the same way instead of being left out), the stored-PR listing answers without a fallback notice, a branch names the G5 PR it heads, and PR text, refresh failures and the agent's ship prompt carry no GitHub or provider wording or steps. Where GitHub is expected but missing (no connection, no publication, a provider error), G5 still says so with the recovery step. Reason: owner request, 2026-10-28; G5 PRs work without GitHub, and a complaint about absent GitHub data misleads people and makes agents chase recoveries that cannot apply.

<a id="g5.git.pr-g5-identity"></a>
[D · g5.git.pr-g5-identity]

A live GitHub PR read or listed through `g5d pr` names the G5 PRs whose stored provider link is that repository and number, newest first, with their IDs, states and source and target workspaces. It names only G5 PRs whose source or target workspace the reader can open under the [audience rule](../features/workspaces.g5.md#g5.workspace.audience), so it never discloses a workspace the reader cannot read. The link is read from stored records with no provider call. A local [repository pull request](#g5.git.pr-repository) names its own G5 PR.

<a id="g5.git.pr-cli.push-updates"></a>
[D · g5.git.pr-cli.push-updates]

A push to the source workspace's branch updates its open PR, as a push to a GitHub PR's head branch does. That covers a plain push and a force-with-lease rewrite: remote integration forwards it to the provider, and [refresh](#g5.git.pr-refresh) records it. Agent guidance, `g5d` help and server messages therefore name `git push` as the way to update a PR's code and never require a further publish step after a forwarded push. Where a push is not forwarded (Auto-push off, or no [App write access](#g5.git.auto-publication)), `g5d pr view`, `g5d pr checks` and a recorded refresh failure say so and name the explicit republish ([update](#g5.git.pr-cli.update)). Reason: this is `gh`'s model and it already works, while the separate publish verb refused rebases and committed whatever else was in the checkout.

<a id="g5.git.pr-cli.edit"></a>
[D · g5.git.pr-cli.edit]

`g5d pr edit` changes a G5 PR's title and saved description and applies them to its provider PRs, as `gh pr edit` does, without saving or pushing source work. Retargeting, labels, reviewers, assignees and milestones are refused with the reason.

<a id="g5.git.pr-cli.selectors"></a>
[D · g5.git.pr-cli.selectors]

Every `g5d pr` command that takes a PR accepts a G5 PR's ID or [name](#g5.git.pr-local-number) (`PR3fa9c1`), a GitHub PR's number or URL, or a branch name, and defaults to the workspace's outgoing PR. A number is always GitHub's and needs an unambiguous repository as [PR reads](#g5.git.pr-read-repositories) do. A command that acts on a G5 PR maps a GitHub PR to it through the [stored link](#g5.git.pr-g5-identity) of an open G5 PR and refuses when none or several match.

<a id="g5.git.pr-cli.unsupported"></a>
[D · g5.git.pr-cli.unsupported]

`gh pr` verbs and flags that G5 does not provide fail before any request, with the reason and the G5 alternative. These are `review` (an agent never [decides approvals](../concepts/security.g5.md#g5.security.agent-human-exceptions)), `ready` and drafts, `checkout`, `reopen`, `update-branch`, `diff`, `lock`, `unlock`, `revert`, merge strategies other than rebase, `--jq`, `--template` and `--web`. They are never silently ignored.

<a id="g5.git.pr-cli.update"></a>
[D · g5.git.pr-cli.update]

`g5d pr update` stays, hidden from the main help, as "republish now". Workspaces whose [pushes are not forwarded](#g5.git.pr-cli.manual-forwarding) use it to publish, and anyone can use it to retry a failed refresh. It never saves: it publishes the pushed head, and like [create](#g5.git.pr-cli.create-save) it refuses uncommitted work and pushes unpushed commits first. With `--title` or `--body` it is [edit](#g5.git.pr-cli.edit). It says that a push is the normal way to update a PR.

<a id="g5.git.pr-cli.create-save"></a>
[D · g5.git.pr-cli.create-save]

`g5d pr create` publishes committed work only, as `gh pr create` does. It pushes a checkout's unpushed commits to its workspace branch (fast-forward only), and refuses before any effect when a member checkout has uncommitted changes, listing them. It never commits. The UI's Create PR and Update PR keep the [save](#g5.git.pr-save). Reason: committing on the agent's behalf swept up other sessions' edits in shared checkouts and could not publish a rebase.

<a id="g5.git.pr-cli.manual-forwarding"></a>
[D · g5.git.pr-cli.manual-forwarding]

An open PR does not forward pushes from a workspace with Auto-push off; [automatic publication](#g5.git.auto-publication) stays the only automatic path. While the provider branch lacks such a push, refresh records once, in the PR's history, that it was not forwarded and that `g5d pr update` (Update PR in the UI) publishes it, instead of waiting for it. Once anyone publishes the branch, including with the UI's Push, the PR follows as usual. `g5d pr view` and `g5d pr checks --head` say the same.

<a id="g5.settings.github-connection"></a>
[D · g5.settings.github-connection]

When the deployment offers the GitHub App, offer a disconnected user optional GitHub authorization after login only when the organization they are entering uses GitHub and they are more than a viewer of it; an account entering no organization is not offered it. An organization uses GitHub when it tracks a repository on a GitHub remote or configures a GitHub issue domain — the two surfaces where this shared connection publishes under the user's own identity. The server decides the offer for any member entering the organization, independent of the administrator-only App binding. The offer preserves the intended destination and lets the user continue without connecting, and an explicitly requested connection page still resumes directly. Account settings owns connected-account status, connection, reconnection and disconnection. PR and external-issue surfaces direct users to this shared account connection; neither creates its own grant, and they still prompt in context a user the after-login offer skipped. Connection errors do not prevent platform sign-in, and a stored grant is not proof of current repository access.

## Captured state and recovery

<a id="g5.git.saved-read"></a>
[D · g5.git.saved-read]

Retained dirty or unpublished snapshots preserve recovery state. Public browsing follows the [owner](../features/workspaces.g5.md#g5.workspace.owner-read) and [non-owner](../features/workspaces.g5.md#g5.workspace.durable-read) read rules; recovery storage must not silently replace the committed branch view they require.

<a id="g5.git.flush-proof"></a>
[D · g5.git.flush-proof]

VM flush receipt completion requires the current incarnation and proof of exactly the workspace’s repository members and their immutable retained branch and snapshot commits. Restore uses the saved local head and uncommitted bytes even after published history advances.

<a id="g5.git.pr-recovery-cut"></a>
[D · g5.git.pr-recovery-cut]

Record merge admission for the exact source, target and repository versions before provider effects. Once a provider accepts an admitted merge, recovery retains that admission, including an administrator's [override](../features/workspaces.g5.md#g5.workspace.merge.admin-override.formal), despite later approval changes; different versions need their own admission. Recovery lands the admitted cut: each provider-bound member through its accepted provider merge, and each local-only member at the source head admitted before acceptance; a later commit to any member waits for its own admission. Reconcile open PRs from durable published heads after missed notifications. Refresh must neither replace accepted versions nor report completion for stale prepared heads. Provider-first merge checks the target at merge time: the provider PR's target branch, and the target workspace's head and live remote tip must all match the target head its own fresh preflight classified. It then records that head. It does not use the publication-time target or a creation-time base SHA, so a target move needs no republication. A remote tip the target workspace has not imported yet refuses for retry. Accepted-merge import requires the provider tip it imports to contain the accepted merge and to descend from the target workspace's head at import time, so the import only advances the target and keeps anything the target gained since the merge.

<a id="g5.git.provider-merge-target-serialization"></a>
[D · g5.git.provider-merge-target-serialization]

Provider merge coordination serializes competing G5 PRs by repository and target remote branch across the live-target check and provider acceptance. Before dispatch it durably reserves that target and binds the provider PR, source and checked target versions under [merge recovery](#g5.git.pr-recovery-cut). An unresolved provider outcome retains the reservation across timeout, rollback and restart; elapsed time or an open-PR read alone cannot authorize another PR. Reconciliation preserves the admitted target, and stale completions cannot replace a newer attempt. Release the reservation only after known refusal or proven non-dispatch, each with no earlier unresolved invocation, or reconciliation of the terminal provider outcome. A merge override does not waive this mechanical exclusion. Unrelated targets progress independently. Direct provider writes do not participate in this coordination.

<a id="g5.git.remote-binding"></a>
[D · g5.git.remote-binding]

Make Remote rechecks the captured workspace versions and shared repository identity when binding locally. Create missing provider branches without overwriting a competing branch, and confirm their head before binding. Earlier provider effects remain recoverable if local binding fails. Base creation requires the [confirmed administrator exception](#g5.git.base-create-exception).

<a id="g5.git.external-check-policy"></a>
[D · g5.git.external-check-policy]

Organization administrators may require a passing server-side observation of external checks when merge is admitted; the default is off. Any failing check or status blocks, required or not; so does a [required check](#g5.git.required-checks) still pending or not yet reported, and unreadable or incomplete status. Checks the base branch does not require are advisory until they fail, so a readable repository whose base branch requires none passes unless a check fails. Local-only members are outside this policy, and so is a repository the provider has [already merged](#g5.git.provider-merged-outside): the policy evaluates only repositories G5 has yet to merge. Provider branch protection independently governs acceptance. An administrator's [merge with override](../features/workspaces.g5.md#g5.workspace.merge.admin-override) waives the policy for one merge.

<a id="g5.git.provider-rules"></a>
[D · g5.git.provider-rules]

No G5 merge bypasses the provider's rules silently. Before any repository merges at the provider, every merge reads each provider pull request's merge state; one GitHub reports as blocked by branch protection or rulesets refuses the whole merge before any provider effect, naming the repository, GitHub's state and the [required checks](#g5.git.required-checks) it reports. A state GitHub is still computing is re-read briefly and otherwise refuses for retry. Only the [GitHub override](#g5.git.provider-override) proceeds past a blocked repository. Reason: GitHub's merge endpoint bypasses the rules for any caller allowed to bypass, with no opt-in, and G5 calls it as its App installation (or, without the App, as the acting person), so an organization that made the App a bypass actor would otherwise have every G5 merge bypass its rules unseen and attributed to the App ([#550](https://github.com/G5Labs/G5-Lean/issues/550)).

<a id="g5.git.provider-override"></a>
[D · g5.git.provider-override]

A person with current ordinary merge authority in G5, including a source owner merging into another person's parent, may **merge with GitHub override** while GitHub reports repositories blocked. Organization administration is not required to request this provider bypass; it grants no exemption from G5 requirements ([owner decision](../../chat_intent/issue-1116/override-policy.md)). G5 calls GitHub's merge for the blocked repositories with that person's own connected GitHub identity (their App user token in an App-bound organization), never the App installation's or anyone else's, so GitHub decides whether that person may bypass and records the bypass under their name. A refusal says their GitHub account cannot bypass these rules, with no fallback to another identity. G5 merges the blocked repositories first, so a refusal stops before any irreversible provider effect, then the others as an ordinary merge. It waives nothing G5 imposes: approvals and the external-checks policy must pass or be waived by an administrator's [G5 override](../features/workspaces.g5.md#g5.workspace.merge.admin-override) in the same confirmation. The confirmation, re-check, record and flagging are the [shared override rules](../features/workspaces.g5.md#g5.workspace.merge.admin-override.formal); it is offered beside Merge, never replacing it, and is a human decision under the [browser-session rule](../concepts/security.g5.md#g5.security.browser-session-actions) (the [CLI decision](#g5.git.provider-override.cli-question)). Merge when ready and agents never use it. Reason: GitHub, not G5, knows who may bypass its rules; its per-PR mergeability does not depend on the viewer and its viewer-specific answers are unreliable for rulesets, so only GitHub's response to that person's merge settles it ([#550](https://github.com/G5Labs/G5-Lean/issues/550)).

<a id="g5.git.provider-override.cli-question"></a>
[D · g5.git.provider-override.cli-question]

The GitHub override is browser-only like the G5 override, so one dialog shows every waiver and one record holds it; `g5d pr merge` offers no `--admin`.

<a id="g5.git.provider-merged-outside"></a>
[D · g5.git.provider-merged-outside]

A repository G5 finds already merged on GitHub when it comes to merge it (someone merged it there, bypass included) is recorded as merged outside G5, with the provider's merge commit and time, and G5 finishes the pull request locally without merging it again. Without a recorded successful G5 response, an unconfirmed G5 invocation is conservatively attributed the same way. Its [durable target attempt](#g5.git.provider-merge-target-serialization) preserves the checked versions for recovery but does not identify who caused a separately observed provider effect. The [external-checks policy](#g5.git.external-check-policy) skips such a repository; G5's [approval requirements](../features/reviews.g5.md#g5.governance.approval.requirements) still apply to finishing the merge, so an unmet one waits for approval or an administrator's [G5 override](../features/workspaces.g5.md#g5.workspace.merge.admin-override). The pull request and its Activity show the repository as merged on GitHub outside G5. Accepted-merge recovery of G5's own provider merges is [#696](https://github.com/G5Labs/G5-Lean/issues/696)'s and shares the policy skip.

<a id="g5.git.pr-auto-merge"></a>
[D · g5.git.pr-auto-merge]

Merge when ready (`g5d pr merge --auto`, after `gh pr merge --auto`) is a per-pull-request decision recording who turned it on and when; G5 then performs the ordinary merge as that person once the pull request is ready. Turning it on does not check [merge authority](../features/workspaces.g5.md#g5.workspace.merge): the source workspace's owner or a person who may already merge the target turns it on, and a workspace agent never does. Such a person, the source workspace's owner or the source workspace's agent may turn it off, which stops a merge that has not yet started. A workspace VM never holds a person's own session, so a person turns it on through the web UI rather than a VM command line. Later source pushes keep it on, so a fix-and-rebase loop needs no new decision; what may land stays governed by the target's [approvals](../features/reviews.g5.md#g5.governance.approval.requirements), and merge when ready never becomes a [merge with override](../features/workspaces.g5.md#g5.workspace.merge.admin-override) or a [GitHub override](#g5.git.provider-override), even when its author could make one; a [GitHub-blocked](#g5.git.provider-rules) repository is a waiting reason. G5 instead checks the enabling person's merge authority over the target at merge time, and turns it off, saying why, once they no longer hold it — the target's requirement retired, source ownership lost, membership gone or a transfer pending. Reason: an agent should finish the merge once it is green, but agent guidance alone cannot stand in for a human decision, and a target without requirements would otherwise admit an agent's merge with no human involved ([request](../../chat_intent/merge-when-ready/request.md), [source-owner merge](../features/workspaces.g5.md#g5.governance.pr.source-owner-merge)).

<a id="g5.git.pr-auto-merge.readiness"></a>
[D · g5.git.pr-auto-merge.readiness]

Expands [g5.git.pr-auto-merge](#g5.git.pr-auto-merge). Ready means the merge is admitted with [external checks](#g5.git.external-check-policy) required even when the organization does not require them, and the provider heads have been observed unchanged for two minutes, so checks registering late after a push can still block. A merge attempt that does not land is retried when the source, target or provider heads change, otherwise at most every ten minutes; meanwhile the pull request shows the latest reason it is waiting. One scan owner across API replicas evaluates the pull requests with merge when ready on every 30 seconds, sized for tens at a time. Under the [read budget](#g5.git.provider-read-budget), checks are read on every pass only while checks are pending, heads are settling or a merge is due. In every other waiting state the checks re-read backs off, from one minute doubling to five. This covers failing or unreadable checks (a GitHub rate limit included), unmet approvals, and an attempt that did not land. A repository GitHub already reports merged is outside the checks gate and the settle window, as under the [external-checks policy](#g5.git.external-check-policy); the merge records it [merged outside G5](#g5.git.provider-merged-outside). A change to the pull request's source heads, target heads or admission ends the backoff, as G5's own records show.

<a id="g5.git.pr-auto-merge.with-agent"></a>
[D · g5.git.pr-auto-merge.with-agent]

Ship with agent turns on [merge when ready](#g5.git.pr-auto-merge) and starts a chat in the source workspace, under the clicking person's agent choice, whose agent keeps external checks passing and the branch rebase-mergeable into the target, until the pull request merges or merge when ready is turned off. It publishes fixes and rebases by pushing the workspace's branch (a rebase with force-with-lease), which [pull-request refresh](#g5.git.pr-refresh) forwards to the provider ([push updates](#g5.git.pr-cli.push-updates)). It needs ownership of the source workspace at click, whose agent runs only its owner's chats; like [merge when ready](#g5.git.pr-auto-merge) it does not check target merge authority until merge time. Its chat is queued before the decision commits, so a refusal of the source-ownership check changes nothing. While the source workspace's VM is stopped or failed, the owner's click is **Start and ship with agent**: the same request records their [Start consent](../features/chat.g5.md#g5.chat.start-and-send) with the chat bound to it, so one click starts the workspace, turns merge when ready on and queues the agent's first message; a Start that ends before compute is ready withdraws that chat as every Start and send message is withdrawn, and leaves the merge-when-ready decision as made, so Ship with agent can be pressed again. While a Start is already under way the chat simply queues behind it. Merge when ready without an agent stays available, and an existing decision is kept. [Prepare to Ship](../features/ui.g5.md#g5.workbench.ship-to-parent) always offers it, as the primary control of a blocked ship.

When the [optional VM stop](#g5.git.pr-auto-merge.stop-vm) is selected, the prompt explains that G5 handles shutdown immediately after the merge; the agent issues no separate stop command. Turning merge when ready off ends the agent’s work without stopping the VM.

<a id="g5.git.pr-auto-merge.stop-vm"></a>
[D · g5.git.pr-auto-merge.stop-vm]

Selecting **Stop VM after merge** asks G5 to invoke the source workspace's ordinary [Stop](workspace_vm.g5.md#g5.lifecycle.stop) immediately after a successful merge through Ship to parent, Ship with agent or Merge when ready.

- Each person has a server-stored default, initially off, for all workspaces they own. An explicit per-pull-request choice takes priority, and existing saved choices are preserved.
- Only the source owner may change the choice on an open pull request. Changing a default or choice neither enables merge nor starts an agent.
- Until merge, a pull request without an explicit choice follows its current owner's default. Successful merge fixes the effective choice; later default changes cannot revise it.
- Ownership transfer clears the previous owner's choice. Unmerged pull requests follow the new owner's default; a completed merge cannot stop the new owner's VM.
- Saving and shutdown do not hold up the merge response or other pull requests waiting to merge. Normal Stop owns recovery after admission; an API failure before admission can leave the VM running without undoing the merge or creating a separate shutdown job.

<a id="g5.git.pr-merge-announcement"></a>
[D · g5.git.pr-merge-announcement]

A merged pull request reaches both workspaces' open readers at once. G5 notifies the person merge when ready merged as and the source workspace's owner, except whoever performed the merge themselves, and gives the source workspace's agent chats with queued, running or waiting work a platform notice for their next turn. Merge when ready turned off by anyone other than the source workspace's agent is announced the same way, with its reason. Nothing wakes an idle agent: the people are notified, and an agent working toward the merge learns it from its own reads. A [merge with override](../features/workspaces.g5.md#g5.workspace.merge.admin-override.formal), G5's or GitHub's, is announced as one, naming its actor, reason and what it waived (and a self-override as such), and also notifies the target workspace's owner and the reviewers whose requests it bypassed.

<a id="g5.git.pr-conversation"></a>
[D · g5.git.pr-conversation]

Authorized workspace participants and agents may comment, reply and resolve review threads under the acting person’s provider identity. The attached repository, PR and thread must agree. Exact comment receipts support replay and uncertain-outcome reconciliation without reposting; reread idempotent resolution after an ambiguous result. These writes do not confer formal approval.

## Synchronization history

<a id="g5.governance.remote.sync-history"></a>
[D · g5.governance.remote.sync-history]

The remote integration attempt history (`remote_sync_runs`) records outcomes, not every background pass. A background pass that repeats the latest run's observation exactly (no-op, fetched or fetch failure with the same tips, error and acting user, and no watermark or merge-run effect) extends that run's last-seen time instead of adding a row, so an idle or persistently failing membership adds none. A background no-op superseded by a later run is dropped. Runs older than 30 days beyond a membership's newest 100 are pruned, except a membership's latest integration run, which the [synchronization state](#g5.git.sync-state) and its retry read, and any run that opened a merge run. Current tips stay in the provider observation and policy rows, not in this history.

## SCM import authority

<a id="g5.security.tenant.scm-import"></a>
[D · g5.security.tenant.scm-import]

SCM import requires a person session, credential `workspaces:write` scope and current target-organization write authority held through the committing transaction. Repository discovery requires current read authority rechecked after provider I/O. A scoped database label alone grants neither permission; eligible ordinary members may import without administrator status.

## Publication feedback

<a id="g5.governance.pr.follow-push"></a>
[D · g5.governance.pr.follow-push]

[Automatic PR refresh](#g5.git.pr-refresh) retries expected settling states within a bound before reporting an Activity failure. Terminal or exhausted failures and manual attempts report the failing step and an actionable reason; internal details stay in logs. A reported settling run may restart within its policy. Retries preserve the [accepted publication versions](#g5.git.pr-recovery-cut).

## Provider read budget

<a id="g5.git.provider-read-budget"></a>
[D · g5.git.provider-read-budget]

Every workspace in an organization draws on one GitHub App installation's API budget, so G5's repeated GitHub reads follow provider change, not elapsed time or the number of waiters. Each automatic or waiting reader therefore:
- stops reading on a terminal provider state;
- backs off while nothing it can observe has changed;
- waits for GitHub's reported rate-limit reset rather than retrying sooner.

A read that decides an effect, such as merge admission, stays fresh. Reason: on 2026-10-31 QA exhausted its installation's limit through check watching, merge-when-ready polling and refresh retries of completed provider PRs ([#1016](https://github.com/G5Labs/G5-Lean/issues/1016)).

<a id="g5.git.pr-refresh.completed-provider"></a>
[D · g5.git.pr-refresh.completed-provider]

Suppose [automatic PR refresh](#g5.git.pr-refresh) finds a repository's provider PR merged or closed on GitHub. It then records that observation for that provider PR, reports the [pause](#g5.governance.pr.follow-push) once, and makes no further GitHub read for it. The pause lasts until an Update observes the provider PR open again or a replacement provider PR is recorded. Evidence and [merge recovery](#g5.git.pr-recovery-cut) are unchanged. An unresolved provider merge keeps reconciling as before.

<a id="g5.git.check-observation"></a>
[D · g5.git.check-observation]

A check read that finds a [required check](#g5.git.required-checks) still pending or not yet reported, or a test-merge revision GitHub is still computing, reports that single observation. Any other read could settle the [external-checks verdict](#g5.git.external-check-policy), including one where only advisory checks are still pending. It repeats its observation of the same revisions and of the pull request; if anything changed, it reports pending. Reason: only a settled result ends a wait or admits a merge; a pending one only leads to another read.

<a id="g5.git.check-waits"></a>
[D · g5.git.check-waits]

`g5d pr checks --watch` is a bounded wait:
- It backs off while its observation is unchanged.
- It waits out a GitHub rate limit until the reported reset.
- It stops once every watched provider PR is merged or closed.
- After its timeout, 30 minutes by default, it stops and reports pending.

Prepare to Ship polls pending checks every 30 seconds and holds off until the reported reset while GitHub rate-limits the read. VM agent guidance relies on the watcher rather than rerunning reads.

## Metadata
### g5.git.authority

- human-provided
- Source: [Ben, line 162](../../../docs/ben_new_ontology.md#L162).

### g5.git.protocol

- human-provided
- Source: [Ben, line 163](../../../docs/ben_new_ontology.md#L163).

### g5.git.authentication

- human-provided
- Source: [Ben, line 164](../../../docs/ben_new_ontology.md#L164).

### g5.git.vm-authority

- human-provided
- Source: [Ben, line 165](../../../docs/ben_new_ontology.md#L165).

### g5.git.commit-identity-question

- human-verified
- security
- Raised in the 2026-10-29 meeting and settled by the owner on 2026-10-30 as decision 2 ([request](../../chat_intent/security-invariants/request.md#decisions-the-owner-has-made-2026-10-30)); the reason is the agent's reading of that choice. Governs the Git exception in [attribution](../concepts/system_invariants.g5.md#g5.security.tier1.attribution).
- Re-checked 2026-11-06 at main `89eeea5e4`: the dispatcher sets `GIT_AUTHOR_*` and `GIT_COMMITTER_*` to the owner (`backend/g5d-dispatch/src/git.rs`); `receive_pack` (`backend/g5-api-routes/src/routes/git.rs`) authorizes ref updates through `require_push_ref_updates` and inspects no commit identity.
- Gap: G5 keeps no record dedicated to pushes. A VM push leaves the branch update and a live `workspace.branch_updated` event, and `workspace_revision_events` (migration 100) records observed branch heads and push times per workspace, not who pushed; and the `user_action.http` audit stream records end users' requests, not VM credentials, so attributing an old commit to its push needs a durable push record that stores the workspace and its owner at push time ([follow-up](../../pr_ontology_diff_design/security-invariants/ontology-plan.md#follow-up-work)).

### g5.git.query-api

- human-provided
- Source: [Ben, line 166](../../../docs/ben_new_ontology.md#L166).

### g5.git.multi-repo-merge

- human-provided
- Source: [Ben, line 167](../../../docs/ben_new_ontology.md#L167).
- [ProviderImport](../../uplift/ben-ontology/investigations/git.md) found that background import can publish one fetchable member after all provider merges finish; requiring every member closes that modeled path. [GIT-GAP-1](../../../docs/code_gaps.md) remains: atomic publication alone does not ensure complete inputs.

### g5.git.provider-binding

- human-provided
- Source: [Ben, line 168](../../../docs/ben_new_ontology.md#L168).

### g5.git.save-publication

- human-provided
- Source: [Ben, line 169](../../../docs/ben_new_ontology.md#L169).

### g5.git.push-policy

- human-provided
- Source: [Ben, line 170](../../../docs/ben_new_ontology.md#L170).

### g5.git.pull-policy

- human-provided
- Source: [Ben, line 171](../../../docs/ben_new_ontology.md#L171).

### g5.git.publication-authority

- human-provided
- Source: [Ben, line 172](../../../docs/ben_new_ontology.md#L172).

### g5.git.credentials

- human-provided
- Source: [Ben, line 173](../../../docs/ben_new_ontology.md#L173).

### g5.git.branch-creation

- human-provided
- Source: [Ben, line 174](../../../docs/ben_new_ontology.md#L174).

### g5.git.sync-state

- human-provided
- Source: [Ben, line 175](../../../docs/ben_new_ontology.md#L175).

### g5.git.divergence

- human-provided
- Source: [Ben, line 176](../../../docs/ben_new_ontology.md#L176).

### g5.git.base-import

- human-provided
- Source: [Ben, line 177](../../../docs/ben_new_ontology.md#L177).

### g5.git.base-create-exception

- human-provided
- [Git reconstruction](../../uplift/ben-ontology/investigations/git.md) traces the explicit creation permission in human request `fe523ff9` and the later Ben no-push rule.
- MakeRemote checked confirmed Base creation as implemented; the supplied proposal now makes that creation the explicit exception. RemotePolicy covers steady-state synchronization, excluding initial creation.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 41 (GT-cell-48).
- Amendment ([#1096](https://github.com/G5Labs/G5-Lean/issues/1096)): Make Remote is administrator-only on every workspace (owner-only off Base); on Base an administrator may run it as its owner does. [Design](../../pr_ontology_diff_design/issue-1096-make-remote-admin-only/ontology-plan.md).

### g5.git.pr-save

- human-provided
- Source: [Ben, line 178](../../../docs/ben_new_ontology.md#L178).
- Consolidates [PR update, line 183](../../../docs/ben_new_ontology.md#L183), including its push requirement; its stable ID references this owner.
- Scoped to the UI by the owner on 2026-10-27, when they accepted [g5.git.pr-cli.create-save](#g5.git.pr-cli.create-save) ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#owner-answers-2026-10-27)).

### g5.git.merge-recovery

- human-provided
- Source: [Ben, line 179](../../../docs/ben_new_ontology.md#L179).

### g5.git.ci-revision

- human-provided
- Source: [Ben, line 180](../../../docs/ben_new_ontology.md#L180).
- [ExternalChecks](../../uplift/ben-ontology/investigations/git.md) checked revision-pinned, passing admission and found that checks can change before the provider accepts the merge. Original policy requires admission-time observation, so the stronger effect-time failure is an assurance limit, not GIT-GAP-2.

### g5.git.check-suites

- agent
- Source: [issue #492](https://github.com/G5Labs/G5-Lean/issues/492): CodeRabbit, Claude and Cursor left `queued` suites with zero runs on all 40 sampled heads, so external checks never passed while GitHub's merge box ignored them. A five-minute empty-suite window was replaced by [named required checks](../../chat_intent/issue-492/required-checks.md) before merging.
- Implemented · `g5-scm/src/providers/checks.rs` reads suites only to map runs and to keep a queued suite's completed runs pending; they are never rows. API regressions cover queued reruns without a suite row and a silent App's empty queued suite leaving no row (`required_checks_come_from_the_base_branch_and_are_expected_until_reported`).

### g5.git.check-read-settling

- human-provided
- The owner asked why Ship to parent reports "PR revisions changed while reading checks" when nothing seemed to change, and to fix it as proposed (2026-10-30, [intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#check-reads-that-settle-2026-10-30)). The classification and the three-read bound are agent design; limiting repeats to reads that could settle reconciles it with [g5.git.check-observation](#g5.git.check-observation) (#1016) on the 2026-11-02 rebase.
- Implemented: `Moved` and `observe` in `g5-scm/src/providers/checks.rs`. Tests: `moved_tests::only_a_change_to_the_pr_itself_counts_as_its_head_moving`, `api_pull_request_reads::checks_include_test_merge_failures_and_wait_for_unknown_or_changed_revisions`, `checks_revalidate_same_head_reruns_and_preserve_distinct_apps`.

### g5.git.required-checks

- human-provided
- The user asked to name the checks G5 looks for instead of inferring them, chose reading GitHub's required checks for the base branch, allowed caching them per repository, and asked Prepare to Ship to split required from other checks ([intent](../../chat_intent/issue-492/required-checks.md)). Judging the revision GitHub judges (amended from head-only after review, per GitHub's "Troubleshooting required status checks"), name-only status matching, the ten-minute display cache with a fresh read at merge and treating an unreadable set as unavailable are agent design in the [plan](../../pr_ontology_diff_design/issue-492/ontology-plan.md). The endpoint shapes were checked against live public repositories on 2026-10-14; GitHub lists the branch read under Contents read and the rules read under Metadata read.
- Implemented · `g5-scm/src/providers/checks.rs` (`CheckReader::required`, `RequiredChecksCache`, `required`/`expected` rows, `baseRefName`, `requiredChecks`); `RequiredChecksCache` holds successful reads for ten minutes, keyed by branch URL, owned per `WorkspacePullRequestProviderLifecycleService`, and `GithubAppCapability::PullRequestChecksRead` mints Contents read for the base-branch protection read. API tests through the user route: `required_checks_come_from_the_base_branch_and_are_expected_until_reported` (protection plus rulesets, App-bound runs, case-insensitive status contexts, expected rows, cache reuse without re-reading the branch, advisory not-required pending, blocking not-required failure) and `required_checks_are_judged_on_githubs_evaluated_revision_and_unreadable_requirements_are_unavailable` (an expected row on a test merge that has checks despite a passing head, a requirement met on the test merge, the head judged when the test merge has none, unenforced protection ignored, a refused rules read making the repository unavailable, a `/` branch sent as one `%2F` segment). The merge-route test asserts each decision re-reads the branch requirements. CLI: `checks_watch_waits_for_expected_required_checks_and_marks_them`. Prepare to Ship: `ExternalPrChecksSection` (`Required` and `Not required` groups, expected rows, the "requires no checks" note), covered by `PrepareToShipModal.test.tsx`. Limits: live GitHub, a private repository's protection read and GitHub's handling of `%2F` branch names are unverified.

### g5.git.pr-identity

- human-provided
- Source: [Ben, line 181](../../../docs/ben_new_ontology.md#L181).

### g5.git.pr-providers

- human-provided
- Source: [Ben, line 182](../../../docs/ben_new_ontology.md#L182).

### g5.git.pr-repository

- agent; implementation authorized by the owner on 2026-10-30
- Source: the owner asked for an adapter design modeled on issues (2026-10-28) and then for "the middle option": a thin workspace-level record grouping per-repository PRs (2026-10-29) ([intent](../../chat_intent/pr-adapter/request.md)). The binding rule reuses the expectation test of [g5.git.pr-no-github](#g5.git.pr-no-github); "the binding follows the member" and "a local repository PR holds no state of its own" are agent design in the [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#repository-pull-requests), and depend on [g5.git.pr-local-conversation](#g5.git.pr-local-conversation).
- Implemented: `services/git/pull_request_adapters.rs` (binding from recorded heads, `changes_member`, `github_publication_target` shared with publication), attached to every summary in `coordinated_repo_publication.rs`. Tests: `api_pull_request_reads::checks_report_local_repository_pull_requests_beside_githubs`, `api_pull_request_listing::local_repository_pull_requests_are_read_checked_and_listed_by_g5_number`, `github_listing_shows_the_repositorys_local_pull_requests_beside_githubs` (a member both workspaces track remotely stops being local).
- Gap: binding changes after Make Remote rely on publication's existing first-PR creation during refresh or update; no new system test exercises Make Remote during an open PR.

### g5.git.pr-adapter

- agent; implementation authorized by the owner on 2026-10-30
- Source: as [g5.git.pr-repository](#g5.git.pr-repository); the owner/adapter split and the guard placement follow [g5.issue.native.adapter](../features/issues.g5.md#g5.issue.native.adapter). [Plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#the-adapter).
- Implemented: the PR service's reads dispatch by adapter (`g5_selection`, `ensure_github_number` and `LOCAL_CONVERSATION_REFUSAL` in `workspace_pull_request_read.rs`; `read_checks` iterates the repository pull requests in `workspace_pull_request_checks.rs`; the listing takes the binding's repository-level rule `is_github_repository` and its shared per-PR rule `PUBLISHES_TO_GITHUB_SQL` from `pull_request_adapters.rs`). Each GitHub repository pull request carries the GitHub adapter's record (`github`), and every first-party surface reads GitHub publication facts from it: `githubPublications` in the UI (`lib/types.ts`), `github_publications` in `g5d` (`pr_json.rs`). Activity's publication labels and remote tracking's import hold are PR-service functions (`github_publication_labels`, `github_import_hold`). The GitHub comment-receipt and thread-resolution protocol is in `workspace_pull_request_conversation.rs`, whose GitHub client is private. Tests: `pull_request_adapter_ownership::routes_never_name_the_github_repository_client`, `api_pull_request_listing::an_unpublished_github_member_binds_github_whether_open_or_ended`, the `g5d` and UI `githubPublications` unit tests.
- Compatibility: the PR summary still serializes the raw `providerRepos`, because a workspace VM's `g5d` can predate the API; first-party surfaces read it only from a server that sends no repository pull requests.
- Gaps: the GitHub adapter is the existing lifecycle code behind the PR service, not a separate module, and publication and merge still call `RepositoryClient` from `workspace_pull_request_provider_lifecycle.rs` and its merge module. Nothing enforces the boundary at compile time: `g5-scm`'s client and `AppState.repository_credentials` are public, and the ownership test scans only route files for three names. `g5d` still classifies selectors (G5 ID, number, branch) before calling the PR service.

### g5.git.pr-local-number

- human-provided (the `PR` name: the owner, 2026-10-30, in the PR review follow-up; six zero-padded hex digits, random numbers and no sequence: the owner, 2026-11-02, replacing the 2026-10-30 sequence with gaps, [intent](../../chat_intent/pr-adapter/request.md#random-hex-pr-names-2026-11-02)); agent (the numbering itself, accepted by interpretation of the owner's request to build the whole plan, [intent](../../chat_intent/pr-adapter/request.md#implementation-request-2026-10-30); the range 1–0xffffff, lowercase display, uniqueness per organization, the 32-try limit and the retry after a concurrent pick; selectors needing all six digits, so short `pr`-prefixed branch names such as `prefab` stay branches, while an eight-character branch of `pr` and six hex digits can only be selected some other way; refusing a bare number in a repository not on GitHub).
- Proposed in the [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#decided-questions). Keeping `number` for GitHub in `--json` is agent design, so an agent never passes a G5 number where `g5d` reads a bare number as GitHub's.
- Implemented: migration `145_pull_request_numbers.sql` (column, random backfill, `CHECK` 1–0xffffff, unique per organization, the `trg_wpr_take_number` trigger that picks an unused number); the savepoint retry around the creation insert (`coordinated_repo_publication.rs`); `g5_pull_request_name`, `parse_g5_pull_request_name` (`pull_request_adapters.rs`); `g5_selection` and `ensure_github_number` (`workspace_pull_request_read.rs`); `PR` hex selectors and rendering in `g5d` (`pr_write.rs`, `pr.rs`); the UI shows the server's name as given. Tests: `migration_145_pull_request_numbers::numbers_pull_requests_and_closes_those_of_deleted_workspaces` (including a replayed random draw that must skip a taken number), `api_pull_request_listing::local_repository_pull_requests_are_read_checked_and_listed_by_g5_number`, `api_pull_request_listing::github_listing_shows_the_repositorys_local_pull_requests_beside_githubs`, the `g5d` selector and rendering tests. Not covered by a test: the creation retry after a concurrent pick of the same number.

### g5.git.pr-local-conversation

- agent; accepted by interpretation of the owner's request to build the whole plan
- As [g5.git.pr-local-number](#g5.git.pr-local-number).
- Implemented: `LOCAL_CONVERSATION_REFUSAL` (`workspace_pull_request_read.rs`), used by the info read and `conversation_target` (`workspace_pull_request_conversation.rs`) for a G5 PR's ID or name and for a repository not on GitHub.

### g5.git.pr-description

- human-provided
- [Issue #816](https://github.com/G5Labs/G5-Lean/issues/816) section 4; [design](../../pr_ontology_diff_design/issue-816-workspace-landing/ontology-plan.md). The description precedence follows the serializer's override-over-Project-Doc order; commit-derived bodies are per repository and are not a G5 description.
- The single read for every surface is the owner's 2026-10-28 instruction ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#one-description-read-for-the-ui-and-g5d-pr-view-2026-10-28); [design](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#part-3-one-description-read-for-the-ui-and-g5d-pr-view-2026-10-28)); the plain-text comparison line is agent design. The owner then asked whether the server could assemble a listing's bodies instead of refusing them and accepted the batch read (same session).
- Implemented: `GET /api/internal/workspaces/:id/pull-requests/:pullRequestId/description` (`get_pull_request_description_internal`) calls `read_pull_request_description`, as the user route does; `g5d` attaches that read (`attach_description` in `g5d/src/cli/pr.rs`) to single stored-PR output when text or `body` is shown, and the landing shows the saved text only on a failed read, with a staleness note. `GET …/pull-requests/descriptions?ids=…` (user and internal) calls `read_pull_request_descriptions`, which shares `load_context_record`, `read_context_pr_body_doc` and `canonical_description` with the single read, at most 100 PRs, reading Project Docs after the transaction and only where no saved description wins. Tests: `api_pull_request_reads::pr_description_batch_resolves_like_the_single_read_without_provider_calls` (a committed Project Doc, saved, blank and absent cases equal to the single read, no GitHub request, VM route, refusals), `api_pull_request_reads::pr_description_compares_provider_bodies_with_the_serializer_without_writing` (VM route equals the user route), `g5d` `vm_pr_commands_use_internal_reads_and_produce_json_and_readable_output` and `create_and_update_use_the_ui_request_shape_and_project_the_same_result`, `WorkspaceLanding.test.tsx`. Gap: the single read's Project Doc case is covered through the batch test's comparison, not with a publication.

### g5.git.pr-update

- human-provided
- Source: [Ben, line 183](../../../docs/ben_new_ontology.md#L183).

### g5.git.pr-write-identity

- agent
- Source: [issue-387 request](../../chat_intent/issue-387-g5d-pr-gaps/request.md) and [amendment](../../pr_ontology_diff_design/issue-387-g5d-pr-gaps/ontology-plan.md), including first provider creation during update.
- [PullRequestIdentity](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/workspace/README.md) checked that Update creates no G5 PR and replaces no known terminal provider PR without explicit consent, while failed first publication can recover. It covers one repository; lost provider receipts, rediscovery, recorded-merge recovery and automatic refresh remain outside it.

### g5.git.pr-refresh

- human-provided
- Source: [Ben, line 184](../../../docs/ben_new_ontology.md#L184).

### g5.git.live-rebase

- human-provided
- Source: [Ben, line 185](../../../docs/ben_new_ontology.md#L185).

### g5.git.stopped-rebase

- human-provided
- Source: [Ben, line 186](../../../docs/ben_new_ontology.md#L186).

### g5.git.lifecycle-rebase

- human-provided
- Source: [Ben, line 187](../../../docs/ben_new_ontology.md#L187).

### g5.git.rebase-history

- human-provided
- Source: [Ben, line 188](../../../docs/ben_new_ontology.md#L188).

### g5.git.dirty-rebase

- human-provided
- Source: [Ben, line 189](../../../docs/ben_new_ontology.md#L189).

### g5.git.conflicts-rebase

- human-provided
- Source: [Ben, line 190](../../../docs/ben_new_ontology.md#L190).
- Children: [models](#g5.git.conflicts-rebase.models).

### g5.git.conflicts-rebase.models

- human-provided
- architecture-decision
- Expands: [conflicts-rebase](#g5.git.conflicts-rebase).
- Source: [chat intent](../../chat_intent/periodical-reverification/request.md), owner decision 13 (2026-11-06): text resolution, automatic checks and light fixing, so models parse and run, without bringing them up to date. The owner then chose a hand-off on Git conflicts only, and checking the affected models rather than the whole catalog (questions `clean-replay` and `affected-models`). Later the same day the owner asked that the rule be ignored when there are no models or one side changed none, and accepted its cost; a reconciled statement that links models no longer triggers it on its own. Design: [Base re-verification plan](../../pr_ontology_diff_design/periodical-reverification/ontology-plan.md#rebase-and-models).
- Implemented: the merge-and-rebase paragraph of `.agents/AGENTS.md` and the repository's `AGENTS.md`, and one clause in the resolution prompt in `git_conflict_resolution.rs` (template revision 5), which checks for changed `.tla` and `.cfg` files on each side. A clean replay hands off nothing (`rebase_source_in_vm` in `workspace_merge_service.rs`). The rule is agent behaviour and is not exercised by tests.

### g5.git.stopped-rebase.resolve-with-agent

- Expands: [parent](#g5.git.stopped-rebase)
- human-provided (that Resolve with Agent starts a VM when none is attached, in the same PR as Start and ship with agent); agent (joining a Start under way, the refusals that start no VM)
- Source: owner request in workspace chat (2026-11-06), [decisions](../../chat_intent/ship-with-agent-wo-vm/request.md); design: [plan](../../pr_ontology_diff_design/ship-with-agent-wo-vm/ontology-plan.md#follow-up-start-and-resolve-with-agent); mock and screenshots in the docs repository (`ship-with-agent-without-vm.md`, workspace 15159c69).
- Implemented: `start_and_resolve_with_agent` and `source_rebase_state` (`routes/workspaces.rs`, `workspace_vm_service.rs`), `ResolutionAdmission::PendingStart` with `lock_owner_pending_start` (`workspace_merge_service.rs`, `workspace_runtime.rs`), the withdrawn-brief exclusion in `active_workspace_resolution`; `DirectCleanMergeChoiceModal.tsx`. Tests: `api_workspace_pull_requests.rs` (bound brief, joined Start, withdrawal and a fresh attempt, refusals before a Start), `DirectCleanMergeChoiceModal.test.tsx`, `WorkspacePageBasePull.test.tsx`.

### g5.git.publication-head

- human-provided
- Source: [Ben, line 191](../../../docs/ben_new_ontology.md#L191).

### g5.git.multi-repo-results

- human-provided
- Source: [Ben, line 192](../../../docs/ben_new_ontology.md#L192).

### g5.git.rebase-completion

- human-provided
- Source: [Ben, line 193](../../../docs/ben_new_ontology.md#L193).

### g5.git.rewrite-approval

- human-provided
- Source: [Ben, line 194](../../../docs/ben_new_ontology.md#L194).

### g5.git.make-remote

- human-provided
- Source: [Ben, line 195](../../../docs/ben_new_ontology.md#L195).
- Amendment ([#1096](https://github.com/G5Labs/G5-Lean/issues/1096)): restricted to an organization administrator, and additionally the owner off Base; Publish stays the owner's separate operation. [Design](../../pr_ontology_diff_design/issue-1096-make-remote-admin-only/ontology-plan.md).

### g5.git.make-remote-create

- human-provided
- Source: [Ben, line 196](../../../docs/ben_new_ontology.md#L196).

### g5.git.make-remote-import

- human-provided
- Source: [Ben, line 197](../../../docs/ben_new_ontology.md#L197).

### g5.git.make-remote-pause

- human-provided
- Source: [Ben, line 198](../../../docs/ben_new_ontology.md#L198).

### g5.git.login-separation

- human-provided
- Source: [Ben, line 199](../../../docs/ben_new_ontology.md#L199).

### g5.git.repository-access

- human-provided
- Source: [Ben, line 200](../../../docs/ben_new_ontology.md#L200).

### g5.git.repository-operation

- human-provided
- Source: [Ben, line 201](../../../docs/ben_new_ontology.md#L201).

### g5.git.auto-publication

- human-provided
- Source: [Ben, line 202](../../../docs/ben_new_ontology.md#L202).

### g5.git.authorization-loss

- human-provided
- Source: [Ben, line 203](../../../docs/ben_new_ontology.md#L203).

### g5.git.pr-authorship

- human-provided
- Source: [Ben, line 204](../../../docs/ben_new_ontology.md#L204).

### g5.git.server-credentials

- human-provided
- Source: [Ben, line 205](../../../docs/ben_new_ontology.md#L205).

### g5.git.protections

- human-provided
- Source: [Ben, line 206](../../../docs/ben_new_ontology.md#L206).

### g5.git.workflow-files

- agent
- Source: [issue-612 request](../../chat_intent/issue-612-fix-workflow-edit-publication-github-app-tokens/request.md) and [design](../../pr_ontology_diff_design/issue-612-fix-workflow-edit-publication-github-app-tokens/ontology-plan.md).
- Trigger: QA create-PR attempts on October 17, 2026 failed with HTTP 500 after GitHub refused an installation token minted with only `contents: write` for a branch editing `.github/workflows/`.

### g5.git.workflow-files.no-escalation

- agent
- Source: [design](../../pr_ontology_diff_design/issue-612-fix-workflow-edit-publication-github-app-tokens/ontology-plan.md), "Rejected" alternatives; split from [g5.git.workflow-files](#g5.git.workflow-files).

### g5.git.workflow-files.refusal

- agent
- Source: [issue-612 request](../../chat_intent/issue-612-fix-workflow-edit-publication-github-app-tokens/request.md) and [design](../../pr_ontology_diff_design/issue-612-fix-workflow-edit-publication-github-app-tokens/ontology-plan.md); split from [g5.git.workflow-files](#g5.git.workflow-files).

### g5.git.receipt.limits

- human-provided
- Source: [Ben, line 207](../../../docs/ben_new_ontology.md#L207).

### g5.git.network.lease

- human-provided
- Source: [Ben, line 208](../../../docs/ben_new_ontology.md#L208).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 42 (GT-Q3).

### g5.architecture.git_owner

- human-provided
- Source: [Ben, line 209](../../../docs/ben_new_ontology.md#L209).

### g5.architecture.repository_credentials

- human-provided
- Source: [Ben, line 210](../../../docs/ben_new_ontology.md#L210).

### g5.architecture.governance_owner

- human-provided
- Source: [Ben, line 211](../../../docs/ben_new_ontology.md#L211).

### g5.architecture.git_execution

- human-provided
- Source: [Ben, line 212](../../../docs/ben_new_ontology.md#L212).

### g5.architecture.scm_provider_client

- human-provided
- Source: [Ben, line 213](../../../docs/ben_new_ontology.md#L213).

### g5.git.pr-cli

- human-provided
- Source: [Ben, line 214](../../../docs/ben_new_ontology.md#L214).

### g5.git.pr-reads

- human-provided
- The owner, 2026-10-28: "if the user has read access to the workspace, then 'g5d pr' should satisfy read-only requests about its prs. list, view, bodies, etc - all should work and provide all non-intentionally-secret information. No complicated permission model for what 'g5d pr' shows. I want my agent to be able to broadly audit PRs in the G5 system." ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#pr-reads-follow-workspace-read-access-2026-10-28)). The commands, the `--all`/`--workspace` listing and the single readable-PR lookup are agent design in the [plan](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#part-5-pr-reads-follow-workspace-read-access-2026-10-28); the owner said "yes, implement" on 2026-10-28.
- Implemented: `readable_pull_request_ids`, `ensure_readable_pull_request`, `PullRequestReader`, `audience_repositories` and the reader-widened `attached_repository` (`workspace_pull_request_read.rs`); `list_readable_pull_requests` (`workspace_pull_request_list.rs`); `list_workspace_pull_requests_by_ids` (`coordinated_repo_publication.rs`); the info route's G5-ID selector and `all`/`workspace` listing; description, batch and checks reads; GitHub listing bodies on request (`with_body`, `g5-scm` `pull_list.rs`); `g5d pr list --all/-W`, `view <G5 ID>` through the server, and `--json body` on every listing. Tests: `api_pull_request_listing::pr_reads_follow_workspace_read_access_from_any_workspace` (a plain member's VM reads another workspace's PR by ID, through both listings, descriptions, checks and a GitHub URL; personal and formal workspaces stay restricted; close from a non-side workspace is refused), `g5d` `reads_reach_every_readable_pr_and_listings_fill_bodies`. Gap: live GitHub listing bodies are verified only against the GraphQL fixture shape.
- Deleted workspaces: the owner, 2026-10-29, "PRs should remain visible to tools after the workspace is tombstoned (source, target or both)" ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#deleted-workspaces-2026-10-29)). Applying the audience rule to the retained registry identity and owner, with current active organization membership is agent interpretation. Implemented: the PR summary queries no longer require live workspaces and report the deleted sides; `pr_audience` (`workspace_pull_request_read.rs`) is the audience rule without liveness, used by every PR read and listing. Test: `api_pull_request_listing::deletion_closes_open_prs_and_keeps_them_readable_from_either_side`.
- UI parity: the owner, 2026-10-30, "UI and the tool should return the same things" ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#deleted-workspaces-2026-10-29)).

### g5.git.pr-read-repositories

- agent
- Source: [G5-Lean#830](https://github.com/G5Labs/G5-Lean/issues/830) findings: reads were limited by the attached repositories, not by the workspace owning the PR. That existing behavior is recorded here, extended to personal reference repositories by [#831](https://github.com/G5Labs/G5-Lean/issues/831); [design](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md).
- Implemented: `readable_repositories` and `attached_repository` with `RepositoryUse::{Read, Write}` in `g5-api-core/src/services/git/workspace_pull_request_read.rs`; view, checks and listing read, and conversation (`workspace_pull_request_conversation.rs`) writes. Tests: `api_pull_request_listing::personal_workspaces_read_base_prs_but_never_write_through_them`, plus the existing `api_pull_request_reads` suite for ordinary workspaces.

### g5.git.pr-list

- human-verified
- Source: [G5-Lean#832](https://github.com/G5Labs/G5-Lean/issues/832), under [g5.git.pr-cli](#g5.git.pr-cli). The issue proposed answering from stored G5 PRs first and asking GitHub only for PRs created outside G5. The agent [design](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#listing) proposed GitHub for membership with G5 identity joined from stored links, because only GitHub can tell which PRs were created outside G5 and the issue's acceptance asks for `gh pr list` behavior. The owner chose that on 2026-10-27 ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md)).
- Implemented: `RepositoryClient::list_pull_requests` (`g5-scm/src/providers/pull_list.rs`, GraphQL `repository.pullRequests` newest created first); `list_repository_pull_requests` (`g5-api-core/src/services/git/workspace_pull_request_list.rs`) behind `GET /api/v1/workspaces/:id/pull-requests/github` and its internal counterpart; `g5d pr list` repository mode (`g5d/src/cli/pr.rs`, `pr_json.rs`). Tests: `g5-scm` `pull_request_listing_follows_gh_and_keeps_only_the_selected_repository`, `api_pull_request_listing::repository_listing_follows_github_and_names_only_readable_g5_prs`, `g5d` `repository_listing_*`. Limit: live GitHub is unverified; fixtures model GitHub's GraphQL schema.
- Amended for [repository pull requests](#g5.git.pr-repository) by the PR adapter [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#surfaces); implemented as recorded under [g5.git.pr-adapter](#g5.git.pr-adapter).

### g5.git.pr-list.search

- agent
- Source: G5-Lean#832 "Bound and share any provider search budget, as `g5d issue search` does"; [design](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#listing).
- Implemented: GraphQL `search(type: ISSUE)` with `build_pull_request_search_query`; the internal route reserves through `reserve_agent_search` with `count_external_domains`, the same keys and window sizing as agent issue search. Test: `api_pull_request_listing::agent_pr_searches_spend_the_shared_search_budget_and_people_do_not`.

### g5.git.pr-list.local

- human-verified
- The owner chose "github + G5 links - if no github integration then fall back to local" on 2026-10-27 ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md)). Which failures count as "no integration", and the stored filters, are agent interpretation in the [plan](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#local-fallback).
- Implemented: `list_stored` and `NO_INTEGRATION` (`no_scm_connection`, `no_scm_token_manager`, `github_app_unavailable`) in `workspace_pull_request_list.rs`. Test: `api_pull_request_listing::listing_without_github_integration_answers_from_stored_prs`.
- Listing only PRs that change the repository, with `headRefOid`, is the owner's acceptance of the agent's suggestion on 2026-10-28 ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#pr-reads-follow-workspace-read-access-2026-10-28)); implemented in the `list_stored` query (recorded heads plus the preflight outcome) and covered by the same test's unchanged-repository cases.
- Amended for [repository pull requests](#g5.git.pr-repository) by the PR adapter [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#surfaces); implemented as recorded under [g5.git.pr-adapter](#g5.git.pr-adapter).

### g5.git.pr-no-github

- human-provided
- The owner, 2026-10-28: "We should not complain about missing github data if the workspace (or base if you're listing) doesn't have any remote repos (eg if we can prove that there's no expectation of github then don't complain it's missing)" ([intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#no-github-expected-2026-10-28)). The expectation test, the per-surface outcomes and `checks` succeeding are agent design in the [plan](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#part-4-no-complaints-where-github-is-not-expected-2026-10-28); the owner accepted `checks` exiting 0 and asked to keep `provider_failed` rather than add an event type.
- Implemented: `read_checks` returns `noExternalChecks` for a PR with no provider record and no GitHub candidate; repository listing returns `githubExpected`; `g5d` treats `noExternalChecks` as passing, prints the stored-listing notice only when `githubExpected` is not false, resolves a branch to a stored G5 PR (`BranchPullRequest::G5`), and gates the Auto-push-off and close lines on provider records; refresh failures and post-save incompleteness use neutral wording when `expects_github` is false; Activity labels a `provider_failed` entry with no repositories "PR update failed"; `merge_with_agent_prompt` takes `github`. Tests: `api_pull_request_reads::checks_for_a_pr_without_github_report_that_no_external_checks_apply`, `api_pull_request_listing` (`githubExpected`, neutral `--label` refusal), `workspace_pull_request_auto_merge::merge_with_agent_starts_a_loop_chat_in_the_source`, `g5d` `commands_do_not_complain_where_github_is_not_expected` and `auto_push_off_is_reported_only_for_a_pr_with_github_records`, the prompt and refresh-message unit tests, `WorkspaceActivityPanel.test.tsx`. Gaps: a GitHub PR whose first publication failed before any provider record also shows "PR update failed"; a branch resolved to a G5 PR keeps the GitHub field set for `--json`.
- Amended for [repository pull requests](#g5.git.pr-repository) by the PR adapter [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#surfaces); implemented as recorded under [g5.git.pr-adapter](#g5.git.pr-adapter).

### g5.git.pr-g5-identity

- agent
- Source: G5-Lean#832 acceptance ("with their G5 PR id and GitHub number") and [g5.git.pr-providers](#g5.git.pr-providers); [design](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#g5-identity).
- Implemented: `attach_g5_pull_requests` and the `readable` audience predicate in `workspace_pull_request_list.rs`, which mirrors `workspace_access_for_role` (`g5-api-core/src/db/scoped.rs`) as a set query; a change to the audience rule must update both. Used by listing and by live `view`.
- Amended for [repository pull requests](#g5.git.pr-repository) by the PR adapter [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#surfaces); implemented as recorded under [g5.git.pr-adapter](#g5.git.pr-adapter).

### g5.git.pr-cli.push-updates

- human-verified
- The owner's 2026-10-27 statement that pushing to the branch usually updates the PR, "just like github" ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#gh-alignment-request-2026-10-27)). It was verified the same day on PR #854: a force-with-lease push reached GitHub and the stored PR in about 60 s ([plan](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#what-a-push-does-today-verified)). Retiring the publish step from guidance is agent design.
- Existing behavior: remote integration (`branch_integration_service::should_push`, `push_mode` default `auto` since migration 040) and the refresh scanner (`workspace_pull_requests::run_loop`, `synchronize_after_push`). Implemented 2026-10-27: guidance (root and packaged `AGENTS.md`, `docs/g5d-pr.md`, the IE persona), `g5d pr --help`, the Ship with agent prompt and server messages name a push, and `update` only as the republish ([inventory](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#where-guidance-treats-update-as-required)).

### g5.git.pr-cli.edit

- agent
- Design: [plan](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#decisions), under [g5.git.pr-cli](#g5.git.pr-cli).
- Implemented: `mode: edit` on the create/update request (`workspace_pull_request_create::edit_text`, `coordinated_repo_publication::edit_open_pull_request_text`, `edit_provider_pull_requests`); `g5d pr edit`. Tests: `api_workspace_pull_requests::edit_changes_only_the_open_prs_text_without_saving`, `edit_applies_text_to_github_without_pushing_or_opening_prs`.

### g5.git.pr-cli.selectors

- agent
- Design: [plan](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#decisions).
- Implemented in `g5d` (`pr_write.rs`: `classify`, `branch_pull_request`, `g5_pull_request`) for `view`, `checks`, `merge`, `edit` and `close`; a branch resolves through the [repository listing](#g5.git.pr-list). Tests: `g5d` `selectors_name_g5_ids_github_prs_or_branches`. Gap: no end-to-end test of a branch or number selector against the API.
- Amended for [repository pull requests](#g5.git.pr-repository) by the PR adapter [plan](../../pr_ontology_diff_design/pr-adapter/ontology-plan.md#surfaces); implemented as recorded under [g5.git.pr-adapter](#g5.git.pr-adapter).

### g5.git.pr-cli.unsupported

- agent
- Design: [plan](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#decisions).
- Implemented: `gh_habit_hint`, `pr_verb_hint` and `pr_flag_hint` (`g5d/src/cli/mod.rs`); `merge --rebase` is accepted. Test: `g5d` `gh_pr_verbs_and_flags_g5_lacks_fail_with_guidance`.

### g5.git.pr-cli.update

- human-verified
- The owner accepted recommendation (a) on 2026-10-27 ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#owner-answers-2026-10-27)); options in the [plan](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#open-questions-for-the-owner).
- Implemented: `update` is `#[command(hide = true)]`; without text it runs `publish_committed_work` and sends `save: false`, and with text it sends `mode: edit` (`g5d/src/cli/pr.rs`).

### g5.git.pr-cli.create-save

- human-verified
- The owner accepted the recommendation on 2026-10-27 ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#owner-answers-2026-10-27)), narrowing [g5.git.pr-save](#g5.git.pr-save) to the UI. Counting untracked, non-ignored files as uncommitted, because Save would have committed them, is agent interpretation.
- Implemented: `save` on the create/update request (default `true` for the UI; `g5d` sends `false`), and `g5d` `publish_committed_work` on the shared native runner (`NativeGit::uncommitted_changes`, `checkout_branch`, `push_branch` fast-forward). Tests: `api_workspace_pull_requests::g5d_create_publishes_committed_work_without_saving` (no VM RPC), `g5d` `committed_work_is_pushed_and_uncommitted_work_refuses`. Outside a workspace VM, `g5d` cannot see a checkout and publishes the pushed head.

### g5.git.pr-cli.manual-forwarding

- human-verified
- The owner accepted the recommendation on 2026-10-27 ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#owner-answers-2026-10-27)).
- Implemented: refresh records `NOT_FORWARDED` once when the provider branch is behind (`provider_branch_behind`, `refresh_after_push_tracked`), the stored PR carries `sourceAutoPush`, and `g5d pr view` and the `checks --head` pending message name the republish. Test: `api_workspace_pull_requests::auto_push_off_records_once_that_a_push_was_not_forwarded`. Gap: a workspace with Auto-push on but no App write access still reports only the generic settling failure, after five minutes.

### g5.settings.github-connection

- human-provided
- Source: [Ben, line 215](../../../docs/ben_new_ontology.md#L215).
- Organization-aware offer: [#955 request](../../chat_intent/issue-955/request.md) and [design](../../pr_ontology_diff_design/issue-955/ontology-plan.md). The owner settled "uses GitHub" as a GitHub remote–tracking repository or a GitHub issue domain; the person's own connection read answers the offer for the organization they name, under that organization's membership-verified tenant scope, with no new cross-org read authority.

### g5.git.saved-read

- agent
- Source and checks: [Git reconstruction](../../uplift/ben-ontology/investigations/git.md).
- SavedRead distinguished complete branch views from complete recovery snapshots and exposed stopped/failed browsing that substitutes saved dirty content for the governing branch view. The branch comparator passes; [GIT-GAP-5](../../../docs/code_gaps.md) remains, and receipt validity is an upstream premise.

### g5.git.flush-proof

- agent
- Source and checks: [Git reconstruction](../../uplift/ben-ontology/investigations/git.md).
- FlushProof checked complete-member/ref evidence, current-incarnation completion and restoration of saved local heads and dirty bytes despite newer publication. Native capture/restore fidelity and receipt-pruning races remain outside the two-repository model.

### g5.git.pr-recovery-cut

- agent
- Source: [Git reconstruction](../../uplift/ben-ontology/investigations/git.md) and [issue-551 target checks](https://github.com/G5Labs/G5-Lean/blob/eedcc8aeddfbda8a25cd4a95922575328ac3a631/.g5/ontology/features/governance.g5.md#respect-live-target-ownership-during-merge).
- Merge-time target: [owner request](../../chat_intent/issue-582-merge-to-base-detect-unrebaseable-pr-branches/request.md) in #582; [design](../../pr_ontology_diff_design/issue-582-merge-to-base-detect-unrebaseable-pr-branches/ontology-plan.md). Admission pins source heads only, so a target move needs no new admission.
- PRRecovery checked frozen admission across provider acceptance, lost replies, later withdrawal and durable refresh, with recovery under stable availability and fair scheduling. Unmanaged provider changes remain outside the PR lock: live target preflight cannot prevent a later target advance, and the historical check covers pinned-target ancestry, not the new import-time target ancestry requirement; upstream tests are not local API/database execution.
- Override retention: agent ([issue #873 design](../../pr_ontology_diff_design/issue-873/ontology-plan.md#record-and-recovery)); implemented through the recorded admission; the partial-acceptance retry is not separately exercised.
- [Issue #697 implementation and evidence](../../pr_ontology_diff_design/issue-697/ontology-plan.md) durably selects the checked target before provider effects and recovers it after rollback or lost acknowledgement. Native tests compare retained targets with independent Git effects, including a real partial batch and later approval withdrawal. The earlier model summary is not proof of this implementation; browser qualification remains recorded separately in the plan.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 43 (GT-Q4, GT-Q1).

### g5.git.provider-merge-target-serialization

- agent
- Source: [issue #697](https://github.com/G5Labs/G5-Lean/issues/697).
- [Design, alternatives, risks and execution evidence](../../pr_ontology_diff_design/issue-697/ontology-plan.md). The owner approved implementation on 2026-10-28. Migration 135 and the provider lifecycle retain attempt/execution identities, target selection and pending/uncertain/accepted state on existing provider records, with database-enforced target exclusion. Migration 136 gives only the repository trigger the restricted lookup owner's read authority, preserving bootstrap callers' table permissions. Native fault and recovery tests exercise the production HTTP/database/Git boundaries with provider doubles; browser qualification is tracked in the plan. No new model evidence is claimed.

### g5.git.remote-binding

- agent
- Source and checks: [Git reconstruction](../../uplift/ben-ontology/investigations/git.md).
- MakeRemote checked create-without-overwrite, provider-head confirmation, captured local/identity checks and paused divergent binding. It abstracts one member and provider observations; the [confirmed Base creation exception](#g5.git.base-create-exception) now supplies the governing policy.
- Amendment ([#1096](https://github.com/G5Labs/G5-Lean/issues/1096)): records that Make Remote and Publish are separate operations and that the administrator rule is enforced before any provider I/O. [Design](../../pr_ontology_diff_design/issue-1096-make-remote-admin-only/ontology-plan.md).

### g5.git.external-check-policy

- agent
- Source: [Git reconstruction](../../uplift/ben-ontology/investigations/git.md); original policy and accepted design at `32b2d174` require admission-time observation.
- ExternalChecks checked selected-source identity and a passing server observation at admission. Checks can change before provider acceptance; its stronger effect-time comparator assumes enforcement the application does not supply.
- On 2026-10-14 the user chose GitHub's required checks as the named checks the gate waits for ([intent](../../chat_intent/issue-492/required-checks.md)); that only required checks make the gate pending, and that a repository requiring none passes unless a check fails, are the agent recommendations the user didn't contest. Implemented · `external_checks_verdict` and the frontend `aggregateVerdict` wait only on required rows and fail on any failure, with unit tests. `api_workspace_pull_requests.rs::provider_ship_waits_for_required_external_checks_under_org_policy` drives the merge route with the policy on: refused before any provider call while a required check is expected (with a not-required run in progress and a silent App's empty suite), then merged once it passes.
- Override waiver: human-verified from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873) ([design](../../pr_ontology_diff_design/issue-873/ontology-plan.md)); implemented (`merge_override::blocking_checks`).
- Skipping repositories merged outside G5: human-verified from [#550](https://github.com/G5Labs/G5-Lean/issues/550) proposal 5; implemented in `routes/workspaces.rs::policy_checks_snapshot`; test `api_workspace_pull_requests.rs::a_repository_merged_on_github_is_recorded_and_outside_the_checks_policy`.

### g5.git.provider-rules

- human-verified ([#550](https://github.com/G5Labs/G5-Lean/issues/550) proposal 2, included at the owner's request on 2026-10-27); enforced for every merge is human-provided (the owner's answer in the workspace question set `admin-merge-override` (docs repository, `docs/95474a2a-3498-4db1-9b56-845d3ecabfe9/questions/`), 2026-10-27, over report-first or override-only); agent (re-reading an uncomputed state, refusing the whole merge).
- Implemented · `preflight_one_provider_pr_for_merge` reads `mergeable_state` (re-read while `unknown`) before any provider merge; ordinary merges in App-bound organizations still call GitHub as the App installation, never past `blocked`. Test: `api_workspace_pull_requests.rs::a_github_blocked_repository_refuses_the_merge_until_a_github_override`. Design: [plan](../../pr_ontology_diff_design/issue-873/ontology-plan.md#github-rules). Rollout needs the repositories G5 merges checked for merges that pass today only through the App's bypass.

### g5.git.provider-override

- human-verified ([#550](https://github.com/G5Labs/G5-Lean/issues/550) proposals 3 and 4 and its decision that the override sits beside Merge; [#873](https://github.com/G5Labs/G5-Lean/issues/873) "share one dialog and one record, each with its own authority").
- Assumption, unverified against live GitHub: GitHub honours a person's ruleset or branch-protection bypass when the merge uses their GitHub App user-to-server token (the identity `user_authored` operations use). If not, the design returns to the owner. Implemented · `RepositoryOperation::MergePullRequestWithBypass` (user-authored), bypass plans first; the App-bound person-token path is not exercised by a test.

### g5.git.provider-override.cli-question

- human-provided: the owner's answer in the workspace question set `admin-merge-override` (docs repository, `docs/95474a2a-3498-4db1-9b56-845d3ecabfe9/questions/`) (2026-10-27). The ID keeps its former question name.

### g5.git.provider-merged-outside

- human-verified ([#550](https://github.com/G5Labs/G5-Lean/issues/550) proposals 5 and 6: accept overrides done on GitHub; GitHub overrides never skip G5 approvals); agent (the record fields, the #696 boundary).
- Implemented · `mark_merged_outside` (reconcile and preflight), `merged_outside_g5` (migration 133), `policy_checks_snapshot` skips merged repositories ([#696](https://github.com/G5Labs/G5-Lean/issues/696) shares the skip).
- [Issue #697 integration](../../pr_ontology_diff_design/issue-697/ontology-plan.md#integration-with-updated-main) preserves this attribution while retaining the independently owned checked-target evidence through an unconfirmed invocation.

### g5.git.pr-auto-merge

- human-provided (the per-PR knob, gh's `--auto`/`--disable-auto` syntax, an agent never turning it on); agent (who may turn it off, pushes keeping it on, G5 performing the merge, the automatic turn-off)
- Source: [merge-when-ready request](../../chat_intent/merge-when-ready/request.md); design: [merge-when-ready plan](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md). `--match-head-commit` is deferred by the human.
- Implemented: `PullRequestMerger::WhenReady` (`routes/workspaces.rs`), `services/git/workspace_pull_request_auto_merge.rs`, the agent's turn-off route and `g5d pr merge`; evidence in [the plan](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md#evidence).
- Never an override: human-verified from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873) and [#550](https://github.com/G5Labs/G5-Lean/issues/550); `WhenReady` has no override path by construction once implemented.
- Amended 2026-11-05 ([#1104](https://github.com/G5Labs/G5-Lean/issues/1104), [source-owner merge](../features/workspaces.g5.md#g5.governance.pr.source-owner-merge)): turning it on no longer checks target merge authority; the background merger (`PullRequestMerger::WhenReady::target_db` → `source_owner_may_merge`) rechecks it each pass and the existing `Outcome::Revoked` turn-off covers a lapsed authority.

### g5.git.pr-auto-merge.readiness

- agent
- Expands: [g5.git.pr-auto-merge](#g5.git.pr-auto-merge) (formal)
- Source: [merge-when-ready plan](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md#readiness). Requiring checks regardless of organization policy interprets "once the merge is green"; the settle window covers the [verdict](#g5.git.external-check-policy) counting a repository with no reported checks as passing. Inherits that policy's limit: checks can change between the observation and provider acceptance.
- Checks-read pacing (issue #1016): agent; design in the [issue-1016 plan](../../pr_ontology_diff_design/issue-1016/ontology-plan.md#owners-and-changes). Implemented · `auto_merge.rs::ChecksPacing`. Tests: `auto_merge.rs::idle_checks_back_off_until_records_change`, `workspace_pull_request_auto_merge.rs::idle_checks_back_off_pending_checks_are_read_and_merged_prs_skip_the_gate`, `github_request_budget.rs::{merge_when_ready_passes_stay_within_budget, merge_when_ready_stops_reading_a_merged_provider_pr}`. Not honoring a rate limit's `retryAt` is a known gap: the backoff caps at five minutes.
- Implemented in `g5-api-routes/src/auto_merge.rs`; the checks gate and settle window are tested against a GitHub fake. Issue #697's `provider_merge_targets::merge_when_ready_uses_target_reservation_and_imports_provider_history` additionally lands and imports a real Git-backed provider-first merge through the ordinary when-ready pass ([execution evidence](../../pr_ontology_diff_design/issue-697/ontology-plan.md)); the older [evidence](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md#evidence) remains historical.

### g5.git.pr-auto-merge.with-agent

- human-provided (a Merge with agent button, renamed Ship with agent on 2026-10-26, that turns on merge when ready and launches a looping CI/rebase chat); agent (keeping the no-agent choice, the authority conjunction, the loop's end conditions)
- Source: [follow-up request](../../chat_intent/merge-when-ready/request.md#follow-up-knowing-it-merged-and-merge-with-agent); design: [plan](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md#merge-with-agent).
- Implemented: `start_merge_agent` (`routes/workspaces.rs`), `auto_merge::merge_with_agent_prompt`, `MergeWhenReady.tsx`; with an agent, a decision already on is kept. The prompt is the owner's 2026-10-25 replacement ([request](../../chat_intent/merge-when-ready/request.md#follow-up-the-merge-with-agent-prompt)). Evidence in [the plan](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md#evidence).
- Amended 2026-11-05: Start and ship with agent on a stopped VM, the owner's decision after first choosing a seeded composer ([decisions](../../chat_intent/ship-with-agent-wo-vm/request.md), [plan](../../pr_ontology_diff_design/ship-with-agent-wo-vm/ontology-plan.md)); `begin_merge_agent_start` and the `bind` in `start_merge_agent` (`routes/workspaces.rs`), covered by `workspace_pull_request_auto_merge.rs`.
- Amended 2026-10-27: the owner accepted [#830 plan Q4](../../pr_ontology_diff_design/issue-830-g5d-pr-cross-workspace/ontology-plan.md#open-questions-for-the-owner) ([chat intent](../../chat_intent/issue-830-g5d-pr-cross-workspace/decisions.md#owner-answers-2026-10-27)). The prompt's step 3 now publishes fixes with `git push`, as rebases already were ([push updates](#g5.git.pr-cli.push-updates)).
- Amended 2026-11-05 ([#1104](https://github.com/G5Labs/G5-Lean/issues/1104)): the click-time authority conjunction drops to source ownership only; target merge authority moves to merge time, as for [merge when ready](#g5.git.pr-auto-merge).
- Amended 2026-11-07: the owner first requested a prompt instruction to stop the workspace VM one minute after merge, including the exact `g5d` command, then made it conditional on a shared option ([request](../../chat_intent/stop-vm-after-auto-merge/request.md)). The owner superseded the delay and guarded command with immediate API-side Stop on 2026-11-09; [implementation and validation](../../pr_ontology_diff_design/stop-vm-after-auto-merge/ontology-plan.md).

### g5.git.pr-auto-merge.stop-vm

- human-provided
- Expands: [Merge when ready](#g5.git.pr-auto-merge).
- Source: [owner's immediate-stop revision](../../chat_intent/stop-vm-after-auto-merge/request.md#immediate-stop-revision-2026-11-09) and [Ship to parent extension](../../chat_intent/stop-vm-after-auto-merge/request.md#ship-to-parent-revision-2026-11-09). Optional stop, all three shipping paths and immediate API follow-up are the owner's request; default off, per-PR scope, owner-only choice and transfer reset are agent assumptions.
- The [approved personal default](../../chat_intent/stop-vm-after-auto-merge/request.md#personal-default-2026-11-10) adds one user preference and reuses the PR choice for inheritance and the completed merge decision; compatibility and transfer rules are agent decisions.
- Ordinary Stop replaces the delayed PR handoff; [design, failure boundary and evidence](../../pr_ontology_diff_design/stop-vm-after-auto-merge/ontology-plan.md).

### g5.git.pr-merge-announcement

- human-verified (the agent's proposal, including not waking an idle agent, accepted with "go ahead")
- Source: [follow-up request](../../chat_intent/merge-when-ready/request.md#follow-up-knowing-it-merged-and-merge-with-agent); design: [plan](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md#announcements).
- Implemented: `auto_merge::announce` (`g5-api-routes`), called after every landed merge and every turn-off except the agent's. Evidence in [the plan](../../pr_ontology_diff_design/merge-when-ready/ontology-plan.md#evidence).
- Override announcement and its recipients: human-verified from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873); implemented in `auto_merge::announce`; tested by `workspace_merge_override.rs::an_administrator_merges_with_override_and_it_stays_flagged`.

### g5.git.pr-conversation

- agent
- Source and checks: [Git reconstruction](../../uplift/ben-ontology/investigations/git.md), merged issue-387 conversation contract.
- CommentRecovery checked frozen destinations, deduplicated receipt replay and recovery after an ambiguous send without reposting. Provider marker integrity and post-admission authority changes are premises; thread validation and idempotent resolution were inspected, not modeled.

### g5.governance.remote.sync-history

- agent
- [Issue #565](https://github.com/G5Labs/G5-Lean/issues/565): QA held 425K rows, 96% background no-ops, with a deleted remote branch failing every pass; the issue proposed folding no-ops, retention and a terminal state for a missing branch.
- Implemented · `remote_integrations/worker.rs::extend_repeated_background_run` and `remote_sync_run_retention.rs`. Tests: `remote_tracking_background_loop.rs::{repeated_background_observations_extend_one_run, persistent_background_fetch_failure_extends_one_run_until_recovery, retention_prunes_superseded_noops_and_expired_runs, retention_pass_prunes_only_its_own_organization}`; the last runs one organization's pass under row-level security and checks another organization's identical history is untouched. A missing remote branch still fails, and is fetched, every pass; only its rows collapse. The issue's terminal or backoff state for it is not adopted. The sync-runs popover still times a folded run by its first observation (`startedAt`), not its last-seen time.

### g5.security.tenant.scm-import

- agent
- Source: [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/concepts/security.g5.md).
- PR #549 exercises account/session and revoked-authority cases. [Current authority](../concepts/security.g5.md#g5.security.current-authority) governs transactional admission; this is not an audit of every alternate writer.

### g5.governance.pr.follow-push

- agent
- Source: [upstream contract and evidence](https://github.com/G5Labs/G5-Lean/blob/3d17915df39bcf4cbe0cabc08500de63ff46fbb2/.g5/ontology/features/governance.g5.md).
- PR #519 distinguishes settling from final failure; [PR identity](#g5.git.pr-write-identity) still forbids silently replacing a closed provider PR.

### g5.git.provider-read-budget

- human-provided (issue #1016 and the owner's request to audit GitHub API use and reduce it); agent (the rule's statement) · architecture-decision · test-coverage
- Source: [request](../../chat_intent/issue-1016/request.md); design: [plan](../../pr_ontology_diff_design/issue-1016/ontology-plan.md). The audit's QA calibration is in the plan.
- Tests: `github_request_budget.rs` requires the exact GitHub requests of each representative caller (checks reads, merge-when-ready passes, merged provider PRs, idle loops without a provider PR); `g5d` `pr.rs::watch_read_budget_over_a_full_timeout` pins when a watch reads.
- Webhook-driven provider state and a shared per-head check snapshot are deferred follow-ups ([#1023](https://github.com/G5Labs/G5-Lean/issues/1023), [#1024](https://github.com/G5Labs/G5-Lean/issues/1024)), not part of this rule's implementation.

### g5.git.pr-refresh.completed-provider

- agent · architecture-decision · test-coverage
- Expands: [g5.governance.pr.follow-push](#g5.governance.pr.follow-push), [g5.git.provider-read-budget](#g5.git.provider-read-budget)
- Source: [plan](../../pr_ontology_diff_design/issue-1016/ontology-plan.md#owners-and-changes). A dedicated observation, not `provider_status`, because merged status belongs to merge recovery.
- Implemented · `provider_completed_pr_number` (migration 143), `ensure_automatic_refresh_allowed` and `automatic_refresh_paused`. Test: `api_workspace_pull_requests.rs::automatic_pr_refresh_stops_reading_a_completed_provider_pr`.

### g5.git.check-observation

- agent · architecture-decision · test-coverage
- Expands: [g5.git.ci-revision](#g5.git.ci-revision), [g5.git.provider-read-budget](#g5.git.provider-read-budget)
- Source: [plan](../../pr_ontology_diff_design/issue-1016/ontology-plan.md#owners-and-changes). Replaces an unconditional second observation; the owner chose single reads for polling. Settling follows the merge gate's verdict, so a result admitting a merge is always confirmed (review of PR #1029).
- Implemented · `g5-scm/src/providers/checks.rs::observe`. Tests: `api_pull_request_reads.rs::pending_check_reads_observe_once_and_settled_reads_confirm`, `github_request_budget.rs::an_open_pull_requests_checks_read_stays_within_budget`.

### g5.git.check-waits

- agent · user-facing-behavior · test-coverage
- Expands: [g5.git.provider-read-budget](#g5.git.provider-read-budget)
- Source: [plan](../../pr_ontology_diff_design/issue-1016/ontology-plan.md#owners-and-changes).
- Implemented · `g5d` `pr.rs::watch_checks` and `externalPrChecks.ts::checksRefetchInterval`. Tests: `pr.rs` watch unit tests, `pr_commands.rs::{checks_watch_waits_out_rate_limits_until_the_reported_reset, checks_watch_stops_once_every_github_pr_is_completed}`, `externalPrChecks.test.ts`.
