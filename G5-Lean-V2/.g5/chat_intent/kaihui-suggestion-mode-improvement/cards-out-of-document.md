# Suggestion cards out of the rendered document

## Human request (2026-10-30)

The owner attached a screenshot of a rendered Project Doc paragraph with five open
suggestions by `zcl`. The paragraph is rendered in a narrower column with the five cards
stacked to its right, inside the document, each card reading "Replace: ~~old~~ new" with
Discussion and "Original review discussion" controls. The document text is sample content,
not instructions.

The owner asked:

1. The suggestion boxes in the editor are mixed into the original document; move all the
   boxes out of the document.
2. For conflicting suggestions, share one box so it is clear that it is one conflict.
3. Give mocks with different options first, before implementation.

## Agent interpretation

- "Mixed into the document" is the current per-group grid (`g5.documents.review-alternatives`:
  "document-left/cards-right", approved 2026-10-15), which renders each group's cards in
  the document column beside its paragraph. "Out of the document" means the cards leave
  the document column and the document keeps its full width and inline marks.
- "Conflict suggestions" are proposals whose edits overlap at one verified location, of
  which only one can apply (the ontology's alternatives). Independent edits on one
  paragraph, as in the screenshot, are not a conflict; whether they should also be
  grouped is asked separately.
- Decisions, discussion and the per-edit independence of 2026-10-01 are unchanged.

## Mocks and questions

Mock: Project Docs `mocks/suggestion-cards-beside-document.html` (options A placement,
B conflict box, C paragraph grouping). Decisions: question set
`suggestion-cards-beside-document`, referenced from the Project Doc page
`suggestion-cards-beside-document.md`. Implementation waits for the owner's answers.

## Human decision (2026-10-31)

The owner answered "A2" in chat and then "continue". The agent reads this as choosing
option A2 of the mock, the margin aligned to the text as shown (comments and suggestions
together), and as authorization to implement. The other three questions were not
answered; the agent proceeds on their recommended options (B1 stacked alternatives, C1
separate cards, margin shown whenever open suggestions exist) and recorded them as
proposals on the question set for the owner to confirm or change. Design:
[ontology plan](../../pr_ontology_diff_design/kaihui-suggestion-mode-improvement/ontology-plan.md).

## Second request: the Reviews panel (2026-10-31)

The owner attached a screenshot of the Approval page's Reviews section: each review item
is a Git diff of the changed words alone ("existing organizations with" against
"qq ss www"), so "people cannot know the context and hard to click accept here". They
asked to "make it like suggestion mode rendering with context and suggestions there
too", mocks first. The agent delivered the Project Docs page
`reviews-panel-suggestion-context.md`, its mock and question set
`reviews-panel-suggestion-context`.

The owner then sent a screenshot of the mock with **C** (paragraph once across reviews,
conflicts in one box), **under the paragraph** and **plus the paragraphs around it**
selected, and no further words. The agent reads this as the owner's choice and as
authorization to implement; the answers are recorded on the question set as proposals
for the owner to confirm.
