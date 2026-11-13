# Owner decisions on #830 (2026-10-27)

## Human instructions

Answers to the design questions in the workspace chat:

- Exposure (#831): "Allow, read-only". Every process in a personal VM, including code from
  Base's reference checkouts, may read Base repositories' GitHub PRs (view, view -c,
  checks, list) through `g5d`.
- Listing source (#832): "github + G5 links - if no github integration then fall back to
  local."
- Next step: "Implement now".

## Interpretation

- "No GitHub integration" is read as the selected repository having no GitHub access path:
  a local (non-GitHub) repository, or a GitHub repository whose organization has no GitHub
  App binding and whose owner has no GitHub connection, or a deployment with no GitHub App
  configured. In those cases repository listing answers from G5's stored PRs and says so.
- A GitHub integration that exists but fails (expired or revoked connection, inactive or
  uncovered App binding, missing permission, provider errors, rate limits) stays an error
  with its recovery step. Silently swapping in stored records there would hide the
  reconnection that `g5.git.authorization-loss` requires.

## gh alignment request (2026-10-27)

### Human instructions

> rebase branch onto main, then audit places where 'g5d pr' doesn't match 'gh pr' and
> produce a design to better align them. Pay careful attention to 'g5d pr update' which I
> think is mostly irrelevant now but lots of agent instructions seem to think is necessary
> (note that pushing to the branch is usually sufficient to update the pr automatically,
> just like github).

### Interpretation

- Design only, continued in this workspace's plan (Part 2), with the open choices
  recorded as Q blocks for the owner.
- "Usually" is taken to mean workspaces with Auto-push on and App write access. The
  agent verified that case on PR #854: a plain force-with-lease push reached GitHub
  without `g5d pr update`. The remaining cases are recorded as exceptions, not dropped.

## Owner answers (2026-10-27)

### Human instructions

> I accept your recommendations for Q1-Q4. Implement it. Also tell me how the system
> would work if a user turned off auto-push.

### Interpretation

- Q1 (a): `g5d pr update` is a hidden "republish now" with no Save; `-t`/`-b` make it edit.
- Q2: `g5d pr create` publishes committed work only, refusing uncommitted changes and
  pushing unpushed commits. The UI keeps Save, and `g5.git.pr-save` is scoped to the UI.
- Q3: no forwarding from Auto-push-off workspaces; report it instead.
- Q4: Ship with agent step 3 publishes fixes with `git push`.
- "Implement it" covers Part 2's decisions 1, 2 and 4 and the refusals, but not the
  items the design marked as deferred.

## One description read for the UI and `g5d pr view` (2026-10-28)

### Human instructions

> I want the UI and the g5d command to go through the same code path so they are aligned
> and remain aligned

Context: the owner asked where the workspace landing's "Implementation description" comes
from and whether `g5d pr view` returns it without GitHub. The agent found that the landing
reads the server's canonical description (saved, else the PR-body Project Doc) while
`g5d pr view` printed only the saved `descriptionOverride`.

### Interpretation

- Taken as authorization to implement, recorded as Part 3 of this workspace's plan.
- "The same code path" means the same server read (`read_pull_request_description` behind
  the description route), not a second resolver in `g5d` or the frontend.
- It covers a stored G5 PR's description. A GitHub PR read by number or URL keeps showing
  GitHub's body, as `gh pr view` does.

### Follow-up: listing bodies

> for 'list --json body' - can the server assemble the result to avoid lots of description calls?

The agent proposed a batch read of the text only (no provider comparison) for the listed
PR IDs, sharing the single read's resolver. The owner replied:

> go ahead

### Follow-up: keep the saved text on a server error

> yeah, still show the saved description on a server error (we have it we should show it -
> a note it could be stale is ok).

## No GitHub expected (2026-10-28)

### Human instructions

> More broadly, We should not complain about missing github data if the workspace (or base
> if you're listing) doesn't have any remote repos (eg if we can prove that there's no
> expectation of github then don't complain it's missing)

### Interpretation

- "Remote repos" is read as repositories G5 would publish to GitHub: a GitHub repository
  tracking a remote branch. A repository with a non-GitHub remote is treated the same as a
  local-only one, because no G5 PR surface reads any other provider.
- The test is applied per scope: a PR's members for PR surfaces, the selected repository for
  a repository listing (Base's repositories in a personal workspace).
- Design first (plan Part 4); implementation waits for the owner.

### Owner answers

> #1 is ok.  #2: Can we use an existing event_type instead of adding a migration?

- #1: `g5d pr checks` exits 0 with "no external checks apply" for a PR expecting no GitHub.
- #2: keep `provider_failed`, labelled by the PR's repository count; no migration. Taken,
  together with #1, as the go-ahead to implement Part 4.

## PR reads follow workspace read access (2026-10-28)

### Human instructions

Context: in the preview, a personal workspace's agent found Jordan's G5 PR in a listing
but could not read its description, because `view` and the description read served only
PRs where the asking workspace is source or target.

> The design for reading descriptions you state doesn't align with "works like github" -
> The expectation is that if the user has read access to the workspace, then 'g5d pr'
> should satisfy read-only requests about its prs. list, view, bodies, etc - all should
> work and provide all non-intentionally-secret information. No complicated permission
> model for what 'g5d pr' shows. I want my agent to be able to broadly audit PRs in the G5
> system.
> for #2, I accept your bounded fix suggestion

### Interpretation

- "Read access to the workspace" is the audience rule the UI uses to open a workspace.
  The agent reads as its owner.
- #2 is authorized: list a G5 PR under a repository only when it changes that repository,
  and give the recorded head. #1 is a design (plan Part 5) for the owner to read first.

### Owner answer

> yes, implement

Taken as acceptance of plan Part 5, including `--all`, `--workspace` and the VM exposure.

## Deleted workspaces (2026-10-29)

### Human instructions

Asked whether G5 keeps what it needs to show historical PRs after a workspace is deleted,
and shown that PR records are kept but hidden while open PRs stay open forever, the owner
answered:

> deletion should close open PRs - nothing more to do.  PRs should remain visible to tools
> after the workspace is tombstoned (source, target or both)

### Interpretation

- Deletion closes each open PR into or out of the workspace, as an ordinary close does, in
  the removal transaction. "Nothing more to do" is read as answering the other two parts of
  the agent's question: GitHub PRs are left as an ordinary close leaves them, and approval
  evidence whose commits are later collected reads as unavailable, with no extra retention.
- "Visible to tools" covers `g5d pr` and the API behind it, for a deleted source, target or
  both. Who may read is the audience rule applied to the deleted workspace's retained
  visibility, owner and invitations, so deletion never widens an invite-only workspace's
  audience. The UI's lists are not covered by this instruction.

### Follow-up: the UI (2026-10-30)

Asked whether the UI should keep hiding these PRs, the owner answered:

> UI and the tool should return the same things.  To avoid confusing the user in the UI,
> historical PRs could be grouped separately - or left off if the goal of the UI is to allow
> actions on the PRs.

Interpretation: the UI and tools read the same answers. Action surfaces already request open
PRs, and deletion closes them, so they leave these PRs off with no filter of their own. PRs
left open by earlier deletions are closed once, so they do not reappear there. Where the UI
shows PR history, PRs with a deleted side are grouped apart and name the deleted workspace.

## Check reads that settle (2026-10-30)

### Human instructions

> why does "Ship to base" occasionally say "PR revisions changed while reading checks; retry for
> current results." when looking up check status? Especially if I don't think it did actually
> change while reading?

The agent explained that the read compares the PR's head, base tip, test-merge commit and
mergeability at its start and end, so GitHub recomputing the test merge or the base advancing
triggers it, and proposed re-reading when only those moved and naming what moved. The owner
answered:

> go ahead with your fix

### Interpretation

- A moved head, state or base branch still ends the read at once.
- Anything else re-reads, up to three reads, and the final message names what kept moving.
- Made on the #830 branch (PR #854), the only branch this workspace pushes.
