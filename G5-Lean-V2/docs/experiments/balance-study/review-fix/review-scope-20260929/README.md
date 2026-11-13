# Can reviewers infer their obligations from ontology changes and governance?

2026-09-29. **The main review areas were repeatable across these three features;
complete recovery of their governing conditions was not.** Both repetitions produced
useful inventories. They also omitted some of the same rules, even after retrieving them.

We ran **two fresh GPT-6 Sol/xhigh workers on each of three features**. Workers received
the request, actual historical ontology diff, changed IDs, governing references and
searchable ontology. Inputs were identical within each pair. They received no code,
prose acceptance checklist, prior findings, peer output or evaluator inventory. This was
review preparation only: no implementation, defect search, repair or runtime test.

## What the two repetitions recovered

“Both” means the two saved inventories explicitly retain the concern. It is not a
population reliability estimate. The examples below distinguish missing review checks
from implementation defects, which this experiment cannot establish.

| Feature | Both retained | Important variation or shared omission |
|---|---|---|
| Failed workflow retry | Correct failed slice and outside-success evidence; atomic fresh run; source history; exact submission replay; current authority/no Start; owning allocation; truthful lineage/progress. | Both omitted concrete held-versus-final-failure settlement and invocation-reservation/replay checks. Bootstrap details differed: one missed child dependence on bootstrap success; the other missed current authorization at the later graph-submission boundary. |
| Host deletion by draining | Durable one-request drain; empty-host path; exit/provider proof; explicit force with per-VM loss; ownership; separation from rotation replacement. | Only the second retained the allowed unrestorable provider-bundle exception. Neither retained the deterministic failure fingerprint/notice rule. The first retained atomic release settlement and authorized progress reads more explicitly. |
| Stale editor Save | Observed incarnation plus version; equal-hash stale rejection; buffer preservation; fresh attachment reads; confined atomic writes; all ordinary editor variants. | Only the first retained live-cache invalidation; only the second retained the no-implicit-commit/flush boundary. Neither retained replay-capacity refusal without evicting unexpired results. |

**Design and compatibility were represented, but unevenly.** All six named owners and
affected consumers. Both host inventories made dependency direction and private state
boundaries concrete. Retry and Save retained ownership but left some API hiding,
dependency-direction and conditional verification/evolution checks implicit or absent.
These are prospective checks on the changed implementation, not authorization to repair
unrelated architecture or add migrations.

No material contradiction of the feature contracts was established. One retry inventory
overstated a reverse source-to-retry navigation link as mandatory; the ontology requires
retry-to-source lineage, leaving the reverse control as a reasonable optional design.
[Every assessed group, disagreements and qualifications](EVALUATION.md).

## Why the misses matter

The visible read records distinguish at least some omissions from retrieval failures:

- Both host workers retrieved the exact bundle-exception text; only one retained it.
- Both retrieved the deterministic durability-block rule; neither retained its fingerprint
  and owner-notice conditions.
- Both Save workers retrieved replay-capacity/no-eviction text; neither retained it.

Thus supplying relevant text did not guarantee its conversion into a review obligation.
The records do not expose the internal cause—attention, compression or applicability
judgment—but they rule out “the text was unavailable” for these examples. All workers
finished below the five-minute cap; none was cut off.

Combining the pairs would recover useful one-sided details, including the host exception
and both Save cache/durability checks. **Their union still misses shared conditions.**
This supports consolidation as a useful step, not as proof of completeness. No agent
actually consolidated or reviewed code in this experiment.

## What this says about the historical eight retry categories

Both retry inventories recover the broad subjects behind all eight: graph/input,
history, replay, atomic refusal, authority, launch admission, ownership/scheduling and UI.
They organize them into 24 and 26 rows; neither was asked for eight. The number and
headings are evaluator choices, while the underlying obligations are source-grounded.

**Scope qualification:** we used the retained D2 H ontology draft, which includes
bootstrap retry. The later approved coding plan excluded bootstrap and did not preserve
an adopted ontology diff/governance handoff. This is evidence about the same feature's
actual draft, not proof of exact recovery of every clause in that later eight-scope rubric
(particularly accepted-launch details). No missing approved diff was reconstructed.

## Effort and limits

| Work | Summed agent time | Output tokens | Noncached / cached input tokens |
|---|---:|---:|---:|
| Six Sol inventories | 16m 50s | 69,198 | 270,084 / 4,697,088 |
| Astra reference + three independent evaluations | 14m 29s | 42,109 | 266,781 / 3,067,776 |

Each Sol run took 2m 42s–2m 57s; two runs cost about 5½ worker-minutes per feature.
These are summed effort, not elapsed user wait. Native input totals count repeated calls
and cached context; output totals already include reasoning tokens. Parent preparation,
source audit, reconciliation and writing are additional, unmetered effort. The experiment
also required duplicate Markdown/JSON artifacts, so this is not a production cost estimate.

Astra froze the reference before Sol dispatch; fresh Astra contexts evaluated each pair
against source. The parent audited the judgments and retained originals plus corrections.
All six input hashes stayed unchanged and all cited ontology IDs exist. Metadata was
removed uniformly to exclude earlier implementation assessments. Isolation was by
directory/instruction, not OS sandbox; the parent knew earlier experiments.

Two repetitions per feature are a pilot. There is no no-handoff control, no measurement
of planners producing this handoff, and no evidence here of defect-detection or repair
reliability. A later code reviewer might recover an omitted detail; that was not tested.
[Frozen procedure and provenance](PROTOCOL.md).

## Next decision

Keep the explicit ontology handoff and persist the consolidated review inventory, with
allowed cases and exceptions visible. Do not equate agreement between reviewers with
complete coverage.

The next useful small test is **one independent source-to-inventory audit after
consolidation**: for changed and governing commitments, mark each material condition as
retained, inapplicable with a reason, or unresolved. Compare with these frozen inventories;
measure recovered shared omissions, unsupported additions and extra effort. Do not supply
the known missing-rule list to that auditor. This targets the observed loss between
retrieved text and inventory; it has not been run or added to the skill.

The local ignored `private/` archive preserves input hashes, raw patches, worker outputs,
public action records, numerical usage receipts, the frozen reference, independent
assessments, parent adjustments and exposure audit. The concise records here are the
versioned study result.
