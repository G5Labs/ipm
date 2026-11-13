# Owner request

2026-10-24: “Work on G5Labs/G5-Lean#809: Migrate Codex to App Server with explicit turn completion and test coverage parity. Read the issue with `g5d issue show` before you start.”

Follow-up: “continue the work in the previous prompt, also upgrade codex binary to the latest stable version.”

The linked issue supplies the migration scope and acceptance criteria. Interpretation:
implement the infrastructure migration and its tests without adding interaction features;
latest stable is resolved at the time of this request, then pinned exactly.

2026-10-25 follow-up: the owner asks for tests proving both exact provider binaries
support every offered model, native mode and per-model effort, with no UI/API/backend
catalog drift. Interpretation: add a required exhaustive native compatibility gate
and fix discrepancies it exposes; preserve G5's existing noninteractive Codex policy.

2026-10-25 follow-up: “can we update claude cli to the most recent version while
we are makin this change? before making the change evaluate what has changed
between the upgraded version and the version currently in use”. Interpretation:
evaluate the release delta first, then upgrade the shared runtime pin to the most
recent stable release and verify affected native contracts in this PR.

Clarification: “upgrade claude to latest stable, not latest.” The explicit stable
channel requirement selects npm's `stable` tag, currently 2.1.308.
