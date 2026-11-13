# Can the native coder act on concrete review feedback?

**Yes for the four bounded obligations tested here.** The original I-O author received
a short description of four confirmed gaps and discriminating behavior, plus the corrected
nudge contract. It received no evaluator tests, code patch, new skill or prescribed
implementation sequence. Its separate repair commit is
`caef0c35592b0e063861110c6ad312c8da091c87`; first delivery `844536f` remains preserved.
See the protocol (`repair-protocol.md`) and repair delta (`implementations/i-o/repair-delta.patch`).

## Independent recheck

The same final common probe bytes were applied to both original candidates and the
repair. The ten primary cases were unchanged; the two post-review cases used the same
strengthened lifecycle-owner nudge setup. Probe hashes match across all three runs.

| Obligation | First I-O delivery | Repaired I-O |
|---|---|---|
| Accepted model survives config change before dispatch | Failed | Passed through actual reservation/invocation-payload inspection |
| Valid future capacity nudge permits fresh retry | Failed | Passed after the real lifecycle owner creates the nudge |
| Job cap one blocks a second ready descendant | Failed | Passed after the first actual reservation/start confirmation |
| TaskService owns complete task/initial-input allocation | Architectural deviation | Independent source tracing confirms TaskService's substantive validation/allocation operation, with ChatService retaining session creation |
| Other nine common runtime functions | Passed | Still passed |

The repaired candidate passes all **12** common test functions. This is bounded evidence,
not a 10/10 PR rating. The execution log (`implementations/i-o/repair-probes.txt`) and
[source adjudication](repair-and-selector-adjudication.md) distinguish runtime checks
from architecture inspection. No real provider turn or browser journey ran.

The repair also rechecks live credential/account eligibility while using accepted launch
choices, and rechecks job capacity after acquiring the existing workspace settings lock.
The author's own targeted tests include credential removal and two existing dispatch
regressions; these are reported in verification (`implementations/i-o/repair-verification.md`),
separate from the independent suite. Previously created retry tasks without the new
accepted-launch field retain legacy resolution; the patch does not reconstruct historical
accepted choices.

## Effort and scope

The repair took **9m56s**, another **28,041 output tokens** and **110,147 noncached input
tokens** in the author's existing context. Its code/test delta adds 524 and deletes 234
lines, including moving initial-input work from ChatService into TaskService. This is
about 41% of initial I-O output again, plus the separate review/evaluation effort. Concrete
supervision is effective here and is not free. No additional compaction occurred during
the repair.

Other first-delivery UI recovery and sync projection findings were deliberately outside
this four-obligation repair and remain open. User-scoped provider transfer, concurrent
reservation stress, full browser recovery and real guest execution remain unverified.
The maintained application's source was not changed by this experiment.

## Interpretation

The coder could implement the missing behavior once review selected and explained the
right boundaries. That supports a useful division of work: native coding handles the
repair; supervision establishes the intended outcome and the counterexample that must
stop failing. It does not show that ontology alone caused the repair, since feedback
explicitly supplied all four issues, or that an automatic review will reliably find
them. The selector trial demonstrates why that latter question remains difficult.

This case therefore supports confidence built through **design, independent evidence,
targeted repair and recheck**, with remaining boundaries stated. It does not support
confidence from an initial conformance declaration, nor an expectation that a heavier
generic harness will eliminate supervision.
