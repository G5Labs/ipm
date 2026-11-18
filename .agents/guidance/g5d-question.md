# `g5d question`: question sets

Read this before writing questions for a person into a Project Doc or an HTML explainer. `g5d question <command> --help` lists every flag.

Question sets may replace in-document questions. Use `g5d question create --set <set-id> --title <title> --file <questions.json>` to create/extend a set; it prints the set path and question IDs.

Reference each question from:

- A Project Doc: `[Q<N>. Title]` containing a `g5-question-ref` fence with `{"path":"docs/<workspace-id>/questions/<set-id>.g5q.json","id":"<question-id>"}`.
- An HTML explainer: `<div data-g5-question="<set path>#<question-id>"></div>`.

In a Project Doc the title line comes first, then a blank line, then the fence:

````markdown
[Q1. Which store keeps the answers?]

```g5-question-ref
{"path": "docs/<workspace-id>/questions/<set-id>.g5q.json", "id": "<question-id>"}
```
````

Embedded `g5-question` fences remain valid. Read with `g5d question list` (`--path` for inherited sets); propose answers with `g5d question answer`.

## Who should answer

Give each question `personas`: any of `be` (Business Engineer: what the product should do), `se` (System Engineer: design and technical trade-offs) and `ie` (Integration Engineer: integration, verification and rollout). It routes the question to the right person; the workspace owner still records the answer.

## A questions page

When a document exists to get many questions answered, write it as an HTML explainer (`docs/<workspace-id>/explainers/<name>.html`). G5 draws the cards, their numbers, the index and the tally from the question sets and keeps them current as answers arrive. Write no script or markup of your own for those.

```html
<nav data-g5-question-index></nav>   <!-- every question, kept at the left of the page -->
<div data-g5-question-tally></div>   <!-- still to answer, by persona and importance, with filters and sorting -->
<section data-g5-question-section="Money and exchange rates">
  <div data-g5-question="docs/<workspace-id>/questions/<set-id>.g5q.json#<question-id>">
    <div data-g5-question-explain>What it decides and why it matters.</div>
    <div data-g5-option-visual="<option-id>">A screenshot or small mock of this option.</div>
    <div data-g5-question-more>Earlier similar decisions, then sources.</div>
  </div>
</section>
```

- Cards are numbered Q1, Q2, … in page order, straight through every section. Do not number questions yourself or restart numbering in a section; refer to a question by its title.
- Put a section's question placeholders directly inside the section element, so that filtering and sorting can hide and move them.
- The three inner blocks are optional. They are your HTML and are shown inside the card: the explanation above the options, each picture in its option, the rest below the answer controls. Anything else inside a placeholder is dropped.
- The page loads nothing from the network: inline your styles and embed images as `data:` URIs.
- The card's title is the question's `prompt` and its options come from the set, so write those in plain language in the set itself.

In a Markdown document, number the `[Q<N>. Title]` lines in one sequence through the whole document, also across sections.
