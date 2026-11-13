# Issue 248: clean up the old harness

## Human instructions

[Issue 248](https://github.com/G5Labs/G5-Lean/issues/248):

> Cleanup all old harness, skills, prompts, MCP tools, etc. The new harness is just the codex/claude harness in the root of the repo, with out hooks, MCP tools and simple skills.

Asked what agents inside a workspace should get once the old profiles are gone, the user
chose **Install the root harness**. The option read:

> G5 installs this repo's AGENTS.md workflow and the four ontology skills into every workspace in place of the old profiles. The install mechanism stays and is re-pointed.

Asked how to treat product features that depend on old skills or MCP tools:

> remove their skills and MCP tools, and document the gap between the UI, API and ontology commitments and the new harness

On the first design:

> 1. make this design significantly more concise.
> 2. don't add ontology on the legacy harness removal/retire etc in the ontology, make the ontology edit minimal that just contains what's the new harness, which should be already there.
> 3. Keep everything minimal and concise

## Clarifications

On the ontology wording for basic chat, after implementation:

> Don't say "g5.workflow.basic-chat says "with no G5 harness"." in the ontology, just say "Basic chat should behave very close to chat without any harness" This is because our harness is specific to pre-implementation design and post-implementation review, so regular chat should be fine.

On the open questions after the review, having learned that "Review with my agent" and
its two MCP tools were added on 2026-09-25:

> 1. ok, just forget about this scenario, this is an edge case
> 2. ok, it is fine as it is for now -> concisely record this in some ontology block's gap
> 3. Ok, ok, when is this review with my agent added, who added it, and who added the MCP tools? if they are reasonable keep these two required MCP tools for now.
> 4. Completely remove both Code-analysis engine and Agent search engine

Items 1 and 2 answer the Codex `AGENTS.override.md` edge case and agents started from the
workspace terminal without the harness.

## Agent interpretation

"with out hooks, MCP tools and simple skills" is read as: without hooks or MCP tools, and
with simple skills. The ontology gains one block saying the harness is provided in every
workspace. The removal and its gaps live in the [design](../../ontology-plan-issue-248.md).
