# Trust and scope: models/verus/dep_resolve.rs

The resolver model's trust items D1–D12 (abstractions, bounds, premises PU and PR, the version tie D12) are kept in
`update_steps.TRUST.md`, section D, written by the same lane; this file points there so every proof file has its own
TRUST entry (review REVIEW-2 N9).

| Item | Where |
|---|---|
| Transcription fidelity of BuildDependencyGraph and helpers | update_steps.TRUST.md D1–D5; cross-checked by `models/explorer/inst/check.py` (twin vs fine resolver, 70,656 instances) |
| Premises PU (no manifest names a module twice), PR (no requirement names the root) | update_steps.TRUST.md D6, D7 |
| Failure reasons (loop or no admissible version) | D8–D11 |
| The failing requirement's module version is not tied to its chosen version | D12 |
| Termination is not claimed | dep-explore-termination is a bounded check only |
