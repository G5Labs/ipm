# Offer Connect GitHub after login only when the organization needs GitHub (#955)

Durable source: [#955](https://github.com/G5Labs/G5-Lean/issues/955), written by the
owner (Ben). The issue separates the owner's stated intent from parts the agent
derived; the derived parts are proposals until confirmed.

## Owner's stated intent (from the issue)

- Offer Connect GitHub after the organization is known, and only when that
  organization uses GitHub.
- Only offer it when there is a clear organization that needs GitHub.
- Repeat prompts on later organization switches are out of scope for now.
- It is fine if the offer delays the onboarding tour.
- A user who is only a viewer of the organization should not be offered it.

## Decision taken in chat (2026-10-30)

The one open question the issue flagged — **what "needs GitHub" means** — was put to
the owner with three candidate signals. The owner chose:

> **An organization needs GitHub when it has at least one repository tracking a
> GitHub remote OR at least one GitHub issue domain configured.**

Interpretation (agent): this is the union of the two actual uses of the shared
personal connection — publishing PRs (GitHub-tracking repos) and posting issue
comments / state changes (GitHub issue domains). An active App binding alone, with
no GitHub repos and no GitHub issue domains, does **not** by itself count as needing
GitHub, because the user has nothing yet to act on under their own account.

## Derived points adopted (from the issue, now settled by this design)

- **"Clear organization"** = the organization the user is entering: their active
  organization from the JWT session, falling back to their first membership. An
  account with no organization is entering none, so it is not offered the
  connection. Decided in chat (2026-10-30): rather than parse the organization out
  of the preserved `returnTo` path, use the already-resolved active organization —
  the same object sign-in's default Home landing uses. The shared GitHub connection
  is account-level, so the exact organization only needs to tell whether this user's
  context involves GitHub at all; the rare case of a `returnTo` deep link into a
  different organization than the active one is judged against the active
  organization, which is acceptable because repeat prompts and organization switches
  are out of scope.
- **"Viewer"** = the organization role `viewer`. Owners, admins and members are
  offered; viewers are not.
- **Member-readable signal:** the admin-only App-binding read cannot gate a member's
  landing, so the server decides the offer for the entered organization. (First put
  on the `/me` organization summaries; moved by the owner's decision below.)
- **Keep what already works:** the offer stays optional with "Continue without
  connecting"; the intended destination is preserved; the explicitly requested
  `/account/github?returnTo=…` resume path still works; connection errors never
  block sign-in; the in-context prompts (Home Issues tile, Prepare to Ship, issue
  composers) are unchanged, so a user the up-front offer skips is still asked when
  an action actually needs GitHub.

## Decision taken in chat (2026-10-30, review)

Asked who calls `/me` and whether it was the right place, the owner chose to move the
signal off `/me` ("Option A"): the person's connection read
(`GET /github-app/user-connection`), which the after-login card already makes, answers
whether to offer the connection when the landing names the entered organization.
`/me` is the profile bootstrap every page load waits on, and only the login landing
needs the answer.
