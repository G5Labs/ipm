# Independent JSON compatibility review two

Initial findings saved before reading the author's review notes or peer reports. This is an independent, full-scope, read-only discovery pass for the JSON compatibility follow-up. No production or ontology changes were made and no further agents were delegated.

## Result

No actionable code or design findings in the reviewed follow-up. The implementation satisfies the selected-field/object-or-array contract on the inspected and exercised paths. This is bounded field-selection compatibility; the documented workspace `status` array, G5 extensions, partial nested author/comment objects, and unsupported jq/template formatting are deliberate scope limits.

The `g5.governance.pr.json` metadata still describes the work as planned. After consolidation, the implementing agent should replace that temporary assessment with the final implementation/evidence assessment and preserve the live-provider limits. This is review completion work, not evidence of a runtime defect.

## Inputs and scope

Started with `Ontology changes and governance` in `../ontology-plan.md`, then read the original request, remaining design, JSON compatibility amendment, actual ontology diff and current governing text/footer entries. The recorded JSON block matches the current owning ontology change; the design's larger diff also contains the preceding authorship/connection/retrieval work and was treated as background.

Reviewed `backend/g5d-cli/src/{lib.rs,pr.rs,pr_json.rs,dispatch.rs,process.rs}`, `backend/g5d/tests/pr_commands.rs`, `docs/g5d-pr.md`, both `assets/agents/*` workflow-recovery references and affected governance. Extended inspection to `stable_context.rs`, the server PR reader, API envelope construction, durable PR summary serialization, logging, parser callers, and the Git/governance module contract. The five CLI sources, integration test, documentation and governance file were byte-compared with `/tmp/g5-pr-json-review-snapshot.tgz` and matched.

No known-gap hints were supplied. No peer report or author review explanation was read before this report was saved.

## Commitment coverage

| Commitment | Actual path | Counterexample/check | Result |
|---|---|---|---|
| `g5.governance.pr.json`: field selection and shapes | `Selection::parse/project`, `pr::execute/select_output` | Unknown/empty comma entries, deduplication, selected GitHub object, workspace list/status arrays, no wrappers | Preserved by source inspection, five JSON/parser unit tests and compiled CLI transport tests. Explicit empty option values behave as documented field discovery. |
| `g5.governance.pr.json`: field discovery before I/O | Early PR dispatch and `Selection::parse` before `stable_context::discover` | Missing credentials/workspace and deliberately corrupt metadata; invalid fields must not reach discovery or API | Compiled no-context test passes; direct CLI discovery at root, PR and leaf positions also succeeds. |
| `g5.governance.pr.json`: compatible values and honest unknowns | `field_value`, `WorkspacePullRequestSummary`, `WorkspacePullRequestProviderRepoSummary`, `workspace_pull_request_read.rs` | GitHub `true/false/null` mergeability, lowercase REST merge state, zero/one/multiple provider records | Names map to actual serialized fields; unknown mergeability stays `UNKNOWN`; unsupported singular multi-provider aliases stay null. Unit tests discriminate these cases. |
| `g5.governance.pr.json`: comment demand | `needs_comments` → `request_path` → existing bounded provider reader | Selecting `comments` without `--comments` must request comments; retain has-more indication | Compiled CLI test asserts `comments=true` in the real HTTP request and returned comments/has-more projection. Server remains bounded to the existing first page of 100 comments. |
| `g5.governance.pr.json`: non-PR compatibility | `Cli` command factory, `scope_json_flag`, `GlobalFlags`, process error formatting | Root/intermediate/leaf boolean placement and a positional argument after `--json` | Parser tests pass. Direct compiled `issue show 42` checks retain JSON error output at all three flag positions, including root plus leaf usage. |
| `g5.governance.pr.read`, `g5.security.read`, `g5.security.read.durable`, `g5.security.admission`, `g5.security.credentials.publication` | Existing public/internal read clients and authorized PR read routes | Explicit checkout versus ambient VM identity; denied provider lookup; selected output must not imply new permission or mutation | Source selection is unchanged. Seven compiled transport tests pass, including workspace discovery, explicit checkout refusal and clean stdout on permission failure. JSON follow-up does not add credential, persistence or compute operations. Actual server authority was inspected as a dependency but its prior API suite was not rerun here. |
| `g5.architecture.governance_owner`, `g5.design.minimal-mechanism`, `g5.design.change-locality` | CLI-only projection module; narrow parser scoping; existing API and credential owner | Field adaptation should not change API JSON, all `GlobalFlags` consumers, provider policy or schema | Preserved on inspection. The custom command factory isolates the PR option while retaining derived argument extraction and existing non-PR consumers. |
| `g5.quality.review.cases`, `g5.quality.evidence.claims` | Tests and this report | Source inspection, mocks and live provider behavior must remain distinct | Evidence separated below; no claim of deployed or live GitHub verification. |

## Executed evidence

Reused the current compiled artifacts supplied by the main agent; no new compilation or shared test-state mutation was needed. Each transport test uses its own temporary home/workspace and mock server.

- `DYLD_LIBRARY_PATH=/opt/homebrew/lib target/debug/deps/g5d_cli-923c3e826392c280 pr_json:: --nocapture`: **5 passed**.
- Same unit binary with `pr::tests:: --nocapture`: **3 passed**.
- `DYLD_LIBRARY_PATH=/opt/homebrew/lib target/debug/deps/pr_commands-b256fc6a51d07b2c --test-threads=1`: **7 passed** after enabling localhost bind permission. The initial sandboxed attempt passed the no-network test and stopped the other six at mock-server port binding; those were environment failures, not implementation failures.
- Direct compiled `target/debug/g5d` invocations from a unique temporary directory, with task credential/workspace variables removed, verified bare field discovery, equals syntax parsing, actionable unknown-field refusal, repeated-option behavior, and non-PR boolean JSON errors. No external provider was contacted.

The assertions are discriminating for the new behavior: wrapper retention, absent projection, accidental context discovery, missing comment demand and changing the non-PR flag into a string option would fail the relevant checks.

## Limits and proposed follow-through

Live GitHub data, deployed guest execution and nested field parity beyond the explicitly supported reader were not exercised. The seven transport tests mock the API boundary; they establish the compiled CLI behavior, not server authorization enforcement or provider token selection. Earlier read/authorship API evidence remains outside this incremental rerun. The broader CLI suite and every command tree leaf were not rerun independently; the changed command tree passes Clap's consistency assertion, focused placement checks and inspected shared extraction paths.

No repair is proposed. Consolidate this report with the other independent reviews, resolve any contrary evidence, and update the JSON block's footer with final scoped evidence.
