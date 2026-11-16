# Iteration 44: `arrange` and XML comment carry-over

Batch 7, product `571e04a1` unchanged. [Model](../../../Verus_verification/consolidated/iter44/comments.rs), [cases and controls](../../../Verus_verification/consolidated/iter44/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [devtools.authoring](../../../ontology/features/developer-tools.g5.md#devtools.authoring) ("`arrange` puts a manifest's resources in the standard order"); nothing owns comments.

## Gap and consumers

`Utils/XMLCommentHandler.cls` (10 members), `Utils/Module.cls::ExportDocumentForObject` and `Main.cls::Arrange` had no links. The handler has three callers. `arrange` is ordinary and was exercised. The Publish-phase version update (`Lifecycle/Base.cls` ~1669) is excluded package/publish scope. `Utils/Module.cls::LockWithCurrentDependencies` has no callers at the pin.

## Predictions and observations

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| a1/a2 `arrange c44`, no source-control class | fails: "Not logged into source control system" (#5889); document unchanged | resources reordered | not predicted (gate unread); discovered in explore1 |
| n1 `arrange n44` (no comments), same namespace | fails the same way | reordered | (discovered) |
| s1 same with `%Studio.SourceControl.Base` configured | succeeds; resources sorted A, Z inside a new `<Resources>` wrapper | reordered | rewritten |
| s1 comments C1 (before Name), C2/C3 (inside dependencies) | kept in place | unspecified | kept (paths unchanged) |
| s1 comments C4/C5 (before each Resource) | dropped | unspecified | not predicted; discovered (path changed by wrapper) |
| s1 comment C6 (last child of Module) | dropped | unspecified | dropped at end tag |
| C0 (before `<Export>`) | never in the stored document, even after load | unspecified | not stored |
| s2 second arrange | identical result | | stable |
| module.xml on disk | unchanged (7 comments) | | not written |

## Evidence

- Formal: `comments.rs` 5 verified, 0 errors. Four controls were each rejected: `resource-path-unchanged`, `trailing-attached-to-parent`, `no-source-control-ok` and `dependency-comments-dropped`.
- Native: [receipt](native/20261116T175351554424Z/observations.json), 168 fields gated. Replay: 43 fields in Verus ([receipt](replay/receipt.json)). Flips of `s1.c4.count`, `a1.ok` and `s1.c1.count` were each rejected.
- Attempts: explore1 (arrange fails everywhere) and explore2 (direct GetStatus probe) are kept and not counted.
- Command: `python3 .g5/Verus_verification/consolidated/iter44/run.py` (12 s).

## Reach candidates

All 12 are newly reached against the iteration41 map: the 10 `XMLCommentHandler` members, `Utils/Module.cls::ExportDocumentForObject` and `Main.cls::Arrange`.

## Limits

Duplicate attribute-free paths (the `ReplacePaths` counter), whitespace text nodes, non-indented output and real source-control extensions were not assessed. SAX, XSLT, `XMLExportToStream` ordering and `%Studio.SourceControl` are native-qualified providers. No ObjectScript body is verified directly.

## Proposed owner text

- **`devtools.authoring`, `arrange` bullet, added sub-bullets:**
  - "As built, `arrange` works only when the namespace has a source-control class. Otherwise it fails with "Not logged into source control system" and changes nothing (defect C:D-next-1)."
  - "It rewrites the stored `<Name>.ZPM` document, not `module.xml` on disk. Resources are written in name order inside a `<Resources>` element, and fields such as `LastUpdated` and dependency `DisplayName` are added."
  - "Comments before `Name` and inside dependencies are kept. Comments before each resource and a module's last comment are dropped (defect C:D-next-2)."
- **Metadata:** "Model: [comments.rs](../../Verus_verification/consolidated/iter44/comments.rs) `arrange`, `without_source_control_nothing_changes`, `resource_comments_dropped`, `trailing_comment_dropped`, `leading_and_dependency_comments_kept`."
- **C:D-next-1, Medium · Decision: Open: "`arrange` fails in a namespace without a source-control class".**
  - Found by: iteration44 exploration, then source review of `Utils/Module.cls::ExportDocumentForObject` (`%Studio.SourceControl.Interface.GetStatus` status thrown); control `no-source-control-ok`.
  - Reproduced by: E2E reproduced, receipt `iter44/native/20261116T175351554424Z`. `arrange c44` and `arrange n44` fail with #5889 in a fresh namespace (a1, n1). Passing neighbour: after configuring `%Studio.SourceControl.Base`, both succeed (s1, s3).
  - Repair, unchecked: treat an absent source-control class as "not in source control, editable".
- **C:D-next-2, Low · Decision: Open: "`arrange` drops comments placed before resources and a module's last comment".**
  - Found by: iteration44 explore3, then source review of `Utils/XMLCommentHandler.cls` (`GetPosition`, `endElement`, `Reapply`); controls `resource-path-unchanged`, `trailing-attached-to-parent`.
  - Reproduced by: E2E reproduced (s1). C4/C5 are lost because the export wraps resources in `<Resources>`, so their recorded `/Module/Resource[@Name=...]` paths no longer match. C6 is lost because a comment with no following sibling is discarded. Passing neighbours: C1–C3 are kept.
  - Repair, unchecked: map resource comments by resource name onto the exported path, and keep trailing comments as the parent's last child.
