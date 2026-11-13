# Documents: editor

The one editor, its modes, rich-text preview, Markdown rendering and text measure. See [Documents](../documents.g5.md#g5.documents).

## Editor

<a id="g5.documents.editor"></a>
[D · g5.documents.editor]

One editor serves every file, choosing its reading and editing features from the file's type and the person's authority.

<a id="g5.documents.editor.experience"></a>
[I · g5.documents.editor.experience]

Give a person the same viewing and editing experience wherever they open a file. Files, Project Docs, Core Ontology and links use one editor, with features set by the file and the person's authority, not the entry point.

- An explicit historical revision stays a separate read-only view.
- Opening the same current file from another entry point keeps the person's draft and view.

<a id="g5.documents.editor.ownership"></a>
[D · g5.documents.editor.ownership]

Give each file one authoritative editing session with shared save and conflict coordination, so state cannot diverge. Extend the existing [draft lifecycle](files.g5.md#g5.documents.files-view.drafts) rather than add a parallel store.

- Views may own cursor and pending composition state.
- The session wraps the [text and history engine](#g5.documents.editor.engine) without copying its state elsewhere.
- Reuse existing read, Save, suggestion and discussion authorities, including the shared backend Save.

<a id="g5.documents.editor.modes"></a>
[D · g5.documents.editor.modes]

A Markdown file is one rendered document in two modes, with no separate preview tab: rich text reads and edits it in place under [rich editing](#g5.documents.preview.rich-editing), and plain text shows its Markdown in the common [engine](#g5.documents.editor.engine). The file row is the only toolbar: it names the path and what the buffer shows and carries every control, including the mode switch; there are no mode tabs.

- While the rendered document is read, the row offers Compare, the Human and Risk/Question filters, Expand all and Collapse all where statements fold, the mode switch, Edit or Suggest changes, Push review, Save, Done, Discard and Comments.
- The switch says "Switch to plain text editing" or "Switch to rich text editing" while editing, and drops "editing" while reading.
- HTML and JSONL keep their specialized previews as the rendered mode and edit as plain text; SVG does the same, with its [image](files.g5.md#g5.documents.viewers.images) as the rendered mode.
- Other supported text files are plain text only, read-only until Edit. Raster images and PDFs open in read-only [viewers](files.g5.md#g5.documents.viewers) in the same tab, and any other binary file offers Download.
- The same modes apply wherever a file opens, keeping the selected mode, whether the person was editing and the draft. Editing an owner file or composing a reader suggestion uses either mode.
- Comments, review decisions and question actions keep their domain controls.
- Switching modes, or entering and leaving editing, keeps the reading view's state: Compare, filters, open details, folds and position.
- [Opening defaults](files.g5.md#g5.documents.source-editing) are a separate choice from the editing decision.

## Preview and rich text

<a id="g5.documents.preview"></a>
[D · g5.documents.preview]

The rendered preview shows a file as a reader sees it; for Markdown it is also the Rich text editing surface.

<a id="g5.documents.preview.rich-editing"></a>
[D · g5.documents.preview.rich-editing]

A person who may edit the file or suggest changes edits rendered Markdown as a structured document in Rich text, with paragraphs, headings, lists, quotes, code blocks, tables, links and inline formatting, created and changed in place. Plain text shows the same draft's Markdown.

- Rich text opens in place of the reading view, from Edit, the `e` key or a double-click, at that block.
- Done is always available while editing and returns to the reading view showing the unsaved draft, where Save, Push review and Discard remain until the person decides.
- Whatever the editor does not model stays opaque: front matter, raw HTML, math, footnotes, reference definitions and reference-style links and images, alerts, question fences, and record identity, anchor and metadata lines.
- Opaque content renders as in the reading view and is edited only in plain text; in Rich text it can only be deleted or moved whole, and saves its source verbatim. Front matter and record identity, anchor and metadata lines cannot be deleted or duplicated in Rich text.
- Ontology statements are the exception: Rich text adds and deletes them whole under [statement creation](ontology.g5.md#g5.documents.ontology.creation).
- In ontology files Rich text shows prose only, hiding a statement's identity, anchor, identity-heading, paired metadata lines and the Metadata heading, while a statement with a diagnostic or unpaired metadata keeps those lines visible for repair.
- Each statement keeps the reading view's bullet and outline, editing keys stop at its boundary, Shift+Enter adds a paragraph within it, and an empty statement shows a placeholder to type or remove it.
- Saving serializes the document back to Markdown through the same guarded draft write, Save, Push review, Discard and conflict rules as plain text.
- By default unchanged top-level blocks keep their source bytes and edited blocks follow the file's marker style, so a diff or suggestion shows what changed; the owner does not rely on this.
- Above the [preview deferral](#g5.documents.editor.bounds), Rich text waits for Load preview and Edit opens plain text.

## Bounds and engine

<a id="g5.documents.editor.bounds"></a>
[D · g5.documents.editor.bounds]

Admit complete UTF-8 text without NULs within the 32-MiB [read ceiling](files.g5.md#g5.documents.read.bounds), the same across openers. Source stays available; never silently truncate it or lower its admission to satisfy Preview.

- Markdown, JSONL and SVG previews share a threshold of 1 Mi UTF-16 code units (1 MiB of ASCII text); above it, defer rendering behind an explicit Load preview, except for [findings files](ontology.g5.md#g5.documents.findings.files).
- HTML files (`.html`, `.htm`, `.xhtml` and `.shtml`, case-insensitively) use the existing sandboxed HTML preview without a preview-size gate, like [PDFs and raster images](files.g5.md#g5.documents.viewers); the read ceiling still applies.
- Read admission and interactive performance are separate limits that may be revisited with representative measurements; production performance at the threshold and for large HTML remains unverified.
- Owner writes through the shared Files PUT keep the 32-MiB JSON request envelope.
- Reader suggestions keep a separate 2-MiB UTF-8 limit on base and proposed text, shown before Push, and keep an oversized draft for correction.

<a id="g5.documents.editor.engine"></a>
[D · g5.documents.editor.engine]

Monaco is the common engine for plain text — including Markdown and ontology source — code files and Compare. It keeps native undo/redo and the file's model through mode switches, and plain text wraps at the [text measure](#g5.documents.text-measure).

- The rendered document derives from source; for Markdown, [Rich text](#g5.documents.preview.rich-editing) serializes its document back into the same draft, which the retained model adopts as one edit.
- SpecEditor's custom rich-text editing and history retire once its reading, review and domain features move to the shared editor.
- The existing plain-text loading fallback joins the same draft lifecycle.

<a id="g5.documents.text-measure"></a>
[D · g5.documents.text-measure]

File editors read and edit text in one column at most 8 inches (768 CSS pixels) wide, wrapping there. Rendered Markdown and Rich text share that centred column, so entering or leaving editing moves no text, and plain text wraps at the same measure for Markdown and code.

- HTML and JSONL are exempt and keep unwrapped lines.
- A top-level table narrower than the measure stays flush with the text; a wider one grows to the pane, centred, and scrolls inside itself, so nothing is clipped and the page never scrolls sideways. A table nested in a list or statement frame stays within its block.
- Plain text cannot exempt table rows, which wrap like any line.
- Compare keeps its own side-by-side layout.

## Markdown

<a id="g5.documents.markdown"></a>
[D · g5.documents.markdown]

One Markdown implementation renders files wherever they are read, combining standard Markdown with G5's own semantics.

<a id="g5.documents.markdown.shared"></a>
[D · g5.documents.markdown.shared]

One Markdown preview implementation renders files for reading, with standard Markdown and G5's own semantics; the opener cannot pick a different dialect for the same file. It preserves source anchors for comments, navigation and review, and keeps the current HTML trust boundaries.

- Rendering never rewrites source, including metadata or unknown syntax. The only rendered-view writes are explicit domain actions and [Rich text](#g5.documents.preview.rich-editing)'s serialization, which keeps unchanged blocks and opaque constructs verbatim.
- Domain actions use their existing authorities; rendering cannot execute a decision or file write.
- Statement images use this shared image renderer.
- Review controls show the current preview directly, without the retired collapsed-Boundary guidance or a redundant Edit document text action.

<a id="g5.documents.markdown.code-copy"></a>
[I · g5.documents.markdown.code-copy]

Every rendered code block offers a Copy control — in Claude and Codex chat replies, Project Docs, file and Markdown previews, and issue and question text. Copy places the block's exact text on the clipboard, never highlighted markup, prompts or line numbers.

- It appears whether or not the fence names a language, and on restricted content, whose fences render as inert code.
- Mermaid, React Flow, sandboxed-widget, math and question fences keep their own rendering and get no control.
- Copy is keyboard-reachable and labelled, briefly confirms success and visibly reports a missing or refused clipboard, without breaking narrow chat or detached-panel layouts.
- It is a person's explicit action, separate from the [device-code copy](../chat.g5.md#g5.chat.ui.device-code-copy).

## Metadata

### g5.documents.editor

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Defines the `g5.documents.editor` level for [ID levels](../ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [Experience](#g5.documents.editor.experience), [Ownership](#g5.documents.editor.ownership), [Modes](#g5.documents.editor.modes), [Bounds](#g5.documents.editor.bounds), [Engine](#g5.documents.editor.engine).
- Source: [ontology writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) clean-up, 2026-11-06.

### g5.documents.editor.experience

- human-provided
- Expands: [Editor](#g5.documents.editor).
- Source: the owner's [unification request](../../../chat_intent/workspace-colab-harden/unified-editor.md), 2026-10-23. The [design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md) records the implementation boundaries; the owner subsequently chose source-only Markdown editing.
- Implemented by canonical current-file tabs and FileView. Scope and browser evidence: [shared editor implementation](../../../pr_ontology_diff_design/unify-file-editing-and-render/ontology-plan.md#implementation-and-measured-capacity), including the editor-consistency follow-up. Historical views remain separate.

### g5.documents.editor.ownership

- agent
- Expands: [Editor](#g5.documents.editor).
- Decision and alternatives: [unified editor design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#ownership-and-reuse), supporting the owner's lower-maintenance goal.
- `fileDrafts` owns retained Monaco models, view selection and the one pending Save slot for source and domain writers. Project Docs, Files and ontology join that same key. Backend doc/file saves reuse `write_workspace_file_and_maybe_push`.

### g5.documents.editor.modes

- human-provided, agent
- Expands: [Editor](#g5.documents.editor).
- The owner's [source-only decision](../../../chat_intent/workspace-colab-harden/unified-editor.md#source-only-editing--2026-10-24) supersedes the agent's Rich + Source recommendation: rendered Markdown is for reading, and edits use source.
- Agent clarification follows the [file-view design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#user-experience): available previews depend on the file type, consistently across entry points.
- FileView coordinates the rich and plain text modes and shared Save/recovery controls; HTML and JSONL receive the same draft through specialized previews. Legacy Doc handles resolve before content dispatch.
- The owner's [preview block editing design](../../../../docs/preview-block-editing-design.md) (direction approved 2026-10-26, [request](../../../chat_intent/rich-text-editing/request.md)) narrows source-only editing to allow one block's source to be edited in place in Preview.
- The owner's [rich-editing decision of 2026-10-29](../../../chat_intent/rich-text-editing/request.md#human-decision--rich-editing-in-preview--2026-10-29) supersedes both: rendered Markdown is edited as a structured document in Rich text under [rich editing](#g5.documents.preview.rich-editing).
- The owner's [one-surface decision of 2026-10-30](../../../chat_intent/rich-text-editing/request.md#human-decision--one-surface-with-a-richplain-text-switch--2026-10-30) removes the mode tabs and the read-only Preview tab: rich text and plain text are the two modes of one surface. The switch while reading, HTML/JSONL on the same switch and the retained editing state are agent choices in the [plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#one-surface-with-a-richplain-text-switch--2026-10-30).
- Later that day the owner [moved every control to the file row](../../../chat_intent/rich-text-editing/request.md#human-decision--tables-exempt-from-the-measure-controls-in-the-file-row--2026-10-30), including the switch first placed in the status row; the row's order and the merged toolbar are agent choices in the [plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#owner-follow-up--tables-exempt-controls-in-the-file-row--2026-10-30).
- The owner [moved Compare and the filters into the file row](../../../chat_intent/issue-925/decisions.md#human-decision--reading-tools-in-the-file-row--2026-11-02) on 2026-11-02; showing them only while the rendered document is read, before the mode switch, is an agent choice in the [plan](../../../pr_ontology_diff_design/issue-925/ontology-plan.md#reading-tools-in-the-file-row--2026-11-02).
- [Issue #925, K2](../../../chat_intent/issue-925/decisions.md#human-scope--2026-10-28): view switches keep the reading view's state; restated for the [one-surface](#g5.documents.editor.modes) modes when #925 was rebased onto it; [plan](../../../pr_ontology_diff_design/issue-925/ontology-plan.md).
- K2 is implemented by `FileView` holding the reading view's `readingState` for its mount; tests in `DocumentPreview.test.tsx` and `file-editor-925.spec.ts`.
- Rebased onto #925 on 2026-11-03: the ontology verification display's Expand all and Collapse all join these reading tools in the file row, and its fold and verification-details store lives in the same `readingState`, so folds survive view switches too (agent reconciliation, [plan](../../../pr_ontology_diff_design/ontology-formal-verification/ontology-plan.md#rebase-onto-925-3-november-2026); test in `DocumentPreview.folds.test.tsx`).

### g5.documents.preview

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Defines the `g5.documents.preview` level for [ID levels](../ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [Rich editing](#g5.documents.preview.rich-editing).
- Source: [ontology writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) clean-up, 2026-11-06.

### g5.documents.preview.rich-editing

- human-provided, agent
- Expands: [Preview](#g5.documents.preview).
- Source: the owner's [rich-editing decision of 2026-10-29](../../../chat_intent/rich-text-editing/request.md#human-decision--rich-editing-in-preview--2026-10-29), which supersedes the [source-only decision of 2026-10-24](../../../chat_intent/workspace-colab-harden/unified-editor.md#source-only-editing--2026-10-24) and retires the per-block source editor of the owner's [block-editing design](../../../../docs/preview-block-editing-design.md) (PR #871). The opaque set, protected record lines, block-level preservation and style inference are agent choices explained in the [plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#rich-editing-in-preview--2026-10-29).
- Amended 2026-10-30 by the owner's [one-surface decision](../../../chat_intent/rich-text-editing/request.md#human-decision--one-surface-with-a-richplain-text-switch--2026-10-30): the read-only Preview tab beside Rich text is gone; Done is always available and returns to the reading view with the draft applied, where Save, Push review and Discard remain ([plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#one-surface-with-a-richplain-text-switch--2026-10-30)).
- Implemented by `RichMarkdownEditor.tsx` (Tiptap over ProseMirror), `markdownDocument.ts` (Markdown to document and back, opaque nodes, preservation), `FileView` (entry, role gate, draft writes, Save and Push) and `CodeEditor.onBeforeSave`; tests in `markdownDocument.test.ts`, `RichMarkdownEditor.test.tsx`, `FileView.test.tsx` and the `file-editor` and `markdown-layout` browser specs.
- Ontology prose-only Rich text: the owner's [decision of 2026-10-29](../../../chat_intent/issue-925/decisions.md#human-decisions--2026-10-29) with the agent's five conditions it accepted; [plan](../../../pr_ontology_diff_design/issue-925/ontology-plan.md#prose-only-ontology-rich-text).
- Prose-only ontology Rich text is implemented by `hiddenRecordLines` in `markdownDocument.ts` (one hiding rule with the reading view) and `statementLayout` with the `OntologyStatements` extension in `richExtensions.ts` (statement bodies, boundary keys, selection skipping); tests in `markdownDocument.test.ts`, `RichMarkdownEditor.test.tsx` and the `file-editor` browser spec.
- Whole-statement add and delete, Shift+Enter and the empty-statement placeholder: the owner's [decisions of 2026-11-02](../../../chat_intent/issue-925/decisions.md#human-decisions--2026-11-02); [plan](../../../pr_ontology_diff_design/issue-925/ontology-plan.md#rich-text-statement-add-and-delete--2026-11-02).

### g5.documents.editor.bounds

- human-provided
- Expands: [Editor](#g5.documents.editor).
- The [corrected capacity assessment](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#verified-starting-point-and-corrections) withdrew an unsupported universal 2-MiB limit.
- The [measured implementation](../../../pr_ontology_diff_design/unify-file-editing-and-render/ontology-plan.md#implementation-and-measured-capacity) settles this question: native Chromium source workloads through 32 MiB remained available; large Markdown AST/DOM work warrants explicit deferred Preview. These development measurements are not a cross-device latency guarantee.
- The owner selected 1 MiB on 2026-11-09 for [issue #1180](https://github.com/G5Labs/G5-Lean/issues/1180), retaining the existing code-unit measure. The [capacity follow-up](../../../pr_ontology_diff_design/unify-file-editing-and-render/ontology-plan.md#preview-threshold-follow-up--2026-11-09) records the change and outstanding performance evidence.
- After PR #1188 merged, the owner requested the [HTML-family exception](../../../chat_intent/raise-file-preview-size-limit/html-preview.md); the [audit and design](../../../pr_ontology_diff_design/unify-file-editing-and-render/ontology-plan.md#html-preview-exception--2026-11-09) distinguish the preview gate from remaining read and decode limits.

### g5.documents.editor.engine

- agent
- Expands: [Editor](#g5.documents.editor).
- Amended 2026-10-29 by the owner's rich-editing decision: Rich text serializes the edited Markdown document back into the draft; Monaco keeps Source, code files and Compare ([plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#rich-editing-in-preview--2026-10-29)).
- Amended 2026-10-30: plain text wraps at the 8-inch [text measure](#g5.documents.text-measure) by the owner's [one-surface decision](../../../chat_intent/rich-text-editing/request.md#human-decision--one-surface-with-a-richplain-text-switch--2026-10-30).
- [Design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#ownership-and-reuse), following the owner's source-only decision and question about using Monaco. One source engine removes the need for cross-engine undo or source-preserving rich edits.
- CodeEditor retains native models through navigation, Save and Preview; the loading fallback joins the draft. Native-browser journeys cover source fidelity, undo/redo, person changes, delayed saves and recovery.

### g5.documents.text-measure

- human-provided, agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Source: the owner's [one-surface decision of 2026-10-30](../../../chat_intent/rich-text-editing/request.md#human-decision--one-surface-with-a-richplain-text-switch--2026-10-30): 8 inches, wrapping, HTML and JSONL exempt. The centred column, the Monaco column count derived from the measured font and Compare's exemption are agent choices in the [plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#one-surface-with-a-richplain-text-switch--2026-10-30).
- The owner [exempted tables](../../../chat_intent/rich-text-editing/request.md#human-decision--tables-exempt-from-the-measure-controls-in-the-file-row--2026-10-30) the same day, choosing the measured "let tables bleed" alternative; flush alignment for small tables and the nested-table limit are agent choices in the [plan](../../../pr_ontology_diff_design/rich-text-editing/ontology-plan.md#owner-follow-up--tables-exempt-controls-in-the-file-row--2026-10-30).
- Implemented by `textMeasure.ts`, the `g5-doc-column` rule and CodeEditor's bounded wrap; browser evidence in `markdown-layout.spec.ts` and `file-editor.spec.ts`.

### g5.documents.markdown

- agent
- Expands: [Documents](../documents.g5.md#g5.documents).
- Defines the `g5.documents.markdown` level for [ID levels](../ontology_and_harness.g5.md#g5.ontology.writing.id-levels).
- Children: [Shared](#g5.documents.markdown.shared), [Code copy](#g5.documents.markdown.code-copy).
- Source: [ontology writing principles](https://github.com/G5Labs/G5-Lean/issues/1119) clean-up, 2026-11-06.

### g5.documents.markdown.shared

- agent
- Expands: [Markdown](#g5.documents.markdown).
- [Shared preview design](../../../pr_ontology_diff_design/workspace-colab-harden/ontology-plan.md#ownership-and-reuse). Source-only editing removes the rich-edit parser requirement; consolidate read-only rendering while retaining G5 behavior and source anchors.
- DocumentPreview and read-only statement/review frames share MarkdownPreview; MarkdownRenderer adapts other read-only consumers. Domain actions splice guarded source through existing writers; SpecEditor and its structural mutation/UI helpers are retired.
- [Rich text](#g5.documents.preview.rich-editing) adds the one further rendered-view write: a serialization of the edited document that keeps unchanged blocks verbatim (the owner relaxed byte locality on 2026-10-29).

### g5.documents.markdown.code-copy

- human-provided
- Expands: [Markdown](#g5.documents.markdown).
- Source: the owner's [issue #823 scope amendment](https://github.com/G5Labs/G5-Lean/issues/823) (2026-10-26 comment): Copy on every rendered code block, every `MarkdownRenderer` surface, restricted content included, reusing the existing clipboard pattern. Two choices are agent readings of "rendered fenced code block": math fences get no control, and indented code blocks in the GitHub-style file preview do get one. Design: [issue 823 plan](../../../pr_ontology_diff_design/issue-823/ontology-plan.md#code-block-copy).
- Implemented by `shared/CodeCopyButton.tsx` over `lib/clipboard.ts` (shared with the device-code copy), through the common `MarkdownPreview` code-block renderer. `MarkdownRenderer` and `DocumentPreview` reuse it for chat, issues, questions, reviews and document previews. `DocumentPreview.codeCopy.test.tsx` preserves the retired SpecEditor copy regressions on the shared preview; `e2e/browser/code-copy.spec.ts` covers clipboard text and layout across shared surfaces.
