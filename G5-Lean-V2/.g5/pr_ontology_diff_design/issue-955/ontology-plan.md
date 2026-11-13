# Offer Connect GitHub after login only when the organization needs GitHub (#955)

Request and the settled "needs GitHub" decision:
[chat intent](../../chat_intent/issue-955/request.md) and
[#955](https://github.com/G5Labs/G5-Lean/issues/955).

## Governing block (in this diff)

[`g5.settings.github-connection`](../../ontology/software_architecture/git.g5.md#g5.settings.github-connection).
The after-login GitHub offer gains an organization condition: offer only when the
organization the user is entering **uses GitHub** and the user is more than a viewer
of it; an account entering no organization is not offered it. The statement also
defines "uses GitHub" (a GitHub remote–tracking repository or a GitHub issue domain)
and records that this is a server-computed, member-readable signal, distinct from the
administrator-only App binding.

## Why these owners

- **The signal is a property of the organization, not of the person.** "Uses GitHub"
  depends on the org's repositories and issue domains, both tenant-isolated. The
  admin-only binding read (`GET /orgs/:orgId/github-app/binding`) cannot serve an
  ordinary member's landing, so the server decides the offer and returns it from the
  person's own connection read, `GET /github-app/user-connection`, which the
  after-login card already makes. Only the landing names an organization
  (`?afterLoginOrgId=`), and only then does the read touch organization data and
  answer `offerAfterLogin`; the Home Issues tile, workspace page and settings read
  the status as before. The decision (connection, role, organization use) lives in
  one server place, next to the connection it offers.

- **Not on `/me` (owner's decision, 2026-10-30).** The first cut put a per-org
  `usesGithub` on the `/me` organization summaries to avoid a round trip. `/me` is the
  app-wide profile bootstrap that every page load, organization switch and
  claim-changing token adoption waits on, so every caller paid an organization read
  for a value only the login landing uses, and Identity had to read Git- and
  Issues-owned tables. The owner chose to move the signal to the connection read.

- **"Uses GitHub" = GitHub-tracking repo OR GitHub issue domain.** These are exactly
  the two surfaces where the shared personal connection publishes under the user's
  own identity (PRs; external-issue comments and state changes —
  [`g5.external-issues.creation`](../../ontology/features/issues.g5.md#g5.external-issues.creation),
  [`g5.external-issues.transition`](../../ontology/features/issues.g5.md#g5.external-issues.transition)).
  Owner's decision in chat. An active App binding with neither tracked is the mechanism
  without the need, and is deliberately excluded.

- **The read is confined to the named organization.** The handler opens the
  existing `scoped_db_for_org(&[])` tenant transaction, which verifies membership when
  the capability is minted (a non-member gets the non-enumerating `404`), then reads
  the role and whether that organization has a GitHub issue domain or a repository
  whose effective provider is GitHub (the publication rule: the clone URL is on the
  configured GitHub origin and any stored provider agrees, so manually added
  repositories without a stored provider count). No migration, no RLS change; a
  connected person or a deployment without the App short-circuits before any
  organization read.

## Interaction sketch

- `GET /github-app/user-connection?afterLoginOrgId=<org>`: return the usual
  `{connected, githubLogin}` plus `offerAfterLogin`, true only when the deployment has
  the App, the person is disconnected, is more than a viewer of that organization and
  it uses GitHub (`github_user_grants::org_uses_github`). Without the parameter the
  response is unchanged.
- Login landing (`LoginPage`): once authenticated and past the explicit
  `/account/github` resume, take the organization the user is entering — the active
  organization (`enteredOrg`), the same object the default Home landing uses. When the
  deployment advertises `github_app` and that org exists, ask the connection read for
  it, showing the page loading state meanwhile (no retry). Show the card only on
  `offerAfterLogin: true`; any other answer, including an error, navigates straight to
  the destination (preserved `returnTo` or the default Home). Deciding against the
  active organization rather than parsing the destination path keeps the landing from
  duplicating route resolution; the shared connection is account-level, so this only
  distinguishes GitHub-using contexts. The card keeps "Continue without connecting".

## Scope and preserved guarantees

- Already-connected users are never offered it (the server declines, and the card
  still navigates a connected user onward).
- The intended destination is preserved; the explicit `/account/github?returnTo=…`
  resume path is unchanged; connection errors never block sign-in.
- Repeat prompts on later organization switches remain out of scope; the onboarding
  tour may be delayed by the offer, as the owner allowed.
- In-context GitHub prompts (Home Issues tile, Prepare to Ship, issue composers) are
  unchanged, so a user the up-front offer skips is still asked when an action needs
  GitHub.
- Older clients that never send `afterLoginOrgId` get the unchanged status shape; the
  landing treats a missing `offerAfterLogin` as no offer (fail-closed).

## Tests

- Frontend (`App.test.tsx`): the landing asks about the active organization and shows
  the card when the server offers it, with destination preserved and "Continue
  without connecting"; continues to the destination when the server declines or the
  read fails; neither asks nor offers when the user enters no organization; connected
  users never offered.
- Backend (`api_external_issues.rs`): a disconnected owner of an org with a GitHub
  issue domain is offered it; a viewer, a connected person and a non-member (`404`)
  are not; a read without the parameter keeps its shape; a repository counts only on
  the configured GitHub origin (a provider-less GitHub URL counts; GitLab, G5-local
  and provider/URL-mismatched rows do not).
