# Error presentation follow-up

The owner asked whether failures clearly look like errors and whether the sign-in
presentation correction can be extended across error types in PR #812. They
authorized a straightforward extension. The earlier sign-in requirements remain
in [issue #810](https://github.com/G5Labs/G5-Lean/issues/810#issuecomment-6392096323).

Interpretation: use an error icon and red styling for chat failure notices, show
an explanation once, and preserve genuine assistant output and failure identity.
This concerns chat failure presentation, not process-mode parity or changes to
retry, admission or delivery policy.
