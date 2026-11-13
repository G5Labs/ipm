# Workspace reviews and approvals

Review complete workspace revisions through shared discussion and explicitly addressed approvals. Workspace and organization authority remain with their existing owners; the UI uses [one workspace landing and approval destination](ui.g5.md#g5.workbench.landing).

## Required approvals

<a id="g5.governance.approval.requirements"></a>
[D · g5.governance.approval.requirements]

Each attached requirement applies once to the whole workspace, independently of engineering phase. Evaluate all attached target requirements against the configured destination and pinned source revision before provider merge; local application re-reads that admission. Refuse a different destination. The target requires an approval type and its any/all rule; its configured humans are the eligible pool, not separate target requirements. Display the type once with actually requested reviewers nested beneath it. Phase movement neither gates on approvals nor duplicates, clears or creates a request. Approval configuration selects one earliest relevant phase in a regular visible dropdown, defaulting to Integration Engineering; this recommends when requesting first makes sense and does not limit request eligibility or merge evaluation. Show the recommendation with the required type, once. Preserve legacy phase lists on read; when editing, select their earliest engineering phase (IE if empty) and publish the chosen single association through the existing versioned API.

[Organization administrators](workspaces.g5.md#g5.workspace.approval-administration) own requirement attachment and retirement; ownership alone grants no such authority. An administrator's [merge with override](workspaces.g5.md#g5.workspace.merge.admin-override) waives unmet requirements for one merge without deciding them.

<a id="g5.governance.approval.status"></a>
[D · g5.governance.approval.status]

Each attached requirement has one overall status: Approved, Rejected or Pending. Request lifecycle states (Waiting, Cancelled, Failed, Expired, Superseded, Bypassed) are not requirement statuses, and a rejection superseded by a permitted human approval remains a rejected decision in history. One requirement evaluator computes status from counted evidence under the [quorum](#g5.governance.approval.quorum), [source-commit policy](#g5.governance.approval.source-commit-policy) and [request PR rule](#g5.governance.approval.request-pr); merge admission reuses it. One workspace approvals response returns the complete required list alphabetically with each status, aggregate commit currentness, a structured path to approval, request needs and the request policy. Home, the landing summary, Approvals and Prepare to Ship count or show those requirements once each and never derive status from reviewer rows. Currentness is matching, changed (naming reviewers whose evidence differs from the current source) or unknown, and never changes status. A path to approval names rejecting humans or the rejecting agent, or every approver still needed, and says that a qualifying approval will meet the requirement; it is absent for untouched or approved requirements. The evaluator also derives an agent-escalation display from the same counted evidence, never a status, count category or stored decision: Needs Human Escalation while a counted agent rejection is the only rejection and Any-of human override has an actual route, an eligible active-member human who may be requested, whether or not one is already Waiting; Overridden by Human once a counted eligible human approval overrides that rejection with no counted human rejection. Without such a route, under All-of or with override off, the rejection reads Rejected; a counted human rejection keeps both Rejected.

<a id="g5.governance.approval.source-commit-policy"></a>
[D · g5.governance.approval.source-commit-policy]

`on_source_commit_vector_change` is IGNORE or MAKE_PENDING; IGNORE is selected. Under IGNORE a counted decision keeps counting after later source commits, including a move of one repository in a multi-repository vector; a parent-tip move never rewrites a decision. A later decision, withdrawal or invalidating definition version still changes status under its own rule. MAKE_PENDING counts only decisions on the current source vector and remains a supported setting. Only the evaluator reads the policy; consumers show currentness separately.

## Requested reviews

<a id="g5.governance.approval.request"></a>
[D · g5.governance.approval.request]

New definition versions and human requests require [active organization members](../concepts/security.g5.md#g5.security.active-membership), held through assignment commit. Preserve inactive identities separately from departed members in historical definitions, requests and decisions.

A configured human addressed by an active pending request may approve or request changes with current read access, unless they are an [organization viewer](workspaces.g5.md#g5.workspace.approval-human-authority). The current workspace owner is eligible only when both the requested definition version and its current version allow self approval; self approval is disallowed by default. Each request belongs to its exact definition version and recipient, and to the PR open when it was made, if any, independently of the engineering phase. A decision settles only that request; another review needs an explicit owner request. An unrequested reader may leave general feedback but cannot make a formal decision. Pre-existing decisions without a request remain labelled historical evidence; they are never retroactively attached to a PR or used as its review checkpoint.

<a id="g5.governance.approval.request-pr"></a>
[D · g5.governance.approval.request-pr]

One request rule, `PR_REQUIRED`, applies to human and agent requests alike; it is false now. When false, either kind can be requested and decided without an open PR. When true, both kinds require an open PR, and human decisions require a request bound to it. A pending human request is active only while bound to the open PR, or when made with no PR while none is required; a closed or merged PR never gives an undecided request authority. A settled human or agent decision keeps its effect when a PR opens, closes or merges and when `PR_REQUIRED` changes; its PR binding remains provenance, and its effect changes only through a later decision, withdrawal, invalidating definition version or the [source-commit policy](#g5.governance.approval.source-commit-policy). Human evidence counts only when addressed by a request bound to a PR or explicitly made with no PR; decisions without a request, or on a legacy request recording neither, remain historical. A merge with override closes both kinds of pending human request as Bypassed. The rule governs requests only; PR creation and shipping conditions are unchanged.

<a id="g5.governance.approval.request-note"></a>
[D · g5.governance.approval.request-note]

The requester may attach an optional note to a review request. Preserve it with that immutable request and show it to the reviewer and in request history/timeline. Exact operation retries include the normalized note in their identity; a retry cannot silently replace it or create duplicate requests. A later request may carry a new note.

<a id="g5.governance.approval.request-email"></a>
[D · g5.governance.approval.request-email]

Creating a human approval request emails the reviewer so they need not wait to notice it on the Home attention list. One email is sent per newly admitted request; a retried or reused request sends nothing more, and the requester is never emailed for their own request. The subject begins with the fixed prefix `[G5 Approval]` and names the workspace, definition and requester, and the mail comes from the dedicated [`G5 Approvals` sender](../software_architecture/email.g5.md#g5.email.sender) so recipients can filter on either. The body links to the request. Below G5's own opening line it quotes the requester's [note](#g5.governance.approval.request-note) when there is one, and the opening of the PR's [canonical description](../software_architecture/git.g5.md#g5.git.pr-description), each under a label naming its source as [text G5 did not write](../software_architecture/email.g5.md#g5.email.authored-content). The note is shown as plain text. The description is rendered as the [Markdown subset](../software_architecture/email.g5.md#g5.email.markdown), its links following the [link policy](../software_architecture/email.g5.md#g5.email.link-policy) with the organization's repositories in scope. It keeps only whole top-level blocks, or whole items of a top-level list, within its first 30 lines and 1,500 characters, so a cut never leaves a code fence, list item or table open, and is marked as shortened when cut. When not even the first block or item fits, or the subset refuses the kept text, the first lines are shown as plain text instead, so the email is never withheld because of the description. The description is read under the requester's own authority after the request commits; when it cannot be read the email is still sent without it. The footer names the setting and links the Settings page whose Notifications section holds it. The reviewer's account [notification preference](users.g5.md#g5.user.notifications) is honored before the mail is queued and re-checked at delivery, which also cancels the mail once the reviewer is no longer an active member of the organization or their account email has changed. The mail is queued only for a pending request, but the delivery check deliberately never reads the request, so the system mail worker holds no authority over the organization's approval records; a request settled in the moments before delivery may therefore still be emailed, and its link shows the settled state. Because approval requests are recorded under tenant authority and the [email queue](../software_architecture/email.g5.md#g5.email.handoff) is system-owned, the mail cannot share the request's transaction; it is queued best-effort after the request commits, keyed on the request so a replay coalesces rather than sending twice. On the shared sending reputation it carries a [volume limit](../software_architecture/email.g5.md#g5.email.volume-limits) of 500 requests per organization and 50 per recipient in a rolling 24 hours, since the owner controls both who is asked and how often. The email is therefore not the system of record — the Home list is — and a deployment with email disabled or over that limit still creates the request with no error to the requester.

<a id="g5.governance.approval.decision-email"></a>
[D · g5.governance.approval.decision-email]

When a human reviewer settles their request by approving or requesting changes, G5 emails the workspace's current owner so they can act without watching the Approvals page. A withdrawal, an agent decision and the owner's own decision on a request addressed to them send nothing. A request settles only once, so it is emailed at most once: the mail is keyed on the request and a replayed decision returns the mail already queued or sent. It shares the [request email](#g5.governance.approval.request-email)'s `[G5 Approval]` subject prefix and `G5 Approvals` sender; the subject names the workspace, definition, outcome and reviewer, and the body links to the workspace's approvals. When the reviewer left a comment the body quotes it as plain text under a label naming the reviewer, as [text G5 did not write](../software_architecture/email.g5.md#g5.email.authored-content). The owner's account [notification preference](users.g5.md#g5.user.notifications) — the same one that governs request emails — is honored before the mail is queued and re-checked at delivery, which also cancels the mail once the owner is no longer an active member of the organization or their account email has changed. Like the request email's check, the delivery check never reads approval or workspace records, so a workspace handed to a new owner in the moments before delivery may still be emailed to the previous one. It follows the request email's best-effort handoff after the decision commits: a disabled deployment, a [volume limit](../software_architecture/email.g5.md#g5.email.volume-limits) of 500 per organization and 50 per recipient in a rolling 24 hours, or a queue failure never affects the recorded decision, and the Approvals page remains the system of record.

<a id="g5.governance.approval.request-status"></a>
[D · g5.governance.approval.request-status]

Retain each reviewer’s latest decision with its authoritative time alongside newer active Waiting requests. The compact roster lists only configured approvers who received a request, agent first then humans alphabetically, with outlined person/bot type icons; each shows its latest request and, beneath an unsettled one, its latest earlier settled response with that response's own commit evidence and summary. Placeholder rows for unrequested approvers are absent. Beneath each reviewer, that row's own request note is a compact Request note from its requester disclosure (Requester unavailable when unknown), attached to its individual request even when other requests carry the same text; it starts open only when the request's recipient user ID is the signed-in viewer's, keeps the viewer's manual choice across refresh and adds no container without a note. A human comment reads Reviewer response: and an agent summary Agent response:. A roster report link is a compact high-contrast Read report button with a document icon, while Request again, Retry and Cancel request stay text links. Request again belongs only to the latest settled Rejected or Withdrawn response, right-aligned on its response line, or at the end of its decision row when it has no response, so the decision row keeps Read report; Cancel request keeps its existing permission on Waiting rows; failed agent attempts keep their separate Retry. Request outstanding approvals sits on a non-Approved requirement's title row only while its request needs name a reviewer without an active request. Preserve earlier requests and decisions in history; operational failures and cancellations remain distinct from rejection. Meeting an any-reviewer quorum does not settle another person's pending request. A pending request that a [merge with override](workspaces.g5.md#g5.workspace.merge.admin-override.formal) leaves undecided closes when that merge lands and reads Bypassed by override, distinct from decisions, cancellation and expiry; it never counts as approval.

<a id="g5.governance.approval.quorum"></a>
[D · g5.governance.approval.quorum]

Each append-only definition configures an optional agent (harness, model, effort and prompt) and zero or more humans, with at least one approver in total. Its quorum covers both the agent and eligible humans. Any-of, the default, requires at least one yes and no eligible no. All requires every eligible configured approver's yes. Human approvals may override an agent rejection by default, only in Any-of: a human yes overrides the agent no, but a human no always blocks. Self approval defaults to disallowed and contributes neither a yes nor a no when disallowed. A human-only requirement with no eligible reviewer remains pending. Quorum, human override and self-approval settings require expanding Advanced; the phase recommendation remains in the regular form. Editing pre-fills explicit saved policy choices rather than resetting them on unrelated text changes. Each request and personal checkpoint remains independent even when quorum is met.

Reason: the October 16 human follow-up makes the configured agent part of the quorum and adds an explicit human override, while preserving addressed human authority and agent evidence eligibility. Existing Any-of definitions use the combined rule; historical versions and decisions are not rewritten.

<a id="g5.governance.approval.version"></a>
[D · g5.governance.approval.version]

Record the complete pushed repository vector explicitly selected by the approver. Human decision APIs require that vector; omission or null never selects the current heads. An explicitly selected recorded older revision remains that revision. A push during review never silently changes it. Show which repositories changed afterward and retain each reviewer's last submitted PR/type checkpoint, independently of engineering phase. Whether a later push affects status belongs to the [source-commit policy](#g5.governance.approval.source-commit-policy); changed-revision evidence alone never decides it.

<a id="g5.governance.approval.selection"></a>
[D · g5.governance.approval.selection]

Only the current owner requests reviews through one human/agent composer with modes Request all, Request agents only and Pick approvals and approvers. Bulk modes skip Approved requirements unless Include satisfied approval requirements is checked; Pick follows explicit checkboxes only. Request all sends the evaluator's [request needs](#g5.governance.approval.status): for Any-of, a never-requested configured agent first; every rejecting human again; a new agent review after an agent rejection that humans cannot override; one random configured human when only humans are configured, or after an overridable agent rejection when no human was ever requested. For All it sends every eligible approver still needing to approve. Request agents only sends agents with no active request. No mode duplicates an active request. Resolve the random choice from current eligible candidates and display the fixed result before sending. Report per-recipient outcomes and retry only failures using stable operation identities and unchanged recipients/inputs. The [request PR rule](#g5.governance.approval.request-pr) and all admission policies remain authoritative.

<a id="g5.governance.approval.request-composer"></a>
[D · g5.governance.approval.request-composer]

Focus the request dialog on new selections. A page-level open starts with no mode selected, no Selected approvals and Review request disabled; Request again and Request outstanding approvals open Pick with exactly their reviewer checkboxes checked and Selected approvals shown. Include satisfied approval requirements is always shown and disabled in Pick. The compact ▾ request-mode dropdown has a visible Request mode label above it and reads Select until a mode is chosen; its existing themed choices remain unchanged and its menu may be wider than the trigger. Selected approvals lists only chosen requirement → recipient pairs, without an overall status badge; info actions explain modes and automatic choices, bolding the checkbox and mode names. Keep the optional human note across mode and selection changes. Review request leads to one Submit; full success closes the dialog and refreshes the roster, while failed or unconfirmed recipients keep an actionable retry. Never show Accepted as a decision or a disabled retry with nothing to retry. The dialog has × and outside-click dismissal.

<a id="g5.governance.approval.overview"></a>
[D · g5.governance.approval.overview]

Use three equally prominent sections: Approvals Required to merge into the configured parent with Request, Reviews with eligible non-owner Start Review, and Configure Approvals Required to merge into this workspace. A [personal request banner](#g5.governance.approval.personal-requests) without links leads the page. While the workspace's open pull request into that parent has [merge when ready](../software_architecture/git.g5.md#g5.git.pr-auto-merge) on, one muted line beneath the first section's heading tells every visitor that G5 merges once checks pass and approvals are met; it says nothing while that state is unread, and Prepare to Ship keeps the switch. List required approvals alphabetically, each heading showing its name (opening its description and policy in a popup), its [overall status](#g5.governance.approval.status) (Needs Human Escalation in place of Rejected under the agent-escalation display, whose agent row reads the same or Overridden by Human) and one aggregate commit-currentness icon, then its path to approval and the [decision/wait presentation](#g5.governance.approval.request-status); keep authorized requirement administration visible in the final section. Keep workspace commit/push controls on Source Control rather than Approvals. The sections compose [approval timelines](#g5.governance.approval.timeline), [decision evidence](#g5.governance.approval.evidence), [review submissions/items](#g5.governance.review.items), [workspace comments](#g5.governance.review.comments) and [review navigation](#g5.governance.review.navigation).

<a id="g5.governance.approval.personal-requests"></a>
[D · g5.governance.approval.personal-requests]

The workspace Overview and the Approvals page show the signed-in person a banner of the required approvals whose active request addressed to them they may decide now, taken from the same workspace approvals projection as each requirement's status, so a requirement met by someone else does not hide a still-actionable personal request. It reads `1 approval requested from you` or `N approvals requested from you`, counting the whole personal set, and names the approvals. A known empty set omits it; loading or an unreadable projection says so inside the banner, never zero. On the Overview it sits below the phase navigation and above the summary line, shows the first three names with a route to the rest and a prominent Go to Approvals action; on Approvals the names are plain text with no navigation or other controls, since the page itself reports a failed refresh. It never submits a decision or creates a request.

<a id="g5.governance.approval.timeline"></a>
[D · g5.governance.approval.timeline]

Each requested approval has a collapsed, indented Timeline disclosure after its requested approvers; expanding it shows the existing complete request/decision timeline, filters, cards and Technical details, grouped by reviewer with newest entries and events first. Requested or decided approvals absent from the current parent requirements sit below the required list in a Legacy approvals section, collapsed by default. Include every recorded decision in this read model, including earlier decisions on a request and legacy decisions lacking request details; identify unknown request state without inventing a request or report association.

<a id="g5.governance.approval.evidence"></a>
[D · g5.governance.approval.evidence]

Display each decision's actor, definition/version, summary, exact report and [recorded pushed vector](#g5.governance.approval.version). Every settled review's clickable matching/changed/unknown comparison describes version currentness independently of outcome and merge validity, one row per repository. Aggregate and per-decision indicators share one icon family: circled = in neutral blue, circled ≠ in amber and circled ? in gray, with no indicator without counted evidence. Commit, description and report popups open near their trigger within the viewport, scroll long content, and dismiss with ×, outside click and Escape. Disclose each repository's reviewed/current source commits and available recorded target/base context. Missing evidence remains unknown. Read exact retained reports through the [report view](#g5.governance.approval.report-view).

<a id="g5.governance.approval.report-view"></a>
[D · g5.governance.approval.report-view]

Open the exact retained report in a separate read-only tab with rendered Markdown and raw source modes, reusing the existing Markdown displayer. The rendered mode shows a top-level Governing prompt section as a keyboard-operable disclosure that keeps its section heading, collapsed by default, while the verdict and later sections stay open; reports without it render unchanged. Preserve original bytes in raw mode and existing source-reader authorization. Report links carry their source organization, and reads wait for its adoption through the existing authentication owner. Do not expose reviewed-commit browsing in this report surface.

<a id="g5.governance.review.items"></a>
[D · g5.governance.review.items]

Review counts distinguish shared review submissions (pushed rounds or direct doc/file annotation publications) from unresolved individual items. Items may be suggested edits, doc/file comments or other review evidence. An overall-summary/verdict-only review and a review whose items are all closed can have zero open items. General workspace comments are separate from review publications. Explain these terms through [contextual help](#g5.governance.review.help). Reviews shows the current commit's open text changes to Markdown files by file and paragraph across every review, as the document shows them: each edited paragraph once with every reviewer's changes marked, the paragraphs around it for context, the decision cards under it naming their reviewer, and each connected set of conflicting changes as one box in which one alternative is previewed ([placement](documents/discussion.g5.md#g5.documents.review-placement), [alternatives](documents/discussion.g5.md#g5.documents.review-alternatives)). The review sections keep the reviewer's verdict, summary, findings and decided items and name how many of their changes are shown with their paragraphs. Outdated commits' rounds, changes to other files, comments, artifacts, new files, and a file that is still loading or cannot be read keep the change alone on its card.

<a id="g5.governance.review.help"></a>
[D · g5.governance.review.help]

Put the meanings of open items, published reviews and workspace comments behind one labeled help icon beside Reviews, closed by default. Reuse the shared contextual-help disclosure for this kind of optional explanation, supporting click, keyboard, Escape and outside-click dismissal.

<a id="g5.governance.review.comments"></a>
[D · g5.governance.review.comments]

Create general workspace comments through one action in the Reviews header, reusing their shared threads and drafts. Keep it discoverable while details are folded; opening the composer reveals Reviews. Activity links there instead of mounting another general comment composer. Contextual doc/file comments remain next to their content.

<a id="g5.governance.review.navigation"></a>
[D · g5.governance.review.navigation]

Reviews folds its detailed content by default. Deep links reveal the addressed review, suggestion or workspace comment, and folding preserves drafts.

<a id="g5.governance.approval.targetless"></a>
[D · g5.governance.approval.targetless]

Without a merge target, explain that absence instead of inventing outgoing requirements; retain the incoming configuration section from the [overview](#g5.governance.approval.overview).

## Activity feed

<a id="g5.governance.approval.activity-feed"></a>
[D · g5.governance.approval.activity-feed]

An org-wide approval activity feed reports approval events over a time window to the reader, across every workspace the reader may currently read under [agent read scope](../concepts/security.g5.md#g5.security.agent-read-scope): every ordinary workspace and the reader's own personal workspace, every personal workspace too for an administrator, with Base, formal review workspaces and deleted workspaces excluded, so the feed shows only events the reader could reach by opening each workspace's Approvals page. It is a read projection that starts no compute, records nothing and is not audited; changed membership or role takes effect on the next call. Each event is one of: a request made (recipient, requester, definition version, bound PR and [note](#g5.governance.approval.request-note)); a decision recorded (reviewer or the agent, outcome, comment, reviewed heads, the review for a formal agent decision, the request it settled where one exists, and the request-to-decision latency); or a request closed without a decision, naming its terminal state (cancelled, expired, superseded or bypassed), a withdrawal appearing as a cancellation. Events are newest first with stable keyset paging that neither repeats nor skips. A rollup over the same filtered events reports totals and per-definition, per-reviewer and per-workspace counts with request-to-decision latency and the oldest still-pending request; it reports raw events only and not [requirement status](#g5.governance.approval.status) changes. The request note, the decision comment and the agent's review summary are [text G5 did not write](../software_architecture/email.g5.md#g5.email.authored-content): they are labelled untrusted data in every output and are never instructions. A single call's window is bounded, so an unbounded or oversized request is refused rather than scanning without limit.

## Revision evidence

<a id="g5.governance.review.agent-comments-retired"></a>
[I · g5.governance.review.agent-comments-retired]

G5 provides no agent-originated review-comment handoff, review-round discussion or
decision tools, or UI action that delegates suggestion decisions to an agent.
Remove their runtime configuration, stored agent review records and agent-specific
schema. Preserve human reviews in shared tables. GitHub issue/PR comment commands
and formal agent approvals retain their separate owners and behavior.

<a id="g5.workspace.review-revisions"></a>
[D · g5.workspace.review-revisions]

Saved workspace base means the immutable fork manifest; parent means the readable
parent's current pushed vector. Named review comparisons, ordinary durable file/docs
reads and durable Changes metadata use pushed Git refs. Stop receipts preserve work for
resume but do not publish dirty paths, local heads or snapshot OIDs for browsing.
Current, uncommitted and all-unpushed inspection uses only the
running owner's existing runtime authority. No arbitrary ref picker or new compute wakeup
is introduced. The publication journal observes complete accepted vectors, preserving
unchanged members' push times and marking missing evidence unknown. It never becomes
head authority. Immutable Git evidence refs retain journal members through rewrites and
object collection. Readable inherited observations stop at the child's fork capture.

## Metadata

### g5.governance.approval.requirements

- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); [current design, scoped checks and remaining Cloud qualification](../../../docs/review-flow.md#october-15-workspace-wide-approval-amendment). Earlier phase-contract receipts remain historical.
- Override waiver: human-verified from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873); implemented (`merge_override::apply_override`).

### g5.governance.approval.status

- Amended by [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)): agent-escalation display, owner-selected 2026-11-04.
- human-provided
- Source: [issue #974](https://github.com/G5Labs/G5-Lean/issues/974), shared status contract and exact status copy.
- Design: [issue 974 plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md).

### g5.governance.approval.source-commit-policy

- human-provided
- Source: [issue #974](https://github.com/G5Labs/G5-Lean/issues/974), commit-vector policy; IGNORE selected 2026-10-30.

### g5.governance.approval.request-pr

- human-provided
- Source: [issue #974](https://github.com/G5Labs/G5-Lean/issues/974), PR requirement for requesting approval; `PR_REQUIRED = false` selected 2026-10-30.
- agent
- Assumption: no-PR human evidence is not PR-scoped, like agent evidence ([plan assumption 1](../../pr_ontology_diff_design/issue-974/ontology-plan.md#assumptions)).

### g5.governance.approval.request

- Member-deactivation integration preserves current activity through recipient assignment and retains history; see [the same design](../../../docs/review-flow.md#october-20-member-deactivation-integration). Local real HTTP/PostgreSQL denial/history and shared-dialog checks pass; new-head Cloud qualification remains pending.

- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); [current design, scoped checks and remaining Cloud qualification](../../../docs/review-flow.md#october-15-workspace-wide-approval-amendment). Earlier phase-contract receipts remain historical.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 29 (WS-Q6).

### g5.governance.approval.request-note

- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); [current design, scoped checks and remaining Cloud qualification](../../../docs/review-flow.md#october-15-workspace-wide-approval-amendment). Earlier phase-contract receipts remain historical.

### g5.governance.approval.request-email

- human-provided
- Owner decisions — best-effort post-commit queuing, account-wide preference, deferred one-click unsubscribe — from [issue #991](https://github.com/G5Labs/G5-Lean/issues/991), recorded in [the chat intent](../../chat_intent/issue-991/request.md) and [design](../../pr_ontology_diff_design/issue-991/ontology-plan.md). The best-effort (non-atomic) handoff is forced by the tenant/system pool split and approved there; the Home attention list remains the durable signal.
- agent
- The delivery-time eligibility re-checks (still an active member, unchanged recipient address) and the per-organization/per-recipient volume limits are the agent's reading, mirroring owner email.
- human-provided
- [Owner follow-up](../../chat_intent/issue-991/request.md#owner-follow-up-2026-10-31-preview-check) (2026-10-31, preview check of PR #997): include the request note, if any, and a reasonable amount of the PR description, and point the footer at the setting's actual place (the Settings page's Notifications section, not account settings).
- human-provided
- [Owner decision](../../chat_intent/issue-991/request.md#owner-follow-up-2026-11-02-security-review) (2026-11-02), after a reviewer's security rejection of a definer lookup that read approval requests across organizations for the delivery check: drop the still-pending re-check at delivery — "I'm not concerned about a tiny race in email sending if it buys a simple security story."
- agent
- Using the canonical description (saved, else Project Doc, no provider call), the 30-line/1,500-character bound, labelled quoting and reading under the requester's scope (the system role cannot read workspace PRs) are the agent's reading.
- human-provided
- [Owner follow-up](../../chat_intent/issue-991/request.md#owner-follow-up-2026-10-31-markdown-description) (2026-10-31): render the PR description as Markdown; the note stays plain text.
- agent
- Cutting only between whole top-level blocks or top-level list items, the plain-text fallback when no block fits or the subset refuses, and putting the organization's repositories in link scope (as for owner email) are the agent's reading.
- Evidence: `email::markdown::tests::leading_blocks_keep_whole_top_level_blocks`, the `email::render::tests::approval_request_*` tests and `approval_request_email::the_email_quotes_the_note_and_pull_request_description`.

### g5.governance.approval.decision-email

- human-provided
- [Owner follow-up](../../chat_intent/issue-991/request.md#owner-follow-up-2026-11-02-decision-email) (2026-11-02, PR #997): also email the workspace owner when a human approval occurs; email on approve and request changes; reuse the existing preference; land it in PR #997.
- agent
- Excluding withdrawals and the owner's own decision, the current owner as recipient, the single-use request key, quoting the reviewer's comment, linking the workspace's approvals and reusing the request email's sender, subject prefix, limits and recipient-only delivery check are the agent's reading.
- Evidence: the `email::render::tests::approval_decision_*` test and the `approval_request_email` integration tests `each_decision_emails_the_owner_once`, `a_withdrawal_sends_nothing`, `the_owners_own_decision_sends_nothing`, `the_owners_preference_governs_decision_emails`, `the_gate_cancels_when_the_owners_email_changes` and `email_disabled_still_records_the_decision`.

### g5.governance.approval.request-status

- Amended by [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)): per-request note disclosures, response labels and the prominent report action.
- Amended by [issue #974](https://github.com/G5Labs/G5-Lean/issues/974) ([plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md)).
- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); [current design, scoped checks and remaining Cloud qualification](../../../docs/review-flow.md#october-15-workspace-wide-approval-amendment). Earlier phase-contract receipts remain historical.
- Bypassed by override: human-verified from [issue #873](https://github.com/G5Labs/G5-Lean/issues/873) ([design](../../pr_ontology_diff_design/issue-873/ontology-plan.md)); implemented (migration 133 state `bypassed`, closed at landing).

### g5.governance.approval.quorum

- human-provided; scoped implementation decisions are recorded in the existing design.
- [October 16 combined approver policy and All clarification](../../chat_intent/qa-review-prototype/request.md#october-16-combined-approver-policy); [design amendment and scoped verification](../../../docs/review-flow.md#october-16-combined-approver-amendment). Implemented in the existing definition service and shared requirement evaluator with additive migration099 after the October 19 rebase. The 13-case real API outcome regression now also exercises the actual locked merge gate; affected suites and the real browser configuration round-trip pass locally. Fresh Claude round 1 finds no significant supported production findings; the subsequent test-only gate assertion is independently self-verified. New-head Cloud CI remains pending; earlier independent-agent receipts do not establish this rule.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); [current design, scoped checks and remaining Cloud qualification](../../../docs/review-flow.md#october-15-workspace-wide-approval-amendment). Earlier phase-contract receipts remain historical.

- October 19 integration: the 13 outcome cases now use main’s formal coordinator through evidence persistence and sealing, then compare outgoing state with locked merge admission. Current compilation and affected HTTP/Postgres checks pass; current-head Cloud qualification is pending in [the same design](../../../docs/review-flow.md#october-19-latest-main-integration).

### g5.governance.approval.version

- Amended by [issue #974](https://github.com/G5Labs/G5-Lean/issues/974) ([plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md)).
- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); [current design, scoped checks and remaining Cloud qualification](../../../docs/review-flow.md#october-15-workspace-wide-approval-amendment). Earlier phase-contract receipts remain historical.
- Explicit API input: [issue #697](https://github.com/G5Labs/G5-Lean/issues/697); [implementation plan](../../pr_ontology_diff_design/issue-697/ontology-plan.md). The owner approved implementation on 2026-10-28. The route and service require the explicit complete vector, retain valid historical selections and preserve explicit-vector settlement fingerprints. Scoped execution evidence and browser qualification references are recorded in the plan.

### g5.governance.approval.selection

- Amended by [issue #974](https://github.com/G5Labs/G5-Lean/issues/974) ([plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md)).
- human-provided: [issue 704](https://github.com/G5Labs/G5-Lean/issues/704), including the three confirmed selection rules.
- Design: [unified Approvals](../../pr_ontology_diff_design/issue-704/ontology-plan.md).

### g5.governance.approval.request-composer

- Amended by [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)): final Request mode / Select wording.
- Amended by [issue #974](https://github.com/G5Labs/G5-Lean/issues/974) ([plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md)).
- human-provided: [preview follow-up](../../chat_intent/issue-704/approvals-follow-up.md) requests new approvals rather than previous-request clutter.
- agent: retain failed/unconfirmed operation identities and receipts while clearing completed batches.

### g5.governance.approval.overview

- Amended by [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)): personal banner and escalation badges.
- Source for the merge when ready line: [owner request and choice](../../chat_intent/issue-1095/merge-when-ready-note.md) (2026-11-06): visible on Approvals to every visitor, the one-line option under the heading, chosen from rendered mocks.
- Amended by [issue #974](https://github.com/G5Labs/G5-Lean/issues/974) ([plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md)).
- human-provided: [issue 704](https://github.com/G5Labs/G5-Lean/issues/704) and [follow-up](../../chat_intent/issue-704/approvals-follow-up.md) specify the three sections and merge wording.
- Design: [unified Approvals](../../pr_ontology_diff_design/issue-704/ontology-plan.md).

### g5.governance.approval.personal-requests

- human-provided
- Source: [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) sections 4 and 10; the owner selected banner A on 2026-11-04.
- Design (agent): reuses the projection's caller-decidable bit (the same addressed-request rule as Home); [plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md).

### g5.governance.approval.timeline

- Amended by [issue #974](https://github.com/G5Labs/G5-Lean/issues/974) ([plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md)).
- human-provided: [follow-up](../../chat_intent/issue-704/approvals-follow-up.md) requests per-approval history grouped by reviewer newest first, and collapsed legacy approvals.
- agent: retain all requested/decided approvals absent from the current required list without inferring an unavailable past configuration; preserve legacy decisions lacking request records.

### g5.governance.approval.evidence

- Amended by [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)): owner-selected currentness icon family.
- Amended by [issue #974](https://github.com/G5Labs/G5-Lean/issues/974) ([plan](../../pr_ontology_diff_design/issue-974/ontology-plan.md)).
- human-provided: [issue 704](https://github.com/G5Labs/G5-Lean/issues/704) specifies currentness independently of decision outcome and merge validity, retaining exact actor/version/report evidence.

### g5.governance.approval.report-view

- Amended by [issue #1095](https://github.com/G5Labs/G5-Lean/issues/1095) ([plan](../../pr_ontology_diff_design/issue-1095/ontology-plan.md)): Governing prompt collapsed in the rendered view.
- human-provided: [preview follow-up](../../chat_intent/issue-704/approvals-follow-up.md) requires a separate read-only report tab with raw/view modes and no reviewed-commit browser.
- agent: carry source-organization context into standalone links, adopt it through the existing authentication owner before reading, and isolate report routes from workspace content navigation.

### g5.governance.review.items

- agent: explanation of the existing publication/item model requested by the [follow-up](../../chat_intent/issue-704/approvals-follow-up.md); publication and suggestion owners retain their semantics.
- human-verified: the paragraph display was the owner's choice on 2026-10-31, a screenshot of the mock with C, cards under the paragraph and the surrounding paragraphs selected; the owner has not reviewed the implementation ([request and choices](../../chat_intent/kaihui-suggestion-mode-improvement/cards-out-of-document.md#second-request-the-reviews-panel-2026-10-31), question set `reviews-panel-suggestion-context` in Project Docs, recorded as proposals for confirmation); [design](../../pr_ontology_diff_design/kaihui-suggestion-mode-improvement/ontology-plan.md#the-reviews-panel-paragraphs-with-their-changes-2026-10-31). `ReviewFileSections.tsx` owns a file's section; `ReviewsPanel.tsx` decides which changes it holds.

### g5.governance.review.help

- human-provided: [preview follow-up](../../chat_intent/issue-704/approvals-follow-up.md) requests one contextual-help icon or tooltip instead of default explanations.
- agent: use a shared accessible disclosure so keyboard and touch users can reach the same information.

### g5.governance.review.comments

- human-provided: [follow-up](../../chat_intent/issue-704/approvals-follow-up.md) requests one general comment creation destination.
- agent: preserve contextual doc/file comment owners, shared threads and browser drafts.

### g5.governance.review.navigation

- human-provided: [issue 704](https://github.com/G5Labs/G5-Lean/issues/704) requires folded Reviews, deep-link reveal and preserved drafts.

### g5.governance.approval.targetless

- human-provided: [issue 704](https://github.com/G5Labs/G5-Lean/issues/704) requires incoming administration without invented outgoing requirements on target-less workspaces.

### g5.governance.approval.activity-feed

- human-provided
- The owner's [issue #1099](https://github.com/G5Labs/G5-Lean/issues/1099) (decisions 2026-11-04, [plan](../../pr_ontology_diff_design/issue-1099-approval-activity-feed/ontology-plan.md)): an agent should report approvals requested and fulfilled across the organization over a window. The owner chose the [agent read scope](../concepts/security.g5.md#g5.security.agent-read-scope) over the narrower [agent-inspection scope](agent_approvals.g5.md#g5.agent-approval.human-read-only.agent-inspection) and over admin-only, deferred requirement-status rollups, and kept notes/comments full in machine output and truncated in the table.
- The feed reuses the readable-workspace audience that [`g5d pr list --all`](../software_architecture/git.g5.md#g5.git.pr-reads) applies (`services/git/workspace_audience.rs`, shared with `workspace_pull_request_read.rs` rather than copied), in its live form so feed membership equals the per-workspace drill-in audience. Exposed as `g5d workspace approval-activity`; implementation in `services/workspace/approval_activity.rs` over the agent route `GET /workspace-agent/:id/approval-activity`, with new time-window indexes.

### g5.governance.review.agent-comments-retired

- human-provided: [owner removal instruction](../../chat_intent/workspace-colab-harden/remove-agent-review-comments.md).
- agent: no table is exclusive to agent comments; remove their records and schema
  while preserving human reviews, as scoped in the [active plan](../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#remove-agent-review-comments--authorized-implementation).

### g5.workspace.review-revisions

- human-provided; scoped implementation decisions are recorded in the existing design.
- [issue447 human direction](../../chat_intent/qa-review-prototype/request.md); actual Git/API and owner/reviewer vector, checkpoint, privacy and history tests are recorded in [the same design](../../../docs/review-flow.md). Recovery snapshots retain their [separate role](../software_architecture/git.g5.md#g5.git.saved-read).
