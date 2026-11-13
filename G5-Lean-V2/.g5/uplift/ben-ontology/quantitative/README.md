# Quantitative coverage evidence

Assessment date: October 15, 2026. Application and governing-body baseline:
`072a12d3e99032ee3a7356837542c5edeb825984`. Subsequent archival changes relocate
formal evidence links without changing the inspected application or governing bodies.
This directory implements the requested measurement experiment, not a harness change,
new ontology, production repair or runtime test suite.

## Reproduce and inspect

The [original measurement script](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/uplift/ben-ontology/quantitative/measure.py)
and generated proof ledgers are archived with the removed proof artifacts. The
symbol/behavior inventory remains in this PR. Historical reproduction requires the
script at its original path, the recorded audit inputs and a checkout matching the
declared application/ontology baseline; the script deliberately refuses source drift.
Its dependencies are Python 3.12, `tree-sitter==0.25.2`,
`tree-sitter-rust==0.24.2` and `tree-sitter-typescript==0.23.2`.

The actual run used the bundled Python 3.12 and those dependencies installed in a
temporary directory outside the repository. The initially attempted tree-sitter 0.26.0
binding crashed; its output was discarded. The retained inventory uses 0.25.2.
No dependency, workflow or harness file was changed.

The archived script reads the source inventory from the pinned Git tree and unchanged
application files. It refuses changed application sources or governing ontology bodies.
It reads historical formal artifacts directly from pinned Git objects without restoring
the archived suite. It regenerates derived tables, not the four human-readable domain
assessments or their hand-reviewed JSON inputs. A baseline change needs another semantic
review, not a hash refresh.

| Artifact | Meaning |
| --- | --- |
| `baseline.json` | Revision and declared source scope. |
| `files.tsv` | Every inventoried source file, hash, lines, parse status and syntax counts. |
| `symbols.tsv` | Every recovered named callable/type declaration with location, test disposition and candidate relationships. |
| `excluded-source.json` | Tracked source-like files outside the declared Rust/TypeScript application inventory. |
| `parse-errors.json` | Every parser recovery/error location; these are parser limitations, not a compiler diagnosis. |
| `identity.json`, `runtime.json`, `workspace.json`, `platform.json` | Independently selected obligations, actual reads, governing IDs, dispositions, historical properties and proposals. |
| `inspected-ranges.json` | Union of this pass's recorded current-file reads, with hashes; includes supporting documents separately identifiable by path. |
| [Archived formal read ranges](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/uplift/ben-ontology/quantitative/archived-inspected-ranges.json) | Reads of archived formal documentation; excluded from current-file inspection credit and removed from this PR. |
| [Archived proof evidence](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/uplift/ben-ontology/quantitative/formal-evidence.json), [reference checks](https://github.com/G5Labs/G5-Lean/blob/345b546e9fed5477e6fdf3c77ad87002ed558dd6/.g5/uplift/ben-ontology/quantitative/formal-reference-checks.json) | Recorded receipt integrity, source drift, case classes and witness sequences. Detailed proof data is removed from this PR; aggregate counts remain historical. |
| `summary.json` | Recomputed counts, not an overall assurance score. |

Domain reasoning: [identity](identity.md), [runtime](runtime.md),
[workspace](workspace.md), [platform](platform.md). The combined results and proposed
commitments are in [the coverage report](../coverage.md#quantitative-assessment--october-15-2026).

## What is actually counted

Source scope is tracked `backend/**/*.rs` and `frontend/src/**/*.{ts,tsx}`.
SQL migrations, shell/Python automation, Terraform, separate tools/test trees and frontend
files outside `src` are not silently included in a claim of complete repository coverage.
Some were inspected as supporting evidence; their exclusion is only from this symbol
denominator. Their complete source-like path list is retained.

Rust functions/signatures, structs, enums, traits and type aliases are counted. TypeScript
functions, methods/signatures, classes, interfaces, enums, aliases and directly named
function-valued variables/fields are counted. Anonymous callbacks and macro expansions
are not named symbols. Test paths, test attributes and cfg predicates that require
`test` are separated; `cfg(any(test, production_condition))` remains potentially production.
This is a source-declaration inventory across configurations, not a compiled call graph.
Parser recovery remains explicit, including four files outside test paths. Callable
signatures without bodies are reported separately. Counts are approximate recovered
declarations, not an assertion that every executable function has been recovered.

`obligations` on a symbol means its source range overlaps an inspected range attached
to that audit row. `row_owner_candidates` lists that row's governing/nearest owners;
it does **not** assert that every listed owner governs every overlapping helper. This
conservative index supplies navigation for further semantic review. A missing-intent
row can have nearest-owner candidates without satisfying the missing obligation.
`explicit_file_refs` and `metadata_basename_refs` are separate, weaker file-level links;
the latter resolves only unique source basenames explicitly mentioned in pinned ontology
metadata. Indirect investigation links are not expanded into blanket file/symbol credit.
There is no combined verified mapping percentage.

`full_range` means the declaration's entire line span lies within the union of recorded
reads; `partial_range` means overlap only. Reading a line is not proof, branch execution
or an independent claim to understand every clause in a large function. These fields
credit this pass, not all historical inspection ever performed.

The syntax-site count includes explicit if/match arms, loops, try/catch and conditional
expressions. It is neither a control-flow graph nor an executable branch/path denominator;
short-circuit, implicit exceptional, generated and cross-process transitions are incomplete.

## Semantic and path denominators

The 63 behavior groups were selected independently from source entry points, durable
writers, handoffs, consumers and recovery, then compared with governing bodies. They are
risk-directed and nonrandom. The domains own distinct rows; shared ontology references
are intentional and are not duplicate coverage credit. A row can contain several
related obligations. Changing its granularity changes the ratio, so compare future
measurements only with the same versioned rows or disclose additions/splits.

`adequate` means the declared behavior is entailed by governing text; it does not mean
code conforms. `partial` names missing qualifiers, `missing` names a missing consequential
commitment, and `question` preserves unresolved policy. `traced` means the named bounded
producer/consumer/recovery slice was followed; all other enforcement remains `partial`.
No row claims exhaustive implementation-path recovery. The domain reports also list
uninspected consumers and omitted consequential areas, which are not manufactured into
extra "covered" or "implementation detail" rows.

Archived witness traces contain one reached path per named witness goal. Equal action
name sequences can still carry different state values; distinct action sequences are
not distinct semantic paths or a denominator for all model behaviors. Safety runs explore
more states than their witness logs show. We did not collect TLC action statistics,
extract an interprocedural call graph, execute new TLC cases, run runtime tests or
instrument line/branch coverage in this assessment.

The receipt source binding is domain-wide: any changed bound file makes the full input
set stale for that receipt. Therefore 0 fully unchanged receipt source sets does not
mean every model is semantically wrong. It means a current-source verification claim
requires reviewed, finer source correspondence and appropriate checks. Archived model,
configuration, case, binding, runner and log hashes establish retained evidence integrity,
not current production correctness.
