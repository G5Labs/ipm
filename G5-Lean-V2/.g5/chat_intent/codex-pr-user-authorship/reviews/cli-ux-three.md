# Workspace CLI usability — independent review three

Independent read-only discovery of the uncommitted change against `4dc163da`.
Read the request, pinned plan's Workspace CLI usability amendment and implementation
section, actual ontology diff, review skill, authoring/design guides, governing body
and metadata, every changed production/harness/test file, and affected terminal,
provider launch, CLI discovery/client and internal PR route consumers. No author
implementation explanation, peer findings, or prior review report was used. No
production, test, or ontology files were changed and no further agents were used.

## Initial finding

### F1 — Supported sandboxed Codex turns still cannot reach the direct PR API (P2)

Affected changed claim: `assets/agents/codex-g5/AGENTS.md:18–23` and the new fake-provider
check at `backend/g5d/tests/dispatch_modules.rs:1109–1112`. Actual launch owner:
`backend/g5d-agent-turn/src/runtime.rs:943–953`, bundled
`assets/agents/codex-g5/config.toml:24–25`, and
`backend/g5-api-core/src/services/agent_dispatch.rs:1787–1792`.

The new instructions promise direct authenticated `g5d pr` access, but the supported
`never/workspace-write` and `never/read-only` modes launch native Codex with their
network-disabled sandbox and `approval_policy="never"`. No profile setting or launch
override grants network access to the G5 API. PATH and environment delivery do not
enable the API connection, so `g5d pr view/create/update` still fails in these modes.
The added provider fixture is a shell script and invokes only help/field discovery,
which `pr.rs:20–26` returns before client or network use. It therefore passes even
when actual model-invoked shells cannot reach the API.

Evidence: the repository pins Codex `0.153.3`; that release's
[sandbox policy source](https://raw.githubusercontent.com/openai/codex/rust-v0.153.3/codex-rs/protocol/src/protocol.rs)
defines read-only and workspace-write with network access disabled (lines 1121–1136),
while full access permits it (1150–1156). The unchanged G5 consumer is material to
the newly required agent-shell guarantee. A credential-free local probe of installed
Codex `0.155.0-alpha.16.3` using `sandbox -P :workspace` rejected a loopback socket
operation with `PermissionError: Operation not permitted`. This corroborates the
restriction, but is not execution of the pinned Linux guest build.

Preserve the selected filesystem/approval policy while providing whatever narrow
G5 API connectivity the governing runtime policy permits; do not silently switch
agents to full access. Add a check through a real native Codex shell under each
supported policy that performs a mocked authenticated PR read and write (as
appropriate), plus refusal/error coverage. If restricted modes are intentionally
excluded, make that explicit in the governing design and user guidance rather than
claiming unconditional direct access. This is a remaining usability gap, not a new
authorization bypass.

## Commitment and boundary coverage

| Commitment | Actual path and counterexample/check | Assessment |
|---|---|---|
| `g5.runtime.workspace-cli` | Shared `install_g5d_launcher`; dispatcher state/bin PATH; Linux `/usr/local/bin` symlink; provider home shim. Quoting and G5-only loader tests; ordinary `git` remains image PATH-selected. | Shared implementation is small and coherent. Launcher/library checks passed. Actual Linux standard-path install/login-shell execution remains unexecuted. |
| `g5.runtime.workspace-cli.context` | `workspace_cli_environment` is used by owner terminal setup and `agent.turn`; CLI resolves ambient workspace and internal bearer client. | Real PTY with mock API passed implicit workspace/auth, read/create/update-error cases. Native Codex network boundary remains F1. |
| `g5.security.credentials.platform-token` and `g5.runtime.lease` | Existing long-lived inherited workspace token, unchanged current-row API admission; no new credential store or proxy. | Intentional MVP boundary preserved by inspection. Token expiry/rotation policy is unchanged, not newly claimed as tested. |
| `g5.security.owner`, `.admission`, and `g5.terminal` | Existing PTY admission/UID path plus internal PR routes; write routes enter shared `create_or_update` with typed incarnation caller, read uses scoped incarnation DB authorization. | No new bypass found. Ownership/epoch/lease route implementation is unchanged; no fresh database authority suite run. |
| `g5.security.credentials.publication` | Explicit terminal environment is assembled from Git runtime and three workspace context bindings; shell clears inherited environment; host control and provider publication credentials are not copied. | No unintended credential propagation found. PTY check verifies dispatcher/host secret names absent. |
| `.reserved-env` | PTY `secrets` admission calls `is_exportable_task_env_key`; `G5_`/`G5D_` prefixes and HOME/PATH are reserved. | Runtime attempt to override workspace token is rejected; shared predicate covers API URL/workspace ID as inspected. Existing ontology footer understates these inspected keys and can be updated by the implementing agent if its scope assessment changes. |
| `g5.governance.pr.read`, `.write`, `.json` | Existing CLI early field parsing, internal route selection and API transport; create/update uses one operation; no request replay added. | Read/create projection and one failed update verified through PTY mock API. No duplicate policy, transport route, or publication algorithm added. No live provider publication executed. |
| `g5.design.minimal-mechanism`, `.reuse`, `.adapters` | Launcher and three-value environment helper are shared; existing CLI/API owns PR behavior. | No unjustified new service/protocol or credential authority found. `tempfile` serves atomic concurrent launcher publication. |
| Agent instruction delivery | Codex profile installation refreshes AGENTS.md/skills; Claude receives essential rule through append-system-prompt on fresh and resumed launch branches; full profile and `/pr` guidance agree. | No remaining `gh pr create` recommendation on changed workspace paths. Native Claude skill consumption/model choice is unexecuted. F1 concerns actual Codex tool execution, which the fake provider does not exercise. |
| `g5.quality.review.cases` / evidence obligations | Source inspection plus independently rerun focused binaries; fixtures distinguished from native providers/production VM. | Central native sandbox and Linux login-path boundaries remain limitations; do not equate fake providers with actual native provider execution. |

## Executed checks and limits

- Reused existing built `target/debug/deps/g5d_runtime_core-2476878b07595a91`:
  `runtime_environment::tests`, 3 passed. Includes quoting, argument preservation,
  loader isolation and path deduplication.
- Reused `target/debug/deps/dispatch-71311356f0d96e73`:
  `terminal_runs_g5d_pr_directly_with_workspace_context_and_preserves_errors`, 1 passed.
  Initial sandbox run could not bind the mock API; rerun with approved loopback/PTY
  permissions passed. Test owns its temporary dispatcher root and mock server.
- Credential-free newer local Codex sandbox probe in a private temporary directory:
  workspace profile denies loopback socket creation/bind. An initial legacy-mode-only
  invocation was rejected because this CLI requires a permission-profile argument;
  the corrected explicit workspace-profile probe established the denial.
- Inspected the pinned Codex shell environment source too: its default inherits all
  variables and keeps KEY/SECRET/TOKEN names. Do **not** report token-name filtering
  as a default defect for this pin.
- No build, database mutation, live GitHub request, native model invocation or Linux
  guest boot. Existing binaries contain the added tests and have same-day build times;
  their exact source fingerprint was not independently rebuilt. No claim that tests
  establish actual model choice, live provider publication, every user shell startup
  profile or deployed runtime readiness.
