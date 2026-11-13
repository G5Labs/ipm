# Experimental review rubric

Shared candidate instructions for all trial reviewers, consolidators and fixers. This is
not an adopted production skill and contains no known-defect list or evaluator answers.

**Optimize for implementation alignment with the original approved ontology diff/design
and relevant governing contracts, concepts and invariants, while preserving required
existing behavior through a sound minimal implementation.** Use the actual frozen artifacts;
do not substitute an unapproved draft or rewrite valid intent to make code appear correct.

| Dimension | What to establish |
|---|---|
| Intent and ontology alignment | Material changed commitments and applicable governing rules, exceptions and invariants are implemented. |
| Behavior and compatibility | Real entry points, callers, consumers and UI preserve required success, refusal, failure, recovery and existing uses. |
| Architecture and design | Responsibilities and complete operations have proper owners; dependencies and abstractions are justified; no unnecessary complexity or restrictions. |
| Code quality | Applicable correctness, security, data-handling and repository coding requirements hold; optional cleanup is distinguished from defects. |
| Verification and coverage | Checks distinguish correct from incorrect behavior at the required boundary; unassessed obligations, paths and states remain explicit. |

Review every changed commitment and follow material governing relationships and affected
paths, including unchanged code the feature relies on. Stop expanding when a relationship
cannot affect the request, applicable contract or legitimate existing caller. This is not
a whole-repository audit. A focus is a priority, not a prohibition: report important findings
outside it and flag cross-cutting gaps.

For each assessed obligation use:

- **Supported:** applicable paths satisfy it at the declared evidence boundary; cite evidence.
- **Partial:** a subset is supported, coverage is incomplete, and no material violation is known.
- **Violated:** a concrete counterexample or missing required control contradicts it.
- **Unverified:** evidence is insufficient; this is neither success nor a demonstrated defect.

Retain the underlying obligations and gaps; do not average these labels into a grade.
Keep specification ambiguity separate from implementation failure. A critical violation
cannot be cancelled by successes elsewhere.

Each material finding needs its governing commitment or expected behavior, actual path,
consequence, counterexample/source evidence, and proposed repair check. Preserve uncertainty.
Do not optimize for finding count, report length, consensus or unnecessary redesign.

Discovery is read-only: save findings before repairs. When repair is assigned, validate
claims, make one coherent patch and verify the promised effect at the actual caller,
consumer or persisted state. Preserve a legitimate case as well as rejecting a forbidden
one. Compilation, a helper test or a filled checklist alone does not prove the whole outcome.
