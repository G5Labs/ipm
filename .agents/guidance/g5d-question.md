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
