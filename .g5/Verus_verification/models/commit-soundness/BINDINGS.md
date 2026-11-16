# Commit marker soundness

Does the marker follow retained writes for a live top-level load without a caller transaction?

The proof covers the modeled live top-level load with no caller transaction. It permits the interval between commit and marker write. Refusal policy, nested callers and whole-product correctness remain qualified in the coverage map. The receipt reports the whole crate's obligation count.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Commit-soundness definition and lemmas](../../crates/ipm-load-history/src/proofs.rs)

[Product bindings and assumptions](../../crates/ipm-load-history/BINDINGS.md)

[Coverage and reliance](../../coverage-map.md)
