# Independent JSON compatibility review one

Reviewed 2026-09-30 in a fresh reviewer context using `ontology-review`. Discovery was read-only apart from this report and isolated `/tmp/pr-json-one-*` evidence. No peer reports or implementation-session review notes were read before saving these initial results. No known-defect hints were supplied.

## Scope and initial findings

**No actionable implementation defects found in the JSON follow-up.**

Started with the pinned plan's `Ontology changes and governance`, read the original request and JSON compatibility amendment, then retrieved current governing blocks and metadata. Reviewed `backend/g5d-cli/src/{lib.rs,pr.rs,pr_json.rs,dispatch.rs,process.rs}`, `backend/g5d/tests/pr_commands.rs`, `docs/g5d-pr.md`, both agent workflow-recovery references and the affected governance block. Inspected actual workspace summary serialization and the provider read response to validate aliases and unknown values. The seven main implementation/test/document files matched `/tmp/g5-pr-json-review-snapshot.tgz` byte for byte at review completion.

The requested compatibility is intentionally bounded: selected top-level fields, plain object/array shapes and familiar mergeability values. Workspace status remains an array; nested author/comment data retains the limited read API schema; root boolean `--json` discovers fields while field selection appears after `pr`. These limits are explicit in the saved design/docs and do not constitute hidden full-`gh` compatibility claims.

## Commitment coverage

| Commitment | Actual path and counterexample/check | Result |
|---|---|---|
| `g5.governance.pr.json`: selection and output shape | `Selection::{parse,project}`, `pr::execute`; selected G5/GitHub views, empty and populated list/status arrays, duplicate selection and exact returned keys | Preserved by unit tests, existing CLI tests and direct subprocess checks. Removing projection or retaining the wrapper fails their comparisons. |
| Same: discovery and invalid fields precede I/O | Early PR dispatch and parsing before stable/context discovery; corrupt metadata and absent credentials, typo/empty/source-incompatible field entries | Preserved. Existing CLI test passes despite corrupt metadata; direct probe observed zero HTTP requests and empty stdout for invalid fields. |
| Same: compatible values and honest unknowns | `field_value`; actual `WorkspacePullRequestSummary`/provider JSON producers; mergeability true/false/null, unrecognized merge state, missing provider data and multiple providers | Preserved. All seven singular provider aliases become null for a two-provider record; G5 title/body/state remain available. Unknown mergeability remains `UNKNOWN`. |
| Same: other commands retain boolean JSON | `scope_json_flag`, `Cli::CommandFactory`, `process::run_process`; PR parent/leaf/equals placement and legacy root/parent/leaf placement | Preserved in parser tests and real subprocess error-output checks for `issue list`. Root boolean plus PR field selection also returned the requested object. |
| `g5.governance.pr.read`: provenance, existing workspace selection and bounded comments | `pr::{execute,request_path,select_output}`; selectable `source`, explicit workspace/config/stable metadata tests; independent `commentsHasMore` selection | Preserved. Comment selection adds the existing `comments=true` request; no publication, merge or compute operation is introduced. |
| `g5.security.read`, `g5.security.read.durable`, `g5.security.admission`, `g5.security.credentials.publication` | Same public/current-incarnation API clients and routes; selected fields are local presentation, comments use the existing authorized provider reader; forbidden response test | No authority expansion found in this increment. The compiled CLI suite verifies route/token wiring and failure behavior against mocks. Server authorization and live provider permissions were not independently re-executed in this JSON review. |
| `g5.architecture.governance_owner`, `g5.design.change-locality`, `g5.design.minimal-mechanism` | CLI field formatter owns projection; custom command construction scopes the one incompatible flag; server JSON and `GlobalFlags` consumers retain their contracts | Coherent, localized implementation. No new protocol, credential policy, storage or unnecessary mechanism found. |
| `g5.quality.review.cases`, `g5.quality.evidence.claims` | Exact shape/value assertions, invalid-field refusal, valid legacy forms, request demand and multiple-provider ambiguity | Checks discriminate the intended protections. Runtime evidence and unexecuted boundaries are separated below. |

## Executed evidence

- Reused the existing compiled `g5d_cli-923c3e826392c280` test binary: all five `pr_json::` parser/projection tests passed. Receipt: `/tmp/pr-json-one-unit.log`.
- Reused the existing compiled `pr_commands-b256fc6a51d07b2c` integration binary: all seven tests passed. Receipt: `/tmp/pr-json-one-blackbox-retry.log`. The initial sandbox run could not bind mock-server ports and lacked the libkrun loader path; rerunning with local-port permission and `DYLD_LIBRARY_PATH=/opt/homebrew/lib` resolved both environment blockers. No production failure was inferred from that first attempt.
- `/tmp/pr-json-one-probe.py` exercised 11 additional real `g5d` invocations with an isolated temporary home and local HTTP fixture. Passed selected G5 view aliases in three option placements; all seven multi-provider null aliases; unknown mergeability; `commentsHasMore` alone causing comment retrieval; typo, empty entry and unsupported source-specific field rejection without HTTP; and all three legacy boolean JSON error placements. The script contains exact assertions and can be rerun with the same loader path/local-port permission.
- `git diff --check` passed.

## Limits and proposed reconciliation

No repair is proposed. This review reused compiled artifacts rather than rebuilding, and did not run a complete workspace suite. Provider data and API authorization responses were mocked for CLI execution; live GitHub, deployed guest operation and the earlier server admission matrix remain outside this incremental run. The JSON follow-up does not change those server policies.

The `g5.governance.pr.json` footer still says implementation and verification are pending. After consolidation, replace that planned assessment with the scoped CLI implementation/test evidence and retain the live-provider/deployed-guest limits. No governing meaning or design amendment is needed based on this review.
