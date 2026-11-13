# Claude questions with fixed permission presets

## Human request

On 2026-10-11, the user distinguished provider permission modes from clarification questions and asked what changes are needed if G5 keeps only its existing Claude Bypass and Codex Full Access product presets, enables Claude `AskUserQuestion`, and leaves Codex unchanged. The initial request was for a design.

## Design interpretation

Keep the existing fixed Mode display. Enable Claude's native clarification channel under Bypass without implementing the other provider modes or Codex app-server. Apply the policy through the shared dispatch owner for conversational and templated inputs. Preserve already frozen attempts, explicit tool restrictions and provider permission rules. Retain existing policy rows for compatibility; removing older API or saved policy choices is outside this change.

The user then invoked `build-to-ship` to implement and ship the design. They explicitly took ownership of deployment cleanup: after this PR deploys they will stop all VMs and delete all hosts so replacement VMs run the same code. Mixed-version deployment machinery is outside the requested implementation.

## Question lifetime correction

The owner subsequently required clarification questions to wait without automatic expiry and remain visible and answerable after workspace VM reattachment. The owner selected the happy path in which Claude native session JSONL is persisted and restored, and asked that answering send the answer back to the agent. A generic conversation-message fallback is outside this request. Explicit chat Stop/Cancel can still end the work.

Agent design: preserve the pending tool with Claude's native deferral and restore the same native session and tool-use identity before delivering the accepted answer through its current native response channel. Exact-pin feasibility passed; product implementation and VM recovery validation remain outstanding.

## Clarification status correction

On 2026-10-12, the owner reported that a native question immediately displayed a permanent “Stopped” badge and the label “pending approval”. The owner agreed to “Waiting for your answer”, then “Answered”, with no stop badge for normal question deferral and “Stopped” retained for actual cancellation, and explicitly invoked `build-to-ship` to fix both labels.
