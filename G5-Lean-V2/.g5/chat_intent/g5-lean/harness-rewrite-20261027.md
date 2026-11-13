# Harness rewrite — 2026-10-27

The user authorized this rewrite directly in G5-Lean-V2 for manual review, without requesting a commit or deployment.

Human instructions: keep the highest-level ontology principles in a short paragraph in both agent entry files: correctness first, human readability (concise, consistent, deduplicated, clear) second, agent benefit third; ontology is the source of truth for consequential intent and design, not a narration of implementation mechanics. During implementation, review/fixes and follow-up adjustments in a PR, maintain the owning statements for important decisions and assumptions in the same high-level, readable prose with their essential qualifications. Remove ontology-authoring, uplift and downlift as skills; put the ontology conventions, format, hand-tuned examples and explicit initial-workflow prompts under `.g5/`, read only as needed or, for uplift/downlift, on explicit request. Preserve stable IDs and I/D/Q storage compatibility. Update planning incrementally for an existing ontology and retain its selectively loaded references. Preserve review behavior and the customer entry's MicroVM, branch and platform instructions. Update platform packaging incrementally so customer agents receive the new guidance.

Implementation choices: retain the structural checker as guidance tooling; install guidance in the owner home, with managed-file tracking and legacy-bundle compatibility. Keep the privileged approval-review skill unchanged. Examples illustrate writing and do not import Bank policies or historical proof claims into customer ontology.

Follow-up: the user requested removal of `.agents/README.md` and moving the supporting guidance under `.agents/`. Guidance now lives in `.agents/guidance/` in the repository and provider owner homes, outside skill discovery. Entry-file loading and the customer instruction source remain unchanged.

Follow-up: keep important unresolved assumptions explicit, without creating a large inventory of assumption blocks.

Follow-up — 2026-10-28: the user preferred the revised PairSoft prose and requested that its editing lessons become guidance under `.agents/guidance/`. Add a side-by-side bad/good table with 10–20 real statements from G5, PairSoft and Bank of Ziniu; each reason must contain no more than ten words. The examples teach readable wording while preserving meaning, not new product policies. Keep the existing complete hand-tuned examples.
