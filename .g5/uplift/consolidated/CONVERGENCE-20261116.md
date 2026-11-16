# IPM Verus uplift: continuous run of 2026-11-16, convergence record

Product `571e04a19aba9f9b4bc0ea23fbe09182038f4e55`, unchanged (`git diff --quiet 571e04a1 HEAD -- src preload tests modules wheels module.xml`). The run started at the immutable tag `cloud-uplift-start-20261116` (`18d15d05`) and continued iteration28's foundation on a Linux x86-64 VM. Iterations 30-67 were sealed as local commits on `consolidated_uplift`; nothing was published.

## Basis

- Guidance: `$G5_HARNESS` = `/home/ziniu_wu_g5labs_ai/experiments/iris-ipm-continuous-20261116/harness`. Hashes of the iteration, scoring, uplift, foundation, shared reproduction, playbook and AGENTS files are recorded in [iter30/working-ledger.json](iter30/working-ledger.json) `guidance.sha256`.
  - Iteration: `Verus/uplift-iteration.md` `af3f5f72…`.
  - Scoring: `Verus/uplift-scoring.md` `f67cb908…`.
  - Both changed since the iteration28 snapshot; the playbook and AGENTS were unchanged.
- Reconciliation: iteration28 was the last completed iteration, and its formal side replayed on Linux. The stopped iteration29 (ModuleRoot configuration) stays unfinished and uncredited, and its archive-bounded lead remains in the queue. The iteration28 findings audit changed ontology owners only; its registers, IDs and decisions were extended, never rewritten.
- Toolchain and isolation:
  - Verus `0.2026.09.27.3cf1832` (Linux build).
  - IRIS 2026.1 build 234U, from a run-specific snapshot `ipmcont20261116-iris:571e04a1-fresh`.
  - Containers named `ipmcont20261116-*`, network none, all removed. No other experiment was touched.
  - Entry points are in the [runbook section](../../Verus_verification/consolidated/RUNBOOK.md).

## What the run produced

| Measure | Start (iteration28) | End (iteration67) |
|---|---|---|
| Resolved Verus locator members (map union, frozen 1,216 census) | 407 | 591 |
| Unlinked members in scope / excluded by scope | 809 total | 499 / 126 ([residual](common30/residual-final.json)) |
| New canonical owner statements (D/Q) | | 30 (listed in each iteration handoff) |
| New defects | | 38: High 3, Medium 12, Low 23 (C:D34-C:D71) |
| New questions | | 5 (C:Q24-C:Q28) |
| Sealed iterations | | 38 (30-67); all replay from a clean `git archive` extraction ([record](common30/clean-replay-final-20261116.txt)) |
| Freshness gate | | all theorem and correspondence checks fresh; no added or removed production files ([receipt](common30/freshness-final.json), [demonstration](common30/freshness-demo/README.md)) |

- Every iteration has a Verus model, rejected semantic controls, a fresh native run on the pinned product, and Verus replay of the observed fields with focused flips rejected. The per-iteration counts are in [run-summary-final.md](common30/run-summary-final.md).
- No shipped ObjectScript body is verified directly. All bindings are transcriptions with native agreement, and the provider premises are named in each model header.

The High defects are discoveries from this search, not disclosed repairs:
- [C:D39](BUGS.md#c-d39): a filesystem-repository install picks a prerelease over its own release.
- [C:D40](BUGS.md#c-d40): a Copy resource deletes the classes that `Overwrite="0"` kept.
- [C:D60](BUGS.md#c-d60): a CSPApplication without DeployPath deletes the module's source folder on uninstall.

## Sampled workflows and predictions

Each iteration ledger records the owner prediction, the source prediction and its exposure (all were source-informed after the owner prediction), followed by the observation.

Misses were recorded rather than hidden:
- iter30: non-developer reload is a no-op.
- iter32: the flat layout never syncs.
- iter34: default repository flags and the SQL re-sort.
- iter37: `/checkuptodate` does not keep a newer target.
- iter42: the broken-class fixture compiled.
- iter43: the root hook never fires.
- iter47: YAML loses CR rather than failing to parse.
- iter51: stored names are lowercase.
- iter52: the timestamp changes on every save.
- iter55: `init` does not wrap.
- iter57: the deploy folder is created.
- iter60: both includes leak.
- iter61: a duplicate name is refused.
- iter64: declared mappings roll back.
- iter65: FileCopy and WebApplication refresh only at Activate.

Workflows sampled:
- Commands: phase selection, `list-dependents`, `list-installed`, `show`, `history`, `config`, `default-modifiers`, `information`, `help`/`version`, `zn`/`exec`/`init`, `import`, `arrange`, `generate`.
- Resource kinds: unit tests, Copy, CPF, CSP, WebApplication, LOC (default and legacy), DFI, Interoperability, automatic mappings.
- Load, sync and storage paths: installed requirements, sync deletion and discovery, version index, installer class, preload, manifest document editing, record hooks, source-control hooks and editor callbacks, Document phase.
- Cross-cutting sweeps: uninstall leftovers, failed-load side effects, developer versus normal refresh.

## Convergence test

- **Strongest remaining candidate (iterations 52 and 66).** The module record hooks found no defect, only question C:Q28. The `document` phase, the largest unbound phase method, found only documentation drift (C:D69, Low).
- **Different underserved workflows (iterations 53, 55, 67).**
  - `generate`: two Lows.
  - `zn`/`exec`/`init`: two Lows.
  - The editor-facing composite source control, the largest subsystem without links: two Lows, one reachable only through a direct API call.
- **Final cross-cutting sweeps (iterations 64, 65).** Failed-load side effects and developer-versus-normal refresh across eight to nine resource kinds found nothing new; every observation matched owned statements.
- **Yield trend.**
  - High defects came from iterations 34, 37 and 57.
  - Mediums came from untested commands and processors (30, 32, 33, 36, 39, 42, 44, 56) and from placement and uninstall cleanup (59, 63).
  - Over the last eight lanes (60-67): one Medium (63), six Lows, and three lanes with nothing new.
  - Every in-scope shell command and every in-scope resource-processor kind now has at least one lane.

## Remaining distribution and why another batch is not justified

[residual-final.json](common30/residual-final.json) lists 499 unlinked in-scope members: 170 of six lines or fewer, 217 of 7-20 lines, and 112 longer. By subsystem:

| Subsystem | Unlinked | Content |
|---|---|---|
| General | 99 | SemanticVersion/Index generators, IConfigurable/AbstractSettings plumbing, SASchema (IDE schema generation), LogManager, Sync output text |
| Utils | 89 | Output capture, RunCommand helpers, LockManager/CodeGeneration/ComparisonAdaptor compile-time generators, the legacy ZPM migration (P:F-NSP-9 needs a genuine legacy producer), and LockWithCurrentDependencies (no caller) |
| ResourceProcessor | 58 | Base-class no-op hooks, export/artifact hooks (excluded scope), sync-support flags |
| Repo | 49 | Repository display and catalog, interface stubs |
| ExtensionBase | 42 | Studio menu and namespace routing for mapped source control |
| StudioDocument | 42 | Document plumbing (locks, timestamps, streams) |

126 members are excluded by scope: external registries 64, archive 27, verify/build 15, package/publish/python/deployed 15, and analyzer, external package and similar 5.

- **Material predictions are unlikely to change.**
  - The remaining unlinked code is mostly delegating plumbing, IDE plumbing, generators and helpers whose observable effects reach users only through the commands, phases and processors already bound.
  - Where those carriers were bound, the outcome a user sees is already predicted.
  - The last convergence pair and both final sweeps added no Medium or High finding.
- **Cost of settling the largest unknowns.**
  - Mapped source-control routing, the legacy ZPM migration with a genuine ZPM 0.7 producer, and the Class/Routine/LocalizedMessages mapping variants (same rule as the bound Package variant) are each about one delegated lane: roughly 0.9M tokens and 5 minutes of wall time.
  - The migration also needs a legacy product image that is not in this environment.
  - The archive/ORAS/registry/verify behaviours remain outside the inherited scope boundary and are left unassessed.

## Honest limits

- Locator reach (591/1216) is not semantic or whole-member coverage. Earlier iterations' semantic credits are carried, not re-audited.
- Predictions were source-informed after the owner prediction, and the exposure is recorded per ledger. Delegated lanes were builder-selected, so no unseen-case discovery claim is made.
- Native evidence is IRIS Community 2026.1 on Linux x86-64 only. HL7/X12 documents were not exercisable here.
- Interactive prompts (`init`, `generate` without `-t` was driven by piped answers) and Studio clients were simulated server-side.
- No owner decisions were made. 38 defects and 5 questions are open for the product owner, and no repair was implemented or checked.
- The findings registers stay at their audited locations (`uplift/consolidated/BUGS.md`, `Verus_verification/bugs_issues.md`, `playbook-uplift/BUGS.md`). A G5 viewer handoff would still need them reconciled into `.g5/TLA_verification/`.

## Cost

- Wall time: about 3 hours (15:57-19:00 UTC).
- 29 delegated lanes used about 22.0M subagent tokens. Main-thread tokens are not available in-session.
- Native and formal runs took 6-30 s per iteration. Iteration35's replay takes about 4 minutes.
