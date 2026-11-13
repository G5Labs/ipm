# G5-Lean backend re-architecture — open questions (round 1)

Companion to [lean-backend-design-v1.md](lean-backend-design-v1.md). Each question
carries the working assumption the design currently uses (`[A#]` in the design
text) so that the design is complete under stated assumptions. Please answer
in place; a short "yes", "no", or "option b" is enough. Priorities:

- **Blocking**: a different answer changes the design materially.
- **Important**: changes a section or a phase, not the architecture.
- **Confirm**: the assumption is likely right; a one-word confirmation suffices.

---

## A. Scope and product

### Q1. Retained feature list (Blocking, §2)
**Question.** Which product areas survive? Candidates the survey found:
ontology tree and spec editor with uplift/downlift/sync jobs; grounding and
intent ledgers (the bulk of the MCP tool surface, ~22K lines); docs and
explainers; issues and hotlists; org home; source-control change graph; intent
log viewer; platform issue reporting; Codex support; provider accounts and
quota; self-hosted licensing; UI telemetry; comments.
**Why.** Several removals in §3 are larger or smaller depending on this.
**Working assumption.** The design is feature-agnostic: ontology/grounding
tooling is kept but decoupled from git and CRDT; everything else is retained
until this list says otherwise.
**If different.** Feature-specific tables, routes, and tools are dropped in
Phase 7; nothing in Phases 1 to 6 changes.
**Answer.** _(your working assumption is correct, ontology/grounding
tooling is kept but decoupled)_

### Q2. Deployment targets (Important, §6.6)
**Question.** Keep GCP SaaS (GKE control plane, API-provisioned GCE hosts) and
self-hosted Linux hosts as the two targets? Is the macOS native host dev-only?
Is the Azure canary lane retained? Are the Tungsten / PairSoft / PNC customer
images retained?
**Working assumption.** GCP SaaS + self-hosted Linux; macOS dev-only; Azure
lane and customer-specific images dropped.
**Answer.** _(your assption is correct except keeping Azure lane)_

### Q3. "Grok Bot" experience (Important, §2)
**Question.** Which aspects of the Grok Bot VM/agent experience matter most:
one persistent environment per repo or branch, the agent having a real shell
and long-lived processes, the user being able to open a terminal into the same
environment, or something else?
**Working assumption.** A persistent per-workspace VM with native git, a
browser terminal, and agent-started dev servers reachable from the UI.
**Answer.** _(your assumption is mostly correct, just one note, the VM won't be as persist as a local laptop, means that the VM will be terminate when idle for a while and re-provisioned per user request, before terminate we auto commit changes in the workspace and push to internal git for persistence so that we can resume when re-provision VM)_

## B. Workspace VM

### Q4. VM substrate (Blocking, §4.1, §6)
**Question.** One libkrun microVM per workspace on a shared worker host (reuses
today's boot path, isolation, and hosts), or one dedicated cloud VM per
workspace (simpler isolation story, slower start, higher idle cost, new
provisioning code)?
**Working assumption.** libkrun microVM on worker hosts.
**If different.** §6.2 host daemon becomes a cloud-provider driver; the tunnel
and dispatcher design is unchanged.
**Answer.** _(yes)_

### Q5. Idle policy (Important, §6.1)
**Question.** Default idle timeout before checkpoint-and-stop? Configurable per
org and per workspace? Should an open browser tab count as activity?
**Working assumption.** 30 minutes; org default with workspace override; an
open tab does not count, but file RPC, terminal, preview traffic, and running
tasks do.
**Answer.** _(yes)_

### Q6. VM disk durability (Important, §5.4, §6.5)
**Question.** Is the VM disk ephemeral (rebuilt from git plus checkpoint on
every start) or retained across idle stops on the same host (warm caches, but
host-affine)?
**Working assumption.** Ephemeral in the model; retained on the same host as an
optimization when the host still has it.
**Answer.** _(yes)_

### Q7. Concurrent tasks on one working tree (Blocking, §6.3)
**Question.** Shared working tree by default with opt-in `git worktree`
isolation for batch jobs, or a worktree per task always?
**Working assumption.** Shared by default, opt-in isolation.
**If different.** Chats and single agent tasks would each work in their own
worktree, which brings back a merge step per task.
**Answer.** _(no isolation for agents, agents will work in the same folder, agents are expected to know this conflict)_

### Q8. Background work while a chat is idle (Important, §8.2)
**Question.** Today a parked chat can keep subagents or shell jobs running and
"wake" when they finish. Is that required, or does a turn end when the agent
returns?
**Working assumption.** A turn ends when the agent returns; no background work
outlives it.
**Answer.** _(both claude and codex will work in the same way as codex today, i.e, sidecar starts codex/claude process with resume option for each turn, we assume that codex/claude exit only when all subagent and task are done, do G5 does not need to track/maintain subagent/tasks, also codex/claude process exit is the single truth of turn-end)_

### Q9. VM sizing (Confirm, §6.6)
**Question.** Default vCPU, memory, scratch disk per workspace VM, and the
per-org cap on concurrently running VMs?
**Working assumption.** 2 vCPU, 8 GiB, 64 GiB scratch; org cap configurable,
default 8.
**Answer.** _(microVM will be configurable as it today, i.e, equal size for all microVM thus user only need to specific number of microVM per GCP instance)_

### Q10. Preview reachability (Important, §6.4)
**Question.** Keep routing browser→app traffic through the API relay
(outbound-only hosts, single API replica today), or give each workspace a
subdomain served by its own ingress?
**Working assumption.** Keep the relay, keyed by workspace and port; subdomain
mode stays optional.
**Answer.** _(it should be the same as it today, subdomain is optional)_

### Q11. Browser terminal (Confirm, §6.7)
**Question.** Add a browser terminal into the workspace VM in this iteration?
**Working assumption.** Yes.
**Answer.** _(yes)_

## C. Git and persistence

### Q12. Checkpoint representation (Blocking, §5.4)
**Question.** Choose one:
(a) checkpoint commits on the workspace branch itself (simple; pollutes history;
needs squash before PR);
(b) a per-repo shadow ref `refs/g5/checkpoints/<workspace>` holding a commit of
the working tree on top of HEAD, force-updated (clean history; restore
reproduces the working tree);
(c) a bundle or tarball of the working tree in object storage (captures ignored
files too; loses git semantics).
**Working assumption.** (b).
**Answer.** _(b)_

### Q13. What must survive VM loss (Important, §5.4)
**Question.** Tracked and untracked-not-ignored files: yes. Ignored files such
as `node_modules`, `target/`, build caches: no (rebuilt). Provider session
directories: yes, via the workspace state repo. Docker images on the scratch
disk: no. Correct?
**Working assumption.** As stated.
**Answer.** _(yes)_

### Q14. Multi-repo workspaces (Blocking, §5.1)
**Question.** Keep N repos per workspace with per-repo push atomicity, or move
to exactly one repo per workspace?
**Working assumption.** Keep multi-repo; drop atomic multi-repo cuts.
**If different.** `workspace_repos` collapses into `workspaces`; docs would
need to live in the same repo (see Q15).
**Answer.** _(yes)_

### Q15. Org docs repo (Important, §5.1)
**Question.** Keep the separate org `docs` repo as a member of every workspace
(same branch name), as today?
**Working assumption.** Yes.
**Answer.** _(yes)_

### Q16. Git refs as authority (Confirm, §5.3)
**Question.** Confirm that internal git refs become authoritative and Postgres
only caches head SHAs for list views.
**Working assumption.** Yes.
**Answer.** _(yes)_

### Q17. Non-fast-forward pushes on workspace branches (Confirm, §5.3)
**Question.** Agents rebase, so the workspace branch must accept non-FF updates
with compare-and-swap (force-with-lease semantics) from the workspace's own
identities. Protected branches never do. Confirm.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q18. Job branches (Important, §7.4)
**Question.** Tasks commit directly on the workspace branch by default, and only
batch jobs that request isolation get a `job/<id>` branch plus a final merge
task. Or should every job keep its own branch as today?
**Working assumption.** Direct by default; branch on request.
**Answer.** _(there will be NO job isolation at all, all jobs work in the same folder/branch and we expect that agents are aware and can resolve conflicts by themself, G5 only need to care the branch that workspace map to)_

### Q19. Provider tokens and publication (Blocking, §5.5, §11.4)
**Question.** Provider (GitHub/GitLab) credentials never enter the VM; all
provider pushes happen server-side on explicit actions with the acting user's
token; upstream changes reach workspaces only through the mirror's protected
branches. Is that acceptable? Do agents need to fetch private git dependencies
from the provider (which would require a read-only org token on the host proxy)?
**Working assumption.** Acceptable; private git dependencies via an optional
org-level read-only token, otherwise unsupported.
**Answer.** _(yes)_

### Q20. Mirror storage (Confirm, §4.1)
**Question.** Keep the current `RepoStorage` abstraction and deployment choice
(local disk for self-hosted, NFS Filestore for SaaS)?
**Working assumption.** Yes.
**Answer.** _(Yes)_

## D. Task lifecycle

### Q21. Job kinds (Important, §7.1)
**Question.** Reduce job kinds to profiles over one task machinery (`uplift`,
`downlift`, `sync`, `explainer`, `agent_review`, `custom`), with chat turns and
single agent runs as tasks without a job?
**Working assumption.** Yes.
**Answer.** _(Yes, not that all jobs for the workspace will run from the same vm of the workspace)_

### Q22. Retries (Confirm, §7.2)
**Question.** No automatic retry after an agent-reported failure; one automatic
re-queue after VM loss when the task opts in; otherwise human retry. Confirm.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q23. Job pause and rollback (Confirm, §7.2)
**Question.** Keep pause (stop dispatching queued children) and cancel; drop
rollback statuses because git is the undo. Confirm.
**Working assumption.** Yes.
**Answer.** _(we do not need pause, but we need cancel)_

### Q24. One human-in-the-loop table (Confirm, §7.1)
**Question.** Unify escalations, sync questions, and agent tool-permission
approvals on `task_questions` with one timeout mechanism.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q25. Attempt audit (Confirm, §7.1)
**Question.** Keep a small `task_attempts` audit table with a short reason
vocabulary? Which reasons do operators actually use today?
**Working assumption.** Keep it; reasons: `exit_code`, `provider_error`,
`vm_lost`, `timeout`, `cancelled`.
**Answer.** _(Yes)_

## E. Chats

### Q26. Shared chats (Important, §8.1)
**Question.** May any workspace member send into another member's session
(with `author_id` recorded per message), or do sessions stay owner-writable and
read-only for others as today?
**Working assumption.** Any member may send.
**Answer.** _(Yes)_

### Q27. Context links (Confirm, §8.3)
**Question.** Drop cross-session context links entirely?
**Working assumption.** Drop.
**Answer.** _(Yes)_

### Q28. Provider transcript durability (Confirm, §5.4, §8)
**Question.** Provider session files live on the VM and are checkpointed into
the workspace state repo; the Postgres transcript mirrors are dropped. Confirm.
**Working assumption.** Yes.
**Answer.** _(NO, session files must be persisted to Postgres (and only to Postgres) so that user can look at chat history even without workspace VM, and when user send new message to the chatbox while session file does not exist on the vm, we restore the chant session file from Postgres before starting claude/codex --resume)_

### Q29. Task-scoped chat sessions (Important, §8.3)
**Question.** Sync and merge task dialogs are ordinary chats with a `task_id`
reference, not a separate session type. Confirm.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q30. Preview chat-attach (Confirm, §6.4)
**Question.** With the agent and the app in the same VM, "attach chat to
preview" is the default and the attach machinery is deleted. Confirm.
**Working assumption.** Yes.
**Answer.** _(Yes)_

## F. Documents and editing

### Q31. Co-editing and presence (Confirm, §9.5)
**Question.** No real-time co-editing or presence in this iteration; per-file
optimistic concurrency on Save; a document-only CRDT later. Confirm.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q32. Save while the VM sleeps (Important, §9.2)
**Question.** Save wakes the VM (one write path, first save after idle is slow),
or Save while asleep commits server-side to the branch (two write paths, no
wake)?
**Working assumption.** Wake the VM.
**Answer.** _(Yes)_

### Q33. Comment anchors (Confirm, §9.3)
**Question.** Anchor comments to `(path, line, line hash)` instead of CRDT
positions.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q34. Editors to keep (Important, §9.3)
**Question.** Keep Monaco, the block-style spec editor, the structured question
editors, and the explainer views, all rebound to local state with Save? Any of
them droppable?
**Working assumption.** Keep all four.
**Answer.** _(Yes)_

## G. Governance

### Q35. Enforcement points (Blocking, §10.1)
**Question.** Enforce at the internal git boundary (receive-pack denies
protected refs; server-side merge checks approvals) and rely on provider
branch protection as a second gate that G5 verifies at repo connect. Both, or
only one?
**Working assumption.** Both.
**Answer.** _(yes)_

### Q36. Minimal approval model (Blocking, §10.3)
**Question.** One `approval_rules` row per protected branch (required
approvers, minimum count, optional agent reviewer profile) and one
`approval_decisions` row per decision bound to the PR head SHA. Drop stages
(planning/finalizing), definition versions, request ledgers, and immediate
verdicts. Acceptable?
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q37. Who may approve (Important, §10.3)
**Question.** The rule's approver list plus org admins? Provider `CODEOWNERS`?
**Working assumption.** Rule list plus org admins.
**Answer.** _(Yes)_

### Q38. Agent PR creation (Important, §10.2, §11.1)
**Question.** Agents open PRs through `g5d pr create` (platform op, no provider
token in the VM), not `gh pr create`. Confirm.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q39. Provider PR mirroring (Important, §10.2)
**Question.** Create a provider PR for every G5 PR on a provider-backed repo (so
external reviewers see it) and merge through the provider API when one exists?
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q40. Workspace mutation policy (Confirm, §10.4)
**Question.** Drop the read-only/freezing mutation policy; base workspaces are
read-only through branch protection; "freeze" becomes stop-and-archive.
**Working assumption.** Yes.
**Answer.** _(Yes)_

### Q41. Fine-grained scopes (Confirm, §10.4)
**Question.** Collapse to the four org roles plus workspace visibility and
members; drop per-user scope grants and workspace permission overrides (no UI
uses them today).
**Working assumption.** Yes.
**Answer.** _(Yes)_

## H. Agent surface

### Q42. LLM proxy (Confirm, §11.4)
**Question.** Keep the host-side LLM proxy (provider keys stay on the host,
usage metering and quota capture continue) rather than placing keys in the VM.
**Working assumption.** Keep.
**Answer.** _(Keep)_

### Q43. Ontology and grounding MCP tools (Important, §11.1)
**Question.** If the ontology feature is retained, keep the MCP tool surface
as is (minus the rebase-pause and CRDT-peer coupling)? Does `task complete`'s
spec-delta validation need a replacement (for example a check in the agent
review task)?
**Working assumption.** Keep the tools; spec validation moves to a review
profile, not a completion gate.
**Answer.** _(yes)_

### Q44. Retained `g5d` commands (Confirm, §11.1)
**Question.** `pr`, `ask`, `task`, `preview`, `ports`, `issue` (if retained),
`auth`, `doctor`, and `mcp`. Anything else needed by agents or laptops?
**Working assumption.** That list.
**Answer.** _(yes)_

### Q45. Profiles and hooks (Confirm, §11.5)
**Question.** Rewrite `CLAUDE.md`/`AGENTS.md` around native git; reduce hooks to
`SessionStart` and `Stop`; delete CRDT-era skills.
**Working assumption.** Yes.
**Answer.** _(yes)_

## I. Migration and process

### Q46. Phase order (Important, §13)
**Question.** Proposed order: git boundary → CRDT removal → workspace VM →
preview/terminal → governance → lifecycle and schema squash → feature cleanup.
An alternative is workspace VM first (biggest change, removes sticky runtimes
and golden images earliest). Preference?
**Working assumption.** The proposed order.
**Answer.** _(yes)_

### Q47. Migration squash (Confirm, §12)
**Question.** Replace the 292 migrations with one editable baseline until the
first production deployment.
**Working assumption.** Yes, starting in Phase 0.
**Answer.** _(yes)_

### Q48. Test and CI baseline (Confirm, §15)
**Question.** Keep the Rust end-to-end crate and the Playwright flows as the
safety net (pruned per removed feature)? Keep Cloud Build as CI, or move back
to GitHub Actions for the lean repo?
**Working assumption.** Keep both suites; CI choice deferred to Phase 7.
**Answer.** _(Keep Cloud Build as CI and deferr to the last Phase)_
