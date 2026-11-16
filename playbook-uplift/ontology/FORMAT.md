# Ontology file format (IPM playbook uplift)

Every module file is `ontology/NN-slug.md`. `scripts/ontology_index.py` parses them and writes `ontology/INDEX.md`.
Run `python3 -I scripts/ontology_index.py --check` from the experiment root; it exits non-zero on any gate violation.

## Layout of one module file

```
# NN — Title

Module root: XXX-0

## Glossary
| Term | Defined by |
|---|---|
| developer mode | LOAD-D3 |

## Statements
| ID | Parent | Kind | Statement | Evidence | Source | Notes |
|---|---|---|---|---|---|---|
| XXX-0 | SYS-0 | R | One plain sentence naming the module's purpose. | I | path#Lnn | |
| XXX-D1 | XXX-0 | D | A **thing** is ... | I | ... | |
| XXX-B1 | XXX-0 | B | ... | I; X:Tests.cls#L90 | ... | conditions |

## States
| ID | State |
|---|---|
| S1 | The module is not installed. |

## Commands
| ID | Command |
|---|---|
| K1 | `load <dir>` |

## Cells
| Cell | State | Command | Outcome | Decision |
|---|---|---|---|---|
| XXX-C1 | S1 | K1 | XXX-B4 | |
```

Rules the script enforces (each is a gate failure):

1. **IDs** are unique across all files. Prefix = module prefix (e.g. `LOAD`), then `-0` (root), `-D<n>` (definition), `-B<n>` (behaviour), `-C<n>` (cell).
2. **Parent** resolves to an existing statement. Only module roots (`Kind R`) may have parent `SYS-0`. `SYS-0` is the system root in `00-system.md`.
3. **Kind** is one of `R` root, `D` definition, `B` behaviour (business or technical requirement), `N` named exception.
4. **Reading grade.** Flesch–Kincaid grade of the Statement text (only that column, after removing backticks and `**`) is at most 8. Keep sentences short and words plain. Put code paths, SQLSTATE-like codes, globals and version notes in Source/Notes.
5. **Definitions first.** Every glossary term (this module's glossary plus `00-system.md`'s) that appears in a statement must be defined by the statement itself (a D statement that names it in `**bold**`), by an ancestor, or by an earlier statement (file order) that is a sibling of the statement or of one of its ancestors (lexical scope; see README "Interpretations").
6. **Evidence** is a `;`-separated list of tags:
   - `P:<name>` proved: names a Verus proof function recorded as verified in `evidence/proofs.json`.
   - `M:<name>` model-checked: names a check recorded as passing in `evidence/model-checks.json`.
   - `O:<name>` observed on IRIS: names an observation recorded in `evidence/iris-observations.json`.
   - `T:<Class>[#Method]` an IPM test **run here** on IRIS, recorded in `evidence/ipm-tests.json`.
   - `X:<path>#Lnn` an existing IPM test that asserts it, **not run here**.
   - `I` established by inspection of source/docs (put the anchor in Source).
   - `N` not implemented at this revision (the statement is intent, the code does not do it).
   - `F:<finding-id>` the code contradicts the statement; finding in `findings/NN-*.md` as a heading `### F-<PREFIX>-<n>`.
   - `OPEN:<DEC-id>` an open human decision: a row with columns `ID | Module | Question | Options | Owner | Status` (Status `open`) in a table under a `## ` heading in `decisions/NN-*.md`.
   Conditions on any evidence go in Notes.
7. **Cells.** Every (State × Command) pair of the module appears exactly once. Outcome lists statement IDs (comma-separated) or is empty; if empty, Decision must name an open `DEC-` id.
8. **No pipes** (`|`) inside cells. Use "or".

Evidence honesty rules for authors:
- Never write `P`, `M`, `O` or `T` unless you ran the check and its record exists. Builders doing read-only uplift use only `I`, `X`, `N`, `F`, `OPEN`.
- `X` means "a test claims it", not "it holds". A test with an empty body or a TODO is not `X`; mention it in Notes.
- A statement that the code contradicts keeps the intended behaviour as its text, gets `F:<id>`, and the finding is recorded in `findings/NN-*.md` (heading `### F-<PREFIX>-<n>`: what the code does, what intent says, evidence, impact).
