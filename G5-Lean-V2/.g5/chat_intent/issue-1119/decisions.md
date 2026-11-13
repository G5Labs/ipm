# Issue 1119 owner decisions

## Human instructions

- 2026-11-05, issue comment: "'bullet' means each paragraph - I am not trying to cap the sub bullets. Rule 5 should apply consistently to the whole ontology. Let's stage so the change doesn't conflict with everything."
- 2026-11-06, workspace chat: Diff Graph statements that ran three-sentence paragraphs one after another "seem to break the sentence length rules".
- 2026-11-06, workspace chat: "Let's delete 'A statement may have any number of bullets' and add a bullet 'A statement may have no more than 10 bullets. Anything larger is worthy of a new concept, definition, and collection of statements'".
- 2026-11-06, owner edit in the ontology editor: the ten-bullet rule became its own statement, `g5.ontology.writing.bullet-count`.

## Interpretation

- The three-sentence cap covers all of a statement's text outside its bullets, across all its paragraphs.
- Capping each bullet at three sentences is an agent reading of rule 1 ("No ontology bullet should have more than 3 sentences"); the owner has not confirmed it.
- The ten-bullet limit counts every bullet in a statement, including nested ones.
