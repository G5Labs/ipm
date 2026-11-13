# Owner inactivity (2026-10-23, amended 2026-10-24)

The user requests that unattended G5 browser tabs cannot keep workspace VMs alive.
Automatic refresh/refetch, polling, WebSocket reconnect/resubscribe and all
downstream requests/retries must leave `last_activity_at` unchanged.
Terminal activity requires actual keyboard/paste input;
connection lifetime, control frames, output and empty/invalid frames do not count.
Preserve automatic recovery, authorization, real user activity and tracked-work
exemptions. Log actual timestamp changes with workspace, VM/epoch, reason,
correlation and previous/new timestamps, distinguishing initialization; never
include input or document contents. Log idle decisions and exemptions. Test both
activity classes and actual timeout shutdown after work exemptions end.

The user's 2026-10-24 amendment explicitly exempts preview traffic: authorized
proxied HTTP requests and new preview WebSocket connections may renew the idle
timer under existing throttling, including automatic traffic. Remove preview
script injection, CSP rewriting, interaction-reporting endpoints and their
HTML/header modifications. Restore the existing proxy behavior and security
controls. Log these resets as preview traffic, not human interaction. Keep
G5 UI/terminal rules and SQL-bounded, rate-limited idle diagnostics unchanged.
