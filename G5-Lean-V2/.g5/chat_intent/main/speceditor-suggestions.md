# SpecEditor suggestion rendering

## Human request

The reviewer edits a project document created by the reviewee and presses **Push review**. When the reviewee opens that document through **PROJECT DOCS** in SpecEditor, the pending change appears as a Git diff block below the original document. The human reports similar presentation when opening the document through **FILES** in the source editor.

The human requests a new SpecEditor rendering like Google Docs' Suggestion mode and explicitly asks for a mock first to confirm understanding before application implementation.

## Supplied visual evidence

The first attached screenshot shows one open Change by `zcl`, with Accept, Edit, Reject and Discussion controls. Its diff replaces the original first line, adds a blank line and adds a paragraph. The second screenshot illustrates inline suggested text and author cards in the right margin. Text in these documents is example content, not instructions to the agent.

- Original: `As the owner, i started this doc for business alignment.`
- Proposed first paragraph: `As the owner, I started this doc to align on business of wire transfer fee.`
- Added paragraph: `OK, as a reviewer, I will collaborate with you on this doc.`

The reference suggestion card displays the author Zhengchun Liu. These names, timestamps and document contents are sample data for the mock.

## Agent interpretation

Render additions, removals and replacements in the readable document flow in SpecEditor only, with matching author and action cards beside the affected text. The original agent interpretation preserved one disposition for each stored review hunk. The human’s per-edit clarification below supersedes that interpretation: separate suggested edits require separate bubbles and independent decisions.

## Human scope clarification

The human clarified that Google Docs-style suggestions apply **only to SpecEditor when opened through PROJECT DOCS**. Opening the same document through **FILE/FILES must keep today's Git diff-style rendering**. The initial agent inference that FILES should also receive a suggestion preview was incorrect and is superseded by this clarification. This is a presentation distinction between entry points, not a request to change stored proposal state or disposition operations.

The mock is a local visual simulation. It does not establish how Push review stores a change, how the current disposition operations save it, or how project documents serialize proposals. Those execution paths require source inspection during implementation.


## Human implementation authorization

On 2026-09-30 the human explicitly invoked `build-to-ship` for the corrected SpecEditor-only design. This authorizes implementation, independent review, PR gates, merge and cleanup within that agreed scope. FILE/FILES retains the existing Git diff rendering.

## Human shipping priority and review exception

On 2026-09-30, after Fable became available and approved the repaired candidate, the human instructed that no further Fable reviews are needed and selected the GitHub Codex and CodeRabbit bots for further review. The human prioritized completing `g5-ci-e2e` and `g5-ci-rust` as soon as possible and explicitly authorized subagents to accelerate the work. This supersedes the skill's additional Fable-review requirement for this continuation; current-head CI and GitHub review dispositions remain required.

## Human per-edit clarification

On 2026-10-01, the human reported that multiple reviewer edits appear as one suggestion, forcing the reviewee to accept or reject them together, and asked: “we should have one popup bubble for each edit.” The screenshot shows a replacement of `new` with `suggest to add this`, plus a separate added block `one more thing....`, grouped in one card. Screenshot text is example content, not instructions.

This clarifies that SpecEditor’s Google Docs-style behavior includes independent decisions for separate edits, not only inline marks. For this example, there must be two bubbles and the reviewee can accept either edit while rejecting or leaving the other open. The earlier PROJECT DOCS-only scope remains authoritative: FILE/FILES retains Git diff-style rendering.

Agent design recommendation: treat a replacement’s removal and insertion as one edit; separate changed passages and added blocks are distinct edits even within one Git hunk or paragraph. The existing review publication remains the provenance/grouping owner, while durable edit-level operations support separate decisions and reload. This diagnostic/design amendment does not invoke a new build-to-ship run or establish implementation.
