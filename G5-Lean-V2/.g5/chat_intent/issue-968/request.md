# Native question deferral and the question card (#968)

The owner requested work on [issue 968](https://github.com/G5Labs/G5-Lean/issues/968):
design, implementation, verification, a PR, green checks, no merge conflicts, and inline
replies to high-priority review findings; later, rebases onto `main` without merge commits.

## Human instructions

On 2026-11-03, after review, the owner wrote: "I do not want user to know this much
details below, let's not show to the UI surface", listing the two new question-card
statuses and their labels ("Answer saved, not delivered", "Question not kept"), the fact
that they replaced the "Stopped" label in the card summary and tool status, that users
would have to understand two new ideas (an answer saved but not delivered; a deferred
question not kept), and the four explanation sentences the card showed for those states.

On 2026-10-31 the owner also pointed out that the delivery-state ontology block restated
two rules the recovery decision owns and that the duplicates would drift.

## Interpretation

The card keeps the familiar "Stopped" label for both states and explains no cause; the
turn's existing notice remains the one place a failure is stated, in short plain words.
The saved answer stays visible with the "Send as a new message" action, because that is
the recovery the issue requires and it introduces no new vocabulary. The internal
mechanics (end reasons, the undeliverable retry disposition, the withdrawn question) are
unchanged and remain owned by `g5.chat.native-question-recovery`.
