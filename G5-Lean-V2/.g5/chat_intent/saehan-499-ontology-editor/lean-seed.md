# Lean workspace seed — 2026-10-14

The human approved implementing and running `just seed-lean-workspace` against the
existing localhost stack: use local committed HEAD including `.g5/ontology` and
unpushed commits; preserve ancestry; create G5-Lean with an independent code copy
and empty docs; leave source and other workspaces intact; refuse existing targets
unless replacement is explicit. Existing seed commands remain unchanged. Only the
committed source mode is in scope.

The human subsequently requested exactly one ordinary standalone G5-Lean workspace,
with normal future forks. They authorized updating the command and tests, resetting
the local dev stack (discarding its workspace data), reseeding and verifying a
disposable fork, then removing that fork. The artificial seed parent is rejected.
