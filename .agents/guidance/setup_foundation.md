# Set up and hand off a verification foundation

Continue with the [TLC setup guidance](TLC/setup_foundation.md). For a Verus foundation, use the [Verus setup guidance](Verus/setup_foundation.md). Both use the same [findings and reproduction rules](TLC/setup_foundation.md#findings), [simplification comparisons](TLC/setup_foundation.md#simplification-comparisons) and [viewer format checks](TLC/setup_foundation.md#format-checks).

- <a id="project-runbook"></a>[Project runbook](TLC/setup_foundation.md#project-runbook)
- <a id="artifact-paths"></a>[Artifact paths](TLC/setup_foundation.md#artifact-paths)
- <a id="execution-evidence-and-reuse"></a>[Execution evidence and reuse](TLC/setup_foundation.md#execution-evidence-and-reuse)
- <a id="check-the-handoff-against-real-evidence"></a>[Check the handoff against real evidence](TLC/setup_foundation.md#check-the-handoff-against-real-evidence)
- <a id="prepare-material-for-the-viewer"></a>[Prepare material for the viewer](TLC/setup_foundation.md#prepare-material-for-the-viewer)
- <a id="findings"></a>[Findings](TLC/setup_foundation.md#findings)
- <a id="simplification-comparisons"></a>[Simplification comparisons](TLC/setup_foundation.md#simplification-comparisons)
- <a id="format-checks"></a>[Format checks](TLC/setup_foundation.md#format-checks)

## Open questions

For a TLC or Verus uplift, write each unresolved question of intent once in `open_questions.md` under that foundation's verification root. Give it a stable Markdown anchor. The entry may use a label such as `[Q · erasure.concurrency]`. Record the question, plausible answers, evidence and current disposition there. Use an embedded `g5-question` fence with schema 1, a stable `id`, `status: open`, the question in `prompt`, `severity` (critical, high, medium or low), the adopted `default_assumption`, `allow_free_text: true` and the supported `options` for an answerable card. Include the adopted default as an ordinary answer option labelled in its description as “Default assumption.” Reuse an exactly matching option and keep its label stable. The UI calls the register Ontology clarification questions and defaults to all open and closed questions sorted by importance. Keep context and evidence immediately above the card. Preserve the saved card answer and attribution on subsequent exports. Preserve existing anchors when revising entries.

In each ontology statement that relies on the answer, state the most reasonable working assumption and identify it as provisional. Add a relative link to the register entry in the statement's footer metadata and put `- uncertain` on its own line. One question can qualify several statements, and a statement can link to several questions. Keep the question declaration and text in the register.

When an attributable answer settles the question, record the answer and its source at the same anchor and revise every affected statement in the same change. Remove a statement's `uncertain` tag only when its other uncertainties are also settled. An observed behavior or passing proof can inform the assumption. Human confirmation requires a human source.
