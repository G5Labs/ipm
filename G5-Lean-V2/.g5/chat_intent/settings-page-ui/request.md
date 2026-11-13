# Settings page template for Settings and Admin — request and interpretation

## User instructions (2026-10-26)

> Read Claude session f2ce741b from this folder. Without moving any of the settings around (so
> keep the inconsistent options for account at org/workspace/user level in the same page they are
> now), implement the new settings page, just for the user and for the org, but not for the
> workspace. Basically the only change in this PR should be the UI for the settings, showing the
> menu on the left and the new revamped elements in the scrollable page.

Session f2ce741b produced [issue #746](https://github.com/G5Labs/G5-Lean/issues/746) (settings
ownership, mocks on branch `design/settings-scope-mocks`). This request ships only its page
template.

Answers to the agent's questions (2026-10-26):

- Section order on Settings: the issue's order — Appearance, Accounts, Defaults, Editor, Access,
  then Execution policy when a workspace is open. Nothing leaves the page.
- Admin sub-pages (Members, Approval definitions, Workers, GitHub App): links in the left index,
  no link cards in the body.
- Save model: keep the existing Save buttons; no autosave stamp.

Later in the same session: "when you are done, before the codex reviews, open the PR, then start
the codex reviews in the background, then just dev-up so I can inspect manually".

## Agent interpretation

- One shared shell (`SettingsPageShell`) owns the scroll container, the scroll-spy index, the
  `?section=` parameter and the page header; content components declare their sections in page
  order and keep their gating. A deep link keeps re-aligning its section for a few seconds while
  rows above it load, until the person scrolls.
- `main` had already moved organization runtime defaults, accounts and API keys onto Admin and
  removed org writes from Settings (commit 4f18e011e); the "inconsistent" rows kept in place are
  the Workspace-scope rows of Defaults and Accounts and the Execution policy on Settings.
- Admin's Organization identity, the settings card, Issues, Defaults, Accounts and API keys become
  index sections; Invite teammate moves to the header; the "Workspace Settings" card becomes the
  index footer link "Your own settings are in Settings".
- Settings' Access section keeps SCM connections only (API keys were already on Admin); the index
  footer links Admin for owners and admins.
- Details, the shell controls, the account menu and the multi-scope row folding from the issue are
  out of scope.
