# Review harness simplification

On 2026-10-30, the user requested removing the ontology-review skill because native
review and fix loops were sufficient. Replace it with short ordinary guidance in
`.agents/guidance/deep-review.md`: review the current PR against changed and relevant
ontology and all initial requirements, fix confirmed PR defects, and repeat until
no issues are found or the budget ends (default maximum 10 rounds). Requirements
and reasonable implications determine defects and severity; unrequired quality-of-life
features do not qualify. Save a concise report of fixes and vetted ontology nodes
under `.g5/pr_ontology_diff_design/`.

The user also requested an explanation of the planning references and the difference
between the repository and packaged AGENTS.md files, while leaving the planning
skill's broader structure in place for now.

In follow-up, the user accepted removing the standalone implementation-testing
checklist while preserving ontology-based acceptance, runtime trajectories for
modeled designs, and explicit scope for expensive verification in the existing
instructions and planning/model-checking guidance.
