# Repository relocation validation

Validated locally on 2026-09-12. This records the layout change, not a full
platform release qualification.

## Preservation and scope

- Local Git baseline: `5e30916` (`Record G5 Lean baseline before repository
  reorganization`). The relocation is staged for review against that commit.
- The 3,320 baseline files are accounted for by the moves. The nested Git and
  Cloud upload ignore rules were consolidated at the root.
- All tracked shipped-agent assets retain their original bytes. One pre-existing,
  ignored `g5-persona-view-creation` skill was absent from the shipped manifest
  and embedded bundle. Its two files are preserved locally under
  `.g5/local-assets/claude-code-g5/g5-persona-view-creation/`, outside bundle
  discovery and version control.
- Two route-auth test fixtures were restored from the reference checkout into
  `tests/fixtures/security/`. Tests of shipped skills use the retained product
  profiles in place of development-profile files deleted before this work.
- The MCP launcher test no longer tests the deleted development SessionStart
  hook. Its retained launch, staleness, concurrency and writer-fencing checks run.
- Customer repository paths in parsers, API examples and legacy-format fixtures
  keep their original semantics. A repository move does not change those formats.
- Local credentials and dependency/build caches remain ignored. The reference
  checkout's source files were not edited.

## Checks performed

Commands below run from the repository root unless noted otherwise.

| Check | Result |
| --- | --- |
| `python3 scripts/check-layout.py` / `just layout-check` | 26 Cargo members, 474 literal embedded Rust inputs, 253 Docker inputs and two agent profiles resolve |
| `SQLX_OFFLINE=true cargo check --workspace --all-targets --offline` | Passed |
| `cargo fmt --all --check` | Passed |
| `cargo test --offline -p g5d --test crate_architecture` | 44 passed |
| `cargo test --offline -p g5d-worker --lib profile` | 22 passed |
| `cargo test --offline -p g5d-foundation --lib golden::recipe::tests` | 16 passed |
| `cargo test --offline -p g5-api-routes --lib bundle_asset_list_matches_profile_sources` | Two passed |
| CLI loading of the root `.g5/workspace.yaml` build declaration | Passed |
| `npm run build` in `frontend/` | Passed; existing large-chunk warning |
| `npx vitest run` in `frontend/` | 230 test files, 4,230 tests passed |
| Frontend user-guide test after final Justfile changes | Eight passed |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | 204 passed; this is unittest discovery, not the separate pytest suite |
| Cloud Build ordering and Rust lane checks | Five and two passed, respectively |
| Self-hosted Azure canary contract and deployment-freshness wrapper tests | Passed using static checks and command stubs |
| Prepared-workspace Git topology contract | Passed with local fixture repositories |
| `bash scripts/tests/g5d-mcp-staleness-test.sh` | 99 assertions passed |
| Execution-lifecycle writer scan and its self-test | Passed |
| Docker Compose dev/test configuration | Parsed; local build contexts and Dockerfile paths exist |
| Test-runner Docker context | Covers every literal embedded input in the Rust source tree |
| GitHub/Cloud Build/site YAML and shell/Python syntax | Parsed successfully |
| `terraform fmt -check -recursive infra/terraform` | Passed |
| Root Justfile recipes and selected dry runs | Parsed; build and infrastructure paths resolve to the new layout |
| `git diff --cached --check` | Passed |

Rust test execution on this macOS host used `SQLX_OFFLINE=true` and
`DYLD_LIBRARY_PATH="$(pkg-config --variable=libdir libkrun)"` to locate the
already-installed libkrun library.

## Existing guard failures

These failures also reproduce in the original reference checkout:

1. `python3 scripts/check_codex_profile_parity.py`: the Claude and Codex
   project-doc-intent-labeling skill copies differ, and both code-review skills
   lack two rules expected by the checker (five reported findings). The assets
   were preserved rather than changing their behavior during relocation.
2. The original local CI workspace/cache mismatch in
   `bash infra/docker/test-dev-build-profile.sh` was repaired during the Just
   command follow-up. This guard now passes in Lean.

## Validation limits

The initial relocation pass did not build Docker images or execute live
workflows. The [Just command follow-up](just-commands.md) records subsequent
successful Docker/local-stack startup and Helm checks; Helm now has a Docker
fallback. Lean uses separate Docker volumes to retain the old G5 local data. Cloud
deployments and the separate pytest suite remain unexecuted. Retained legacy development utilities may still require
ontology, docs-repository or local-profile inputs deleted before this relocation.
Their restoration/removal belongs to the upcoming feature and architecture work.

Feature selection, crate consolidation and backend redesign remain pending the
user's specifications. No cloud resources or deployments were changed.
