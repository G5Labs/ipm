# Shared chat turns

Issue: [G5Labs/G5-Lean#817](https://github.com/G5Labs/G5-Lean/issues/817).
Authorized branch: `issue-817-unify-chat-rendering-across-claude-and-codex`.

User prompts and assistant turns follow the
[chat presentation rule](../../ontology/features/chat.g5.md#g5.chat.presentation),
including the owner's 2026-10-27 request for assistant and timeline bubbles in a
different color. Every assistant turn shares one surface regardless of provider,
execution state or loaded detail. The [view-state rule](../../ontology/features/chat.g5.md#g5.chat.view-state)
and [native permission actions](../../ontology/features/chat.g5.md#g5.chat.native-permission-actions)
govern the compatibility boundaries below. This specializes [saved output and replay](../../ontology/features/chat.g5.md#g5.chat.replay)
and the existing [error notice contract](../../ontology/features/chat.g5.md#g5.chat.error-notice).

`taskTimeline` remains the native protocol adapter; `providerOutput` owns output
provenance, snapshot/delta deduplication and adjacent chunk coalescing. Chat groups
those normalized events by message identity (chronological windows only for old
unowned events). A shared assistant turn owner projects saved content, DTO tools,
artifacts and timeline activity into blocks, and owns prose, thinking, tool,
image, status and approval rendering. Sparse messages use the same owner with no
events. DTO-only tool calls have no narration offsets and keep their source
order; loaded timeline facts supply chronological placement. Remove the separate trajectory and saved-assistant JSX rather than adding
another response renderer. Existing message/citation/timestamp and artifact helpers
are reused; no new durable records or execution lifecycle are needed.

Stable turn/tool identities retain disclosure choices as rich history arrives or
older history is prepended. Question drafts and in-flight delivery retain the existing
request-keyed store. Bounded tab-local session storage retains question drafts and
message/tool disclosure preferences across browser reload; execution/delivery state
is never inferred from that disposable view state. Each tab cache retains at most
50 nonempty turn-preference records and 50 question drafts; empty defaults do not
consume that capacity. Native block snapshots stay at
their first delta’s chronological position so completion cannot reorder narration
around tools. One approval card handles awaited delivery, repeat suppression,
errors and resolved/timeout/late-answer states across all inputs. Native execution
fences also scope pending delivery, failures and saved question drafts, so reused
provider request IDs cannot inherit another task or attempt’s state. Native
permissions lack the legacy interrupt action and persistent permission-rule updates
(the current reply route accepts allow/deny and optional updated input); Codex's
current output transport does not gain interactive capabilities. Claude background
work controls remain conditional on actual reported work and the existing callback.

Verification covers a shared Claude/Codex fixture matrix for sparse/rich, native/legacy,
and execution states; artifacts, citations, images, ordered text/tools, disclosure and
question recovery, delivery errors, and existing output/error regressions. Browser
coverage uses the actual ChatPanel with controlled transport fixtures in center/panel
layouts and light/dark themes, including reload and older-history hydration. Report
live-provider coverage separately; fixture transport cannot establish provider delivery.

Validation: the affected component, output, native-journal, approval, activity and
streaming suites pass (494 tests across the final affected runs). Chromium passed
all 17 shared browser scenarios, including both providers and event formats in
center/side-panel layouts and light/dark themes, completion/Stop/error, sparse
reload and older-history hydration. The native question reload test passed again
after delivery state was scoped to the full execution identity. TypeScript and the
production build passed with `NODE_OPTIONS=--max-old-space-size=6144`; the default
heap was insufficient. ESLint passed with warnings; changed files pass Prettier.
Browser fixtures use the real ChatPanel with deterministic transport and tab-local
persistence. Live Claude/Codex execution and real backend reconnect/persistence were
not exercised; no configured local provider test stack was available.
