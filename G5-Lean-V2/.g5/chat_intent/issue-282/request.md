# Issue 282 request

The user asked to read [issue 282](https://github.com/G5Labs/G5-Lean/issues/282) and latest `origin/main`, then implement coherent session resume in a separate branch and worktree. The issue requires complete provider-session generations, verified native resume for Claude and Codex, delivered-history fallback when resume is unsafe, truthful notices, preservation of fork/carry/Continue behavior, and ontology alignment.

The user also required self-verification before a Claude CLI review, at most six review-and-address rounds, targeted then workspace validation, a PR with required Cloud CI, main freshness checks before cloud CI, bounded CI watching, and focused diagnosis of unrelated fixture failures.

## Follow-up after the PR 286 review at `24c85189`

User instructions:

- Review PR 286 against issue 282 and `origin/main`: e2e and value-loop coverage, matched Claude/Codex coverage and test reuse, design completeness and parity, failure semantics including a stopped VM, end-of-turn efficiency and the warm local check, and whether the ontology describes the needed properties and matches the code.
- Fix both the recommended items and the strong follow-ups from that review, post the review on the PR, and state that each fix adds tests where applicable.
- Decisions chosen by the user: append in place on the existing file row; native Claude resume only in the working directory the saved set came from (a changed target rebuilds); amend unmerged migration 057; the API undoes a fence at settlement.

Agent interpretation:

- The posted review is [PR 286 comment](https://github.com/G5Labs/G5-Lean/pull/286#issuecomment-5967082245).
- Recommended: the complete Claude session directory (including `tool-results/`), the pending-completion working directory, truthful failed-save wording with a consistent publication block, same-VM/chain/Codex failed-save e2e, and the parity tests listed in the comment.
- Strong follow-ups: append-only transfer, a Stop-during-turn e2e for both providers, and undoing a fence when the input never reached the provider. A rejected resume is excluded from the undo because restoring a set the CLI refuses would repeat the refusal on every turn.
