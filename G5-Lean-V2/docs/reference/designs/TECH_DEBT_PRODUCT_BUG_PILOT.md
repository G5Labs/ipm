---
type: view
title: Product Bug Pilot — Workspace and Org Flows
status: draft
view_persona: pm
source_of_truth: designs/TECH_DEBT.md
root_spec_ids:
  - g5/client/workspace-page
  - g5/client/hooks
linked_nodes:
  - g5/client/workspace-page/org-management
  - g5/client/workspace-page/top-bar
  - g5/client/hooks
new_nodes: []
affected_source_files:
  - client/src/components/org/CreateOrgDialog.tsx
  - client/src/components/org/OrgMembersPage.tsx
  - client/src/components/org/InviteDialog.tsx
  - client/src/components/workspace/WorkspaceSwitcher.tsx
  - client/src/hooks/useOrg.ts
linked_docs:
  - designs/TECH_DEBT.md
---

# Product Bug Pilot — Workspace and Org Flows

Status: Draft
Purpose: Evaluate downlift against concrete, user-visible product bugs rather than architecture-heavy sync guidance.
Source of truth: `designs/TECH_DEBT.md`

## Goal

Use a small set of exact bugs with clear affected files and observable acceptance checks. A successful downlift for this pilot should produce concrete code deltas, not only spec rewrites.

## Included Bugs

### 1. Invalid org-members routes must not expose a live invite flow

Source: `TECH_DEBT.md` §38.1311

Affected files:
- `client/src/components/org/OrgMembersPage.tsx`
- `client/src/hooks/useOrg.ts`

Required correction:
- Treat a missing `currentOrg` as an invalid-route state before rendering org-scoped admin affordances.
- Do not show `Invite +` when the route org cannot be resolved.
- Do not open `InviteDialog` with `orgId=""`.
- Do not issue requests to `/api/v1/orgs//members`.

Acceptance checks:
- Visiting a stale `/:org/settings/members` slug renders an invalid-route or empty-state screen, not live admin actions.
- The invite button is absent or disabled when no resolved org exists.
- No member/invitation mutation can fire with an empty org id.

### 2. Create-org wizard navigation must freeze during async step mutations

Source: `TECH_DEBT.md` §38.1320

Affected files:
- `client/src/components/org/CreateOrgDialog.tsx`

Required correction:
- Disable Back / Skip / Next while repo import is in flight.
- Disable Back / Skip / Next while invite sending is in flight.
- Prevent stale async callbacks from advancing a later step after the user has already navigated elsewhere.

Acceptance checks:
- Starting repo import prevents step-navigation actions until the import settles.
- Starting invite sending prevents step-navigation actions until the send batch settles.
- Completing an earlier async action cannot unexpectedly jump the wizard forward after the user has already changed steps.

### 3. Workspace switcher clear-search must reset keyboard selection

Source: `TECH_DEBT.md` §38.1313

Affected files:
- `client/src/components/workspace/WorkspaceSwitcher.tsx`

Required correction:
- Reset `highlightedIndex` when the clear-search button is used.
- Return the switcher to a neutral no-selection state after clear-search.

Acceptance checks:
- Narrow list, move keyboard selection, clear search, then press Enter: no unrelated workspace is activated.
- Visual highlight and keyboard target stay aligned with the current filtered list.

### 4. Workspace switcher must distinguish loading and error from true empty state

Source: `TECH_DEBT.md` §38.1318

Affected files:
- `client/src/components/workspace/WorkspaceSwitcher.tsx`

Required correction:
- Preserve `loading` and `error` as first-class render states.
- Reserve “No workspaces found” for the true loaded-and-empty case.

Acceptance checks:
- Initial load shows a loading state, not “No workspaces found.”
- Fetch failure shows an error state, not “No workspaces found.”
- Empty-org state still renders honest empty-state copy once loading succeeds.

### 5. Workspace switcher Enter handling must not hijack nested controls

Source: `TECH_DEBT.md` §38.1319

Affected files:
- `client/src/components/workspace/WorkspaceSwitcher.tsx`

Required correction:
- Scope Enter-to-select behavior to the list interaction model.
- Let clear-search and `New Project` keep their own keyboard activation semantics.

Acceptance checks:
- Pressing Enter on the clear-search control clears search and does not select a workspace.
- Pressing Enter on `New Project` activates that control and does not select a workspace.

## Implementation Mapping

| View slice | Linked node | Files | Expected delta |
|------------|-------------|-------|----------------|
| Invalid org-members routes must not expose a live invite flow | g5/client/workspace-page/org-management | `client/src/components/org/OrgMembersPage.tsx`, `client/src/components/org/InviteDialog.tsx` | Guard invalid org routes, suppress invite affordances when `currentOrg` is unresolved, and prevent dialog activation with an empty org id |
| Invalid org-members routes must not expose a live invite flow | g5/client/hooks | `client/src/hooks/useOrg.ts` | Refuse invite/member mutations when `orgId` is empty so `/api/v1/orgs//members` cannot be issued |
| Create-org wizard navigation must freeze during async step mutations | g5/client/workspace-page/org-management | `client/src/components/org/CreateOrgDialog.tsx` | Freeze Back / Skip / Next during in-flight repo import and invite sends, and prevent stale async completions from advancing later steps |
| Workspace switcher clear-search, state rendering, and Enter handling | g5/client/workspace-page/top-bar | `client/src/components/workspace/WorkspaceSwitcher.tsx` | Reset keyboard selection on clear, distinguish loading/error from true empty state, and keep nested controls from triggering workspace selection on Enter |

## Downlift Expectation

This pilot should prefer code edits first, with spec updates only where they document the shipped fix. A run that only rewrites specs without changing the affected source files should be treated as a failure for evaluation purposes.

## Suggested Evaluation Order

1. `WorkspaceSwitcher.tsx`
2. `CreateOrgDialog.tsx`
3. `OrgMembersPage.tsx` and `useOrg.ts`

## Success Criteria

- The durable job branch contains edits to the affected source files above.
- Relevant specs may be updated, but only alongside concrete code changes.
- The final diff can be explained directly in terms of the five bugs above.
