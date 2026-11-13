# G5 ontology statements

## Top level G5 goals

- Enable agentic development of complex enterprise software by a range of personas, including less technical product owners as well as knowledgeable developers  
- Humans guide development by providing and evaluating high-level intent; the platform stores this intent and history to guide agents to produce software aligned with the intent  
- Make it easy for enterprise users of various roles to align on the intent of the software they are building.  
- Software produced should be well architected, aligned with the intent provided, and maintainable.  
- Users only need a thin client to use the platform \- all development resources are hosted remotely  
- Integrate with typical infrastructure that enterprises use as part of the software development lifecycle.  
- Support a variety of models and be resilient to future model improvements

## Core concepts

- Organization: an entity consisting of a group of humans who collaborate to build enterprise software.  An organization is a strong tenancy isolation boundary: data and metadata should never cross between organizations.  An organization has a set of policies for how they operate and interact with G5.  
- Repo: storage for source code and intent artifacts for an organization.  A repo is internally modeled as a git repository.  
- Workspace: isolated space for proposing, aligning, implementing, and approving changes to the software.  Workspaces own a specific repository branch, have a configurable and clearly defined lifecycle, enable collaboration where allowed, and make experimental development "safe" with strong resource isolation and approval mechanisms.  
- Workspace VM: at most one instance of computing resources attached to a workspace that owns modifications to the workspace repos.  Provides an api for workspace tasks that require significant resources.  VM failures can lose work that isn't pushed to the Repo.  
- Host: organization-scoped provisioned resources that host workspace VMs while ensuring vms remain isolated from each other.  
- Ontology: minimal collection of english statements that capture the intent and shape of the system to be built.  Code must align with the ontology and this is enforced with agent mechanisms.  
- Issue: tracking unit of work (typically a bug/feature/task/etc).  Commonly stores planned future work and integrates into the enterprise work tracking flow.

## Organization

- Organizations have a list of users who are authorized to use organization resources on G5, depending on their role (admin, member, viewer)  
- Organization viewers can observe organization resources that they have access to, but cannot be granted write permissions on anything  
- Organization members can own workspaces and have read and write permission on organization resources.  
- Organization admins can do everything a member can and can also invite new users, adjust roles of other users in the organization, remove other users, change organization level settings, and grant/revoke permissions on organization-owner entities to themselves or others.  
- Organization invites turn into a membership upon the user logging into the system or they expire after a reasonable length of time  
- Organizational policies include: scaling limits on the number of hosts  
- Organization is a strong tenancy boundary: any resources owned by an organization are not accessible by any other organization.  Examples include by are not strictly limited to: workspaces, hosts, workspace vms, repos.  Any place where organization specific information is stored must include tenancy boundary protection (eg row-level access control for the database, separate pools of hosts, etc)  
- Organization creation commits its owner membership, dedicated docs repository and Base workspace metadata together so setup cannot leave a partly created team. \[D · g5.team.onboarding\]

## Users

- A user is identified by an email address, has basic profile information, and notification settings  
- A User is part of one or more organizations  
- Users authenticate to the platform through an external oauth service such as github  
- User has <!-- gdoc_comment_begin: c1 -->user-level<!-- gdoc_comment_end: c1 --> org-specific configuration settings for model selection, keys, user interface preferences

## Repo

- A Repo belongs to an organization (which may have several Repos), has a name, and maintains a full git repository of the content  
- An organization can add and remove repos  
- Repos are resilient to failures in the G5 infrastructure and should be backed up  
- Repos are served by the platform through the standard git interface  
- Repo operations enforce read/write permissions: only authorized organization users may read, only authorized workspaces / central API may write.  
- A Repo may optionally be connected to an external repository  
- Each organization has a "docs" repo for storing alignment docs.  Every workspace includes the docs repo.

## Workspace

- A workspace has an organization, a unique name within the organization, an owner who is a member, and creation / update times.  
- A workspace has a unique branch name which it owns across all the organization repos.  This branch name is derived from the workspace name, including a uniqueifier.  
- Workspaces, other than the root "Base" workspace, all have a parent workspace from which they were forked.  
- A member with read access to a workspace can create a new child workspace through forking, becoming the owner of the child.  Forking does not directly impact the state of the parent workspace.  
- When a workspace is forked from a parent, it forks the branch state of the parent workspace branches. It<!-- gdoc_comment_begin: c2 --> \[optionally\] inherits any dirty file state<!-- gdoc_comment_end: c2 --> from the parent at the instant it was forked.  
- A child workspace has read access to its parents up the chain to the Base workspace.  
- The "Base" workspace is read-only, owned by an organization admin, and cannot attach a workspace VM.  Its branch state can only be modified by approved merge operations or external repository imports.  
- The current workspace owner is the only user that can modify the workspace branch state or send api calls to a workspace VM that modify its state.  The only exception is the Base workspace, which can also be modified by organization admins.  
- The workspace owner can transfer ownership to another member. This process terminates any currently running workspace VM before the transfer is complete to avoid leaking any owner-specific credentials in the workspace VM.  
- A workspace supports a variety of modification mechanisms such as chat, preview, and shell access through api calls to an attached workspace vm.  
- The Repo branch metadata is the authority on workspace repo state.  This state is not duplicated elsewhere in platform metadata.  
- A workspace may be associated with a primary issue, which it uniquely owns in the platform.  A workspace may have any number of related issues.  The state of the primary issue reflects the state of the workspace and the platform helps maintain this linkage  
- A workspace has three stages: business engineering, system engineering, and integration engineering.  Each stage has a set of associated operations.  Progression between stages may be gated by securing the necessary approvals.  
- A workspace can be rebased on its parent to pick up changes.  This rebase operation acts like a regular fast forward rebase between the workspace branch and the workspace parent's branch for all repos, using an agent to resolve any conflicts that arise.  
- A workspace can be merged into its parent.  This operation may be gated by configured required approvals and automated proof of quality (eg passing tests, verified behavior).  For repos with external integrations, this can involve external pull request creation.  
- A workspace can be a "review" workspace which is an ephemeral workspace used for users to review its parent.  Review workspaces work similarly to regular workspaces, except file changes are observed by the parent and shown to users as "suggestions" when viewing the parent files.  Review workspaces can be merged into the parent, but it isn't required.  
- A non-Base workspace can be deleted by its owner.  Deleting a workspace cleans up repo branches and some of the database footprint, but may retain information for longitudinal analysis.  Deleting a parent cascades to deleting its children (no orphans).  The existance of non-review child workspaces owned by other users requires additional confirmation prior to delete.  Actual deletion may be delayed to support an "un-delete" operation.  Deleting a workspace stops its workspace vm, if any.  
- Each workspace has a Project Docs folder where the workspace stores artifacts for aligning on intent.  
- The Project Docs folder for a workspace is stored in the docs repo under a path determined by the workspace id  
- Agents working on a workspace vm should be guided to put their documents into the workspace's project docs folder and discouraged from modifying documents in other workspace's project docs folders  
- Non-owners querying workspace file state see the most recently committed git branch state  
- Owners querying workspace file state use the workspace VM file access to see current dirty state if the VM is up, otherwise they also see the most recent git state.

## Workspace VM

- The externally important Workspace VM state is its git repositories.  The VM is expected to commit and push to the central repository state that it wants to persist.  
- vm setup includes checking out all the workspace repository branches under appropriate paths  
- the workspace VM supports a "flush" mechanism that captures dirty checkout state, persisting it to git in a way that doesn't affect the workspace branch directly and allows a subsequent VM restart to restore that dirty state  
- if the VM checkout paths change to no longer point at the correct branches, the UI interactions and flush mechanism should produce appropriate errors.  THe VM offers an api call to restore the appropriate branch state.  More complicated misalignment is the responsibility of the user / agent to correct.  
- there is at most one workspace vm for a workspace at any point in time.  A failed vm cannot block launching a new workspace vm.  
- The central dispatch process in the VM exposes an API for filesystem access, module invocation and inspection, runtime information and diagnostics.  
- the dispatcher forks new processes to service API requests  
- The filesystem API supports list, read, state, write, delete, rename and watch options.  These operations only work on files in a git checkout and do not work on gitignored files.  
- Modules are an abstraction for operations on the VM.  The VM explicitly does not support an "execute this command" operation, forcing implementors to build a module (or align with an existing one).  
- Modules supported include workspace.bootstrap (start workspace), agent.turn (run an agent turn), workspace.preview (run preview, long-running), workspace.changes (reports repository state and performs structured git operations), and workspace.diagnostics (capture diagnostic bundles and runs fixed read-only probes).  
- Each VM exposes a versioned catalog of supported modules. Each module declares its typed operations, outcomes, resource limits and requested capabilities in the immutable catalog.  
- Only the current active owner may mutate or use VM files, logs, execution, terminal or preview; administrator status, creation, invitation and past ownership confer no bypass. Transfer is the explicit authority change. \[D · g5.security.owner\]  
- The platform does not support an ownership transfer of a running workspace VM  
- A command or result for one VM instance must never be applied to its replacement. Late results must not overwrite the replacement's current editor, proposal or operation state; they may still be kept as history. \[D · g5.data.identity.incarnation\]  
- Git-provider credentials remain server-side rather than entering the guest environment. \[D · g5.security.credentials.publication\]  
- User settings and secrets must not override reserved platform identity, runtime or provider-home environment keys. \[D · g5.security.credentials.reserved-env\]  
- Bind live caches and streams to the owner and VM incarnation, invalidating them before rendering read-only content after stop, transfer or revocation. Read and Save completions follow the [VM instance invariant](http://.g5/ontology/concepts/system_invariants.g5.md#old-vm-work-cannot-change-its-replacement). Also discard late results after Stop, ownership transfer or access revocation. [Durable reader authority](http://.g5/ontology/concepts/security.g5.md#serve-nonowners-from-durable-state) forbids a live fallback. \[D · g5.documents.read.live-cache\]  
- File mutation preconditions include the editor's observed VM instance, under the [VM instance invariant](http://.g5/ontology/concepts/system_invariants.g5.md#old-vm-work-cannot-change-its-replacement). \[D · g5.documents.save\]  
- A successful Save acknowledges only the submitted content, preserving later edits and leaving them unsaved when they differ. \[D · g5.documents.save.acknowledgment\]  
- A workspace runs one preview at a time: while a preview is queued, running or closing, a start is refused and Close is the only offered action. A new preview starts only after the previous one settled or its VM was lost, without automatic restart or stale-event effects. The preview shares the owner's VM with chat and terminal work, so a replacement that silently killed the old process left its side effects (containers, daemons) running beside the new one.  
- Preview is a visible cancellable service under live-access authority, exempt from bounded-work slots and wall-clock limits.  \[D · g5.preview.service\]  
- Closing a preview is a bounded graceful stop owned by the preview task: the guest stops the application, runs the profile's optional cleanup\_command with the same shell, working directory and environment as command, and only then exits. Cleanup also runs when the application exits on its own  
- Native subscription credentials remain in the owner VM home with restrictive permissions. Transfer purge owns release before an ownership change. \[D · g5.provider.credential-files\]  
- Reject and scrub retired CODEX\_AUTH\_JSON instead of materializing uploaded or copied subscription credentials. \[D · g5.provider.credential-files.import\]  
- The native login cache may be read by tools running as that owner and may disappear with VM replacement; it is not durable platform credential authority. This developer-desktop trust model provides neither a credential broker nor isolation from other tools running as the owner. \[D · g5.provider.credential-files.cache\]  
- Each workspace repository is materialized from its mapped internal Git branch at the commit selected for boot.  
- Resume restores saved local commits, checkout state and uncommitted files when a recovery snapshot exists.  
- origin refers to G5’s internal repository, not directly to the external provider (if configured).  
- The workspace branch is available locally as refs/heads/<branch> and its G5 tracking ref as refs/remotes/origin/<branch>.
- The parent branch is fetched into refs/remotes/origin/<parent> when available.  
- Observed provider branches use refs/remotes/provider/\*, separate from G5 branch tracking.  
- Pinned reconciliation inputs use refs/g5/provider-inputs/\*.  
- Recovery uses private refs/g5/restore/\* refs populated from immutable flush records.  
- Normal refresh fetches the mapped branch and workspace-specific provider refs; it does not fetch every branch or tags.  
- Fetching tracking and provider refs leaves the current checkout, index and files unchanged.  
- Ref visibility does not grant write authority; pushes remain subject to G5’s workspace and namespace policies.

## Host

- A Host belongs to a specific organization and is not shared between organizations  
- Each Host has and exposes lifecycle and health information  
- Hosts have a kind, which determines which settings are relevant  
- Cloud Hosts have instance\_type, instance\_id, and region fields.  While initially supporting GCP, the platform should generalize to run on any major cloud provider.  
- macOS Hosts are for development and are not a production environment  
- A host can be drained which gracefully shuts down all contained workspace VMs while preventing new VMs from starting on the host.  Once no VMs are running on the host it is fully drained and the host is de-provisioned  
- A Host reports telemetry useful for system observability and debugging (eg active VMs, disk usage, cache state, error information, retry counts)  
- A host has a configured capacity in slots and it advertises the resources (cpu, memory, disk) provided to each slot on the host  
- A host may have a configured VPC  
- A Host can start a VM.  This boots a workspace VM with its image, resources, repositories, and incarnation information  
- A Host can stop a VM either gracefully or with a kill operation  
- A Host reports on VM status as requested  
- A host reports heartbeat information on host liveness.  Lack of Host liveness is insufficient to show that contained VMs are dead  
- A Host has a versioned named image which it runs  
- Each Host runs one cache door that its VMs reach by a fixed alias. It serves verified artifacts and trusted compiled objects from a bounded, least-recently-used store on the host disk, filling misses from the organization's fleet tier or the public upstream; the store dies with the host.  
- A cache changes build time, never results. Verified artifacts are checked by their consumer, so a cache entry can only fail, never substitute; compiled objects run unverified, so they come only from trusted producers, and a guest's own objects never leave its VM.  
- Across hosts, warmth comes from a per-organization fleet tier of base image tiers and compiled objects, written only by the deployment's producer and read with a credential scoped to that organization.  
- Base image tiers are prebuilt images keyed on their recipe inputs (Dockerfiles, lockfiles, build configuration, toolchain), never on source files; a VM pulls a published key through the door and builds an unpublished one locally.<!-- gdoc_comment_begin: c3 -->  
- <!-- gdoc_comment_end: c3 -->Workspace execution uses an isolated libkrun microVM; backend failure never falls back to an unisolated process.  
- The customer OCI image supplies the guest filesystem and development environment.  
- Image selection uses committed code-repository pins, falling back to the organization default when no pin exists.  
- Image pins must identify immutable digests; malformed, mutable or conflicting pins are rejected.  
- Each VM incarnation records its selected image and immutable boot specification.  
- Each VM receives its own writable filesystem and workspace storage.  
- G5 supplies a separate read-only runtime bundle containing its dispatcher, provider CLIs, libraries and pinned module catalog.  
- The guest image ships a baked compile cache for the dependency graph; cargo builds in the VM read further objects through the host's cache door, hitting the organization's own crates only at commits a trusted producer has built.<!-- gdoc_comment_begin: c4 -->  
- <!-- gdoc_comment_end: c4 -->Missing or incompatible runtime components prevent readiness.  
- Bundled tools supplement the image’s command path; image-provided tools take precedence.  
- Image environment variables reach agents, terminals and previews, subject to platform-owned key restrictions.  
- Repository checkout and recovery follow the exact versions selected in the boot specification.  
- At startup the host divides its measured CPU and RAM budget equally by the configured maximum VM count, using integer division. Reserve RAM for the host: at least 1536 MiB or ten percent of measured RAM, capped at half of RAM. Reject unsupported or impossible sizes before registration instead of silently oversubscribing or substituting fixed defaults. Explicit CPU/RAM overrides remain available within the same aggregate budget. \[D · g5.runtime.placement.sizing\]  
- Negotiate host protocol and VM specification independently and reject unsupported versions. \[D · g5.runtime.control.versions\]

## Host Control Plane

- The Host Control plane (HCP) is responsible for provisioning and management of hosts within an organization.  
- Host Control Plane exposes an API for providing information on current hosts, provisioning, draining, and de-provisioning hosts, allocating and reporting on workspace VMs, reclaiming slots from idle workspace VMs, and host image management.  
- HCP provides a central point to query workspace VM state so users and admins can understand the state of the system  
- Organization administrators can provision new Hosts  
- For Cloud Hosts, provisioning requests additional cloud resources to satisfy the request.  If the cloud provisioning fails, the system retains sufficient error information to well inform the administrator what problem occurred.  For transient failures, reasonable retry mechanisms are appropriate.  
- If configured with auto-scaling limits (min, max), workspace VM start requests that cannot be satisfied with existing resources can trigger host provisioning.  
- Any schedulable workspace vm request should get scheduled on an appropriate host

## Git

- the platform internal Git service is the durable authority for repository content and workspace branches  
- it exposes a standard git interface which uses smart HTTP to advertise refs, transfer objects, and receive objects (git-upload-pack and git-receive-pack)  
- authentication uses HTTP Basic carrying a G5 JWT  
- VM git requests use current workspace-incarnation authority, which is checked against the workspace, repository, and live incarnation  
- the platform exposes an API for querying workspace repository files, file content, commit history, and commit diffs.  
- When merging a workspace with multiple repos into its parent, ensure that the merge is observed atomically within the platform.  For repos without external repositories, this means perform the update to the internal git repos under the appropriate locks.  For external repos, ensure that when the parent pulls from the remote, it pulls across all repos.  
- External synchronization explicitly binds a workspace repository to a provider repository and branch.  
- Saving to G5 and publishing externally are separate operations with separate outcomes.  
- Push policy chooses automatic forwarding or an explicit user push.  
- Pull policy chooses automatic imports or an explicit user pull; imports leave live VM files unchanged.  
- Publication requires current authorization and respects the provider’s branch protections.  
- Provider publication credentials remain on the server.  
- New external branches require explicit creation; synchronization never silently recreates deleted branches.  
- Synchronization distinguishes local history changes, provider history changes and divergence.  
- Divergence pauses synchronization until the owner requests resolution; provider history rewrites require separate approval.  
- Base workspace imports provider history but never pushes to the provider.  
- Create or Update PR saves pending source work before publishing an exact committed version.  
- Merge operations retain their selected versions and reconcile partial completion safely.  
- PR and CI status identify the provider revision observed and disclose missing or stale information.  
- A G5 pull request identifies a source workspace, a target workspace and the exact repository versions proposed for merge.  
- One G5 pull request may link to provider pull requests across multiple repositories.  
- Creating or updating a pull request saves and pushes pending source work before external publication.  
- Later source pushes refresh open pull requests under the applicable publication policy.  
- A running workspace rebases in its current owner VM, including clean, fast-forward and no-op cases.  
- A stopped workspace may rebase centrally only when its saved state is clean and fully published.  
- Rebase refuses during workspace lifecycle transitions rather than switching execution locations.  
- Rebase incorporates missing G5 commits and preserves unpublished local commits without duplicating existing changes.  
- Dirty files or unresolved Git state block the affected repository without discarding its work.  
- Replay conflicts pass to a resolution task in the same VM.  
- Publication verifies that the captured G5 branch head has not changed.  
- Multi-repository results identify completed members and blocked or failed members.  
- Rebase completes when G5 accepts the result; external publication follows the workspace’s push policy separately.  
- A local rewrite needs no extra approval when the provider is unchanged; provider rewrites require separate approval.  
- "Make Remote" operation explicitly binds a workspace repository to an external branch.  
- A missing external branch is created from the saved G5 head.  
- Compatible provider history is imported without overwriting divergent history.  
- Incompatible histories bind paused until explicitly reconciled, either by agent or by adopting the provider history.  
- Signing into G5 through GitHub does not grant repository access.  
- GitHub Repository access uses a connected personal credential or an organization’s GitHub App installation.  
- Each GitHub repository operation requires current authority for the selected repository and action.  
- Automatic publication requires organization GitHub App credentials.  
- Missing, expired or revoked authorization requires reconnection; G5 never substitutes another user or the installation bot.  
- New GitHub pull requests use the initiating user’s connected GitHub identity.  
- GitHub publication credentials remain server-side and are not exposed to workspace VMs.  
- GitHub permissions and branch protections remain binding.  
- Default capture limits are 100 MiB per file and 2 GiB per tree/pack, with oversize failing rather than silently excluding content. \[D · g5.git.receipt.limits\]  
- Agent profiles require force-with-lease because the server cannot prove that the client incorporated a prior head. \[D · g5.git.network.lease\]  
- Git owners expose checked receive-pack, publication and merge operations over reusable RepoStorage, GitRepo and workspace Git mechanisms, all executed through the Git execution layer. Retained remote integration owns provider forwarding and incoming publication behavior. \[D · g5.architecture.git\_owner\]  
- RepositoryCredentials resolves repository and operation authority for Git/governance callers. Binding snapshots, installation IDs, permission mapping, token minting and caches stay private to that service and its provider adapter. \[D · g5.architecture.repository\_credentials\]  
- Git/governance coordinates publication and merge over exact Git versions through prepare/apply/land operations and separately authorized approval decisions. \[D · g5.architecture.governance\_owner\]  
- g5-scm is the only platform code that opens repositories, links libgit2 or starts the git program. Its public operations use owned Git inputs and outcomes, keeping backend handles, command arguments and environment assembly private. Policy owners compose complete operations without their own libgit2 access; entry adapters call those owners. The native and libgit2 implementations share backend-independent types, and native-only consumers need not link libgit2. Application content consumers retain the authorized reader, which uses this layer without relinquishing source selection, pinning or read authority. Projection maintenance uses its declared typed-reader path. Moving a raw read into g5-scm does not authorize a caller to bypass either owner. This hides the libgit2-versus-native variation (stable abstractions) and gives credentials, environment and ref safety one home. Git that users and agents run interactively in the guest is their own tool (familiar Git), not platform code. \[D · g5.architecture.git\_execution\]  
- Each repository provider (GitHub, GitLab) has one client behind ScmProvider that owns its provider operations, pagination and error meaning. Pull-request reads and lifecycle, merges, external issues, App installation binding, user repository-access checks, SCM-connection validation, organization/group listing and remote integrations request its typed operations; no caller receives its raw HTTP client or branches on the provider name itself. Clients consume credentials scoped to the requested operation. They own provider protocol and provider-specific errors; account, installation-binding and publication policy remains with their existing owners. Provider selection belongs at the composition boundary, not in each consumer. Two owners stay outside it because their credential and authority differ: OAuth login identity (authorize, token exchange, user and email lookup) in AuthService, and platform bug reports in PlatformIssueGithubClient. Calls that use a stored SCM connection token are provider operations, even when AuthService exposes them. \[D · g5.architecture.scm\_provider\_client\]  
- the 'g5d pr' CLI surface area and functionality intentionally closely follows 'gh pr' to enable agents to naturally use it  
- When the deployment offers the GitHub App, offer disconnected users optional GitHub authorization after login, preserving their intended destination and allowing them to continue without connecting. Account settings owns connected-account status, connection, reconnection and disconnection. PR and external-issue surfaces direct users to this shared account connection; neither creates its own grant. Connection errors do not prevent platform sign-in, and a stored grant is not proof of current repository access. \[D · g5.settings.github-connection\]

## Chat

- Chat's goal is to support people asking coding agents to investigate and change their workspace through conversations. \[I · g5.purpose.agents\]  
- Each chat belongs to a workspace.  
- Model configuration selects the agent, provider account, endpoint, model and reasoning effort.  
- Settings inherit from organization to workspace to user, with more specific values taking precedence.  
- Each launch can override inherited choices or use platform defaults.  
- Claude reads ANTHROPIC\_MODEL; Codex reads OPENAI\_MODEL.  
- Claude Code or Codex are the primary agents the platform integrates with  
- Only the workspace owner controls live agent interaction; authorized readers can read and search saved conversations without a VM.  
- Follow-up inputs queue without interrupting the current attempt and remain editable or withdrawable until reserved.  
- Input becomes delivered history only after confirmed handoff to the provider.  
- Uncertain delivery remains explicit; possible duplicate redelivery requires the owner’s choice.  
- Completed assistant output is saved in order and remains visible after interruption or failure.  
- Live streaming and saved history preserve the same message identities without replay duplicates.  
- Claude supports native questions and permission requests; Codex uses a noninteractive prompt-and-output transport.  
- Stop ends the current chat turn; Continue starts another attempt; Cancel ends the logical task.  
- Forking a conversation carries selected delivered history into a fresh session.  
- Chat sessions, transcript messages and attachment records are durable platform records in the platform database.  
- ChatService owns conversations; TaskService owns queued inputs, execution attempts and task outcomes.  
- Session, task and initial input creation share one transaction.  
- Dispatch reserves input without network calls, commits the reservation, invokes the runtime, then applies the result.  
- Each attempt freezes its input revision, attachments, provider configuration, permissions and execution limits.  
- Chat execution invokes the agent.turn module through the Workspace Runtime facade.  
- Runtime requests identify the current workspace, VM incarnation, execution attempt and authorized actor.  
- Provider adapters translate Claude and Codex protocols into shared delivery, event and outcome records.  
- Confirmed input acceptance creates the delivered user message and assistant placeholder atomically.  
- Journaled events replay with stable identities so retries cannot duplicate transcript output.  
- The platform determines task completion from execution evidence, not assistant text alone.  
- When native session recovery is unavailable, a new attempt reconstructs context from eligible delivered history.  
- The dispatcher owns cancellation and process cleanup, retaining execution capacity until descendant exit is proved.  
- Agent authentication is separate from G5 login and repository authentication.  
- Claude supports subscription tokens, Anthropic API keys, compatible bearer tokens and OpenRouter keys.  
- Codex supports OpenAI API keys, OpenRouter keys and native ChatGPT subscription login.  
- Credentials resolve for the current workspace owner, and each admitted turn fixes its authentication route.  
- Named accounts belong to organization, workspace or user scope; invalid selections fail without credential fallback.  
- Stored keys and tokens are encrypted; account APIs expose only bounded hints.  
- An API-key turn overrides cached subscription login without replacing that cache.  
- ChatGPT subscription login completes before the original prompt is delivered.  
- Device authorization challenges are private to the current owner and attempt, expire promptly, and never enter shared chat history.  
- Failed or interrupted login does not mark the prompt delivered or automatically retry it.  
- Native subscription credentials remain in the owner VM and are excluded from conversation recovery bundles.  
- VM replacement may require subscription login again; imported Codex authentication files are rejected.  
- agent.turn is a declared module that runs one agent execution attempt inside the workspace VM.  
- The platform supplies a fixed input containing the prompt, attachments, provider configuration, permissions and limits.  
- A supervisor launches the selected Claude Code or Codex CLI in the workspace environment.  
- Provider adapters translate native output into shared messages, tool activity, usage, questions and execution events.  
- Chat input texts are limited to 512KB  
- After a run-ending result, the Claude adapter keeps the attempt and Claude's input open while Claude reports background tasks or background sub-agents that it owns. It closes the input only when Claude is between turns, reports no remaining background work and has no pending native request; exit, harvest and settlement then proceed normally. The agent, not the platform, decides whether that work continues or stops.  
- The owner can end the wait without cancelling the logical task. The adapter stops Claude's background work through Claude's native controls where the pinned CLI supports them, otherwise through the bounded stop path. A visible notice records that the work was stopped  
- agent.turn collects token usage and the platform persists it in a manner visible through the chat API.  
- The dispatcher clears inherited environment variables and injects validated dispatch secrets into the agent.turn process.  
- Reuse native memory across sessions and teammates through the codebase's Git, without an additional harness. The agent controls reading, saving, consolidation and pruning; G5 adds no cross-provider reconciliation, semantic conflict resolution or extra pruning. \[I · g5.story.reuse-native-memory\]

## Workspace VM shell access

- The workspace owner can open an interactive shell while the workspace VM is running.  
- The browser connects through a workspace-scoped authenticated WebSocket.  
- The Workspace Runtime facade relays the connection through the host’s PTY channel to the guest dispatcher.  
- The shell runs inside the VM with access to its live workspace files and native tools.  
- The terminal protocol supports opening, input, resizing, closing and reattachment by terminal identity.  
- Reattachment uses an output offset to resume available terminal output.  
- Terminal credentials resolve for the current owner; platform-owned environment keys cannot be overridden.  
- Image environment and tools remain available alongside G5’s runtime tools.  
- The shell receives workspace context so g5d can make authorized API and internal Git requests.  
- Shell edits remain local until an explicit durability operation saves them to internal Git.  
- Interactive terminal access is separate from declared module execution and does not provide platform process-control authority.

## Security

- secrets stored in the database are stored encrypted  
- Force RLS on tenant tables and use scoped transactions, with explicit typed organization/owner/resource checks even in privileged system-role operations. Deployment table-owner remediation uses the bounded cleanup form or explicitly identified historical execution context; neither grants runtime access, and both restore FORCE atomically while RLS stays enabled. \[D · g5.security.tenant.rls\]  
- Redact configured secret values before public output while preserving exact private native-request state.

## Issues

- Issue integration currently supports GitHub Issues, but extensions to support Jira Cloud are planned  
- Organization administrators configure which GitHub repositories are available as issue domains.  
- Domains separately control content trust and whether G5 may publish comments.  
- Users can search configured domains by text, issue state and assignment.  
- Issues can be resolved from references and linked to workspaces as primary or related.  
- Each workspace has at most one primary issue; each issue has at most one primary workspace within its organization and tracker.  
- Links store stable issue identity and are checked against the issue returned by GitHub.  
- Linking an issue does not grant access to its repository.  
- Issue details and discussion are fetched live from GitHub under current domain authorization.  
- Workspace Activity combines the primary issue’s discussion with G5 workspace activity.  
- A failed GitHub read leaves workspace activity available and reports the issue error separately.  
- Authorized collaborators can post comments using their connected GitHub identity.  
- Comment publication requires domain permission and refuses public or unknown-audience destinations.  
- Comment writes retain publication records so uncertain outcomes can be reconciled without blindly posting again.  
- Issues with the platform itself can be reported by any user by clicking a “Bug” button.  Platform issues are created under a separate deployment authority, clearly labeled, and contain an option screenshot constructed asynchronously from the page content.  The “Bug” button is clickable for all practical UI states.

## Ontology

- Ontology is a consistent, deduplicated representation of all non-trivial human intent and design and important agent decisions, with no conflicting or duplicated descriptions. \[I · g5.workflow.ontology\]  
- Ontology uses minimal, concise descriptions to clarify the essence of non-trivial human intent and design and important agent decisions. It does not restate what can be recovered from the code. The authoring format preserves conditions, exceptions and reasons that change a commitment's meaning. \[I · g5.ontology.minimalism\]  
- When a consistency conflict is detected, the ontology provides the lineage needed to understand where the conflict originated. Metadata preserves sources and attribution; reconciliation preserves valid intent when implementation diverges. \[I · g5.ontology.lineage\]  
- The ontology automatically follows good design principles and abstractions and encodes system invariants and policies. These guide and enforce agents to generate better and more reliable code. \[I · g5.ontology.design-principles\]  
- Ontology will serve as the foundation for formal verification: specifying formal specifications, understanding what is important, and distinguishing what can be enforced by tests, human verification, agent review or formal verification. Evidence distinctions keep each form of verification's claims tied to its actual evidence. \[I · g5.ontology.formal-verification\]  
- Ontology defines important invariants, boundaries, data models, and design and coding principles that can be shared across projects within the same organization. Shared meaning follows consistency and deduplication; the boundary contracts identify responsibilities. \[I · g5.ontology.sharing\]  
- A human must be able to read the design and plan and understand the request and how it would be implemented at a high level. \[I · g5.harness.human-readable-plan\]  
- Identify changed ontology and all relevant or governing concepts for the review scope, including unchanged blocks, and verify that the actual implementation supports their commitments. Follows material callers and consumers. \[I · g5.workflow.ontology.review.conformance\]  
- Review architecture and design quality within code review, applying the minimal-design criterion to the design and its implementation. Applies system design principles. \[I · g5.workflow.ontology.review.architecture\]  
- Not all changes require ontology changes:  Unless a modification materially changes the intent of the design or adds a significant new concept, refrain from adding ontology.&nbsp;&nbsp;

## UI

- Use consistent terminology for concepts and entities everywhere in the platform.  Don't invent terminology that would confuse a semi-technical user.  
- Follow accessibility guidelines so the interface works for everyone  
- If operations are not available in the current state, clearly mark them as disabled and use tool-tips or similar affordances to indicate why they are unavailable and how the user can make them available.  
- Almost every action available in the UI should be available to an agent in chat.  Maintain equivalence by enforcing a tight api: both the UI and the agent invoke the same backend API to accomplish the action.  Agent accessibility can be implemented by adding surface to the 'g5d' cli command.  
- Some UI actions launch chat with specific prompts.  These chats should act like any other chat, but should clearly identify the UI element they were launched from  
- &nbsp;

## System Invariants

- Choose the cleanest design with minimal state and compact code. Justify new state, migrations or mechanisms against concrete requirements and simpler use of existing operations. Preserve complete implementation, integration, risk mitigation and testing; minimizing line count is not the goal. \[D · g5.design.minimal-mechanism\]  
- A useful abstraction lets callers request meaningful work without understanding its implementation. Hide recurring complexity behind stable domain concepts, aiming for fewer concepts to learn and local changes: replacing a provider, transport or storage mechanism should leave unrelated workspace/task policy unchanged. \[D · g5.design.abstraction\]  
- State whether new data must survive process loss before selecting its cache, journal or store, respecting the existing durability boundaries. Asynchrony or visible progress alone does not require persistence or stronger recovery. \[D · g5.design.durability\]  
- Existing migration SQL and checksums are immutable. When persisted schema or data needs to change, add a numbered forward migration; a feature does not need new persistence merely because it changes behavior.  The exception to this policy is if a migration was added by the current feature branch  
- Records, references, events and resource access remain tenant-scoped. A cross-tenant identifier never grants access, including through privileged operations. \[D · g5.security.tenant\]  
- All entry paths, including routes, hosts, callbacks, CLI and MCP, use an owner's invariant-preserving operations or declared transaction phases. Callers must not assemble that owner's policy, locks and raw state writes themselves; access and dependency boundaries should prevent such bypasses. Ownership may reside in existing functions or modules; it requires no new service. Use the declared dispatch phases and retain the flush callback-lock exception. \[D · g5.architecture.commands\]

---
gdoc_comments:
  c1:
    status: ACTIVE
    token: AUxaQ1lWyP8yVm1L23lUONgDc73pBNvQXEGjJCklBZ2nIxL410UwuP7_dAcGAjJgBIeCT14CibZIzNdlPZD0U_L4egS2VGPFHYxMmg9zZWNEtow1VAOsS4RUqisEVdnIT45rI9N_WtAvysL_0rFtVrDpaXqsCIMBr93Qk8OsD8uID1T6nmosLH8Kea5lWG3GHqLM-t1nc4ABO_3FZ3W7OTCrDjq5ubLeqWIcoK4JaRfNLs-Vs574lZ1Bg7Yzfoik6mZshmeV_AVlkeO5dtg0WKF8WBsJQr9DVO--xIi07H0k0wbv_DKJRYpjUjhug6H-wzL7Wa0tqwJRlnSywU3ckNb9QXEMwx2an1E1Ey85dLBUW9Cuo7HBxid9X2tw4vsEj_VoTVUDVdfgLXjjW4N5gOY6MslDXlZpnhBxSrNXo3dNh_05wJBNSGSlGgr8mpSP-zpe5BDJHQDIUFcZd1TX4_0e
    head:
      author: Zhengchun Liu
      time: '2026-10-11T19:48:03.653Z'
      body: not exactly correct, it is tuple(user, org)-level for model, keys, UI
      quote: user-level
    replies:
    - author: Ben Vandiver
      time: '2026-10-12T14:13:52.180Z'
      body: added &quot;org-specific&quot;?  This fit?
  c2:
    status: ACTIVE
    token: AUxaQ1nd4HAFw-6bWdegUP5x2vFKNClh5zqmxQdUSy8wKgzehKppHf4qpkbXUCZhweOx_k-5BNFggfOdWdp81JGJr7Wfk4XtzcWTsa636QyQNMw28nCLjiARg51_jBFdWG7yRL3jnjLNQrXqSnVy0A7f3w1-C3jb4w0sW_6F51sbit4zK-u60HB1p41ScXso4-KlNBmmepIYDUyUXRgeoOwjTuvUqVwInHpEMvlUlxdSqq0U-T85EWHvO-A6C2zPaFOQyxot_PJFxWCyUJuJ5waIWsmcoX7eJuGWA9-hIVLdKbiNeOF3C_Zakoj__LiZmp25SVjT6Rhj-g-KsyWMwNsn4kibfcVIpePYKgDlnm8IUkPpeVzxFOQ6N-M5gxWu-DKXWhTBy1ZdbzLMSDrXIdMO5YArSCtEoqWF_wDYitWnkJdlj6w0loFIiDI6
    head:
      author: Zhengchun Liu
      time: '2026-10-11T19:54:02.860Z'
      body: but this is not yet shipped to code
      quote: optionally inherits any dirty file state
    replies:
    - author: Ben Vandiver
      time: '2026-10-12T14:14:05.829Z'
      body: Yeah, the optionally part is not shipped yet.
  c3:
    status: ACTIVE
    token: AUxaQ1nNjTgyrECpChJCxOi4AQo9l9-ptBexUxVFKjQx-VVG77g4Wg7zqLvXOYznKyIG2TiuiD0wQDpJ1FYTGJs6Q2MsUAnfSYz-rYAiNl9hDclc5rlS5zsHYgnae1hM6qxcCUUDrwitOttqQxUmz-O6ztyZi2z6Cld6jtwuqzHTBgmePLUvfVEyEmb5AoZiwn0c7txE4U7Bls8DmvINdJ8Jqwo8mx4rpU25TQdkvxL6wm3iRjW6OQGROi4q-Rf50S035h9r1RnTTkarxwjPz7PiT_DIx4wEPO0aHUUBnYulm1UQHlCJfde6eww9sRWBlfaePhsed6x5Ds9YFqzYKqMMpfwlkPHB_jg0T5sEZgrerjUUmGRUqeA4EsVlecvopYXYwX69hrFo6m_dCCNkqbT4EbG1pn4qHD2cH-Pe_iCo1FZ5ZR8k1UdTG8xHEZwTNA7gZNdHM4WX6Ot3skbCFhIf8HkjHOWzsL48EbQuJMpHdc-0SITucTachTm08wWiXMFs6H4iK-ZceFar7ZeckwySNntGV0M6YyoNJEVKSuAzrBkVpujByYMehlNktyh6_7iW5Al0aJhHG_i7Yz6Hm2ghdYPTrlG9hSFyoM5pTSd5hRK55KWS1eovstNraiFYuTh7NLl9-lUcEEW0F2qWg4XICcHl4xI-e5XqZlvhu-CLa72PrixKIpIjMgaZfqYRsm99RDEiSh1epNNIhhD8B_oJv1bkqwg9L0AzwF1litzGxyqQGcGcQoSnXYFtqbVXI6kQ7PX31KONxEx1IAfZzCsYPUjCoX7kHPlu7tWCZLmbOPdd6_Y0WR2Tm3mN7jX8YvzK1Q9dFLibcCEUTRZ14VsBDAiFqI6Vwj-rMscqLADTV0AOMqX7rjSFgCNpRqTyATvf1X7bgZCqR_V2NWZRyqYHmLa3D84VeycNGg_7ljq1vO2QiPbEUN7r8vYFHfx4Ros7bWFIS3iIAEbokmeNBSFkEoPUf_HFV3n1_AXsT1Pv2c38ng12WdKKrTjMcUKdYE8wggeJ7LQ2GoUJnkH8eYU5EeE
    head:
      author: Kaihui Zheng
      time: '2026-10-12T17:37:15.528Z'
      body: '- Each Host runs one cache door that its VMs reach by a fixed alias.
        It serves verified artifacts and trusted compiled objects from a bounded,
        least-recently-used store on the host disk, filling misses from the organization&#39;s
        fleet tier or the public upstream; the store dies with the host. [D · g5.runtime.cache.door]<br>-
        A cache changes build time, never results. Verified artifacts are checked
        by their consumer, so a cache entry can only fail, never substitute; compiled
        objects run unverified, so they come only from trusted producers, and a guest&#39;s
        own objects never leave its VM. [D · g5.runtime.cache.trust]<br>- Across hosts,
        warmth comes from a per-organization fleet tier of base image tiers and compiled
        objects, written only by the deployment&#39;s producer and read with a credential
        scoped to that organization. [D · g5.runtime.cache.fleet]<br>- Base image
        tiers are prebuilt images keyed on their recipe inputs (Dockerfiles, lockfiles,
        build configuration, toolchain), never on source files; a VM pulls a published
        key through the door and builds an unpublished one locally.'
  c4:
    status: ACTIVE
    token: AUxaQ1lduDh_NCGawqSrCVjj8USE43secRMEmM-MHJS-Ya8aaAzMnAorVB1T5WlcPx66xotnDR19UAZ2QpRM-0GS4mgspi2d9o0oqzJJUuf4XxNzN_IEu20qCzxk47tOwFIUkMVts47PqtX07x-4WxSN6j_ETofVkiGZfhVR1awFLn1-Aq5YSOIDSJAjywRYbsakO-kX0geDnDGCvcaxDTqV_1yAsRkzZNZs4y7Cr1EmYW0O_Izvny7uLYeQ7AtTtuo83hr3ndTXpw2ejCUP6DQE8NqWdgptnNKWkrxnp0anRcrLa_Gg6OhSjB7uGbxs8LRaM9NOF5ebyZhragJbOhFGAj7QtVzrT3a2qeHY0_QSlugEPRFzLS8cjirUMrqiov1WA51CWhGdjqkUALSutFCVMVzNQooU0z9VgzC7LetdbSeSanC0ZNZpO4bD64x_UsLjEWQW3JZ91OnFFIuNZTG30Eov3BiWlmdUAku5BGOU
    head:
      author: Kaihui Zheng
      time: '2026-10-12T17:39:09.523Z'
      body: '- The guest image ships a baked compile cache for the dependency graph;
        cargo builds in the VM read further objects through the host&#39;s cache door,
        hitting the organization&#39;s own crates only at commits a trusted producer
        has built.'
---