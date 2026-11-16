# Trust and scope: models/verus/scopes.rs

| # | Item | Why trusted | What checks it |
|---|---|---|---|
| S1 | Phase lists transcribe `GetCompletePhasesForOne` (Lifecycle/Base.cls#L165-L189); the "*" refresh marker is omitted | Hand transcription | Controls `verify-runs-test-phase`; IPM tests Scopes, IncludeTests (T) |
| S2 | `on_phase` transcribes Document.OnPhase Reload/Compile/own-phase handling (Document.cls#L59-L210); generated resources are compiled but never loaded (#L195-L201) | Hand transcription | 10 controls incl. `generated-never-compiled` (the model's own earlier error, review REVIEW-2 N4) |
| S3 | Export transcribes `%Export` IncludeTests handling (Lifecycle/Base.cls#L1306-L1316) and the generated-resource skip (#L2047) | Hand transcription | Controls `export-ignores-manifest`, `explicit-param-cannot-drop`, `export-includes-generated` |
| S4 | Out of the model: preloaded resources, custom phases, deployed resources, Compile="false" | Scope | Statements on those carry no P from this file |
| S5 | Premises: none | — | `// PREMISES: none` |
| S6 | No executable twin or IRIS conformance replay for this model | Effort | IPM tests Scopes (1 skipped assertion: registry setup) and IncludeTests run here (T) |
