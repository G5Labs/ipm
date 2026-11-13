---
name: g5d-cli-tests-need-clean-g5-env
description: g5d's subprocess CLI tests (pr_commands, cli_commands, issue_commands) fail in a workspace VM unless the ambient G5D_*/G5_* env is unset; some other g5d failures are environmental or pre-existing.
metadata:
  type: reference
---

Inside a G5 workspace VM the agent's shell exports `G5D_WORKSPACE_TOKEN_FILE`, `G5D_WORKSPACE_ROOT`, `G5D_WORKSPACE_GIT_SOCKET`, `G5D_API_URL` and others. The `g5d` integration tests spawn the binary and unset only a few variables, so e.g. `pr_commands` fails with `NOT_FOUND` against its mock server. Run them with `for v in $(env | grep -E '^G5D?_' | cut -d= -f1); do unset $v; done` first.

Seen 2026-10-26 on the issue-710 branch, unrelated to the change under test and still failing after the clean env: `cli::host` block-capacity tests (Permission denied in the VM), `crate_architecture::api_core_and_routes_form_one_way_packages_with_thin_compatibility_shell` (expects `pub use g5_api_routes::routes` in g5-api's lib.rs), and the provider-CLI `dispatch_modules` cases (codex app server, formal review, resumed provider refusal).

**How to apply:** clean the env before judging g5d test failures; compare with CI before chasing the listed ones. Related: [[rust-and-integration-test-setup]], [[pkill-f-kills-own-shell]].
