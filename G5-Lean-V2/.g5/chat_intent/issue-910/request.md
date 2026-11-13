# Owner request

2026-10-28: “Work on G5Labs/G5-Lean#910: Retire codex exec from all G5-managed agent
execution. Before proceeding with this implementation, come up with a plan and evaluate
the risk of the implementation. Proceed with implementation only after my approval.
Read the issue with `g5d issue show` before you start.”

The linked issue supplies scope and acceptance criteria. Interpretation: produce the
design and risk evaluation first; implementation, ontology-dependent code and
publication wait for explicit owner approval of this design.

2026-10-28 follow-up: “ok, proceed. first doing the tests with the real binary.”
Interpretation: the design is approved for implementation. Validate App Server
formal-review confinement and report delivery with the pinned native binary first,
before dependent formal-review code; a failure there blocks the formal migration
rather than becoming an exec exception.
