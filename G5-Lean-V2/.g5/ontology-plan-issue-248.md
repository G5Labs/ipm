# Issue 248: clean up the old harness

Agent review comments, attribution and round tools described in this historical record
were retired by the [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md). They are not supported interfaces.

## Ontology changes and governance

**Comparison base:** `origin/main` at `a1c3e29e`. Request and decisions: [chat intent](chat_intent/issue-248/request.md).

**Ontology changes:** one block added, `g5.harness.provided` in
[ontology and harness](ontology/features/ontology_and_harness.g5.md#provide-the-harness-in-every-workspace).
Existing ontology already describes the harness workflow and its single skill source.
Nothing is added about the legacy harness. At the user's request, `g5.workflow.basic-chat`
now reads "Basic chat should behave very close to chat without any harness", replacing
"with no G5 harness" ([clarification](chat_intent/issue-248/request.md#clarifications)).
After review the user kept, for now, the two MCP tools behind "Review with my agent", so
the block names them; the diff below preserves the original delivery. The [2026-10-06 amendment](chat_intent/agent-harness/request.md) now governs the dedicated instruction source and native provider destinations. The [review](ontology-review-issue-248.md)
updated this block's footer and those of affected blocks to their current state; no other
block text changed.

```diff
--- a/.g5/ontology/features/ontology_and_harness.g5.md
+++ b/.g5/ontology/features/ontology_and_harness.g5.md
@@ -212,2 +212,11 @@ keep valid intent and the original delivery reviewable when the design changes.
 
+### Provide the harness in every workspace
+
+[I · g5.harness.provided]
+
+G5 gives agents in every workspace this same harness: the root workflow instructions and
+the four [maintained skills](../concepts/coding_principles.g5.md#maintain-one-source-for-workflow-skills),
+read natively by Claude Code or Codex, with no hooks. For now its only MCP tools are the
+two with which the owner's agent reads and decides a [review round](governance.g5.md#review-in-a-workspace-you-own).
+
 ## Metadata
```

**Relevant and governing ontology:**

| Commitment | Obligation for this change | Violated by |
| --- | --- | --- |
| [g5.harness.provided](ontology/features/ontology_and_harness.g5.md#provide-the-harness-in-every-workspace) (new) | Every workspace agent gets the dedicated workspace instructions and four skills, with no G5 hooks; the only MCP tools are the two review-round tools. | An agent launched with legacy profile content, a hook or any other `g5-tools` tool. |
| [g5.workflow.ontology.skill-sources](ontology/concepts/coding_principles.g5.md#maintain-one-source-for-workflow-skills) | The provided copy is generated from `.agents/skills`. | A second, hand-edited copy kept in the repository. |
| [g5.git.exclusions](ontology/software_architecture/durability.g5.md#keep-platform-artifacts-outside-native-git) | Provided files stay out of Git and never hide a repository's own files. | A provided file in pending changes; a repository's own `AGENTS.md` overwritten or hidden. |
| [g5.runtime.bundle](ontology/software_architecture/runtime.g5.md#the-platform-supplies-its-complete-runtime) | The harness ships in the platform runtime, independent of the customer image. | Relying on a copy in the customer image or one retained in the guest. |
| [g5.agents.admission.allowed-tools](ontology/features/agents.g5.md#tool-selection-can-only-narrow-permission), [g5.agents.admission.permission-policy](ontology/features/agents.g5.md#freeze-provider-permissions-before-spawning) | A request naming a removed tool is refused before spawn. Launch settings that enforce the frozen policy stay. | Silently dropping an unknown tool; losing sandbox settings along with the old profile. |
| [g5.quality.cutover.history](ontology/concepts/coding_principles.g5.md#preserve-conversation-identity-through-migration), [g5.quality.migrations](ontology/concepts/coding_principles.g5.md#preserve-deployed-data-through-forward-migrations) | History and stored records stay readable. No migration file changes. | Deleting stored records; editing historical SQL. |
| [g5.documents.format](ontology/features/documents.g5.md#keep-local-authoring-distinct-from-shipped-format-support) | The shipped parser and editor keep their format. | Migrating the parser in this change. |
| [g5.documents.explainer.location](ontology/features/documents.g5.md#keep-generated-explainers-discoverable), found in review | Every chat entry point saves explainers under the managed root. | A seed prompt that names no destination. |
| [g5.git.network.lease](ontology/software_architecture/durability.g5.md#ask-clients-to-use-force-with-lease), [g5.documents.shared-checkout](ontology/features/documents.g5.md#disclose-concurrent-work-in-the-shared-checkout), [g5.documents.grounding](ontology/features/documents.g5.md#make-intent-traceability-inspectable), [g5.governance.review-workspace](ontology/features/governance.g5.md#review-in-a-workspace-you-own), found in review | They lose agent-side support; the gap is recorded in their metadata, not rebuilt. | Metadata still claiming the removed guidance or tools. |
| [g5.quality.delivery](ontology/concepts/deployment.g5.md#qualify-the-exact-release-candidate) | Rust and E2E Cloud Build pass on the PR head. | Merging with a failing required check. |

## Scope

Remove the legacy profile content, hooks, the `g5-tools` MCP tools except the two behind
"Review with my agent", the code-analysis and agent search engines only those tools
called, and the checks, scripts and docs that exist only for them. Install the root
harness in every workspace through the existing install path.

UI, API, stored data and ontology commitments stay as they are. No feature is rebuilt or
retired, and the parser is not migrated.

## Design

- **Provide.** Package `.agents/AGENTS.md` and the four maintained skills unchanged into the platform runtime. Each provider reads its native home: Codex `.codex/AGENTS.md`, Claude `.claude/CLAUDE.md`, with skills under the respective `skills/` directory. This incorporates the [2026-10-06 source and destination amendment](chat_intent/agent-harness/request.md).
- **Remove.** Delete everything that is only legacy harness. Keep any operation the UI,
  the API or the user CLI still reaches.
- **Launch settings.** Remove harness registration from provider configuration and keep
  the settings that enforce the permission policy.
- **Removed names.** Platform defaults and prompts stop naming removed skills and tools.
  A request that still names a removed tool is refused.
- **Review-round tools.** Retired by the [October 24 removal](chat_intent/workspace-colab-harden/remove-agent-review-comments.md); no review MCP server is registered.
- **Existing workspaces.** The next start supplies the new harness. Legacy files left in
  a retained root must not stay in effect.

## Gaps left by the removal

These keep their UI and API but lose what backed them:

- **Intent traceability:** agents write no new trace records.
- **Ontology editor:** it reads the retained spec format, not the `.g5` Markdown agents now write.
- **Workflow jobs:** job prompts name workflow profiles that only uplift has a skill for.
  Approval review needs an artifact format only the removed skill described, so its
  runs are expected to fail validation.
- **Browser automation and JSONL merge help:** agents have no tool for them;
  `g5d spec validate` remains.
- **Review hand-off:** agents cannot publish findings or comments and write no
  review-metadata records. The later removal also retires round reading and decisions.
- **Terminal agents:** the harness is installed by platform turns; manual launches before the first provider turn have no installed harness.
- **Agent guidance** on the shared checkout, force-with-lease, linked issues and
  platform reports: gone.

## Acceptance

1. Claude and Codex agents in a workspace can read the provided workflow and open all four skills.
2. No G5-provided hook or review-comment MCP server runs.
3. Provided files never appear in Git, and a repository's own instruction files are untouched.
4. A request naming a removed tool is refused; a normal launch is not.
5. Existing UI and API entry points still respond, and stored records and old conversations stay readable.
6. The required Rust and E2E checks pass on the PR head.

## Resolved during coding

- Nothing is installed into a checkout: both providers read their native homes. A repository's own instruction files stay in effect alongside. The source file is now the dedicated `.agents/AGENTS.md`; providers discover the four skills from their native skill directories. Instruction links are not rewritten.
- Explainer and review prompts named removed skills or tools and are reworded; every
  explainer entry point shares the save instructions. Permission mode, prompts,
  `--strict-mcp-config` and the Codex sandbox are unchanged.
- Both homes record provided skill names so refresh retires only previously provided skills. Claude enables user and project settings to load native user instructions and skills, preserves unrelated owner files, and removes the old `.g5/harness` directory after the replacement is installed.
- A removed tool is refused at API admission and again at turn start, from one shared rule.
- The code-analysis and agent search engines are removed with the architecture assertion
  that retained them; agents search with their providers' own tools.
