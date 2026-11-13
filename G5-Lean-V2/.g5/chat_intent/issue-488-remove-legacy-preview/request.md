# Issue 488: remove legacy (platform-origin path) preview mode

## Human request

On 2026-10-13, while working on [issue 454](https://github.com/G5Labs/G5-Lean/issues/454)
(whose last comment asked the assignee to decide "remove or fix" for legacy preview mode),
the user pasted [issue 488](https://github.com/G5Labs/G5-Lean/issues/488), "Remove legacy
(platform-origin path) preview mode", and asked the agent to "check this issue too for
reference". The agent verified its claims against the checkout, recommended an answer to
each of the four decisions the issue lists, and said it would run the design workflow on
the owner's go-ahead. The user wrote:

> go ahead

## Agent interpretation

The go-ahead adopts the four recommendations as the decisions for the design:

1. **Without a preview suffix** the API stays up and previews are unavailable: the
   session endpoint answers 503 `PREVIEW_ORIGIN_UNCONFIGURED` and the UI shows the reason.
   The API does not refuse to start.
2. **Cloud E2E** gets an HTTPS wildcard preview origin so `workspace-preview.spec.ts`
   keeps exercising the real VM → tunnel → relay → browser path, in subdomain mode.
3. **Local dev** goes without previews for now; a plain-HTTP `*.localhost` variant is a
   separate later change.
4. **Self-hosted** documents wildcard DNS, TLS and the suffix, and passes the suffix
   through the compose environment, rather than shipping without previews.

Not decided by the human and chosen by the agent in the design, flagged there: whether the
post-#221 browser cleanup headers (service-worker 410 with `Clear-Site-Data`, NEL reset,
legacy header stripping) are retired with the mode.

After reading the saved design, the user wrote:

> i would like to remove the legacy code, since it is not running in production anymore, and in CI, self-host, deploy we should align with current mechanisms as production

The agent reads this as authorization to implement the design, with one addition to
decision 4: not only self-hosted but every delivery surface (cloud and docker CI lanes,
the self-hosted bundle and the Helm chart) runs the production mechanism, per-preview
hosts under a suffix, instead of carrying a legacy fallback. Local dev stays without
previews (decision 3).
The refresh-exchange hardening from issue 454 (PR #497) is independent of this decision
and stays.
