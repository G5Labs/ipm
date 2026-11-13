# Intent, formal machines and executable fixture

| Layer | Artifact and symbols | Correspondence |
|---|---|---|
| Human commitment | `intent.md`, `value-commit` | Commit exposes value 1 together with committed status; preparation has no visible effect. |
| L1 intent machine | `Intent.tla`: `Init`, `Commit`, `Spec` | `committed` and `value` are the visible pair; `Spec` permits stuttering. |
| L2 operation machine | `Implementation.tla`: `Init`, `Stage`, `Publish`, `Direct`, `Wait` | `phase` and `stored` transcribe fixture state; `sawStage` is history used only for the path witness. |
| Abstraction map | `VisibleCommitted`; `Abstract` instance | Normally `committed = (phase = "done")` and `value = stored`, exactly the pair returned by `Store.read`. |
| Refinement obligation | `RefinesIntent` | The mapped initial state satisfies L1 `Init`; every mapped transition satisfies L1 `Next` or leaves the visible pair unchanged. This is equivalent to the instantiated L1 safety specification. |
| L3 teaching fixture | `store_fixture.py`: `Store.__init__`, `stage`, `publish`, `commit_directly`, `read` | Initialization matches `Init`; `stage` matches `Stage`; `publish` matches `Publish`; `commit_directly` matches `Direct`; observation matches the map. `Wait` models no operation. |
| Path evidence | `NoStageThenCommit`, `NoDirectCommit` | Negated reachability invariants. Their *targeted* counterexamples establish a stage→publish path, or a direct commit path, respectively. |

The test suite compares the actual staged witness's `(phase, stored)` sequence
against calls to the Python fixture and checks the bad-write and direct outcomes.
That is representative executable correspondence evidence, not a proof covering
every execution of Python. Operations are assumed atomic relative to observations;
the Python language alone does not provide that whole-method guarantee.

The deliberately different variants answer different review questions:

| Variant | What changes | Correct conclusion |
|---|---|---|
| `good.cfg` | Pipeline, correct map, publish 1 | Bounded refinement holds; the independent staged witness is reachable. |
| `bad-map.cfg` | Only the map treats staging as committed | Model-correspondence defect: the map contradicts `Store.read`. Correcting the map restores the model's fidelity; no runtime bug is established. |
| `bad-write.cfg` | Publish writes 2, matching `publish(bad_write=True)` | Behavior defect against existing intent. The final counterexample matches the executable fixture's wrong value. Fixing the write changes behavior, rather than merely relabeling state. |
| `stuck.cfg` / `stuck-witness.cfg` | Remove publish from the model | Safety refinement passes, but the required implementation path is absent. This under-approximates the fixture. It is not a new liveness requirement or a valid repair of the fixture model. |
| `weak.cfg` | Replace refinement with a type invariant allowing 0..2 | TLC correctly checks an insufficient statement; it misses the wrong published value. A completed checker run does not establish specification completeness. |
| `direct.cfg` / `direct-witness.cfg` | Replace stage→publish with one direct commit | A genuine implementation alternative: fewer internal states, same L1 visible contract in this bound. It is a candidate simplification only where integration does not require a separate staging API or side effects. |

There are three reachable states in the normal pipeline and two in the direct
or stuck variant. The models are intentionally tiny to expose these distinctions.
The staged witness rules out the specific omitted-publish case; it does not prove
that a large model is generally nonvacuous, complete, or faithful to all code paths.

TLC 2.19 identifies the failed refinement conjunct as
`line 22, col 35 to line 22, col 66 of module Implementation`, the bracketed
next-state formula in `RefinesIntent`. The adapter requires that exact reviewed
label and a config selecting only `RefinesIntent`. Changing the definition's
location requires reviewing and updating the label, not accepting an arbitrary
property failure. Negated reachability invariants are matched by their exact names.
