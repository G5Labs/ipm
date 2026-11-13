# PR #1111 review follow-ups, 2026-11-06

The owner instructed: “Ship migration 151 through the normal deploy like every other migration.” Remove the retirement-specific deployment document, Helm traffic switch and its test; use the existing deployment workflow. This supersedes the earlier deployment proposal. Merge when ready was turned off by a person; the authorized follow-up is implementation and exact-head CI, not merging or deploying.

The owner corrected the environment inventory: Lean QA is at 150; G5-Lean staging (`g5lean-staging-1ccc`) and production (`g5lean-prod-1ccc`) run the Lean catalog, with Cloud Logging evidence dated 2026-11-05 and 2026-10-30 respectively. Their normal `g5-promote-staging`/`g5-promote-prod` paths will apply 151. This is owner-supplied evidence, not a fresh live inspection by this agent.

Keep preflight refusal for invalid tenants and never select an Admin automatically. Document rebuilding disposable dev volumes containing the old membership-less `g5labs-unassigned` tenant. Add the requested HTTP self-reduction race, former-Owner workspace-transfer/offboarding, restricted migrator refusal, restored schema CREATE grant and event-synchronized cancellation regressions.

## Scoped rebase, 2026-11-07

The owner instructed that main owns migrations 151 (`approval_activity_indexes`) and 152 (`preview_start_consent`), both applied in QA. Renumber the retirement conversion and its upgrade test to 153, preserving main's applied bytes and checksums. Take main's #1118 override-test fixes and drop the duplicate PR fixes. Keep the PR-author audience query in main's shared helper and use only `role = 'admin'`.

Keep the former-Owner HTTP 409 while they own workspaces; remove the fixture's direct Base owner update, transfers and expected HTTP 204. Base transfer and finished transfer/fork records blocking removal already exist on main and are explicitly out of scope. Record that issue #1106's removal-after-required-transfers criterion is unmet. Push the existing branch and obtain green Rust and E2E CI on the exact head. No further production/schema changes, merge or deployment are authorized by this follow-up.
