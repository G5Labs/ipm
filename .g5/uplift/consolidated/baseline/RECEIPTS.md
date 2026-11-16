# Import and baseline receipts

## 1. Direct import of the playbook foundation (plan step 1)

Branch `consolidated_uplift` at `0ac7b931fe6245366ae6c873393dabf1fc9590ca` (G). Destination `playbook-uplift/` did not
exist. Commands, run from the repository root:

```
git restore --source=fdd3669091814f1ad8116bc0fff167082ae6edfd --staged --worktree -- playbook-uplift/
git diff --cached --exit-code fdd3669091814f1ad8116bc0fff167082ae6edfd -- playbook-uplift/   # exit 0
git diff --exit-code -- playbook-uplift/                                                       # exit 0
git write-tree --prefix=playbook-uplift/      # 744959d57752cca670547384c6334c497377d3b6
git rev-parse fdd36690:playbook-uplift        # 744959d57752cca670547384c6334c497377d3b6
```

869 files staged, all under `playbook-uplift/`. `git diff --quiet HEAD -- . ':!playbook-uplift'` and
`git diff --quiet 571e04a1 HEAD -- src preload tests modules wheels module.xml` both exit 0: neither IPM nor the G
baseline changed. The import is staged, not committed. The untouched reference is commit `fdd36690` itself.

## 2. Reproducing the comparison measures (plan step 2, audit acceptance 1)

From the benchmark root: `inventory.py --check`, `render.py --check`, `linkage.py --check` and `check_samples.py`
(all under `results/experiments/ipm-uplift-comparison-20261113/`) exit 0. `linkage.py` prints
`1216 {'G ontology': 32, 'P ontology': 174, 'G models': 11, 'P models': 29}`; the unions recomputed from
`source-links.json` are 37 model-cited and 179 ontology-cited members. `inventory.py` checks 3,021 G files against
`0ac7b931` and 869 P files against `fdd36690`, with CRLF normalization for two P logs.

This checkout's `.g5` equals the frozen copy the comparison scored: `diff -rq --no-dereference ipm/.g5
results/cloud/ipm-verus-quality-review-20261113/final/.g5` reports only the new `uplift/consolidated` folder (a plain
`diff -r` complains about dangling symbolic links in a review scratch folder, which exist in both trees).
`playbook-uplift/` equals `results/experiments/ipm-playbook-uplift` except `__pycache__` folders and the two CRLF logs.

The frozen denominator is [`coverage/members.tsv`](../coverage/members.tsv) (1,216 members in 144 of 177 files),
produced by [`coverage/members.py`](../coverage/members.py) with linkage.py's unmodified rule; `members.py --check`
exits 0.

## 3. Reproducing the inherited proofs (disposable copies)

| Arm | Command | Result | Note |
|---|---|---|---|
| G | `verus lib.rs --crate-type=lib --num-threads 4` on a copy of `crates/ipm-load-history/src` with `pub mod explore;` commented out | `verification results:: 263 verified, 0 errors` | The receipt toolchain is x86-64 Linux; this used the same Verus version for arm64-macos. `explore.rs` is the plain-Rust explorer harness (needs the vendored `explorer` crate), not proofs. The crate's own `cargo verus` path needs `vstd` from the cargo registry, which was not fetched. |
| P | `python3 -I models/verus/controls.py` in a copy of `playbook-uplift` | 8 files, 227 verified, 0 errors; 195 of 195 controls killed in the expected functions | Same toolchain as P's records. |

Explorer runs and P's `full_rerun.sh` were not repeated (`full_rerun.sh` rewrites evidence from recorded logs; it is
not a fresh IRIS run).

## 4. Baseline B under the evidence rule

Two independent graders, one per arm, read the models, bindings, receipts and product source and graded every cited
member (any citation form) against the plan's evidence rule: [G-baseline.tsv](G-baseline.tsv) and
[notes](G-baseline-notes.md), [P-baseline.tsv](P-baseline.tsv) and [notes](P-baseline-notes.md).

| | Qualifying | Ambiguous | Cited, not qualifying |
|---|---:|---:|---:|
| G | 23 | 8 | 27 rows |
| P | 64 | 12 | 42 rows |
| Union B | **81** (6 in both arms) | | |

24 of B are among the 37 citation-linked members; 57 were linked only by forms the citation parser misses (bindings
tables, relative ranges, explorer files). B is a recovered baseline, not a gain. Ambiguous members are outside B; if
all 20 were accepted, B would be 101 (some overlap). The plan's earlier "demonstrated minimum" of 23 is a subset of B.

## 5. Calibration of the inherited checkers (plan step 2, round 1 challenge)

[round1/checker-calibration.md](../round1/checker-calibration.md): G's `evaluate.py` accepted five plausible wrong
observations (a surviving dependency row, a parent row carrying the dependency's package, a developer-mode commit
reported as rolled back, a load returning with an open transaction, a conflicting duplicate line); P's explorer and
checks accepted wrong class fields in the `nc-gap` lane, an open transaction level, and stripped phase results. The
repairs to G's evaluator are in [round1/checker-repairs.md](../round1/checker-repairs.md). P's checkers are left as
imported and their gaps are recorded there.
