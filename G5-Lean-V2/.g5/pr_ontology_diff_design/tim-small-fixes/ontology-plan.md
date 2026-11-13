# tim-small-fixes — design

Request: the owner asked on 2026-10-23 to "be able to preview the final changes, not as a mock".
The branch's change is the Home tile header wrap (`b2b38d931`). Until now it could be seen only in the
Project Docs HTML mocks (`docs/a09b6be0-…/mocks`, served by the `mock` preview profile).

## Governing blocks

- Amended: [`g5.runtime.boot.backend.process-local`](../../ontology/software_architecture/hosts.g5.md#g5.runtime.boot.backend.process-local).
  Frontend-only runs the real client against its in-browser mock API. That mock follows the dev stack's
  default features and layout and answers the organization Home with demo data.
- Unchanged consumers: `g5.preview.profile-label` (profiles stay distinguishable; only the
  frontend-only description changes) and the Home tile blocks in `features/ui.g5.md` and `issues.g5.md`.
  The preview renders their shipped code; nothing in their behavior changes.

## Design

- **Why frontend-only.** `full-stack` and `full-stack-no-worker` run the real API, but they advertise
  `external_issues` only with a configured GitHub App, so the changed Issues and tasks tile never appears.
  Frontend-only already runs the real client. It showed the classic Home because the MSW fixture pins
  `defaultWorkspaceLayout: both`, and it had no Home attention, compute or issue data.
- **Owner.** `frontend/src/mocks/home-preview.ts` holds the mock-mode Home handlers: GitHub user
  connection, issue domains, issue search (free terms, `label:`, assignee), create target, issue detail,
  Create, Link existing, attention, running compute and open-question metrics. It also answers the
  workspace's issue-links and activity reads from the same links, so an issue linked on Home is the
  workspace's primary issue when the workspace opens. Data is in memory and resets on reload.
- **Fixture fidelity.** The shared `/api/v1/resolve` fixture also matches a workspace name
  case-insensitively, as the server does. Before this, every Home link to a workspace with a space in
  its name (`/g5labs/Auth%20Refactor`) read "Workspace not found" in mock mode.
- **Routing.** `fetch-interceptor.ts` consults these handlers ahead of `test/msw/handlers`. It rewrites
  instance info: the dev features plus `external_issues`, and the dev stack's `alignment` default layout.
  A stored per-person layout still wins. It also reads the org's `external_issues_enabled` as on until
  the setting exists, so turning it off in Runtime Config still hides the tile. `VITE_MOCK_FEATURES`
  still replaces the added features.
- **Unit tests keep their defaults.** They import the MSW handlers directly, never this module, and keep
  the fixture's empty and `both` defaults. The name-matching fix above is their only change.

## Tests

- `npm run test:e2e:mock-preview` (new `e2e/mock-preview/`): it starts the profile's own command
  (`VITE_MOCK=true`, port 15174) and checks seven journeys:
  - all four tiles populated, with no unhandled mock route;
  - search by terms and `label:`, the no-match copy, and Clear;
  - Assigned to me and Unassigned;
  - Link existing sets the primary workspace, and Open shows it on the workspace;
  - Create lands on the issue page;
  - a created issue is listed after Back;
  - at 1024 px the header actions sit below the title as one row.
- Regression: the wrap journey fails against main's `HomeIssuesTile.tsx` and `HomeTileParts.tsx`
  (search above title bottom) and passes on this branch.
- `src/mocks/fetch-interceptor.test.ts`: the feature and layout rewrite, the Home reads, the org switch
  default and `searchMockIssues`.
- Not covered: none of these journeys runs in CI. The full-stack profiles were not started for this
  change.
