# Platform operator organization (issue #776)

Request: [G5Labs/G5-Lean#776](https://github.com/G5Labs/G5-Lean/issues/776); owner
decisions in [chat intent](../../chat_intent/issue-776/request.md). Governing blocks are
in [Organization › Platform operator](../../ontology/features/organizations.g5.md#platform-operator).

## Outcome

A deployment has at most one operator organization, slug `operator`. Its memberships are
the platform-operator roster, using the ordinary role, invitation and member controls.
Its Home at `/operator/home` is a platform console: every organization's platform facts
and, for operator members and above, creating an organization for a named owner. Being
an operator grants nothing inside any other organization.

## Owners and interactions

| Concern | Owner | Decision |
| --- | --- | --- |
| Which org is the operator org | `orgs.kind` (`tenant` / `operator`), partial unique index | [identity](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.identity) |
| Reserved and fixed slugs | `org_service::validate_slug` + `update_org` | [identity](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.identity) |
| Roster, roles, invites, removal | existing `OrgService` member operations, unchanged | [roles](../../ontology/features/organizations.g5.md#g5.organization.platform-operator) |
| Platform authority | `platform_operator::require_platform_operator` (current active membership in the operator org) | [authority](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.authority) |
| Cross-tenant facts | `GET /api/v1/platform/orgs` | [tenant boundary](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.tenant-boundary) |
| Org creation for an owner | `POST /api/v1/platform/orgs` → shared creation cascade | [create](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.create-org) |
| Operator org and first operator | platform seed `scripts/seed-prod-org.sh` | [bootstrap](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.bootstrap) |
| Console | `HomePage` → `PlatformHome` when the route org's kind is `operator` | [home](../../ontology/features/organizations.g5.md#g5.organization.platform-operator.home) |
| Who may create orgs | operator API only; self-serve route and UI removed | [creation](../../ontology/features/organizations.g5.md#g5.organization.creation) |
| Who may sign in | `AuthService::handle_callback`: pending invitation or membership | [admission](../../ontology/features/users.g5.md#g5.user.admission) |

### Why a kind column, not the slug alone

Slug reservation is enforced only by the application's tenant writers. Authority must
never derive from a name a tenant path could set, so the flag, which only the platform
seed writes, identifies the operator org, and the partial unique index keeps it single.
The reserved slug only keeps the URL well known.

No deployment has an `operator` org today (owner, 2026-10-24), so there is no collision
handling: if the slug were somehow taken, the seed fails on the slug's unique constraint.

### Bootstrap: platform seeding provisions the operator org

The owner asked that platform seeding provision only the operator org (2026-10-24).
`scripts/seed-prod-org.sh` is the platform seed for cloud environments. Today it creates
the `g5` tenant org, which QA uses as G5's own development org. Its rows are written
directly in SQL by a non-login service user (`worker@g5.internal`), and
`DEFAULT_ADMIN_EMAIL` gets a pending admin invitation. That pattern already provides
what the sign-in bootstrap was invented for, an inviter that needs no owner, so the seed
replaces it:

1. The seed creates or repairs one org with `kind = operator`, slug `operator` and name
   `Platform operator`. It also provisions the org's docs repository, docs-only Base
   workspace, docs mirror and default image policy, the same steps it runs for `g5`
   today. `DEFAULT_ORG_NAME` and `DEFAULT_ORG_SLUG` are removed, and the seed creates
   no tenant org.
2. `DEFAULT_ADMIN_EMAIL` gets a pending `admin` invitation to the operator org unless
   that person is already a member. Accepting it at sign-in uses ordinary
   [invitation authority](../../ontology/features/organizations.g5.md#g5.organization.invitation-authority),
   and the pending invitation admits the person at sign-in. Later
   operators are invited by operator admins.
3. The service user is the seed's database actor, the inviter of record and the Base
   workspace's owner. Its membership is `member`, not today's `admin`, because
   `workspaces_active_owner_required` and the deferred `workspaces_owner_same_org_fk`
   require every live workspace to have an owner who is an eligible, active member. A
   real seed run without the membership failed on that key (implementation,
   2026-10-24). Its `service` identity can't sign in, and platform routes need a person
   session, so it never exercises operator authority. It does appear on the operator
   roster. Making Base ownerless would relax a workspace-wide invariant, and deferring
   Base until the first admin accepts would add an acceptance hook; both were rejected.
4. The seed no longer mints the bare-host registration token or writes the `g5-org-id`
   and `g5-worker-registration-token` secrets. The trace found no consumer: the
   ExternalSecret that maps them has `sync_worker_secret = false` in every environment,
   cloud hosts use per-host JWTs, and `g5d host` rejects `g5w_` tokens. The Terraform
   secret containers, the disabled mapping and the `worker_tokens` table stay; removing
   them is follow-up cleanup.

The operator org has no protected owner. Owners can't be invited, and the seed has no
person to make owner. If operator admins remove or demote the last admin, re-running
the seed re-invites `DEFAULT_ADMIN_EMAIL`. The seed is idempotent, which also makes it
the upgrade path: running it once gives an existing environment its operator org
without touching that environment's tenant orgs. The self-hosted installer
(`infra/self-hosted/scripts/g5-selfhost.sh`) keeps creating its single customer org with
an owner and provisions no operator org, pending the self-hosted question.

This supersedes the earlier "first listed sign-in" answer. `G5_PLATFORM_OPERATOR_EMAILS`
and the sign-in hook are removed from the design. The Base workspace is no longer
created by the ordinary creation cascade. It is the seed's docs-only Base, which still
hosts the Members and Admin pages, as the owner's "keep Base + docs" answer requires.

### Organization creation is operator-only

The owner: "With operators this route shouldn't exist" (2026-10-24). This settles the
issue's hosted self-serve question.

- `POST /api/v1/orgs` is removed; `GET /api/v1/orgs` (the caller's orgs) stays. Self-hosted
  already refused it, so self-hosted behavior is unchanged.
- The frontend loses `/create-org`, `CreateOrgPage`, `CreateOrgDialog`, `useCreateOrg`,
  the Account-flyout "Create Organization" entry and `lib/orgCreation.ts`. That page had
  no gate, so any signed-in person without an org could create one in production. The
  `@g5labs.ai` check only hid the flyout button.
- A signed-in person with no active org lands on `/no-organization`. It names the
  account and says to ask an organization administrator for an invitation. With
  invitation-only admission, only a person whose memberships are all inactive can reach
  it. `no-organization` joins `STATIC_ROUTES` and the reserved slugs. `create-org` stays
  reserved so old links never resolve to a tenant.
- The operator console's create form (name, slug from the name, owner email) is the only
  creation UI. It reuses the dialog's slug derivation, not its repository-connection
  step. A new org's owner connects repositories later from Base's existing settings
  (`WorkspaceSettingsContent` hosts `GithubPatConnect`).

### Sign-in admission without an allowlist

The owner: "The allowlist exists only for bootstrap. If we have a better bootstrap
mechanism then the allowlist could be retired?" Yes. The admission rule today is
*allowlisted, or invited, or a member*. Platform seeding admits the first operator by
invitation, and operators create every org for a named owner, so the allowlist branch
now only admits people who have nothing to use. An identity with no invitation and no
membership has no org and no scopes. It can reach only account routes (profile, SCM
connection, invitation accept/decline); platform reports and telemetry already need
scopes. Dropping that branch leaves *invited or a member*. That is stricter than every
configured value, so the controlled rollout production asked for on 2026-10-23
(refuse unlisted personal accounts) still holds. The one behavior change: an
`@g5labs.ai` account with no invitation is now refused too.

- `AuthService::handle_callback` checks the pending-invitation and existing-membership
  conditions that today bypass the allowlist. Those queries already match the verified
  email set and the provider identity, and already distinguish lookup failure from
  denial. `is_any_email_allowed` and `AuthConfig.allowed_emails` are removed. Deactivated
  memberships still admit the account, with no active org, as today.
- `CallbackOutcome::AllowlistDenied` becomes `AdmissionDenied`. `/login/denied` stays,
  and its copy changes from "not on the allowlist" to "has no G5 invitation".
- The dev/E2E test-token path (`G5_E2E_AUTH` only) drops its allowlist check. It mints
  fixture identities that it immediately makes members.
- `G5_AUTH_ALLOWED_EMAILS` is removed from config. The API logs a warning at startup
  when the variable is still set, so a stale deployment value is visible rather than
  silently trusted. The setting is removed from the Terraform release inputs,
  ExternalSecret mapping, tfvars examples and Terraform tests, from the unused Helm
  values, from the self-hosted `env.template`, `compose.yml` and `apply-install-env.py`,
  from the Azure canary workflow input, and from the staging, production and QA READMEs.
- Self-hosted: the installer's default admin has an owner membership, and everyone else
  is invited, so self-hosted loses nothing. The Azure canary's smoke account is that
  default admin.

### Test fixtures that created orgs

Fixtures used self-serve creation; they move to the operator API:

- The test-token path (`G5_E2E_AUTH` only) also provisions the operator org for the
  fixture operator persona `e2e-operator@g5labs.ai`, through the shared creation path
  with `kind = operator`. This is a dev-only writer, like its existing `g5labs` fixtures.
  Unlike the platform seed, it makes that persona the operator org's owner, because the
  shared path needs one.
- `scripts/seed-ci-org.sh` mints the operator persona's token and creates its org with
  `POST /api/v1/platform/orgs`, owner `E2E_MEMBER_EMAIL`. That is the same owner the
  self-serve call produced. This covers `g5labs-vl`, `-re`, `-pa`, and `g5labs` when the
  test token has not already created it. `tests/e2e` `api_client::create_org_full` does
  the same.
- Playwright: `org-create.spec.ts` becomes the console's create-org spec. `onboarding.spec.ts`
  expects `/no-organization` for a person whose only membership is deactivated, and the
  denied page for an uninvited identity. `golden-path.spec.ts` starts from the seeded
  `g5labs`. `scm-pat-connect.spec.ts` reaches the PAT form through workspace settings.
  RV10 in `workspace-review-recovery.spec.ts` uses `g5labs-member` as its second org.
- Backend integration tests that POST `/api/v1/orgs` (the `api_orgs` cascade, rollback,
  mirror and duplicate tests; `api_workspaces`, `crud_lifecycle_integration`,
  `api_personal_workspaces`) move to the platform route or `OrgService::create_org`.
  `api_self_hosted_license` and `api_auth` account-route assertions, `route_auth_static`'s
  `create_org` exemption, and the allowlist tests in `api_auth` (rewritten as
  invitation/membership admission tests) and `api_orgs` are updated to match.
- Frontend unit mocks (`msw/handlers.ts`, `useOrg.test.ts`, `App.test.tsx`,
  `UserSettingsPanel.test.tsx`) are updated.

### Seed consumers

- `scripts/reset-qa.py` (the disposable QA reset) seeds, then expects the `g5` org.
  The new order is: seed the operator org; the admin signs in at qa.g5labs.ai and hands
  over the session token (the script already needs it, later); then the script creates
  `g5` through `POST /api/v1/platform/orgs` with the admin as owner, and continues with
  its code repository, configuration and host steps. That org is born from the creation
  cascade with a local `code` repository, so the step that installs the G5-Lean code
  repository replaces it. Adopt-Remote does not fit. It fetches the remote with the
  actor's provider credentials, which the reset does not have yet (it installs from the
  operator's local pack). The cascade's README commit also shares no history with G5-Lean,
  so the result would be `merge_required`. The SQL step deletes the cascade's
  repository only while it is untouched: name and slug `code`, no provider,
  `g5-local://workspaces/<base>.git`, Base its only member, no remote link and no child
  workspaces. Any other code repository is still refused. The step acts as the admin,
  who owns `g5`. Its check that `g5-org-id` matches the org and its wait for the worker
  secret go away.
- `scripts/reset-dev-environment.sh` (dev.g5labs.ai) truncates every org, which now
  includes the operator org and the calling operator's membership. It becomes two steps.
  The wipe re-runs the platform seed and stops. After the operator signs in again,
  `--regenerate` creates `internal-g5-devs` with `POST /api/v1/platform/orgs` and
  re-invites the captured people. It adopts an existing `internal-g5-devs` if one is
  there. The captured people exclude `service`-provider accounts, so the seed's service
  user is not invited into the tenant. Open: if dev.g5labs.ai is no longer used (the
  script last changed 2026-10-18), retire the script instead.
- The Azure self-hosted canary's optional GitHub test account must now be the
  installer's default admin. An uninvited account is refused at sign-in.
- The canary workflow (`.github/workflows/self-hosted-azure-install.yml`) still writes
  `G5_AUTH_ALLOWED_EMAILS` into its install environment. It is harmless, because
  `apply-install-env.py` no longer copies that key. Removing it is deferred because
  publishing workflow changes needs the G5 GitHub App's Workflows permission.
- Docs that describe running the seed (`infra/qa/README.md`, `docs/qa-database-reset.md`,
  `docs/database-migrations.md`, `scripts/reset-qa.md`, the staging README's
  initial-seeding step) say it provisions the operator org. Tenant orgs are created from
  the console.
- Text-level script tests: `scripts/test_seed_prod_org.py` (target selection, unchanged
  in substance) and `backend/g5-api/tests/seed_workspace_scripts.rs`.
- Dev and CI fixtures (`seed-dev-org.sh`, `seed-ci-org.sh`, the E2E test-token orgs)
  create test tenants, not a platform, and stay as they are, with one addition
  (2026-10-26): to match prod, `seed-dev-org.sh` (run by `dev-up`, `dev-stack-up` and
  the full-stack preview profiles) mints the fixture operator persona, which provisions
  the operator org, and that persona adds Alex (`demo@g5labs.ai`) as an operator
  administrator through `POST /orgs/:orgId/members`. Alex already exists, so the
  membership is immediate. A re-run restores the administrator role, as re-running the
  platform seed does. Side effect: the fixture persona owns the operator org and, like
  every test-token user, the `g5labs` fixture orgs.

### Authority

`require_platform_operator(conn, user, access)` locks the caller's active membership in
the operator org (`authz.lock_org_membership`, as `require_active_member_on_conn` does).
`Observe` allows any role. `Operate` requires member, admin or owner. Deactivated or
absent membership is refused with 403. Platform routes need a person session, the same
as `GET/POST /api/v1/orgs`. Inviting, removing and changing operators' roles is ordinary
org-admin authority in the operator org; nothing new is needed.

### Creating an org for an owner

`POST /api/v1/platform/orgs {name, slug?, ownerEmail}` runs in one transaction. It
checks `Operate` authority, normalizes the email (trimmed, lowercased), finds the account
by case-insensitive email or creates a user row for it with no provider identity, then
runs the shared creation cascade with that user as owner. Post-commit repository
bootstrap and the personal-workspace hook are the same code the self-serve route uses.
The operator gains no membership. A created account is linked to the person's provider
identity at their first verified sign-in, because `handle_callback` matches accounts by
verified email. Their owner membership also admits them on re-entry. The route emits
`g5_audit` `platform.org_created` with the actor, org, owner and whether the account
was created. Self-hosted mode refuses, as `POST /api/v1/orgs` does.

### Cross-tenant facts

`GET /api/v1/platform/orgs` returns each tenant org's id, name, slug, creation time and
active member count, read on the system pool after an `Observe` check. Workspace, repo,
chat and document content stays behind RLS and tenant membership. The audit middleware
records every `GET /api/v1/platform/*` as a curated read, so each platform read is
attributable. Envelope: one unpaginated list, sized for up to a few thousand
organizations. Pagination is a follow-up if a deployment grows past that.

### Preview origin for full-stack previews

The dev seed made Alex an operator, and opening `/operator/home` in a full-stack preview
then failed with "Could not switch to Platform operator". An org switch renews the
session through the `g5_refresh` cookie exchange, which admits only trusted browser
origins ([refresh exchange](../../ontology/features/users.g5.md#g5.security.login.refresh-exchange)).
The previewed API trusted only `G5_EXTERNAL_URL` (`http://localhost:<edge port>`). The
browser's origin is `https://<task>.<suffix>`, and nothing told the stack that origin:
the tunnel rewrites `Host` to `localhost` and the dev edge drops the forwarded host. The
gap predates #776 and broke every org switch in a full-stack preview. The owner chose
"Platform passes origin" and to fold it into #776
([intent](../../chat_intent/issue-776/request.md#preview-origin-2026-10-26)).

- **Owner of the fact: the API.** Only the API knows a preview's public origin: the
  preview task id it mints, under its configured suffix
  ([origin security](../../ontology/software_architecture/workspace_vm.g5.md#g5.workspace.preview-origin-security)).
  `start_workspace_preview` writes `origin: https://<task>.<suffix>` into the existing
  `workspace.preview` module input when a suffix is configured. It is derived once from
  the task identity and stored only in that spec; no new state or lifecycle.
- **Carrier: the guest dispatcher.** The preview start input accepts an optional bare
  `https` origin (`validate_input` refuses any other form), and the runner exports it as
  `G5D_PREVIEW_ORIGIN` beside `G5D_PREVIEW_PROFILE`. `preview-serve` passes it through
  to the profile command.
- **Consumer: the dev stack.** `just dev-stack-up` (both full-stack profiles) appends
  the origin to `G5_CORS_ALLOWED_ORIGINS`, keeping the `http://localhost:5173` default.
  The compose file passes that variable to the API, whose trusted-origin list already
  includes CORS origins
  ([process-local preview](../../ontology/software_architecture/hosts.g5.md#g5.runtime.boot.backend.process-local)).
  `G5_EXTERNAL_URL` stays local because it also addresses the OAuth stub callback.
- **Cookie name.** Trusting the origin was not enough: the preview relay drops every
  `Set-Cookie` named `g5_*`, which it reserves for the platform
  ([origin security](../../ontology/software_architecture/workspace_vm.g5.md#g5.workspace.preview-origin-security)),
  so the stack's `g5_refresh` never reached the browser and each cookie refresh
  answered 401. The API's refresh cookie name becomes configuration
  (`G5_REFRESH_COOKIE_NAME`, default `g5_refresh`, validated as a token), used by the
  exchange, logout and preview re-auth. `dev-stack-up` sets `preview_refresh` together
  with the origin. The relay rule stays as it is: relaxing it for one stack kind would
  let any previewed app plant a platform-named cookie.
- **Version skew: the module catalog decides.** An older guest's `PreviewInput` denies
  unknown fields, so sending it `origin` would refuse every preview start. The guest
  catalog becomes `2026.10.26-workspace-preview-origin`, and the API drops `origin` at
  dispatch unless the attempt's VM recorded that catalog or a later one at Ready. That
  VM is fixed for the attempt, so the decision needs no extra frozen state. One ordered,
  append-only list of shipped catalogs (`SHIPPED_MODULE_CATALOGS`) answers "at or after"
  for every guest contract. Codex App Server's exact-match gate moves onto it, because a
  newer catalog would otherwise have turned Codex off. An unknown catalog stays on the
  established contract, as before. Without a suffix the API sends no origin at all.

Rejected: trusting `Origin == X-Forwarded-Host` in the dev edge and API (amends the
refresh-exchange rule for one stack kind), deriving the origin inside the guest (the
guest has no suffix), and patching only the running preview (does not survive the next
one).

## Alternatives rejected

- **Sign-in bootstrap from a configured email list** (the owner's first answer). It
  existed only because an org seemed to need a real owner before anyone could invite.
  The platform seed already invites through a service user, so this would be a second
  bootstrap writer. Superseded by platform seeding.
- **Startup-created operator org inside the API.** It would duplicate the seed and
  needs the same service inviter.
- **Making the seed's admin the owner.** No person exists at seed time, and owners
  can't be invited.
- **Operator becomes the new org's owner.** Puts the operator inside the tenant.
- **Per-org operator tiers** (issue comment). Discussed and rejected on the issue.

## Unchanged and open

The self-hosted single-org rule is unchanged. Provisioning adjustments, deployment
health and moving the license token under operators are not built. They stay as Q blocks
beside the owning contract. Self-serve creation and the allowlist are settled above.

## Validation plan

- Platform seed: run `seed-prod-org.sh` against a local Postgres twice (with
  `USE_K8S_PSQL=0` and the cloud clients stubbed, extending `test_seed_prod_org.py`).
  Check: exactly one operator org with Base, docs repo and admin invitation; no tenant
  org, service-user membership, worker token or secret write; a second run changes
  nothing. Then the invited email signs in through the OAuth stub and becomes an
  operator admin.
- Backend integration (`platform_operator::` module, with an operator org created as
  the seed creates it): reserved slugs are refused on create and update, and the operator slug is
  fixed; `Observe` and `Operate` are enforced by role, deactivation and non-membership;
  the directory lists tenant orgs with member counts; create-for-owner works for an
  existing user (with case-insensitive email) and for a new account; that account links
  at a later OAuth sign-in; the operator gains no tenant membership; self-hosted refuses.
- Admission: OAuth callback admits a pending invitation, admits an existing (including
  deactivated) membership, refuses an identity with neither as `AdmissionDenied` even
  when its email is in the domain that used to be allowlisted, and returns 500 on a
  lookup failure. A set `G5_AUTH_ALLOWED_EMAILS` logs the startup warning and changes
  nothing.
- Creation: `POST /api/v1/orgs` is gone for every caller; the frontend has no
  `/create-org` route; an org-less session lands on `/no-organization`.
- Fixtures: `seed-ci-org.sh` against the CI stack through the operator persona; the
  rewritten Playwright specs.
- Static route-auth audit for the new handlers.
- Frontend unit tests: `PlatformHome` rendering by role and the create flow (MSW);
  `HomePage` chooses the console only for the operator kind.

## Validation results

Run in the workspace VM on 2026-10-24 against Postgres 17. After rebasing onto main, which added migrations 112 and 113, the operator migration is 114. The affected integration modules (174 tests), the full Vitest suite (324 files), typecheck and clippy were re-run on the rebased tree and pass.

- **Backend integration.** The full `g5-api` `integration` target ran with 8 threads: 1744
  passed, 9 failed. Two failures were real and are fixed: `route_sql_guard` (the fixture
  membership insert moved into `platform_operator::join_operator_org`) and
  `service_context_layering` (new inventory entry). The other seven pass when run alone
  with the VM's `GIT_AUTHOR_*` unset; the parallel run had exhausted Postgres connection
  slots or hit tunnel and pool timeouts. New coverage is in `platform_operator::` (schema,
  authority by role and deactivation, directory facts, creation for existing and new
  owners, reserved and fixed slugs, 405 for self-serve, the E2E operator persona) and
  `api_auth::` (refusal without an invitation, linking an operator-created owner through
  a secondary verified email).
- **Unit, lint and static checks.** Unit tests (`g5-api-core` filtered, `g5-api-foundation`,
  `g5-api-routes`) pass. Clippy with `-D warnings` is clean for the API crates and
  `g5-e2e`. The SQLx offline data was regenerated with CI's prepare command, and the
  migration checksum and version checks pass.
- **Frontend.** `tsc --noEmit` is clean, and the full Vitest suite passes (324 files).
- **Native stack** (g5-api, Vite and the OAuth stub). Chromium Playwright: `org-create`
  (console list, create for a new owner, duplicate slug, non-operator 403, self-serve
  405), `onboarding` (uninvited OAuth identity on `/login/denied`, org-less session on
  `/no-organization`) and `scm-pat-connect` passed, 15 tests. `invite-members` passed 33
  of 34; the one failure needs stub email delivery, which this stack did not configure.
  The API logged the retired-allowlist warning for a stale `G5_AUTH_ALLOWED_EMAILS`.
  Golden-path and the RV10 recovery spec need workspace VMs and were not run.
- **Scripts.** `seed-prod-org.sh` ran twice against a migrated Postgres with identical
  results: one operator org, Base and docs, one pending admin invitation, the service user
  as `member`, no worker tokens. The reset-qa code-repository replacement ran against the
  database. Script tests (`test_seed_prod_org`, `test_reset_qa`,
  `test_terraform_release`, `test_release_environment_values`,
  `test-self-hosted-bundle.sh`) and the Terraform `fmt`, `validate` and `test` runs for
  the release modules pass.
- **Not run.** A live QA or dev reset, a real provider sign-in accepting the seeded
  operator invitation, and `seed-ci-org.sh` against the CI stack.

