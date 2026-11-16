# Semantic-version translation and resolver boundary

Product pin: `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`. `semver.rs` is a hand translation. No ObjectScript body is mechanically verified. The governing IDs are `version.order`, `version.range`, `version.range.partial`, `version.range.combine`, and `load.refusal.downgrade`; `semver.manifest.json` gives member slices and exact symbols.

`parse` keeps the raw five fields that IPM stores. `version` translates the numeric fields into naturals for the supported unsigned decimal domain. `follows` takes the actual `SemVerPostRelease` choice as an input. It preserves the difference between a release downgrade and two incomparable prereleases with different cores. Build metadata and prerelease text remain distinct, including IPM's build-hyphen parser error. ASCII dot-identifier collation includes canonical signed integers; the locale-dependent non-ASCII case remains open. Main version fields with signs, decimal forms, or noncanonical equality are outside this correspondence claim.

`expand`, `parse_range`, and `expression_set` recover accepted numeric range tokens, caret and tilde forms, partial/year versions, hyphen bounds, unions, and inline AND. `intersect` keeps the first equally restrictive bound after comparing major/minor/patch and operator, so prerelease differences can produce order dependence. The mixed-operator, exact-conflict and empty-inline outcomes are distinct. `and_error` retains the literal `%1` and `%2` placeholders in the observed empty-inline diagnostic. Malformed comparator validation and complete formatted error messages remain open. No behavior is inferred from an invalid token accepted by the translation.

`diamond` composes current-requirement-first search with the parent-name-order final reader; `diamond_composition` proves that composition given an explicit selected version. The native checker checks the selected version against the actual error/installed row and the final admission result. Repository-offered ordering and name-order parent visitation are explicit premises. This is a two-parent diamond abstraction, not a replacement for the full inherited dependency graph.

The existing G `model::guard_refuses` compares bounded numeric version codes; it already represents load refusal control flow but cannot distinguish the real prerelease input. P `dep_resolve::dep_refines` proves the inherited graph invariant under PU/PR, exact ISet range algebra, and versions ordered as naturals. That theorem is retained on that domain. The new witnesses invalidate transferring its exact-range-algebra premise to arbitrary IPM ranges. `exact_range_bridge` is only a congruence fact, not a graph-translation theorem.

Run from the IPM root:

```
python3 .g5/Verus_verification/consolidated/verus/semver_check.py
```

The checker reads native observations as expected assertions, never as axioms. Parsing and canonical comparator fields are checked separately from range satisfaction to stay within Verus's compute interpreter depth. Accepted expression assembly is checked field by field, and renderer outputs are checked independently. It retains generated `semver_native.rs`, exact Verus commands, stdout/stderr, input hashes, tool hashes, and native receipt hashes in `semver-results.json`. Counts are assertions about finite native cases, not distinct native fields, member proofs, or an ObjectScript theorem.

Four bad translations are challenged: total ordering of different-core prereleases, last-bound wins, major-wide partial caret, and correct build-hyphen parsing. A control counts as rejected only on a logical assertion/postcondition failure; parser failures and compute-depth errors do not count.
