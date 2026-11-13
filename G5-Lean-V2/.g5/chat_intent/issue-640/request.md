# Implementation request

The user requests careful reading and complete implementation of [GitHub issue #640](https://github.com/G5Labs/G5-Lean/issues/640) against the latest origin/main, in a separate branch and worktree. The issue states the agreed product design and historical contract sources; no additional design approval is required by this request.

Use Claude CLI as reviewer only after implementor verification passes. Give the reviewer verification commands/results and remind it gh and gcloud are available outside the sandbox. Review/address at most six rounds; evaluate comments independently, explain disagreement, and escalate remaining significant disagreements to the human.

Validation order: smallest production-boundary regression; affected crate/process and architecture/inventory guards; one final workspace check, strict Clippy, formatting and diff check; reviewer; affected regressions and final workspace validation after changes; PR and required Cloud CI. Rebase onto current origin/main before each cloud CI, batch verified review fixes, use one bounded watcher, and diagnose only failed selectors. Full local Docker CI only for Docker/stack/cross-service changes or remote diagnosis. Prove and record unrelated harness failures without expanding scope.

## Agent interpretation

Issue #640 requires consequential integrations at the fork, authorization, task/provider and VM boundaries. Preserve its settled guarantees and report incompatibilities rather than weakening them. Existing uncommitted work in the original checkout and another limits checkout is not part of this branch; historical code is recovered from the issue's pinned commits.
