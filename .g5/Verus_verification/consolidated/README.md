# Consolidated foundation: differential twins checked against IPM on IRIS

> Verus extension: new input-driven models, controls and native comparisons are in [verus/](verus/README.md). Current measurements and incomplete critical coverage are in [the extension report](../../uplift/consolidated/extension/REPORT.md). The original Python/native consolidation below is retained as historical evidence.

Work of the IPM consolidation ([plan](../../uplift/consolidated/PLAN.md), [final handoff](../../uplift/consolidated/HANDOFF.md)).
It sits beside the two inherited foundations, which stay as they were: G's Verus crate and native replays in
`../crates`, `../native`, `../followup`, and P's eight Verus files and explorers imported intact at
[`playbook-uplift/`](../../../playbook-uplift/README.md).

## Pins

| Item | Value |
|---|---|
| Product | `intersystems/ipm` `571e04a19aba9f9b4bc0ea23fbe09182038f4e55` (unchanged; `git diff --quiet 571e04a1 HEAD -- src preload tests modules wheels module.xml`) |
| IRIS | `containers.intersystems.com/intersystems/iris-community:2026.1` (image `546fe52b9e68`), build 234U, ARM64, container `ipm-consolidated-iris`; snapshot after the IPM install: `ipm-consolidated-iris:571e04a1-fresh` (`8db58b93cf76`) |
| Python | 3.11 (no third-party packages) |
| Verus (inherited proofs only) | `0.2026.09.27.3cf1832`, `~/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/` |

## Layout

| Path | Contents |
|---|---|
| `models/*.py` | One executable twin per product behavior ("family"): `MEMBERS` (product member, source slice, model function), `VARIANTS` (wrong models, each attributed to the member whose own behavior it changes), `predict(variant)`, optional `CONTRACT` (the governing rule, with the as-built verdict expected) |
| `checks/framework.py` | Compares every claimed field of the as-built twin with the native run, and every variant with the same fields; credits a member only when the twin matches everywhere and a variant of that member is rejected by a native field |
| `native/iris.sh` | `up` (fresh container and IPM install), `reset` (from the snapshot), `exec NAME SCRIPT` (writes `results/<UTC>-NAME/` with `script.cos`, `raw.log`, `zversion.txt`, `receipt.json`), `state` |
| `native/scripts/*.cos`, `native/fixtures_*.py`, `native/fixtures/`, `native/harness/Obs.cls` | Terminal scripts, fixture generators and the observation harness (`OBS <case> <field>=<value>` lines). The harness is test code loaded into USER, not IPM |
| `native/results/` | Every native run, including superseded ones (suffix `.superseded-*` with the reason) |
| `results/checks/` | One JSON per framework run; `latest.json` is the last full run |
| `../../uplift/consolidated/coverage/` | `members.py` (frozen 1,216-member denominator), `count.py` (coverage and its seeded controls), `counts.json` |

## Reproduce one obligation and its control

Working directory: `.g5/Verus_verification/consolidated`.

1. All families against their recorded native runs: `python3 checks/framework.py`. Expected: every family
   `as-built=MATCH`, 125 of 127 variants rejected (the two survivors, `create-sort-last` and `getarg-error-on-scalar`, are explained in the R1 and R5 handoffs),
   `contract-unexpected=[]`.
2. One obligation, C:D6 (`<2` admits 2.x): `python3 checks/framework.py models/semver.py`, then in the newest
   `results/checks/*.json` look at `families[0].variants["range-partial-keeps-operator"].rejected_by` — the fields
   X26-X29 and S1 reject the intended reading — and `contract` entry "a dependency declared <2 is installed below 2.0.0"
   with `holds=false`.
3. Its native observation: `native/results/20261114T031048Z-r2-resolve/raw.log` shows `OBS S1 mod.sv-lt-dep=present version=2.5.0`.
   To rerun it: `./native/iris.sh reset && python3 native/fixtures_r2.py && FIXTURES=c2 ./native/iris.sh exec r2-resolve native/scripts/r2-resolve.cos`,
   then point the family's `RUNS` at the new directory and rerun step 2.
4. Coverage and its controls: `python3 ../../uplift/consolidated/coverage/count.py` (expected: B=81, new=68, lost=0)
   and `count.py --selftest` (a citation-only row is not counted; removing a model function drops that member; breaking
   one prediction drops the whole family).

## What a result means

A credited member has a source slice, a model function that transcribes its own decision, a native IRIS observation
on the path, an as-built twin that reproduces every claimed field, and a wrong variant of that member's behavior that
the observation rejects. This is native correspondence for the observed cases, not a proof: the twins are hand
transcriptions, the inputs are finite fixtures, and no ObjectScript body is checked mechanically. Contract checks
whose expected value is `False` document product defects; they are not failures of the foundation.
