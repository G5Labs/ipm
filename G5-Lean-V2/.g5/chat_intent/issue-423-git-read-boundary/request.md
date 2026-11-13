# Issue 423: the repository is the unit of Git read access

## Human request

On 2026-10-10 the user asked to understand [issue 423](https://github.com/G5Labs/G5-Lean/issues/423), filed from their weekly automated audit. It reports that Git smart HTTP serves a whole shared repository to anyone who may read any workspace attached to it, so a reader of org-visible workspace A can fetch invite-only sibling B's branch and flush snapshots. The issue itself notes that repository-level Git visibility is an accepted MVP exposure (`g5.security.mvp-exposure.objects`, amendment row 9) and asks for a decision on whether workspace confidentiality is promoted into scope.

The user decided:

> well, i think this is totally fine, if one person is allowed to work on the repo, it simply means that this person can read anything in the repo

and, after the agent's summary of the consequences, asked:

> yes, close it with the decision and amend the ontology

## Agent interpretation

The agent stated, and the user confirmed with the second message, that:

- Repository read access is the boundary. A repository is readable by every member who can read any attached workspace. Since forks share their source repository and org-visible workspaces are readable by all members, this normally means org membership.
- Invite-only is a collaboration control, not a secrecy control: it hides the workspace from lists, chat, files, comments and compute views, but not its branch or flush snapshots from Git.
- Write authority is unaffected: pushes stay confined to the caller's own branch and flush namespace.
- The decision is permanent, not an MVP concession: the parked P8.12 object-views item is retired, and the empty-pack push hazard the issue describes is moot because it only matters for a per-workspace confidentiality goal.

Changes: `g5.security.mvp-exposure.objects` restated as the intended model (ID kept), `g5.scope.security-hardening.git-views` restated as not planned, the durability cross-link retargeted, retirement notes on amendment row 9 and the §6 P8.12 row, and issue 423 closed with the decision.
