---
name: issue-776-platform-operator
description: G5-Lean#776 platform operator org: owner decisions, what shipped on branch issue-776-platform-operator-a-well-known-operator, open follow-ups.
metadata:
  type: project
---

Owner decisions (2026-10-24, chat): operator org slug `/operator`, keeps Base+docs; operator
creates orgs for an owner email (account created if absent); platform seed
(`scripts/seed-prod-org.sh`) provisions only the operator org + DEFAULT_ADMIN_EMAIL admin
invitation (superseded the first "sign-in bootstrap" answer); self-serve org creation
removed; login allowlist retired (invitation/membership-only admission). Owner said
"complete implementation" after reading the design.

PR: G5Labs/G5-Lean#811 (G5 PR 97dcc9ba), opened 2026-10-24; migration is 122 after rebasing over main's 121_user_onboarding (2026-10-27; 121 after 117–120 earlier on 2026-10-27; 117 after 114–116 on 2026-10-26). The canary workflow edit was left out (G5 GitHub App lacks Workflows permission).

Design: `.g5/pr_ontology_diff_design/issue-776/ontology-plan.md`; intent:
`.g5/chat_intent/issue-776/request.md`.

Dev/preview (2026-10-26, owner: "match prod, give Alex operator membership"):
`scripts/seed-dev-org.sh` mints the e2e-operator fixture (creates `/operator`) and adds
Alex `demo@g5labs.ai` as admin via `POST /orgs/:id/members`; re-run PATCHes the role back.
Verified live against a native g5-api + oauth stub (`PG_CONTAINER=<pg> OAUTH_STUB_URL=...`).

Full-stack preview org switching (2026-10-26, folded into #811 at owner's choice): two
layers. (1) The previewed API must trust its preview origin: API sends `origin` in the
preview module input only to VMs on catalog `2026.10.26-workspace-preview-origin`+ (older
guests deny unknown input; `SHIPPED_MODULE_CATALOGS` must get every future catalog bump),
g5d exports `G5D_PREVIEW_ORIGIN`, dev-stack-up adds it to `G5_CORS_ALLOWED_ORIGINS`.
(2) The preview relay drops every `g5_*` Set-Cookie, so dev-stack-up sets
`G5_REFRESH_COOKIE_NAME=preview_refresh`. Symptom of (2) alone: instant 401 on refresh.
CI on 961003d all green 2026-10-26; the owner's end-to-end retry was still pending.

Rebase over main's onboarding tour (#858, 2026-10-27): the tour blocks the page outside its
card and started on any org Home, so `OnboardingTour.tsx` now hosts no step in the
operator org (its Home is the console) and addresses only tenant orgs; `ui.g5.md`
sign-in-landing now names `/no-organization`. Main's `onboarding-tour.spec.ts` used
`/create-org`; it now has the e2e-operator create the org for the fresh identity's email via
`POST /api/v1/platform/orgs` (separate request context) before the OAuth sign-in. Any new
main spec that self-serves an org or signs in an uninvited fresh identity needs the same.

**Why:** owner wants operators, not self-serve users, to create orgs.
**How to apply:** open follow-ups are the self-hosted and operations Q blocks in
organizations.g5.md, removing the dead `g5-org-id`/registration-token secrets and
`worker_tokens` table, retiring `reset-dev-environment.sh` if dev.g5labs.ai is unused, and
removing `settings.auth_allowed_emails` from live release inputs. Related:
[[live-workspaces-need-member-owner]], [[g5-api-tests-sign-in-and-org-creation]].
