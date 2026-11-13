---
name: workspace-settings-propagate-by-fork-copy
description: Changing a workspace_settings column DEFAULT never reaches real workspaces — forks copy the source row; to change an org default, backfill Base rows in the 071/081 RLS-owner migration form and verify on QA by forking.
metadata:
  type: feedback
---

Every working workspace is a fork (`POST /workspaces/:id/clone`) rooted at its
org's Base, and `workspace_fork_service.rs` copies every typed
`workspace_settings` field except `idle_timeout_secs` (`response_deadline_secs`,
`bounded_execution_slots`, `max_spend_usd`, token caps, `permission_policy`)
from the source row. Since branch `admin-page` (2026-10-14) `idle_timeout_secs`
instead comes from the org admin setting `configs` key `default_idle_timeout_secs`
(missing → 28800), edited on the Admin page, so that default changes through
the Admin page, not a Base backfill. A column `DEFAULT` is consulted only by inserts that omit
the column: a brand-new org's Base and the lazy bootstrap. So PR #381's
migration 072 (`SET DEFAULT 28800`) changed nothing for any existing org — the
user saw fresh forks still at 30 minutes on QA (2026-10-13).

**Why:** I verified forks copy the field and still shipped a column-default
change, then told the user the default was 8 h. The user had to discover it
was a no-op.

**How to apply:** To change an org-wide default, ship a data migration that
`UPDATE`s Base rows still on the old value (PR #502, migration 081; precedent
071), written in the straight-line `NO FORCE / UPDATE ... FROM / SET
CONSTRAINTS ALL IMMEDIATE / FORCE` owner-exemption form on **every** RLS
table the statement touches (a plain UPDATE sees zero rows for the Cloud SQL
migrator and `g5-db-migrate` refuses it), plus a populated-upgrade test
through the real runner (`migration_071_workspace_execution_slots.rs` is the
template; seed values explicitly, and seed workspaces with an owner —
`workspaces_active_owner_required`). Don't call it shipped until a fresh fork
from Base on QA shows the new value. See [[rust-and-integration-test-setup]],
[[g5d-pr-comments-and-body]].
