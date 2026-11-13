# Question store for G5 Lean: design request

Branch `g5/question-store-plan`; comparison base `origin/main` at `d5b94253`.

## Human instructions

First request (2026-10-09):

> There are two repositories: G5 (in /Users/davide/Desktop/G5-new/G5-Tungsten-seed) and G5-Lean (in this folder). G5-Lean was forked from G5 and then a lot of features were removed and the architecture was simplified. https://github.com/G5Labs/G5/pull/2532 was designed for G5. If we had to put it in G5-Lean, what would change?

After the agent's comparison (2026-10-09):

> Write up a design plan for the Lean version

## Context the request refers to

G5 PR #2532 ("question store") moves the payload of a document question out of the
document into one signed JSON file per question set in the docs repository
(`docs/<workspace_id>/questions/<set>.g5q.json`), referenced from Markdown
(`g5-question-ref` fence) and HTML explainers (`data-g5-question` placeholder), written
only by one validating service. Its human decisions, recorded in that PR: new questions
use set files and embedded `g5-question` fences are never migrated (Q8); a set file is
referenced from any number of documents and counted once (Q4); the signing key is
derived from the workspace id, with no deployment secret; a decision is a `user` answer
or an agent answer verified as relayed for a person; draft-versus-committed is not part
of the decision gate (Q3); wording edits are revisions, option changes are new questions
(Q5); share links render only for a signed-in invited recipient.

The following are the agent's findings about G5 Lean at `d5b94253`, by source
inspection and three read-only exploration passes; nothing was run.

1. Lean removed the CRDT, branch drafts and task-commit protocol (design v9 §0,
   migration 007). A document write is `fs.write` to the owner's attached VM with a
   content-hash precondition; the editor Save then runs the selected-file checkpoint
   (commit and push of the saved file). The HTML answer Apply writes live only.
2. Non-owners read committed Git; there is one writer per workspace (H072). The
   `workspace_members.role` column is written but no access check reads it, so there
   is no workspace-admin role. No external share link or token exists.
3. Lean removed `g5d-tooling`, the provided-agent MCP tool surface, `mcp_preflight`,
   the task-control JWT and the tera prompt templates. Agents run with native tools in
   the workspace VM as the owner, carry the workspace-incarnation token, and reach the
   API only through an allowlist of internal routes; the actor of every agent write is
   the owner. `g5d` is on PATH in the VM. The only MCP server left serves two review
   tools to Codex.
4. `docs_write_policy.rs` exists but is referenced only by tests; `/work` is mounted
   read-write; agents commit and push natively.
5. Lean keeps the embedded `g5-question` flow (`questionBlock.ts`,
   `StructuredQuestionEditor.tsx`, `questionAnswerWrite.ts`), the HTML answer bridge,
   `WorkspaceStatusBand`, open-question metrics (`capture_open_question_metrics`,
   `open_project_doc_question_sources`) and the home attention list. The escalation-doc
   family is a stub and `question_v1` is gone.
6. WebSocket events are one `WsEvent` enum variant each; maintenance loops are
   hand-spawned in `maintenance/mod.rs`; migrations are numbered (latest 058) with a
   checksum file and an RLS `SET ROLE` pattern.
7. Agent guidance ships from `.agents/AGENTS.md` and `.agents/skills/` into each
   provider's home; the generate and explainer chat prompts in the frontend carry the
   schema-1 question guidance.

## Parallel design

A second, independently written plan for the same request exists at
`/Users/davide/G5-Lean-question-store-design/.g5/pr_ontology_diff_design/lean-question-store/ontology-plan.md`
(branch `design/lean-question-store`, same comparison base). Third request (2026-10-09):

> look at /Users/davide/G5-Lean-question-store-design/.g5/pr_ontology_diff_design/lean-question-store/ontology-plan.md and see if there is something to integrate/change

Integrated from it: references carry an explicit set path so inherited sets resolve
after a fork; the keyed signature is replaced by an unkeyed content digest; agent
writes stay uncommitted; applied-versus-published save outcomes and intent-bound
replay receipts; host confirmation for explainer answers; reuse of existing
file-change and durable-cut notifications instead of a question event; FILES
Markdown stays GitHub-flavored; inherited-set discovery and incomplete summaries.
Not integrated: its tightened revision rule (an outdated answer stops being a
decision until reconfirmed), its MCP operations beside the CLI, and its separate
`features/question_sets.g5.md` page; reasons are in the plan.

## Human decisions

Fourth request (2026-10-09):

> A human answer checkpoints, that's fine. We can also drop the integrity checks entirely. Spawn an opus 5.5 agent to port the code with the new design.

Recorded: `g5.documents.question.answer-checkpoint` is human-verified; `g5.documents.question.writer` loses the digest, classifier, repair and verify operations and keeps schema validation, the one-writer rule and receipt binding. Implementation is authorized from this plan.
