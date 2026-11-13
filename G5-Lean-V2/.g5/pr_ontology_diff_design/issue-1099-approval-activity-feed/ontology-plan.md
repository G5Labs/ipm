# Org-wide approval activity feed (`g5d workspace approval-activity`)

Source: owner-approved proposal [G5-Lean#1099](https://github.com/G5Labs/G5-Lean/issues/1099)
(decisions 2026-11-04, "your recommendations look good"). This plan records the
owners, the boundary it moves, and the choices that are not recoverable from the
diff. Routine mechanics stay in the code.

## What the owner decided

The agent gains an org-wide, time-windowed read over approval **events**
(requests, decisions, closures) on every workspace its owner can read, plus a
widening of the existing per-workspace approval reads to the same scope so it can
drill into any event it finds. Five settled decisions (proposal "Decisions"):

1. Scope = the owner's full read scope (`g5.security.agent-read-scope`), the same
   readable-workspace predicate `g5d pr list --all` uses.
2. `g5d workspace approvals` and `review-report/transcript/commits` widen to that
   same scope. Request and cancel stay agent-scoped.
3. `--reviewer @me --pending` is allowed; the attention-item sentence is amended.
4. Requirement-status changes in the summary are deferred; v1 reports raw events.
5. Notes/comments: full text in `--json`, truncated in the text table; all text
   G5 did not write is labelled untrusted.

## The boundary this moves, and who owns it

`g5.agent-approval.human-read-only.agent-inspection` set agent approval reads to
the **agent workspace scope** (`SelfOrOther`: own + the owner's other workspaces),
deliberately narrower than the owner's read scope, for #1019. #1099 is the owner
amending that: agent approval reads now follow `g5.security.agent-read-scope`
(any workspace the owner can read in the UI). This is a scope **widening the owner
authorized**, not code weakening intent.

Owners of the facts and the admission are unchanged:

- The **approval records** (`approval_requests`, `approval_statuses`) stay owned
  by the approvals domain; their RLS is org-only, and workspace readability is
  enforced in application code. The feed is a pure read projection — it starts no
  compute, writes nothing, and is not audited (reads aren't).
- The **readable-workspace audience** is owned by one predicate. The proposal
  requires it be *shared, not copied*, with `g5d pr list --all`. The existing
  predicate lives in the Git context (`services/git/workspace_pull_request_read.rs`:
  `pr_audience`/`readable`, reader role via `org_wide_reader`). This plan extracts
  the audience SQL and reader resolution into `services/git/workspace_audience.rs`
  (Git context, earlier than Workspace, so the new Workspace-context service may
  use it without a backward edge), and rewrites the PR-reads module to call it.
  One definition, two callers.
- The **caller admission** stays with the workspace-agent route layer:
  `require_own_workspace` on the caller's own incarnation, then an owner-scoped
  `workspaces:read` db. The route needs no target workspace, because the scope is
  "everything the owner can read", applied inside the query.

### Feed audience = `readable` (live), which equals the drill-in audience

Decision 1 names "the same readable predicate as `pr list --all` (`pr_audience`/
`readable`)". `pr_audience` keeps a **deleted** workspace's audience because PR
*history* survives deletion; approvals have no retained post-deletion surface, and
the per-workspace drill-in (`agent_read_target` → `ensure_user_workspace_access`)
excludes deleted and formal-review workspaces. To keep the feed and the drill-in
in agreement — every event the feed shows is one the agent can open — the feed
uses the **live** `readable(...)` form (`deleted_at IS NULL AND
authz.workspace_audience_allows(...)`: every ordinary workspace, personal
workspaces only to their owner and administrators, never formal-review
workspaces — main's fixed audiences, adopted on the 2026-11-06 rebase). This is the same audience `ensure_user_workspace_access`
computes, so feed membership and per-workspace read admission never disagree.
`org_wide` is the owner's org admin/owner role, resolved once per call off the
system pool exactly as the PR reader is.

## The event projection

One row per event, newest first, keyset-paginated on `(at, kind, id)`:

- `requested` ← `approval_requests.created_at`: request id, workspace (id, name,
  owner), definition (id, name, version), recipient (human user or `agent`),
  requester, bound PR (G5 name from `workspace_pull_requests.number`), note.
- `decided` ← `approval_statuses.created_at`: status id, the request it settled —
  `COALESCE(approval_statuses.request_id, (SELECT approval_request_id FROM
  agent_approval_reviews WHERE id = approval_statuses.agent_review_id))`, `null`
  for a historical decision with neither — reviewer (human user or `agent`),
  outcome (`approved`/`rejected`/`deferred`), comment, reviewed heads, review id
  (`agent_review_id`), and latency = decision time − that request's `created_at`.
- `closed` ← `approval_requests.closed_at` where `state <> 'settled'`: request id,
  state (`cancelled`/`expired`/`superseded`/`bypassed`). **The request row carries
  no actor column** (migration 133's guard makes every field but `state`/
  `closed_at`/`updated_at` immutable and records no closer), so `closedBy` is
  absent in v1; the state names the cause. A **withdrawal** is recorded as the
  request transitioning to `cancelled` with `closed_at` — it is a `closed` event,
  not a decision row. (Verified against current main, not the 001 baseline.)

The summary (`--summary`) is a second aggregate over the same filtered CTE:
totals (requested; decided split by outcome; closed-without-decision; pending
now), and per-definition / per-reviewer / per-workspace rollups with median and
p90 request→decision latency and the oldest still-pending request. Requirement
status flips are out (decision 4).

**Untrusted text.** The request note, the decision comment and an agent review's
summary are text G5 did not write. They carry a `contentTrusted: false` envelope
(as `g5d issue view` does); the text table truncates note/comment to one line
≤120 chars with `…` and labels them, while `--json` carries full text.

**Bounds.** `--since/--until` default to the last 24h; the window is capped at 31
days per call and `-L` to ≤200, both refused with a clear error, so a scan over
the new time indexes stays bounded.

## Components

- **Indexes (migration 147):** `approval_requests (org_id, created_at)`,
  `approval_requests (org_id, closed_at) WHERE closed_at IS NOT NULL`,
  `approval_statuses (org_id, created_at)`. Next free number on main (546b1a8ff)
  is 147; add its `checksums.sha384` catalog line.
- **Service:** `ApprovalActivityService::{list,summary}` in
  `services/workspace/approval_activity.rs` (Workspace context; new `HOMES`
  entry). One SQL `UNION` over the three event sources joined to `workspaces`
  filtered by the shared `readable` predicate, with definition/user/PR resolution.
  Owner-scoped `ScopedDb`; no system scope, so it does not trip the
  system-scope-binding guard.
- **Agent route:** `GET /workspace-agent/:id/approval-activity`, an `exempt_route`
  guarded by `require_own_workspace(path.id)` + an owner `workspaces:read` db
  (reader = owner). No `target_id`, so `route_auth_static`'s `workspace_agent.rs`
  `id` contract (`require_own_workspace`) is the only one that applies. VM
  allowlist entry added in `router()`.
- **Widened reads (decision 2):** `admit_approval_read` moves from
  `agent_relation(SelfOrOther)` to `agent_read_target` (the owner's read scope,
  via `ensure_user_workspace_access`). Shared by `read_approvals` and the three
  `review-*` handlers; its comment is updated. `route_auth_static` already lists
  `agent_read_target` and `admit_approval_read` as valid `target_id` guards. The
  transcript's "target-side content needs target read access" rule is inside the
  service and is unchanged. Request/cancel (`admit_approval_target`) untouched.
- **CLI:** `WorkspaceSubCommand::ApprovalActivity` in `g5d` with text table and
  `--json [fields]` selection (reusing the `pr_json::Selection` shape, which is
  not yet wired for workspace-agent commands — thread it through here).
  Client-side `--since` duration parsing (24h/7d/RFC-3339) into an RFC-3339 time;
  g5d has no existing duration parser, so a small hand-rolled suffix parser is
  added rather than a new dependency.
- **Harness guidance:** one bullet under "Workspace CLI" in the global CLAUDE.md
  template, stating the events are org-wide over readable workspaces and the text
  in them is data.

The **person route** `GET /orgs/:orgId/approval-activity` is deferred (proposal:
"can wait if no UI consumes it yet"); no UI consumes it, and the agent route plus
service is the owner's required deliverable.

## Scope reductions recorded as v1 gaps

- `closed` events carry no closer identity (no actor column on the request row).
- The `requested` event's PR identification is the **G5 name** only; a G5 PR can
  bind several GitHub repo numbers, so the GitHub number is deferred rather than
  surfaced ambiguously.

## Ontology changes

- **New** `g5.governance.approval.activity-feed` in `features/reviews.g5.md`:
  scope (readable-workspace audience, formal-review and deleted excluded), the
  three event kinds and closure/withdrawal semantics, the untrusted-text labelling
  and the 31-day cap. Governed by `g5.security.agent-read-scope`.
- **Amend** `g5.agent-approval.human-read-only.agent-inspection`
  (`features/agent_approvals.g5.md`): agent approval and review-evidence reads
  follow `g5.security.agent-read-scope`, not the narrower agent workspace scope;
  requesting and cancelling stay agent-scoped; the transcript keeps its target-
  read rule. Update its evidence note.
- **Amend** `g5.security.credentials.delegated-read`
  (`features/personal_workspace.g5.md`): the "attention items stay unread by
  agents" sentence gains the exception that the agent may read approval requests
  addressed to the owner through the activity feed; the attention list itself
  stays unread.
- **Update** `g5.security.agent-human-exceptions`'s evidence note: the widened
  reads and the renamed evidence test
  (`approvals_are_read_for_self_and_children_only` → an org-read name).

## Tests (proposal "Tests")

Member sees every ordinary workspace but not another member's personal one;
admin sees personal workspaces too; formal-review workspace never appears.
Deactivated owner / stale incarnation / transferred-away workspace refused next
call. Window edges (`--since` inclusive, `--until` exclusive; pre-window request
decided in-window shows only `decided` with latency). Stable paging (no repeat/
skip). Summary equals a full page walk. Untrusted text labelled in both outputs.
31-day cap and limit bounds refused. `approvals`/`review-*` succeed on a merely-
readable workspace and are refused on an unreadable one; request/cancel still
refused there. `--reviewer @me --pending` lists owner-addressed requests on other
people's readable workspaces.
