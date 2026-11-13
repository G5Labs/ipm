---
name: owner-resource-limits
description: "Per-owner org limits (5 running VMs, 50 workspaces) shipped on the issue-710 branch / PR #712 on 2026-10-22; design path, enforcement points, E2E fixture lift."
metadata:
  node_type: memory
  type: project
  originSessionId: cf492e3f-8f76-4ff0-b33b-99897b7ec222
  modified: 2026-10-22T20:31:04.361Z
---

Owner asked (2026-10-22) for org defaults capping running VMs per owner and workspaces per owner as runaway-agent protection, and said to ship it on the issue-710 branch (PR #712), not a new workspace. Design: `.g5/pr_ontology_diff_design/owner-resource-limits/ontology-plan.md`; request record `.g5/chat_intent/owner-resource-limits/request.md`.

Enforcement: `WorkerHostService::place` (owner check before org quota, refuses and fails the intent with `owner_vm_limit`); `workspace_owner_limit` module used by fork `admit_operation` and transfer admission under a per-user advisory lock. Error `G5Error::OwnerLimitReached` → 409 `OWNER_LIMIT_REACHED`. Migration 118 registers the keys.

**Why:** the single E2E user owns ~110 clones per Playwright run, so test-token fixture org creation and `scripts/seed-ci-org.sh` lift both limits (100 / 1000). Persistent QA/dev orgs created before this keep the defaults — an admin must raise them if a long-lived test user exceeds 50 workspaces.

**How to apply:** if CI E2E fails with OWNER_LIMIT_REACHED, check which org the lane used and whether it was created before the lift. Related: [[issue-710-workspace-agent-parity]].
