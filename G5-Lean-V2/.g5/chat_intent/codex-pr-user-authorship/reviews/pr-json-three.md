# JSON compatibility independent review 3

Review mode: fresh independent, read-only discovery, 2026-09-30. No production or
ontology files changed, no delegation, and no peer reports or author review notes
read before saving these findings. Inputs were the saved request, pinned design
(starting with `Ontology changes and governance`, then the JSON amendment), current
ontology bodies and footer metadata, and the frozen shared checkout represented by
`/tmp/g5-pr-json-review-snapshot.tgz`.

Scope: `backend/g5d-cli/src/{lib,pr,pr_json,dispatch,process}.rs`,
`backend/g5d/tests/pr_commands.rs`, `docs/g5d-pr.md`, both provided-agent
`workflow-recovery.md` references, and affected governance. Material dependencies
inspected include stable context/client selection, the public/internal read adapters,
the provider reader and the serialized workspace PR/provider-record types. Earlier
server retrieval work was examined only where it supplies facts or authority to the
JSON adaptation.

## Initial findings

### F1 — P2: Explicitly empty field arguments succeed with non-JSON help

`backend/g5d-cli/src/lib.rs:646` uses the same empty string for the missing optional
value and an explicitly supplied empty argument. `pr_json.rs:74–75` therefore treats
both `g5d pr view 42 --json=` and `g5d pr view 42 --json ""` as successful field
discovery. They exit 0 and write `Available fields:` text to stdout. A shell caller
whose selected-field variable is unexpectedly empty receives successful non-JSON
output instead of an actionable validation failure.

The governing JSON amendment promises that “unknown or empty field names produce
actionable errors”; field discovery is promised for **bare** `--json`. This also
differs from the locally installed `gh`: `gh pr view 42 --json=` exits 1 with
`Find error: no fields specified`. The new sentence in `docs/g5d-pr.md:33–34` documents
the implementation's different behavior but does not amend that governing promise.

Proposed repair: preserve a distinct parse state for an omitted optional field value
versus a provided empty string, retaining bare/root boolean discovery but rejecting
explicit empty field values before context/auth/network. Update the documentation
accordingly. Regression checks should exercise both `--json=` and `--json ""`,
alongside bare `--json`, valid equals syntax, and an empty entry in a nonempty list;
only the bare form should succeed with field help. This is a bounded repair under
`g5.governance.pr.json`, not a requested expansion of the provider field catalog.

No other actionable defect found in the reviewed scope.

## Commitment coverage

| Commitment | Actual path and counterexample/check | Result |
|---|---|---|
| `g5.governance.pr.json`: selected object/arrays and supported fields | `Selection::parse/project`, process stdout behavior; PR unit tests and compiled CLI mock transport tests assert exact shapes, unknown-field refusal, mergeability enum conversion and comments demand | Implemented and exercised, with F1 for explicit empty values |
| `g5.governance.pr.json`: no-context discovery and validation | Early PR dispatch and selection before discovery; compiled CLI tests use corrupt stable metadata, absent credentials and an unusable API URL | Bare help and invalid nonempty lists exercised; F1 narrows the missing negative case |
| `g5.governance.pr.json`: retain non-PR boolean flag semantics | `scope_json_flag`, parser tests and additional compiled `issue show 42` runs with the flag at root, command and nested-command positions | All tested forms preserve the positional argument and JSON error envelope |
| `g5.governance.pr.json`: facts remain unknown, no arbitrary provider | `field_value` and actual `WorkspacePullRequestSummary` / provider serialization; unit cases for zero, one and multiple provider records | Mapping matches producer keys; multi-provider singular facts become null; no synthesized author/check facts |
| `g5.governance.pr.read`: provenance and read-only retrieval | `pr::request_path/execute/select_output`; public/internal routes share `read_pull_request_info`; provider reader uses bounded GET requests; docs identify selected `source` and G5 status-array semantics | JSON adaptation preserves existing retrieval scope and optional provenance; no new state writer or credential policy introduced |
| `g5.security.read`, `.read.durable`, `.admission`, `.tenant`, `.credentials.publication` | Public `scoped_db_for_workspace(...workspaces:read)` and internal `scoped_db_for_workspace_incarnation`, existing server repository resolution, CLI worker/user client selection; compiled CLI permission-failure test | No JSON-induced bypass found. Source inspection plus mocked CLI transport evidence; server authority integration suite was not rerun in this incremental review |
| `g5.architecture.governance_owner`, `g5.design.minimal-mechanism`, `.change-locality`, `.concerns`, `.reuse`, `g5.architecture.policy` | Projection stays in CLI; API DTO/credential ownership unchanged; command-factory adapter scopes the changed flag without modifying every `GlobalFlags` consumer | Mechanism is proportional and preserves legitimate PR/non-PR variants. No unjustified extra abstraction found |
| `g5.quality.review.cases`, `.evidence.claims`, `.evidence.unknown` | Exact projected values and request query assertions, denied transport with empty stdout, parser positional checks; explicit empty runtime counterexample | Tests discriminate the guarantees they claim; limits stated below |

The changed `g5.governance.pr.json` text and its footer exist and agree with the
recorded ontology diff. The footer remains “Planned” pending the main agent's
consolidation and repair/evidence update; no independent metadata edits were made.
Existing authorship/central connection commitments remain background to this scoped
CLI presentation change.

## Executed evidence and limits

- Reused existing compiled artifacts whose timestamps coincide with the frozen
  implementation. `g5d_cli-923c3e826392c280 pr:: --nocapture`: 3/3 passed;
  `g5d_cli-923c3e826392c280 pr_json:: --nocapture`: 5/5 passed.
- `pr_commands-b256fc6a51d07b2c --nocapture`: 7/7 passed after enabling the installed
  libkrun runtime using `DYLD_LIBRARY_PATH=/opt/homebrew/lib` and allowing isolated
  localhost mock-server binds. Initial sandboxed attempts failed on port binding
  and missing runtime-library search path, not assertions; those blockers were
  resolved for the successful run.
- Independent subprocess checks from a unique temporary home reproduced F1 for
  equals and separate empty values; bare help still succeeds, trailing empty list
  entry fails with empty stdout, and three non-PR JSON argument positions preserve
  their JSON error envelopes. These used no network or real credentials.
- An initially overbroad `pr` substring test filter ran 77 CLI unit tests: 76 passed
  and the unrelated `tool::tests::cli_registry_and_agent_registry_produce_identical_grounding_results`
  failed on platform-actor versus local-Git attribution. This is recorded as an
  incidental unresolved test failure, not attributed to this JSON change; the later
  precise PR filters above passed. No shared test fixtures were edited.
- No live GitHub, deployed guest, server database authorization, or full CLI suite
  execution is claimed here. No new build was needed; review used the current
  compiled artifacts and source inspection. Nested comment/author schema differences,
  workspace status grouping, and bounded provider support are disclosed in docs and
  within the pinned design's limited compatibility scope.

Implementation is substantially complete subject to F1. The review pass is complete;
repairs and metadata assessment belong to the main agent's consolidated pass.
