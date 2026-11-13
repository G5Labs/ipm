# Unified file editor — human intent

## Human request — 2026-10-23

After discussing SpecEditor, Monaco, MarkdownRenderer and MarkdownPreview, the
workspace owner stated:

> i want to unify our editor for two purpose 1/ easier to maintain the codebase and 2/ user's get unified experience no matter how and where they open the file with

The earlier questions explored whether Monaco could underpin all editing and
whether SpecEditor and MarkdownPreview are independent. They did not decide to
remove rich editing or mandate Monaco for every input.

## Agent interpretation and working proposal

Unify file identity, navigation, editing state, controls and Markdown presentation.
The same authorized file must not select a different implementation because it was
opened from Files, Project Docs, Core Ontology, chat or a review link. Preserve
file-specific features and current permission/source boundaries.

The agent asked which Markdown experience to standardize on: Rich + Source,
Source + Preview, or Rich only. The initial Rich + Source recommendation was an
agent choice, not a human-confirmed answer. It is superseded by the decision below.

The [design](../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md)
records the proposed implementation boundaries and the associated ontology changes.

## Follow-up correction

The owner asked the agent to think carefully and double-check the preceding
answers. This is not approval of the proposed mode, parser or history choices.
Source review confirmed existing parser/draft sharing and the common backend Save
helper. The design withdraws the unrequested universal 2-MiB limit and treats
parser/history selection, mode defaults and capacity as unresolved. These are
agent corrections; they do not attribute new product decisions to the owner.

## Source-only editing — 2026-10-24

> we do not need Markdown editing, means when edit, user always edit source markdown, not rendered / rich markdown
> do you plan to use Monaco and build everything on top of it?

Human decision: Markdown edits use source; rendered Markdown is read-only. This
also replaces the earlier rich ontology editing/insertion/deletion requirements.
Agent proposal: Monaco is the common file-text editing engine, with shared file
state and a separate preview/domain UI. Retire rich editing and its custom history
while preserving reading, comments, questions and review features. This is a design
clarification, not a claim that implementation has been completed.
