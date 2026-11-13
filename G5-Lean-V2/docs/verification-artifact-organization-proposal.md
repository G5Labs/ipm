# Verification artifact organization for the ontology viewer

Proposal, 31 October 2026. This document expands the agreed direction for organizing verification evidence and identifies the guidance and tooling changes to make next. It does not itself change the agent harness, ontology, runners or viewer.

Keep the existing project artifacts as the source of truth. Give agents a short convention for saving and linking their work, and give the viewer a precise reader contract for discovering and interpreting that work. Preserve the explanations, formats and extra evidence that agents naturally produce.

The reference is the [PairSoft ontology viewer mock](/Users/ziniuw/Desktop/projects/G5-benchmark/results/g5-pairsoft-platform.html#ontology), backed by artifacts in [PairSoftPlatform-ontology-prose](/Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform-ontology-prose). Those absolute links identify the local reference checkout; portable project evidence links should use repository-relative paths.

## 1. Principles and scope

The human's requirements are:

1. PairSoft's agents voluntarily produced the evidence and explanations used by the mock. Preserve that ability instead of prescribing how agents must reason, select concerns or explain findings.
2. Provide minimal instructions about organizing artifacts. Agents remain free to create additional files, fields, analyses and explanations, including material the UI does not interpret.
3. Reuse the existing structure and formats wherever practical.

The resulting recommendation is to standardize the small boundary between saved artifacts and the viewer: discovery locations, identities, references and interpretation of results. The richer machine-readable contract belongs in tooling documentation and adapters. It should not become a long checklist loaded into every agent session.

This proposal covers TLA+/TLC discovery in detail and preserves links to other supporting evidence. It does not mandate more model checking, a particular modeling style, a quota of models, a bindings template, or one proof technique for every claim.

## 2. What already exists

The mock's embedded dataset inspected for this proposal contains 172 model entries and 3,358 run entries, plus source evidence, findings and simplification assessments. These are counts in that saved mock, not a claim about the continually changing live PairSoft checkout.

| Existing artifact | Role to preserve |
|---|---|
| `models/<concern>/manifest.json` | Names the concern, checked question, governing ontology IDs, code bindings and configured runs. |
| `models/<concern>/BINDINGS.md` | Explains correspondence, assumptions, limits and model meaning in ordinary Markdown. |
| `.tla` and `.cfg` files | Define the actual machines, properties, parameters and checking configurations. |
| `results/<concern>/<run_id>/receipt.json` | Records one execution, its inputs, observed result, statistics and provenance. |
| Saved inputs, logs and traces | Support inspection of what actually ran and what happened. |
| `results/index.jsonl` | Already exists in PairSoft. Its runner appends a compact summary row after saving a receipt. |
| `results/latest.json` and `SUMMARY.md` | Generated summaries selecting the latest receipt per concern and run label. |
| Separate validation and applicability records | Assess integrity and whether historical execution evidence applies to the current contract and implementation. |
| Ontology metadata and other linked files | Connect statements to models, tests, source review, findings and design reasoning. |

The existing [PairSoft concern manifest](/Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform-ontology-prose/.g5/TLA_verification/models/account-segment-lookup-and-approval-membership/manifest.json) is a useful concrete example. Its `bindings` field is a list of source-code correspondences; it is not the path of `BINDINGS.md`. Preserve that meaning. Discover the Markdown file by convention, with an optional separately named override if needed.

G5 already has a short [artifact storage guide](../.agents/guidance/TLC/tla-verification.md), [execution and reuse guidance](../.agents/guidance/uplift-evidence.md), and a [developer evidence helper](../tools/tlc/README.md). The helper is developer tooling, not a mandated customer runner. Extend this arrangement rather than introducing another competing workflow.

## 3. Canonical artifacts and derived views

Keep the following organization as the normal reference layout:

```text
.g5/
  ontology/
  TLA_verification/
    models/<concern>/
      manifest.json
      BINDINGS.md
      *.tla
      *.cfg
      ... additional useful files
    results/
      <concern>/<run_id>/
        receipt.json
        ... saved inputs, logs, traces and other run artifacts
      index.jsonl
      latest.json
      SUMMARY.md
    runner/
    tools/
```

When the checkout root is itself the `.g5` repository, these paths begin with `TLA_verification/` and `ontology/`. Do not create a second `.g5` prefix.

The source records have distinct authority:

- The concern manifest describes the current concern and its declared configurations.
- A receipt describes a particular historical execution. Its original expectation and result remain attached to that execution.
- Bindings and linked explanations describe correspondence and scope.
- A separate, pinned applicability review can establish that saved evidence still applies after relevant changes.
- Indexes and summaries are derived views. They can be regenerated and cannot override their source records.

Keep original receipts unchanged when reindexing, renaming display titles or reviewing applicability. A current manifest must not silently replace the expectation recorded for an older run. When these differ, expose the difference and the relevant assessment.

Existing projects can retain useful variations in filenames or layout through a reader adapter or explicit reference. The normal layout gives new work a predictable home without requiring migration of working evidence.

## 4. The smallest authoring convention

For newly created concerns, use `models/<concern>/manifest.json` as the discovery entry point. Keep its essential content small:

| Content | Convention |
|---|---|
| Concern identity | Reuse the existing concern identifier and folder. The reader can use the manifest path as a unique identity within its repository. |
| `question` | A readable statement of what the model investigates. |
| `ontology_ids` | The IDs of the governing statements associated with this concern. |
| `runs` | Reuse the project's existing references to spec/configuration and expected outcomes. An empty list can represent work without declared executions. |
| `title` | Optional. Fall back to the concern name when absent. |
| Bindings and explanations | Use the existing `bindings` structure and `BINDINGS.md`; keep extra explanations in ordinary files. |

Do not require separate inventories of modules and configurations when the existing run declarations already identify them. Distinguish declared configurations from executions: declaring a run does not establish that it happened.

For legacy evidence lacking this entry point, the reader may support a known existing format. If it cannot determine an association, show the artifact with missing association information. Avoid presenting guessed relationships from filenames or prose as authoritative links.

No template is needed for `BINDINGS.md`. An agent can write derivations, tables, rejected alternatives, explanatory diagrams, additional caveats or nothing beyond the useful correspondence. The viewer displays that saved content directly. Optional extra fields must not make an otherwise supported manifest unreadable.

## 5. Deterministic discovery and indexing

The normal reader has two predictable discovery routes: concern manifests under `models/`, and execution summaries in `results/index.jsonl`. Reading the small manifest catalog and one run index avoids opening thousands of receipts for the initial view. Full receipts, models and logs can load when a user opens them.

Keep `index.jsonl` as a run index; there is no need to mix model records into it. Models with no runs remain discoverable through their manifests. The backend can assemble a combined response and an ontology-to-concern lookup from these inputs.

PairSoft already writes the run index in [run_tlc.py](/Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform-ontology-prose/.g5/TLA_verification/runner/run_tlc.py). Its [summarize.py](/Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform-ontology-prose/.g5/TLA_verification/runner/summarize.py) separately produces the latest-run views. Preserve those useful roles and add a rebuild path from retained receipts. A rebuild should be possible without executing TLC.

The developer contract should specify:

- A versioned normalized reader response. Existing unversioned index rows remain supported by the PairSoft adapter; adding a version to future generated rows must not require rewriting receipts.
- Stable run identity using the repository, concern and run ID or receipt path. A human-readable run label is not unique across repeated executions.
- Explicit source references to the manifest, receipt and available artifacts. Preserve each source format's path rules and resolve them to a documented repository root for the reader. Use repository identity when evidence links cross repositories; avoid machine-specific absolute paths in portable records.
- Deterministic ordering and deduplication when rebuilding. An interrupted append, duplicate row or malformed receipt produces a visible diagnostic; valid neighboring records remain available.
- Atomic publication of a rebuilt index so readers do not observe half a rebuild. Coordinate rebuilding with active appenders or use a consistent snapshot so a concurrent execution is not lost.
- Freshness checks covering added, removed and changed source records. An index timestamp alone is insufficient, and hashing only previously indexed receipts misses new runs.

Source revisions, fingerprints and validation references can support freshness. Their capture belongs in the runner/indexer. The UI should distinguish an outdated index from invalid underlying execution evidence; it can display saved material with its status while the projection is refreshed.

Keep current applicability separate from historical execution summaries. If the normalized view includes an applicability result, include the validation/review reference and the current inputs against which it was assessed. A durable `applicable: true` without that context becomes misleading after subsequent edits.

## 6. One reader interpretation, compatible receipt formats

PairSoft concern manifests and G5 helper manifests have different purposes. PairSoft describes a concern containing multiple configurations. G5's [model_evidence.py](../tools/tlc/scripts/model_evidence.py) describes a command, declared inputs and an expected execution result. Do not make one replace the other merely because both are called manifests.

Support their existing receipts through explicit adapters. Keep the original source format and raw values inspectable. The normalized reader should separate these dimensions:

| Dimension | Meaning |
|---|---|
| Execution status | Whether the checker produced a conclusive result, timed out, encountered an error or remains unknown. A conclusive counterexample need not exhaust the state space. |
| Observed result | What the checker reported: satisfaction, a property violation, deadlock or another supported result. |
| Purpose and expectation | Safety, liveness, witness, control, divergence or another recorded kind; what outcome and named properties were expected. |
| Expectation match | Whether the actual execution matched that declared purpose and expectation. |
| Current applicability | Whether the historical result supports the current model, ontology and implementation correspondence. |

Preserve the original `kind` and `variant`, including values outside a short predefined list. PairSoft already uses variants beyond `intended`, `as-built` and `proposed`. The viewer can display unfamiliar values without reclassifying them.

Examples of interpretation:

| Saved evidence | Reader meaning |
|---|---|
| Safety run with `PASS` and a conclusive checker result | The configured properties held within the recorded model and bounds. Current applicability is a separate question. |
| Control run with a matching `EXPECTED_VIOLATION` | The deliberate control was detected. |
| Divergence run with a matching `EXPECTED_VIOLATION` | The modeled implementation gap was reproduced. |
| Witness run with the expected target assertion violation | The target was reached, where the declared witness interpretation and trace support it. |
| Timeout with progress statistics | Exploration is incomplete. The progress counts remain useful but do not establish the property. |
| Complete old run with changed governing source or bindings | The historical outcome remains inspectable; current applicability needs assessment. |

Do not translate `EXPECTED_VIOLATION` to a universal green verification badge. Likewise, G5's `matched` field means execution evidence matched an expectation, not that the implementation is correct.

Retain the configured property names and the property actually reported as violated or witnessed. Reaching the wrong assertion cannot satisfy a named expectation. Preserve bounds, assumptions, fairness and scope through the original configurations and explanations; they need not become a new mandatory prose schema.

The normalized view can expose statistics, timestamps, tool versions, fingerprints, input snapshots and log/trace links when available. Missing historical values remain unknown; do not invent a completion flag, a zero state count or a tool version. For newly executed runs, the runner should collect this information automatically.

Use the existing [PairSoft evidence contract](/Users/ziniuw/Desktop/customers/PairSoft/PairSoftPlatform-ontology-prose/.g5/TLA_verification/runner/EVIDENCE_CONTRACT.md) as the reference for its integrity and applicability semantics. The reader consumes supported assessments; it does not infer semantic correspondence from matching hashes alone.

## 7. Ontology associations and other evidence

Use manifest `ontology_ids` as the declared concern-to-statement association and generate the reverse lookup for the viewer. Preserve human-readable metadata links to `BINDINGS.md`, models, receipts and related evidence. A metadata link need not be rewritten to a manifest link merely to make discovery work.

These relationships have different meanings. A model can be associated with a governing statement without verifying every clause of that statement. A parent can display evidence associated with its children without acquiring their verification status. Display available scope explanations alongside links; do not generate blanket coverage claims from association alone.

If metadata and a manifest disagree, retain their provenance and surface the discrepancy. The indexer may generate reverse links in its output, but should not silently edit ontology prose or choose new governing statements. Resolve stable statement IDs and explicit aliases through the ontology parser rather than relying on display titles or adjacency assumptions about Markdown anchors.

Other evidence stays in its natural location: runtime test reports, source inspections, findings, simplification assessments, diagrams and review notes. Follow existing metadata and artifact links. A generic linked artifact needs only a resolvable reference; a label or type can improve display when present. Richer cards may use adapters for existing structured formats without imposing those formats on every agent.

For Verus or another checker, preserve its tool identity and assurance level. A linked implementation proof should not be relabeled as TLC evidence. If a finding is later rebuilt as a TLA+/TLC model, retain the original as provenance and give the rebuilt model its own manifest and execution records.

## 8. Where guidance and contracts should live

| Location | Proposed change and responsibility |
|---|---|
| [`.agents/guidance/TLC/tla-verification.md`](../.agents/guidance/TLC/tla-verification.md) | Keep this as the single short agent-facing storage reference. Clarify the normal discovery entry point, preservation of existing formats, generated indexing and freedom to add material. Suggested text is below. |
| [`.agents/guidance/ontology-format.md`](../.agents/guidance/ontology-format.md) | Add at most one short clarification beside metadata evidence links: preserve useful artifact links, state material scope, and treat association separately from complete verification. Link the storage guide instead of duplicating it. |
| [`.agents/guidance/uplift-evidence.md`](../.agents/guidance/uplift-evidence.md) | Keep its existing execution, integrity and reuse responsibilities. Its storage-guide link already exists. Do not copy the index schema into this file. |
| [`.agents/skills/ontology-planning/references/model-checking.md`](../.agents/skills/ontology-planning/references/model-checking.md) | Keep the existing storage and reuse links. No additional artifact-writing checklist is needed. |
| [`.agents/guidance/TLC/uplift.md`](../.agents/guidance/TLC/uplift.md) and [downlift.md](../.agents/guidance/TLC/downlift.md) | Their links to the storage guide already exist; retain them. |
| [`.agents/guidance/TLC/pr_incremental_TLA_reverification.md`](../.agents/guidance/TLC/pr_incremental_TLA_reverification.md) | Add a storage-guide link beside its existing instruction to use repository evidence locations. Preserve its current scope and activation conditions. |
| [AGENTS.md](../AGENTS.md), [CLAUDE.md](../CLAUDE.md) and skill entry points | No new mandatory evidence workflow or schema dump. Existing contextual links are sufficient. |
| Proposed `tools/tlc/verification-artifact-reader.md` | Developer contract for discovery, path resolution, normalized fields, adapter versions, indexing, freshness and diagnostics. Put the detailed technical rules here when implementing the reader. |
| [tools/tlc/README.md](../tools/tlc/README.md) | Link the developer contract and explain how the helper's execution records are read. Document real rebuild/inspection commands once implemented. |
| Project `TLA_verification/runner/` | Keep tool-specific receipt production, parsing, index generation and validation with the existing project runner. Document project-specific behavior there. |
| Viewer/backend implementation | Own normalized responses, reverse lookups and lazy artifact loading. Keep receipt-format conditionals in shared reader adapters rather than duplicating them across UI components. |

Suggested replacement for the short storage guide's explanatory paragraphs, retaining its existing layout example:

> Save TLA+/TLC artifacts under the project's `.g5/TLA_verification/`, or `TLA_verification/` when the checkout is rooted at `.g5`. Preserve useful existing organization and formats. For new concerns, use `models/<concern>/manifest.json` to identify the checked question, governing ontology IDs and declared configurations. Keep correspondence and explanations in ordinary Markdown, such as `BINDINGS.md`. Retain each execution's receipt, logs and supporting artifacts under `results/`, including unsuccessful or incomplete runs. Link relevant evidence from ontology metadata. Use the project's runner and indexer for execution records and generated summaries. Additional files, fields and explanations are welcome; the viewer may display linked material without interpreting its structure.

Suggested metadata clarification:

> Link the evidence that supports the statement and describe material scope limits where useful. A model association identifies relevant evidence; it does not by itself establish complete verification. Preserve existing links to bindings, runs, tests and review material. See the TLA+/TLC artifact storage guide for model evidence locations.

The existing ontology commitments [g5.ontology.formal-verification](../.g5/ontology/features/ontology_and_harness.g5.md#g5.ontology.formal-verification) and [g5.harness.guidance](../.g5/ontology/features/ontology_and_harness.g5.md#g5.harness.guidance) already support evidence-linked claims and limited guidance. If implementation introduces a consequential viewer guarantee, refine its owning commitment at that level. Keep JSON keys, cache mechanics and adapters in developer documentation and code.

## 9. Implementation order and acceptance

1. Refine the short storage guide and the few contextual links above. Keep the detailed rationale in this proposal rather than expanding the normal agent instructions.
2. Define the developer reader contract against representative existing PairSoft and G5 helper artifacts. Include incomplete and legacy records, extra fields, controls, witnesses, repeated run labels and separate applicability reviews.
3. Implement adapters and a rebuildable run index using the existing machinery. Preserve receipts and their native formats. Start with the formats actually in use; add new adapters when there is a concrete source to support.
4. Connect the viewer to the reader's normalized output. Render bindings and linked evidence directly, with models/configurations/receipts available on demand.
5. Check representative mock concerns against the live reader for content and meaning, then check unsupported and missing evidence behavior. Changes in live source data should be distinguished from reader regressions.

Acceptance should demonstrate that:

- A new concern is discoverable from its manifest before any run exists.
- Repeated executions remain distinct, with historical receipts reachable and latest-run selection explained.
- Rebuilding the index from receipts recovers its execution inventory without rerunning TLC or changing source records.
- Existing PairSoft and G5 helper formats render through adapters, including legacy rows and optional fields.
- Extra files and fields remain inspectable, while unknown required schema versions produce a useful unsupported-format diagnostic.
- A control detection, reproduced gap, witness and safety result have distinguishable meanings.
- Missing, malformed, incomplete or outdated evidence remains visible with an accurate status.
- Current applicability changes do not rewrite historical outcomes or silently preserve stale verification badges.
- Stable ontology IDs connect the same concern after a prose reorganization, with unresolved IDs surfaced.
- Rich explanations survive as written, and evidence outside TLC remains reachable in its existing location.

The intended result is a dependable viewer built on the evidence agents already produce, with a small storage convention and explicit tooling semantics. Agents retain freedom over their reasoning and supporting material.
