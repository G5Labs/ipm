---
name: agent-other-workspaces-opt-in
description: Owner-only workspace setting (2026-10-23, default on since 2026-10-26, PR #712) that gates every workspace-agent operation on a non-self workspace; where the gate lives and how tests enable it.
metadata:
  type: project
---

`workspace_settings.agent_may_modify_other_workspaces` (migration 119, `DEFAULT true` for every workspace since the owner's 2026-10-26 change — the earlier off default and personal-only migration 121 were removed before shipping; a cross-owner fork takes the default, a same-owner fork copies, formal review and transfer set false) must be on before a workspace agent forks or touches any workspace other than its own (since 2026-10-26 that is any live same-owner workspace, not just children — see [[issue-710-workspace-agent-parity]]); refusals are 403 `AGENT_OTHER_WORKSPACES_DISABLED`. Design: `.g5/pr_ontology_diff_design/agent-other-workspaces/ontology-plan.md`.

The gate is inside `agent_target` (any target ≠ the caller) plus an explicit check in the fork route. Read paths that used `agent_target` only to classify a target now call the ungated `agent_relation`; `admit_self_or_child` gates only when its scopes include `workspaces:write`. A new child-operation route that admits through `agent_target` is gated automatically; a new *read* must use `agent_relation` or `agent_read_target`.

**Why:** the owner asked for a knob gating the dangerous cross-workspace behaviour, then made it default on; stop-all and merging into self stay ungated by choice.
**How to apply:** tests get the setting on by default; to test refusals call `set_agent_opt_in(.., false)` or the settings PUT. If this workspace's own agent gets the refusal, the owner turns it on in the workspace settings page. Related: [[issue-710-workspace-agent-parity]].
