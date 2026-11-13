# One value becomes visible at commit

**Intent ID: value-commit.** A new store exposes `(not committed, 0)`. Committing
publishes `(committed, 1)` together, and the committed value remains 1. Internal
preparation may occur without changing that visible state. A store commits at
most once.

This example covers one store, one commit and no concurrent observers or writers
inside an operation. It makes no promise about eventual commit, scheduling,
crashes, durable storage, authorization or multiple requests. Those would need
additional intent, state and checks when applicable to a real system.

The staged-path witness is evidence that the implementation model includes its
actual publish path; it is not an additional user requirement to use staging.
