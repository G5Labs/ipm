# Change highlights within modified statements

Request: [owner request and clarification, 2026-11-03](../../chat_intent/saehan-ontology-diff-improvement/request.md).
Status: implemented on 2026-11-03; see [Implementation and verification](#implementation-and-verification--2026-11-03).
Follow-up: [shorter Before version](#shorter-before-version--2026-11-04), from UI review
on 2026-11-04.

## Scope

The owner chose option C alone:

- line up sentences, then highlight changed words only inside sentences that changed;
- fall back to today's whole-statement view when a statement changed too much.

It keeps the existing stacked layout (Current above Before) and adds no change summary.
Marks are background color only, as the owner asked: no strikethrough or underline.
Affected surfaces are those using the Statement changes presentation: per-file Compare,
All changes and the chat changes card.

Out of scope:

- the summary (E);
- F, whitespace-insensitive matching;
- G, Diff Graph and the rebase/merge preview, which keep their line diffs;
- D's folding and dimming (the 2026-11-04 follow-up folds only the Before row's
  unchanged sentences);
- H, a one-sentence-per-line authoring convention;
- statements that were Added or Deleted, since the whole statement is new or removed.

## Ontology changes

- New [`g5.documents.ontology.change-highlights`](../../ontology/features/documents.g5.md#g5.documents.ontology.change-highlights)
  states what is marked, rendered-text comparison, when the pair falls back to no
  highlights, and the presentation-only boundary.
- [`g5.documents.ontology.statement-changes`](../../ontology/features/documents.g5.md#g5.documents.ontology.statement-changes)
  drops "word-level highlighting" from its deferrals and links the new statement.

### Governing commitments, unchanged

- [Matching rule](../../ontology/features/documents.g5.md#g5.documents.ontology.comparison-matching)
  and [comparison owner](../../ontology/features/documents.g5.md#g5.documents.ontology.comparison-owner):
  the Added, Modified and Deleted labels and the shared corpus stay as they are.
  Highlights only explain a Modified pair.
- [Statement changes](../../ontology/features/documents.g5.md#g5.documents.ontology.statement-changes):
  restrained color plus text labels; Compare reflects unsaved edits and never alters them.
- [Rich editing](../../ontology/features/documents.g5.md#g5.documents.preview.rich-editing):
  Compare appears in the reading view only, so highlights never meet the rich editor.
- [Chat changes card](../../ontology/features/chat.g5.md#g5.chat.ontology-card) and
  [All changes](../../ontology/features/documents.g5.md#g5.documents.ontology.all-changes)
  reuse the Statement changes presentation, so they inherit the highlights.

## Design

**Owner.** One browser-side presentation owner, in two parts:

- **A pure function** in `frontend/src/lib/`. It takes the rendered text of the two
  versions, with block boundaries, and returns the ranges to mark in each version. It
  returns nothing when the pair should show without highlights.
- **`OntologyComparisonPair`**, which applies the result. Every surface already renders a
  Modified pair through this component, and it holds both rows. It reads each row's
  statement body (`semantic-record-blocks`) and registers the ranges.

No server work, stored state or new lifecycle is added: highlights are derived from what
is on screen each time it renders.

**Interaction sketch.**

1. A pair renders, or its Current row re-renders after an unsaved edit, a refresh, a card
   edit or Show more.
2. The pair extracts the text of both bodies, skipping elements marked as non-content
   (the card's "(edited)", screen-reader notes, diagnostics). Each block boundary ends a
   sentence.
3. It calls the function.
4. It replaces only its own ranges in two shared highlight registrations: one for
   inserted text and one for removed text.

A MutationObserver on the two bodies repeats steps 2–4 when React replaces text nodes,
because replacing a node collapses any Range over it. Unmounting removes the pair's
ranges.

**Many pairs at once.** All changes, a chat card and an editor tab can all be on screen
together. Each highlight name maps to one shared `Highlight`, and each pair adds and
deletes only its own Range objects; no pair calls `CSS.highlights.set` over another
pair's ranges.

**Algorithm.**

1. **Size cap.** If either version has more than 2,000 words, return no highlights.
2. **Sentences.** Split each block into sentences with `Intl.Segmenter("sentence")`,
   normalize whitespace, and align the two versions' sentences with an LCS. If the
   normalized texts are identical, return no highlights.
3. **Rewritten sentences.** Within each gap between kept sentences, pair removed and added
   sentences in order when they share at least half their words (Dice overlap of the word
   multisets). Each such pair is rewritten.
4. **Words.** Inside a rewritten pair, an LCS over word and punctuation tokens
   (`Intl.Segmenter("word")`) marks the inserted and removed tokens. Both sentences are
   marked whole instead in two cases:
   - **Replaced:** fewer than half of the shorter sentence's words survive in order.
   - **Scattered:** the changes split into more runs than the kept words support (two
     runs, plus one per four kept words).

   An inserted clause or a single substituted word is one run, so it stays word-level
   however long it is.
5. **Unpaired sentences** are marked whole: added in Current, removed in Before.
6. **Fallback.** If fewer than a third of the shorter version's words are kept, across
   kept sentences and inside rewritten pairs, return no highlights. Measuring against
   the shorter version means a statement that only grew or shrank keeps its highlights;
   only replaced text falls back.

## Choices and reasons

**Rendered text, not Markdown source.** A source diff would mark syntax and line-wrapping
noise, and its offsets do not map to the rendered page. Rendered text matches what a
person reads.

The cost: a change to a link target, formatting or wrapping alone produces no text
difference, so such a pair shows like today's, without highlights. Labelling that case
would need a summary, which the owner did not choose; F would address the wrapping part
later.

**Custom highlights over markup changes.**

- Wrapping text in `<ins>`/`<del>` would mutate DOM that React owns, breaking
  reconciliation and the rendered-document selection and comment paths.
- A rehype plugin in `MarkdownPreview` keeps React in control. But Compare's Current row
  renders block by block in `DocumentPreview`, not through one `MarkdownPreview`, so ranges
  spanning blocks and links would need plumbing through both pipelines.
- The CSS Custom Highlight API paints over React's text nodes without changing them. It
  leaves content, selection, copying, comments, links and editing untouched, and one
  implementation serves every surface.

Its costs:

- Highlights are not exposed to assistive technology. The complete Before row remains, as
  today.
- Support starts at Chrome/Edge 105, Safari 17.2 and Firefox 140. Older browsers show
  today's pair.

**Color-only marks.** The owner found strikethrough hard to read. Underline is dropped as
well, because read-only links already carry underlines. Each row gets its own restrained
background color (`::highlight(ontology-inserted)` in Current,
`::highlight(ontology-removed)` in Before). The row labels say which side is which, so
hue never has to tell added from removed. The tints need enough luminance contrast with
the row background, in light and dark themes, to show the marked span without relying
on hue. Text color stays unchanged so the text keeps its contrast.

**Browser only.** Compare must reflect unsaved edits, and the matching rule with its
two-implementation corpus is untouched. If G is taken up, Diff Graph can call the same
function with the bodies the server returns.

**No new dependency.** `Intl.Segmenter` handles scripts without spaces between words
better than regex word splitting, and a bounded LCS is a few dozen lines.

## Limits and provisional parameters

- **2,000 words per version.** Larger statements show today's pair.
- **5 million units of work per statement**, counting LCS table cells and words compared
  for sentence overlap. Punctuation tokens are compared too, so the word cap alone does
  not bound the work. A statement that would exceed this shows today's pair. One
  2,000-word sentence without punctuation still fits.
- **Thresholds.** The values are:
  - 50% word overlap to pair sentences;
  - half of the shorter sentence kept, and at most two runs plus one per four kept words,
    to mark words inside a pair;
  - a third of the shorter version kept before the whole-statement fallback.

  They were checked against real statement edits (see below). The ontology says only
  "too little text survives".
- **Sentence splitting** is imperfect for abbreviations such as "e.g.". A wrong split makes
  units smaller, and the words are still diffed correctly.

## Acceptance and tests

**Unit tests for the function:**

- kept, added, removed and rewritten sentences;
- word insertions and deletions inside a rewritten pair;
- re-wrapping or Markdown-only changes (emphasis, link target) give no highlights;
- a heavy rewrite and a statement over 2,000 words give no highlights;
- list items without terminal punctuation are split by block boundaries;
- CJK text;
- determinism.

**Component tests (jsdom, with a stubbed highlight registry):**

- Ranges are registered for Modified pairs in Compare, All changes and the card, and
  never for Added or Deleted statements.
- The statement body's DOM is identical with and without highlights.
- The card's "(edited)" is ignored.
- Two pairs never remove each other's ranges, and unmounting cleans up.
- Without `CSS.highlights`, the pair renders as today.

**Browser tests (Playwright mock-preview, fixture `?ontology&changed`, real Chromium):**

- The registered ranges cover the expected text and paint with a background color and no
  text decoration, in light and dark themes.
- An unsaved Rich text edit followed by Done recomputes them.
- A selection comment's quote and copied text are unchanged.

**Regression:** the existing comparison and card tests, and the unchanged
`tests/fixtures/ontology-comparison` corpus in both implementations.

## Implementation and verification — 2026-11-03

**Code:**

- `frontend/src/lib/ontologyChangeHighlights.ts` holds the pure function.
- `frontend/src/components/editor/useChangeHighlights.ts` handles text extraction, the
  shared registrations and the MutationObserver.
- `OntologyComparisonPair` applies it.
- `index.css` colors `::highlight(ontology-inserted)` and `::highlight(ontology-removed)`.

Elements that are not statement text are skipped through `data-change-highlight-ignore`
(the card's "(edited)", tag badges, diagnostics), and review cards through
`[data-review-suggestions]`.

**Real-diff check.** The function ran over 78 modified statements from the last 60
commits touching `.g5/ontology/features`, using roughly rendered text:

- 72 were highlighted;
- 5 fell back, all wholesale rewrites;
- 1 had identical visible text.

The check exposed scattered word marks inside one heavily reworded sentence that still
shared many words. That case is why a pair keeping fewer than half its words in order is
now marked whole.

**Larger check against open and merged PRs (2026-11-03).** Open PRs' ontology edits were
applied onto `main` (6fb826f06): 24 of 29 file diffs applied cleanly, from 11 open PRs. The
merged-PR set is `main` against `main` at 1 Nov (1c8921c63), 290 commits. Together they
give 80 modified statements. Two refinements came out of this:

- **Statement-level fallback.** Against the longer version it hid statements that kept
  their text and gained several sentences (`g5.git.provider-binding` went from 13 to 90
  words). It now uses the shorter version.
- **Inside a sentence.** An inserted parenthetical was marked as a whole sentence
  (`g5.security.membership-streams`), because kept words were compared with the longer
  sentence. It now uses the "replaced" and "scattered" tests above.

Final results:

| Set | Highlighted | Fell back | Identical visible text |
| --- | --- | --- | --- |
| Open PRs | 31 | 5 | 0 |
| Merged PRs | 43 | 0 | 1 |
| History | 73 | 4 | 1 |

Every fallback is a wholesale rewrite, for example the disk-allowance statements and
`g5.workspace.audience`.

**Tests:**

- `ontologyChangeHighlights.test.ts`: 13 unit cases.
- `useChangeHighlights.test.tsx`: 8 component cases with a stubbed highlight registry.
  They cover Compare and All changes, unchanged statement DOM, recomputation after a text
  change, pairs keeping each other's ranges, no marks for Added statements, no failure
  without support, and text extraction.
- `e2e/mock-preview/file-editor.spec.ts`, real Chromium with fixture
  `?ontology&changed&sentences`: the marked text, a background color with no text
  decoration and different colors per row, selection text unchanged, recomputation after
  an unsaved plain-text edit and Done, and All changes.
- `OntologyChangesCard.test.tsx`: the card marks a modified statement after an owner's
  edit and leaves "(edited)" out of the compared text.
- Neighbouring suites (editor components, card, DocumentPreview, comparison and its
  conformance corpus) pass. After rebasing on `main` d730707e6, the full frontend suite
  (405 files, 6,089 tests), the mock-preview `file-editor` browser suite (38 of 38) and
  main's `ontology-verification` fold checks (5 of 5) pass.

**Live check (full-stack preview).** G5-Lean was seeded at `ff05641ca` and forked as
`G5-Lean-change-highlights`, and this branch's `documents.g5.md` plus a demo edit to
`chat.g5.md` were saved into the fork. As Alex, in real Chromium:

- All changes marked the new sentence in `statement-changes` whole and the removed
  ", word-level highlighting" in its Before row.
- In the card statement it marked word edits ("full", "5"→"8") and a reworded clause.
- Per-file Compare showed the same marks.

**Chat card, live (2026-11-03).** In `G5-Lean-open-prs`, Claude (Opus 4.8) made five
requested edits to `chat.g5.md`. The turn's card listed all five as Modified:

- word edits were marked ("an explicit" / "by the owner", "and searchable" / "or a VM
  stop");
- an appended sentence was marked whole;
- in the long `g5.chat.controls` statement, a replaced final sentence was marked whole
  and a word edit in its second paragraph was marked;
- the fully rewritten `g5.chat.delivery` showed without highlights, as designed.

**Not covered:**

- Firefox and Safari were not run.

## Review — 2026-11-04

**Suggestion previews.** In per-file Compare, a pending suggestion's preview replaces the
blocks it covers in the Current row. Highlights skip the preview, so the covered text,
still in the file, was marked as removed in the Before row. A pair whose Current row
holds a preview (`data-review-preview`, set by `SpecReviewSuggestions`) now shows
without highlights, and the ontology lists this fallback. Test:
`useChangeHighlights.test.tsx` (9 cases now), which fails without the fix.

**Themes.** The browser test now checks the marks in light and dark themes, as the
acceptance list asked.

**Verification.** `tsc` and eslint (no new warnings) are clean. Editor, files, card and
suggestion-overlay suites pass (840 tests), and so does the mock-preview `file-editor`
browser suite (38 of 38).

**Comparison work bound (CodeRabbit, Major).** The word cap does not count punctuation,
but the word LCS compares punctuation tokens. A code-like statement with 1,000 words and
11,000 tokens, edited at both ends, built a table of about 121 million cells: about 2 s
and 484 MB, synchronously, before the page could paint. Each comparison now has a work
budget (see the limits above) and shows the pair without highlights when it runs out.
That case now takes about 20 ms. The worst cases measured under the budget take 45 ms (a
2,000-word sentence, still highlighted) and 135 ms (2,000 one-word sentences, all
changed). Tests: two cases in `ontologyChangeHighlights.test.ts` (15 now). The
punctuation case fails without the fix.

## Shorter Before version — 2026-11-04

Request: [UI review feedback and owner decisions](../../chat_intent/saehan-ontology-diff-improvement/request.md#human-instructions--2026-11-04).
Amends [`change-highlights`](../../ontology/features/documents.g5.md#g5.documents.ontology.change-highlights)
and the link sentence in [`statement-changes`](../../ontology/features/documents.g5.md#g5.documents.ontology.statement-changes).

**Design.** The comparison already knows which Before sentences lost text, so it also
returns them as an excerpt: each sentence split into kept and removed text, with
whitespace collapsed and a flag where sentences were left out. `useChangeHighlights`
returns how the pair shows Before:

- whole: no highlights, or every Before sentence lost text;
- none: nothing removed, with Show before on the Modified row;
- excerpt: only those sentences, with an ellipsis for each gap and Show full on the
  Before row.

Show before and Show full become Hide before and Show less once the whole row is shown.
Where the controls sit is settled in [Toggle placement](#toggle-placement--2026-11-04).
The toggle is per pair and not remembered.

**The full Before row stays mounted**, hidden while collapsed, because the comparison
reads its rendered text and its marks are ranges over its nodes. Moving or remounting it
would collapse those ranges. The pair's rounded last corners therefore skip hidden rows
(`:nth-last-child(1 of :not([hidden]))`).

**The excerpt is plain text.** Its sentences are spans of the compared visible text, so
links, emphasis and code lose their formatting there. The row is read-only and its links
are already disabled, and Show full restores the rendered version. Removed text in the
excerpt uses a class with the removed tint rather than `::highlight()`, because the
excerpt's text is rendered from the comparison, not painted over.

**Unchanged:** both rows' backgrounds and labels; the marks; the fallbacks, which still
show the whole pair; the matching labels. Without the Highlight API the pair shows whole,
as before. The metadata footer is hidden until details are opened, so it needs no rule.

**Known cost:** the toggle under a label adds about two-thirds of a line to a one-line
excerpt row.

**Tests.** Unit tests cover the excerpt's gaps, removed parts, collapsed wrapping, and
the null cases (nothing removed, every sentence touched). `useChangeHighlights.test.tsx`
covers the excerpt and Show full / Show less, additions only with Show before / Hide
before, marks surviving the toggle, the whole-row cases, and unchanged rendered bodies.
In real Chromium, `file-editor.spec.ts` checks the excerpt text, its tint against the
removed highlight, the rounded last row with Before hidden, Show full / Show less, and
additions only after an unsaved edit. The Compare journey's edit became a replacement,
so it still exercises a whole Before row. The full-stack flow
`e2e/flows/ontology-comparison.spec.ts` and the browser suite
`ontology-verification.spec.ts` make additions-only edits, so they now open Before
with Show before. The flow opens it again after reload, Undo and the Compare toggle,
accepting a pair that kept the reader's choice. It requires Hide before either way, so
a whole-row fallback would still fail it.

**Preview check, 2026-11-04** (full-stack preview built from `f32e99bfd`, the merged-PR
and open-PR demo workspaces and the live chat card): of 80 Modified statements in All
changes, 45 hid Before, 27 showed an excerpt and 8 showed Before whole. The chat card's
five edits covered all three cases. Show before / Hide before and Show full / Show less
toggled correctly and the marks survived them, in per-file Compare too. Screenshots are
in the Project Docs UI review page, `change-highlights-ui-review.html`.

## Toggle placement — 2026-11-04

Request: [owner instructions](../../chat_intent/saehan-ontology-diff-improvement/request.md#human-instructions--toggle-placement-2026-11-04).
Amends [`change-highlights`](../../ontology/features/documents.g5.md#g5.documents.ontology.change-highlights);
supersedes "under the label" in the [shorter Before version](#shorter-before-version--2026-11-04).

**Design.** The label column stretches to the row's height and the control sits at its
bottom, nudged onto the baseline of the row's last text line. In the additions-only case
one button on the Current row reads Show before or Hide before, so it neither moves nor
loses focus. Show full and Show less stay separate buttons on the excerpt and the full
Before row; focus follows the click from one to the other.

**Keeping the place.** Collapsing measures the clicked control's position, then scrolls
the nearest scrolling ancestor (or the page) by however far the visible control moved.
The browser's own scroll anchoring cannot do this when its anchor was inside the text
that collapsed, and measuring after the change also absorbs whatever anchoring did.
Expanding does not compensate: the reader reads next, not clicks again.

**Known cost:** a one-line excerpt row still gains about two-thirds of a line, because
the label and control together are taller than the text.

**Tests.** `useChangeHighlights.test.tsx`: one Show before / Hide before control on the
Current row and none on the Before row, and focus following Show full / Show less.
`file-editor.spec.ts` in real Chromium: the control on the last line's baseline, Hide
before at Show before's exact box, and a new journey that collapses a Before row
scrolled partly above a narrow pane and keeps Show full at Show less's position (fails
by 7 px with the compensation removed).

**Preview check, 2026-11-04** (full-stack preview rebuilt from `623d132f7`, the same
demo workspaces and live chat card): every captured control sat 3 px above its row's
bottom on the last line's baseline, except a one-line excerpt (the known cost) and a
1 px offset in per-file Compare. Hide before took Show before's exact box. In All
changes, collapsing two long Before rows scrolled partly out of view kept Show full at
Show less's screen position, with focus on it. The 45 / 27 / 8 split was unchanged.
Screenshots are in the Project Docs UI review page.

## No collapse scroll — 2026-11-04

Request: [owner instructions](../../chat_intent/saehan-ontology-diff-improvement/request.md#human-instructions--collapse-keeps-the-statement-2026-11-04).
Supersedes "Keeping the place" in [toggle placement](#toggle-placement--2026-11-04).

**Design.** The pair no longer measures or scrolls on collapse; the browser's own scroll
anchoring is left as it is. Focus still follows from Show less to Show full, because they
are different buttons.

**Tests.** The collapse journey in `file-editor.spec.ts` now keeps the whole pair in view
in a narrow pane and checks that Show less leaves the Modified row's position and the
pane's scroll unchanged, with Show full coming back higher. With the earlier
compensation restored, it fails: the statement moved 24 px.
