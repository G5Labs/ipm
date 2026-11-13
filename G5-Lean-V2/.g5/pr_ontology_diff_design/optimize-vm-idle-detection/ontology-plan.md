# Workspace owner activity

The [request](../../chat_intent/optimize-vm-idle-detection/request.md) amends
[idle reclamation](../../ontology/software_architecture/workspace_vm.g5.md#g5.lifecycle.idle).
Work is on the initially assigned `optimize-vm-idle-detection` branch; only its
corresponding origin branch is authorized for publication.

Previously the runtime facade renewed activity on every owner RPC and stream open;
the terminal also renewed on heartbeat and every data/control frame. Authorization
and transport success do not establish human activity. Remove those implicit
writes, retaining all admission, live authority checks and recovery. Existing
task state remains the sole source of tracked-work exemptions.

The G5 UI reports trusted interaction separately from its queries, once per
ten-second coalescing window. Reports carry the observed VM incarnation and
event time, expire after thirty seconds, and are never scheduled or retried by a
polling loop. Browser wall-clock skew must not reject genuine interaction: on
each coalesced gesture the observer reads database time from a passive,
authenticated, no-store platform GET on the activity endpoint, then subtracts elapsed
monotonic time since the gesture. A wall-clock difference can only increase
this age, covering browsers whose monotonic clock pauses during OS sleep;
absolute client wall time never dates a report. Delayed samples older than
thirty seconds are dropped. The resulting observation is immutable across authentication retries;
neither the clock read nor a retry can create a newer gesture. The main UI
observer retains its thirty-second freshness fence and five-second future tolerance.
Server-observed terminal input and preview traffic likewise use database time
minus their monotonic age, not the API host's wall clock; lock waits cannot re-date them.
Background
requests therefore cannot inherit foreground intent through
refetches or downstream RPC retries. The runtime owns the fenced timestamp
update; the lifecycle owner still rechecks activity and work under its Stop lock.
Both use workspace/membership-before-VM lock order, so a gesture waiting behind
Stop cannot deadlock it or renew an incarnation after Stop admission.
No new durable lease, session-presence table or background classifier is needed.

Terminal surfaces bypass generic activity reporting. Only validated nonempty
input renews activity. The browser distinguishes trusted keyboard/paste input
from terminal-emulator responses to output; both retain transport functionality.
Control messages and live connection authorization remain independent of renewal.

The owner's 2026-10-24 amendment makes preview traffic an intentional exception.
Each authorized proxied HTTP request or new preview WebSocket connection may
renew activity, including refreshes, polling and reconnects, under the existing
ten-second database throttle. The isolated preview session, current owner and
exact task/incarnation remain the authority. Quiet established connections and
their frames do not add renewals. Preview session minting, platform redirects
and platform admission rejections do not count as proxied traffic.

Restore the pre-instrumentation streaming relay and its existing security
normalization. Remove the preview HTML parser and its new dependencies, observer
injection, activity-specific CSP/cache/header changes, and preview-only clock/report
endpoints. Preview content and CSP are no longer interpreted for activity.
Log these timestamp changes with kind and reason `preview_traffic`, not user
activity. The runtime still owns the authorized, incarnation-fenced write; the
route records once after successful preview stream admission. No new owner,
durable state or classification mechanism is needed.

Timestamp change logs reflect committed changes, including Ready initialization
and lifecycle clearing. Reasons and correlation identifiers are structured and
content-free. Idle selection and locked admission explain exemptions (bounded
work, preview startup, recent activity or retry deferral) and Stop decisions.
Candidate selection filters eligibility and applies its limit in SQL. Exemption
diagnostics use an independent bounded page of expired workspaces at most once
per minute per organization per reconciler, with a UUID cursor to rotate coverage.
The lifecycle loop owns this diagnostic state across service reconstruction;
diagnostics do not delay eligibility or change activity/Stop state. Locked Stop
admission still logs every decision, including newly arrived exemptions.
Shared log formatting lives in the foundation crate so host initialization does
not depend on the later workspace layer. Terminal write failure or cancellation
releases only its own coalescing claim: the next actual input may try again,
but no background retry is introduced. Missing request-ID middleware receives a
generated correlation ID and must not change terminal authorization results.
Verification exercises real HTTP/WebSocket admission, persisted timestamps and
the lifecycle reconciler, plus browser event/recovery tests.

Verification covers G5 file reads, RPC retries and event-WebSocket resubscription
without renewal; genuine browser gestures and terminal keyboard/paste renew,
while stale/replayed reports and revoked owners cannot. Preview regressions
must exercise automatic HTTP requests and WebSocket connection/reconnection,
SQL throttling, correct log classification, rejected authorization, and
unchanged HTML/CSP/cache headers under the existing proxy normalization.
The former preview report URL must be ordinary application traffic, not a
platform endpoint. Once preview traffic stops and work exemptions end, the
configured timeout still admits idle Stop.

Retain Ready replay, lock ordering, configured timeout, work exemptions, bounded
candidate selection and rate-limited diagnostic tests. Real Chromium/xterm input
must still distinguish keyboard/paste from emulator responses, including without
`window.event`. Tests use real HTTP/WebSocket routes and PostgreSQL with a
scripted VM peer; the browser uses real input with a test transport.

Validation of the simplified policy passes seven PostgreSQL/HTTP/WebSocket
activity regressions, 24 broader idle/lifecycle regressions (including G5 preview
status polling), all 212 route tests, nine activity unit cases, and the diagnostic
cadence test. All 28 focused frontend activity/terminal tests and the real
Chromium/xterm input regression pass. Strict Clippy, TypeScript, Rust formatting and focused
format/lint checks pass; the browser test retains nine existing lint warnings.
No parser dependency or license-policy change remains. Deployed-stack CI and PR
reviews remain separate readiness checks on the published head.
