# Durable design artifacts in Files

[Issue #794](https://github.com/G5Labs/G5-Lean/issues/794) asks for
`.g5/pr_ontology_diff_design` to appear in Files and participate in ordinary Git
staging and exact-tree commits. This design records the policy of the implementation
already published in [PR #797](https://github.com/G5Labs/G5-Lean/pull/797), following
the owner's request to make the design explicit before completing ship readiness.

## Governing decision

The [durable authored-content contract](../../ontology/features/documents.g5.md#g5.documents.durable-authored-content)
owns visibility and persistence for the ontology and design roots. Design artifacts
remain ordinary repository files; the existing
[ontology identity](../../ontology/features/documents.g5.md#g5.documents.ontology.current-format)
and [navigation](../../ontology/features/documents.g5.md#g5.documents.ontology.navigation)
contracts continue to govern Core Ontology.

Extend `g5_contracts::g5_paths`, the existing authority used by Files and Git, with
the exact design root and arbitrary authored descendants. Reuse the ontology
subtree's component validation after existing path normalization: reject empty,
`.`, `..`, `.git` and `.g5` components. A similarly named sibling such as
`.g5/pr_ontology_diff_design_old` is not the design root.

Keep the canonical Git exclude patterns and their legacy rewrite recognizer in
that same module. Unignore design content recursively, then exclude `.git` and
`.g5` metadata at the root and at any depth, including file-form metadata. Retain
recognition of the earlier directory-only patterns so rewriting an existing
exclude file removes stale rules.

This reuses the existing owner rather than introducing consumer-specific dot-path
exceptions, a new file index or new persistent state. Existing live file walking,
committed API listings, `GitRepo::add_all`, `GitRepo::add_paths` and exact-tree
validation inherit the shared decision. Their access, capture and publication
boundaries remain unchanged.

## Scope and acceptance

- Files reaches the design directory and shows plans, mocks and nested screenshots.
- The classifier, Git exclusions and exact-tree validation accept authored design
  content and reject nested metadata; malformed descendants and look-alike roots
  remain outside the durable allow-list.
- Existing durable roots retain their behavior. `.g5/chat_intent`, `.g5/memory`
  and legacy `.g5/*.md` are outside this change.

The issue settles these choices; no product decision is outstanding. A rebuilt
guest `g5d` runtime bundle is needed for a running workspace's live Files tree to
use the changed classifier. Updating only the API does not establish that rollout.

## Verification

Self-review traced the shared classifier through live file walking, Git staging
and exact-tree validation. The two CodeRabbit findings were independently checked
with native Git: recursive unignore rules needed explicit metadata exclusions,
and directory-only patterns missed metadata represented as files. Both corrections
stay within the issue's classifier/Git parity requirement.

At implementation commit `96f4ac701ce7f95bec345464bbabd90d0c5c2adc`, all 33 focused
tests passed: the `g5-contracts` path matrix, `g5-scm` Git-exclude parity and staging/
exact-tree checks, and `g5d-dispatch` file-tree tests. The walker regression checks
visible plans, mocks and deeply nested screenshots while pruning metadata. Native
Git parity covers metadata files and directories and preserves `.gitignore` as
authored content. Rust formatting and whitespace checks passed.

The same published implementation passed `g5-ci-rust` and `g5-ci-e2e`; Terraform
checks were skipped as inapplicable. These results do not demonstrate deployment
of the new guest bundle into an existing live workspace. Final PR checks, delayed
reviews and mergeability are checked again after publishing this design update.
