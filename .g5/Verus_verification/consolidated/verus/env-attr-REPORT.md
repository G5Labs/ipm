# Environment and attribute Verus uplift

Product: `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. Both inherited foundations and the index were left unchanged. The only existing Python model edited is `models/env_config.py`; its exact original is retained at `.g5/uplift/consolidated/extension/before-models/env_config.py` and `consolidated/extension/before-models/env_config.py`.

Run from the IPM checkout:

```
python3 .g5/Verus_verification/consolidated/verus/check_env_attr.py
```

The runner writes the exact command, stdout, stderr, exit code and actual dependency fingerprints to `verus/results/env-attr/checks.json`. It verifies both models, tries ten wrong implementations in disposable copies, compares the two independent Python oracles with all their claimed native fields, then generates concrete Verus checks whose expected values come from the raw native observations. Each generated check proves a property of the algorithm; none imports an observation as an axiom. No `assume`, `admit` or `external_body` is used.

Final decisive result: environment model **22 verified**, attribute model **50 verified including the imported 22**; no errors. The native bridges report 24 and 61 respectively, including imported models. Deduplicated by source symbol, this is **63 verification checks**: 50 in the models and 13 concrete native bridge checks (37 model proof functions, 13 model termination checks, 13 bridge proof functions). All **10/10** proof-negative controls fail at their postconditions. The independent native comparisons match **42 environment fields and 74 attribute fields**; all **15/15** environment and **23/23** attribute variants are rejected.

`env_config.rs` represents recursive merge with distinct object/array tags and finite tree depth, lookup through containers, loading files in order, failure clearing, command reset, per-process live-reference identity, compile-process visibility, and ICU replacement-text behavior. Primitive source values overwrite; container source values recurse; object/array and scalar/container mismatches fail. The depth budget is explicit: exhaustion is modeling debt rather than an IPM error prediction. Filesystem/JSON decoding and primitive environment parsing are contracts supplied to the model, with their correspondence checked on native cases.

`attr_evaluator.rs` represents the opening-brace gate, default/command/builtin precedence, local-before-system lookup, ordered repeated placeholder replacement, the unreachable circular-reference error, ASCII case/dot name matching, literal backslashes versus dollar replacement text, macro compilation/cleanup outcome, expression whitespace and swallowed failure, and the stored module/history/status outcome after a final commit. Macro compilation and arbitrary ObjectScript expression execution remain environment inputs. The full string parser, full ICU name grammar, and their composition remain open. The `evaluate` function selects a supplied pipeline outcome after the brace gate; it does not claim that the whole pipeline has been formally translated.

The after-message theorem assumes all phases and the final commit succeeded, no caller transaction exists, and normal history finalization runs. It preserves module and committed history when message evaluation returns an error, and distinguishes the successful message and brace-free macro neighbors. G's `top_step(cleanup_fails)` already represents the broad committed-module/error distinction; its `theorem_top_record_complete` says history success follows returned status. Those symbols cannot express which macro or message caused it, nor generated-class state. P's `honest` represents the intended transactional-error atomicity property on its original phase-result domain; extending it to after-message failure is refuted. Neither fact proves the shipped method body.

## Before/after native challenge

The new native run is `native/results/20261114T-env-ported-probes/`. Its source script is `native/scripts/env-ported-probes.cos`, with four file fixtures at `native/fixtures/env-ported/`. It uses the actual file loader in a fresh IRIS session on ARM64 IRIS 2026.1 build 234U and changes no database state.

| Probe | Actual file-loader observation | Original twin | Corrected twin |
|---|---|---|---|
| EP1 | `[{"x":1}]`, then `[{"y":2}]`, keeps `{"x":1,"y":2}` at position 0 | Lost x | Match |
| EP2 | `[1,2]`, then `[3]`, keeps `[3,2]` | Match | Match |
| EP3 | Missing second file: error, empty configuration | Match | Match |
| EP4 | Present scalar followed by another lookup component: empty | As-built match, prior variant unchallenged | Match; now rejects scalar-return variant |
| EP5 | New phase command with no files: empty configuration | Match | Match |

The corrected twin now recurses into objects at array positions and checks object/array kind compatibility. EP1–EP5 are retained in its independent native comparisons. The newly added `merge-array-object-replace` variant reintroduces the original mistake. EP4 rejects the prior surviving `getarg-error-on-scalar` variant. This supports restoring the eight withdrawn environment member credits and adds the previously uncredited `GetArg` member under the old rule; the overall count is computed by the parent coverage runner.

## Scope and open boundaries

The two manifests map stable members and source slices to the owning ontology, actual Verus symbols, properties, premises, cases and status. A scoped property is never reported as verification of an entire member. `env_native_bridge.rs` checks the complete native shapes of EP1 and EP2, including container tags, exact key domains and all leaves. `attr_native_bridge.rs` checks E05, E15, E20, E21, E23, E30, E32, E34 and P1/P2txn/P3 on the stated matcher/compiler premises. The other native fields remain independent Python correspondence, not formal bridges.

The environment merge/failure policy remains C:Q6. The macro brace-free policy remains C:Q2. Full ICU regex grammar and capture availability, arbitrary expression execution, command path trimming in Verus, concurrent generated-class name races, the archive message site, and caller-held transaction outcomes remain open. Environment replacement can loop if a value recreates a placeholder; no termination theorem about that product loop is claimed. Current locks, singleton allocation and release internals are not proved by the live-reference identity abstraction.

The concrete E34 backslash bridge initially exhausted the default solver budget. It now uses general prefix and inverse lemmas, proves roundtrip for all dollar-free strings, and verifies under the default budget. The final results contain no timeout or resource-limit result accepted as a pass.

## Canonical ontology edits proposed to the parent

- `module.env`: put observed recursive merging at array positions beside ordered merging: later primitive values overwrite, object/array values recurse by key or position, old array tail remains, and mismatched kinds fail and clear configuration. Keep intended policy in linked C:Q6 rather than declaring the observed result intended.
- `module.parameter.expressions`: put the as-built dot-regex name distinction, the macro compile-error class residue, local-before-system nested lookup and no-brace gate beside their rule. Keep full regex grammar and arbitrary interpreter behavior marked as environment premises.
- `module.parameter.literal-values`: retain literal substitution intent; record that backslashes roundtrip while a matched `$5` raises, with an unmatched price parameter as the valid neighbor. Do not weaken the literal-value rule to bless the defect.
- `module.parameter.cycles`: retain intended circular-reference error and attach the observed unreachable guard and residual reference witness.
- `load.status.after-commit` / `load.after-install-message`: preserve final committed module/history when message failure returns ERR; distinguish error status, history Success=0 and Committed=1, not merely status.

All proposed policy decisions and product repairs are separate from the as-built model.
