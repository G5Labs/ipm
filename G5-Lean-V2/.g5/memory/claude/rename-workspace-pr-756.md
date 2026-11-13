---
name: rename-workspace-pr-756
description: Workspace rename feature (branch rename-workspace, PR #756): design and decisions in .g5/pr_ontology_diff_design/rename-workspace; ship loop state and what is deliberately out of scope.
metadata:
  type: project
---

PR #756 "Rename workspaces without touching Git, with reserved former names"
(branch `rename-workspace`, opened 2026-10-23). Decisions (owner zcl, 2026-10-23):
reserve former names until completed deletion; keep Git-branch name syntax; allow
renaming read-only workspaces and ordinary review copies; Base and formal
agent-review workspaces are never renamed. Migration is **108** (main took 105-107
while the branch was open). Deliberately out of scope unless the owner asks:
`workspace.renamed` websocket event, Rename in the switcher/details header, a
browser rename round trip, an Activity entry (the `workspace_renames` log exists).

**How to apply:** when bot reviews arrive (`g5d pr view 756 -c`), accept only P0/P1
(Codex) and Critical/Major (CodeRabbit) findings that are reachable; reply inline to
each. Re-merge main before each push if it moved (it has twice: 105-107 migrations,
and #737's case-insensitive tab identity, both reconciled). Related:
[[migration-definer-data-section]], [[bot-review-scope-discipline]].

**State 2026-10-23 (round 2):** first g5-ci-rust failed on the service-layering
contract (Git may not name Workspace: the name rules moved to
`services/identity/workspace_names.rs` with a HOMES entry), on the populated 045
upgrade (migration 108 now installs `g5_workspace_identity_immutable()` instead of
replacing 039's function, which the migrator does not own) and on the CLI clone test
(now mocks `/api/v1/resolve`). g5-scm `native::tests::deadline_includes_pipes…` is
the known flaky case. Five CodeRabbit minor/trivial threads were fixed and answered.
Head 166efefa published; CI watch and a 30-minute review poll were armed.

**State 2026-10-23 (round 3):** g5-ci-rust green on a3bd0c98; main merged again (15
commits, clean). Two more CodeRabbit minor/trivial threads fixed and answered (CLI
test asserts the resolver query; the page's view-scope key is the workspace id so the
former-name redirect keeps in-flight actions). Head debb67d7 published; e2e lane not
yet green on any head (runs about an hour; each push restarts it). All 7 threads
resolved. Note: pushes to the proxy branch get published to the GitHub PR head on
their own, so memory-only commits also restart CI; batch them with code.

**Follow-up 2026-10-24:** #756 merged (rebase-and-merge; the branch was rebased onto
main with --force-with-lease and now continues linearly). The owner moved Rename from
Home rows to the workspace details Overview (Name row above Description, styled like
Description's Edit). Home rows are Delete and Fork again. The rename hook now updates
cached resolutions in place so the details page replaces its own URL. New PR from the
same branch; ship-loop rules unchanged.

