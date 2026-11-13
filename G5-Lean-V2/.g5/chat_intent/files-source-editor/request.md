# FILES source editor — GitHub-style read-first view and reader suggestions

## Human request

On 2026-10-03 the user asked for research before any design:

> when user opens a file from the FILES tab (left panel), G5 opens it basically as a text editor.
> please research on open source solutions that basically can deliver identical experience as Github (basically on github when user opens a file from repo, it opens as read model by default (preview or code), but also allows user to edit (in preview model or edit source mode)) [...] the edit will be sent to server as a git patch I believe (please correct me if wrong)
> please also consider pros/cons to write one such editor from scratch
> very important, I do not meant to change the SpecEditor (when user opens file from the `PROJECT DOCS` tab) we have today

The research (`docs/research/reports/GitHub style file viewer and editor.md` and its HTML summary, untracked) found that GitHub opens files read-first, edits source only (its Preview tab is read-only) and saves the whole file plus its starting commit rather than a patch. The user then answered design questions in chat:

- Rendered-view editing: "ok, agree" to leaving it out of the first version.
- Editor: "yes, let keep using Monaco".
- Renderer: "SpecEditor is expected to be G5's own renderer which understands G5-specific blocks, for SorceEditor we are working on here, can we use Monaco as the Markdown render?" (Monaco cannot render Markdown; a separate GitHub-style renderer draws Preview.)
- Lost edits: "we surely need to fix this, as part of this effort we are working here".
- Binary files: "we should only support opens source code and markdown (i.e, text files) with our integrated Monaco."
- Who edits: "for owner, their change directly commits to that branch, but for non-owner, their changes ended up with a suggestion to owner (i.e, same as today's review process [...]) and then render as suggestion mode for owner to accept / reject".
- Suggest changes: "this is different as the `start review` button because 1/ non-owner directly work on the owner's branch (no fork), and there won't be any VM attached (i.e, non-owner can `suggest changes` regardless of workspace vm. we surely do NOT need GitHub's \"fork and edit\""
- Non-owner Save: "for non-owner, Save button should be `Push review`".
- Starting compute: "Start workspace to edit" is "true ONLY to owner because owner directly generate a commit to the branch, but for non-owner, they can use the `Suggest changes` regardless of VM because their changes will be render as suggestion to owner instead of landing a commit to the branch/workspace".
- Remaining proposals: "yes to all except the 1st item of 8" and finally "(a), agree with all defaults".

On 2026-10-04 the user asked to implement:

> go ahead with the formal design and then implement it, both of your design and implementation should be approved by codex (please invoke it via cli) with gpt-6-astra model and `extra high` effort
> you should not over-engineer for things that are out of scope, and keep in mind that G5-Lean current does not have any external users, so please do not over-design for rollout as we are fine to have some outage in qa and even manual effort

## Agent interpretation

The agent proposed the following, and the user accepted each item ("yes to all", "agree with all defaults", option "(a)"):

- **Read first.** FILES opens Markdown in a rendered Preview and other text files in a read-only Monaco Code view. Edit switches the same view to editing in place. A link to a line opens Code view at that line. HTML files keep their current preview.
- **Markdown editing.** Edit mode has Edit and Preview tabs like GitHub; Preview renders the unsaved text and cannot be typed into.
- **Preview content.** GitHub-flavored Markdown (tables, task lists, footnotes, alerts, strikethrough, math, Mermaid) with sanitized raw HTML. Links to other repository files open FILES tabs, and relative images load from the workspace.
- **Owner Save.** Editing needs a running workspace; a stopped workspace shows "Start workspace to edit". Save keeps its automatic "Save {path}" commit and push. A version conflict keeps Reload and Save my version and adds Compare, a Monaco diff view where the user merges by hand before saving.
- **Reader suggestions.** Suggest changes works on the owner's branch as the reader sees it, in any VM state, with no fork. Push review takes an optional note that starts the suggestion's discussion. Each push adds suggestions; a reader changes an earlier one by withdrawing it. A stale base warns and offers Compare, and pushing is still allowed. Version 1 covers edits to existing text files. Base and frozen workspaces stay read-only for everyone.
- **Owner review.** Unchanged: Git diff-style suggestions in FILES Code view, Google Docs-style marks in SpecEditor, Accept or Reject per suggestion. Preview lists the file's pending suggestions beside the page.
- **Agent writes.** A clean view refreshes; an editor with unsaved edits shows changed-on-disk with Compare and Reload.
- **Comments.** Threads stay on source lines. Preview lists them beside the page, and new comments are added from Code view.
- **File types.** Only text files open in the editor. Binary files show a card with name, size and Download. Text files over 2 MB show "Too large to display".
- **Drafts.** Unsaved edits survive mode and tab switches, and leaving the page with unsaved edits asks first.
- **Editor assets.** Monaco ships with the G5 frontend instead of loading from jsDelivr at runtime.
- **Out of scope for version 1.** Editing inside the rendered Preview; suggesting new files, renames or deletions; previews for other formats such as CSV, SVG or notebooks; a commit dialog with a message or branch choice; changes to SpecEditor or to the PROJECT DOCS views.
- **Delivery.** No feature flags, staged rollout or compatibility layers: G5-Lean has no external users, and a short QA outage or a manual step is acceptable.
