# Shared chat error presentation

The [owner's follow-up](../../chat_intent/issue-810/error-presentation.md) extends
the existing sign-in correction to chat errors generally. The governing contract
is [chat error notices](../../ontology/features/chat.g5.md#g5.chat.error-notice);
[failed-login notices](../../ontology/features/chat.g5.md#g5.chat.failed-login.notice)
add only login lifecycle reconciliation and active controls.

Chat already owns persisted assistant messages, journal status notices and
client-only live diagnostics. Reuse those records and their message/attempt
identities. Mark platform-authored diagnostic content explicitly in its existing
message row; otherwise a reader cannot distinguish an admission refusal from
real assistant output before a failure. A boolean content marker adds no new
failure lifecycle. Both platform notice writers set it, message reads expose it,
and session copies preserve it. Existing login records remain identifiable by
their durable authentication state. Historical unmarked content is not guessed
from wording or error status.

The renderer presents marked content once as an accessible red notice with an
error icon. An owned timeline explanation replaces its equivalent content
fallback; live diagnostics also remain available when there is no timeline
notice. Actual provider output and distinct diagnostics/attempts remain visible.
Errors lacking a specific explanation retain the generic error fallback. Neither
the marker nor presentation changes whether input was delivered or can retry.

Verify admission refusal through the actual API, normal/tail reads and session
copy, plus login regression coverage. Frontend cases cover marked notices with
and without a timeline, live diagnostics with partial output, missing diagnostics,
distinct attempts, replay/read-only views and non-color error identification.
