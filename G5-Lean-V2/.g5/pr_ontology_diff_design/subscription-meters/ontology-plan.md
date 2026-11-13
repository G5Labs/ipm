# Live subscription meters

## Decision and authority

Implement the [owner's request](../../chat_intent/subscription-meters/request.md)
under [subscription usage](../../ontology/features/chat.g5.md#g5.chat.subscription-usage),
[API-key precedence](../../ontology/features/chat.g5.md#g5.chat.api-key-precedence),
and the existing incarnation, task and browser-channel authority. The provider
owns its usage measurements; the frozen launch owns which credential route this
attempt uses. A login file alone cannot establish that work uses a subscription.

Read Claude's `rate_limit_event.rate_limit_info.unifiedWindows` and Codex's
`account/rateLimits/updated` while their existing processes work. These sources
need no additional model calls, provider polling, credential-file reads or login
changes. Subscription authentication does not prove a paid tier: Codex also has
free plans, and Claude setup tokens need not expose the tier name.

## Alternatives investigated

| Approach | Finding and choice |
| --- | --- |
| Passive native events | Selected. Codex reports `usedPercent`, duration and reset; Claude reports fractional utilization and reset for explicitly identified windows. |
| Native account inspection | Codex `account/read` (`refreshToken:false`) and Claude `auth status` / SDK `accountInfo` identify auth and sometimes plan. G5 already freezes the active route and overrides cached login for API-key launches; avoid redundant subprocesses or RPCs for meter admission. |
| Native usage reads | Codex `account/rateLimits/read` supplies a fuller snapshot, including multiple buckets. Claude `/usage` has structured `usage_report` output in current versions. Useful for an explicit refresh feature, but unnecessary here; Claude's report may be unavailable without `user:profile`, and a command would interfere with the conversation. |
| Claude status line | Official `rate_limits` supplies percentages after a response, but G5 uses headless stream JSON; installing a status-line command adds a second integration to obtain the same observations. |
| Provider HTTP endpoints | Direct ChatGPT/Claude usage requests duplicate native authentication and provider-specific scopes. Claude's usage endpoint can itself be rate limited. No benefit over passive frames for this request. |
| Intercept response headers | Claude's CLI already extracts the unified headers. A proxy or HTTP interception would change the direct-provider boundary for redundant data. |
| Local credentials, transcripts or caches | Credentials can establish cached login, not the effective billable route; transcripts add file discovery and stale-data problems. No token decoding or file watcher is needed. |
| Usage/cost analytics or local token estimates | Request totals and estimated dollars do not determine a shared subscription's remaining quota. Administrative analytics are delayed and may exclude subscription use. Do not manufacture percentages. |
| Existing saved quota snapshots | The UI is reusable, but the old proxy collector is gone, the report client points at a retired endpoint, and saved readings do not meet the chosen live-only lifecycle. Do not revive this persistence path. |

## Ownership and delivery

Normalize only explicit overall five-hour and seven-day observations for the
existing meter. Codex durations, not primary/secondary positions, identify these
windows; a different model/bucket or unknown duration must not masquerade as the
overall five-hour/weekly allowance. Missing, malformed and reset-expired values
are unknown, not zero. Sparse updates preserve only still-valid observations of
the same bucket and credential route. The UI labels the existing headline as
percentage left and its detail as percentage used.

The owner's follow-up mock makes the meter a disclosure button: keep its ring,
remaining percentage and window label at the left of the configuration row
above the input, with provider controls on the right. Click, tap, Enter or Space
toggles the details above it; Escape, outside interaction or focus leaving the
control closes them. Reuse the existing detail presentation and live data.
Only windows with valid, unexpired usage appear. If the preferred window is
unavailable, the available window supplies the button and selected detail;
without either window, hide the meter.

Ordinary module events are journaled before publication, so they cannot carry a
live-only meter. Use a small incarnation-authenticated report operation that
verifies the exact running task/attempt, session, VM and frozen subscription
route, then emits to the existing authorized chat WebSocket channel. It writes
no meter, task event, history entry or billing record. The reporter is bounded,
coalesces the latest observation, runs only in response to provider events and
cannot block/fail agent work. A dead/slow relay may lose updates. Do not add a
background provider check or retry loop.

Keep the last reading only in the open browser's memory, scoped to the chat and
its account/auth route. Session refresh must not overwrite it with a historical
snapshot. A reload may have no reading until further work produces one. Account
changes cannot reuse the previous account's reading. There is no cross-workspace
or account-wide cache and no new durable schema. Retained historical endpoints
can remain for compatibility; the live meter does not depend on them.

## Evidence and verification

Checked on 2026-10-30 against the repository's installed pins: Claude 2.1.308,
Codex 0.164.0. A synthetic loopback Messages response carrying 0.23 five-hour and
0.45 weekly utilization made the real Claude CLI emit both `unifiedWindows` under
`oauth_token`; the same response under `api_key` produced no rate-limit event.
The [reproducible probe](../../../scripts/test-native-subscription-usage.py)
uses a temporary empty home, synthetic credentials, no tools and a loopback proxy. This verifies native transport and units, not live plan coverage.
Historical real-provider recordings already contain both windows in
[`tests/fixtures/agent-turn-native/claude`](../../../tests/fixtures/agent-turn-native/README.md).
The installed Codex schema generator confirms notifications are sparse and
`usedPercent` is an integer; no paid-account requests were made.

Primary references: [Codex app-server](https://learn.chatgpt.com/docs/app-server#authentication-endpoints),
[Claude SDK events and usage report](https://code.claude.com/docs/en/agent-sdk/typescript),
[Claude CLI auth status](https://code.claude.com/docs/en/cli-reference),
[Claude authentication precedence](https://code.claude.com/docs/en/authentication#authentication-precedence),
[Claude status line](https://code.claude.com/docs/en/statusline#rate-limit-usage),
[Claude usage request failures](https://code.claude.com/docs/en/costs#when-the-usage-request-fails).
Claude's richer `unifiedWindows` field is marked internal in the pinned binary,
so pin upgrades must recheck it; absence is supported and causes no agent failure.

Verification covers native-event parsing (including sparse/malformed/wrong-bucket
frames), no journal projection, isolated nonblocking reporting, authenticated
delivery and stale-attempt refusal, no database meter/history writes, browser
account isolation and percentage/reset presentation. Existing provider lifecycle,
chat and API regression checks remain required for the affected seams.

Completed validation on 2026-10-30:

- `cargo test -p g5d-agent --lib -- --test-threads=1`: 188 passed, including
  sparse windows, percentage units, wrong buckets, no journal projection and
  slow/failed delivery without blocking or retrying. An initial parallel run
  hit a timing-sensitive Codex authentication cleanup test; the full serial
  run passed.
- API integration: all 11 `api_task_lifecycle::codex_device_auth` tests and
  all 12 `route_auth_static` tests passed. The new real HTTP/WebSocket test
  reserves separate immutable launches for both subscriptions and API-key
  authentication, checks stale task/attempt/VM and cancellation refusal, and
  verifies unchanged task-event and quota-snapshot counts.
- Frontend: 448 distinct tests passed across `SubscriptionUsageChip`,
  `useChat`, `ws.chatChunk`, `ChatPanel`, `WorkspacePageChatAccount` and
  `WorkspaceSettingsContent`. They cover live receipt, session refetch,
  account changes, memory disposal, display preferences and expired values.
- TypeScript build checking and Rust formatting passed. Changed-file ESLint
  reported no errors (84 warnings).
- `python3 scripts/test-native-subscription-usage.py --claude /opt/g5/bin/claude`
  passed both native authentication modes on the pinned CLI.

No live paid-account allowance was queried. Codex coverage uses its installed
schema and protocol tests; Claude's loopback probe establishes event transport
and units, not provider availability for every plan. Unknown windows and
missing observations remain supported absence. This change has no migration,
new provider polling, meter persistence or deployment step.

The button follow-up passed all 351 existing meter and ChatPanel tests and
TypeScript checking. A Chromium check using the actual ChatPanel verified both
providers, left alignment on the configuration row, unclipped expansion above
the button, click and keyboard toggling, Escape/outside dismissal and a 360px
viewport. The screenshot's 99% is example data, never a production default.

PR verification caught the required button test ID and a deployment gate that
prevented self-hosted chats from subscribing to live observations. The new page
regression passed for cloud and failed for self-hosted before the gate was
removed; both now pass. All 89 affected page, meter, cache and button-contract
tests, TypeScript checking and scoped lint/format checks passed for the follow-up.

The live channel also waits for a resolved session in the displayed workspace.
Regressions reproduced subscriptions to foreign-workspace URL hints in both
deployment modes and subscription before the detail read completed; all pass
with the workspace check. The handler is registered as an Agents-context,
TaskService-owned snapshot reader in the existing architecture guard inventories.

The available-windows follow-up rechecked Codex 0.164.0's generated schema and
the preview's native observations. Its `codex` bucket reported a primary
10,080-minute window and a null secondary window, so those observations contain
weekly usage but no five-hour reading. Primary and secondary are positions,
not duration guarantees. `rateLimitsByLimitId` is part of the explicit
`account/rateLimits/read` response; the passive notification carries a single
`rateLimits` bucket. The existing parser already examines both positions and
uses their durations. No extra provider read or parser change is needed to
display the available evidence accurately. All 354 meter/ChatPanel tests and
the three native-usage parser tests passed for this follow-up, along with
TypeScript checking and scoped lint (no errors).
