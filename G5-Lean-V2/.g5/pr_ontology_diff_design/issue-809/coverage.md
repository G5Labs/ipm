# Provider contract coverage audit

Baseline `48ad8f66bb7bda8ce2e786adb1988edc5e18c984`, inspected 2026-10-24.
C = Claude stream-json; E = Codex exec JSONL; A = Codex App Server.
Names below identify assertions in source; inspection is not an executed pass.
There was no production App Server adapter at baseline, so **all A cases are missing**.

| Contract | Claude coverage / transport | Codex coverage / transport | Layer and existing assertions / gap |
| --- | --- | --- | --- |
| Launch/policy/attachments | C runtime launch tests | E runtime launch tests | `g5d-agent/src/runtime.rs`: checked images, allowed tools, model/effort, reconstructed input. A handshake/identity/config missing. |
| Fresh/resume/refusal | C deferred restore + dispatcher restore | E dispatcher `resumed_provider_refusal_after_input_reports_failure_without_publication` | `g5d/tests/dispatch_modules.rs`: session generations and refusal; A refusal before acceptance missing. |
| Setup and blocked writes | C `agent_turn_setup_deadline_covers_blocked_input` | E EOF launch test | Real dispatcher; A initialize/thread/turn phase timeouts and lost acknowledgement missing. |
| Acceptance/history | C write/flush unit + API input projection | E flush+EOF | Runtime and `g5-api/tests/api_task_lifecycle.rs`; A must use native acceptance rather than flush. |
| Completion/output | C distinct blocks and result-only answers | E mapper recorded fixture | Adapter and real dispatcher; A ordinary, tool-only, multi-tool and deltas missing. |
| Completion versus exit | C malformed/early exit and window edges | E EOF and early exit | Real dispatcher; A all terminal status × process status, delayed exit, trailing pipes and duplicates missing. |
| Stop/Cancel/deadlines | C blocked replies, cancellation, background End | E device-login cancellation | Real dispatcher; A setup/active/shutdown pressure schedules missing. |
| Output limits/redaction | C large lines, oversized frames, NUL, split secrets | E >1 MiB tool line | Real supervisor/dispatcher; A frames and preserved partial output missing. |
| Usage | C per-turn tokens/session-cost accumulation | E per-turn usage mapper | Adapter/runtime; A repeated cumulative usage and previous-session exclusion missing. |
| Queue/FIFO/authoritative settlement | Shared API `api_chat_turn_stop`, `api_task_lifecycle` | Shared tests synthesize journal facts; E shared-VM chats | Database + product; A successor must wait through exit, custody, harvest and finalization. |
| Replay/reconnect | Shared journal replay ordering | Shared journal replay ordering | Real dispatcher replay tests and API projection idempotency; A schedule needed. |
| Native persistence/recovery | C harvest/restore/deferred tests | E session-file unit and dispatcher tests | `runner/session_files.rs`, real dispatcher; A native format/replacement-home compatibility unverified. |
| Auth/routes/account binding | C provider_transport tests | E provider_transport + device auth tests | Unit, dispatcher, API account contracts; A all supported auth routes need native smoke. |
| Process ownership/isolation | C Linux reap/release/sibling tests | E shared mechanism | `agent_execution_tree_is_reaped_before_settlement`, normal release and identity tests; A residency/EOF schedule missing. |
| Module/historical rollout | Shared catalog and API snapshot tests | E ordinary 1.0.5 / specialized 1.0.6, 1.0.7 | New A module negotiation and reserved-attempt reconstruction assertions required. |
| Background/report/End | C window edges, recorded background streams and End schedules | Provider exclusion: Codex has no G5 background End | Retain C tests; A unsupported interactions must fail explicitly. |
| Native requests/deferred answers | C round trips, response identity/deadline, restoration | Provider exclusion: no actionable Codex questions/approvals | Retain C; A unsupported requests must reject without authorization or hanging. |
| EOF/no consumption ack/buffered input | C steering and background EOF/report tests | E initial prompt uses EOF | Retain C buffered native turns; A closes only at matching native completion. |
| Browser/VM | `chat.spec.ts`, `chat-restart-continuity`, shared-VM and steering | E shared-VM, `codex_control_frames`, settings | No executed A product evidence. |
| Specialized callers | C formal review and summary titles | E restricted formal review; summary explicitly unsupported | Preserve dedicated contracts and run their existing regression suites after pin change. |

## Current implementation and evidence (2026-10-25)

The catalog-parity follow-up adds a required exact-pin native matrix and shared
UI/API/backend projection checks. Local probes passed **412 Claude** combinations
and **352 Codex** combinations (all offered route ids × native modes × per-model
efforts). The probes exposed and corrected Claude dotted-id effort loss, Haiku's
absent effort control and Codex's retired on-failure preset. Codex Ultra is checked
as a native selection with its model-specific ordinary-wire encoding. The API's
obsolete independent model inventory is replaced by the catalog projection, with
nullable capacity/pricing metadata. The HTTP, combined production-launch gate,
formal-route rerun and new-head CI are pending at this checkpoint; later PR validation
records supersede this checkpoint.

Environment: Linux 6.12.112 x86_64, Rust 1.97.1, PostgreSQL 16.15; exact
Codex **0.164.0** and Claude **2.1.285**. The repo pin controls installed guest
binaries. An isolated exact Codex package was installed for these probes; the
outer workspace agent's running CLI was not replaced. Schema/feature fixtures
were generated by that package without experimental API opt-in.

The full dispatcher and API results below were executed at
`43de35e398a6929e80a2a75d8e8d87146de406c6`. Final adapter/unit and native reruns
include the additional rejection of contradictory completed-with-error frames.
The PR description records the final published/tested head and CI result.
Local `/tmp/g5-809-*.log` files are ephemeral; this record preserves their results.

| Contract | Current App Server assertions | Executed evidence / remaining boundary |
| --- | --- | --- |
| Launch/config/acceptance | Exact pin, experimentalApi=false, all 78 non-stable features disabled including default-on instant_interrupt; hostile inherited config, forbidden option overrides, model/effort/approval/sandbox, images; only matching turn/start response accepts input | Adapter/runtime tests; real dispatcher configuration and image tests; exact native schema/features/effort probes |
| Refusal/uncertainty | Initialize/thread/turn validation refusal, internal errors, malformed/lost ack, malformed request identity; no delivered history or reconstruction notice before acceptance | Unit and real dispatcher peers; native resume/turn validation refusal probe |
| Native output/completion | Text, tool-only/multi-tool, large Unicode/tool output, duplicate/final deltas, secret redaction, exact native identity, unsupported requests rejected | Adapter/runtime and 25-case production-dispatcher matrix; native production tool turn |
| Native end versus exit | Completed/failed/interrupted, nonzero exit, no terminal, delayed exit, trailing output, wrong/duplicate terminal, ignored EOF, held descendant pipe | Real dispatcher asserts exactly one outcome, delayed settlement, 15s grace and 5s drain; contradictory completed-with-error rejected in unit test |
| Limits/cancellation/isolation | Silent setup at initialize/thread/turn, blocked turn write, active-work timeout lifted only on acceptance, recording exhaustion and partial output, cancellation during setup/work/shutdown, concurrent sibling unaffected | Six App Server dispatcher test functions plus shared Linux custody and API Stop/Cancel tests |
| Usage | Previous-session baseline, duplicate/stale cumulative snapshots, repeated equal-sized model calls, tool statuses | Unit cases and actual pinned native two-model-call tool journey; exact 22 input / 6 output tokens |
| Queue/replay/settlement | Frozen transport is reconstructed unchanged; existing durable settlement, queue, held-input, replay and idempotency owners reused | 76 API/database lifecycle cases; shared real dispatcher replay/custody cases. Product App Server queue/restart assertions await CI |
| Native history/harvest | Actual rollout format, fresh/same-home/replacement-home resume; controlled fake reuses durable generation/receipt scenarios | Native synthetic and authenticated ChatGPT probe passes; real VM stop/save/refusal/reconstruction journeys await CI |
| Credentials/routes | Four Claude auth modes and both Codex API routes, selected endpoint/account key, inherited/cache precedence, redaction and immutable native credential cache | Actual native CLIs through production supervisor against synthetic endpoints; authenticated ChatGPT raw native resume passes. Authenticated upstream API-key/OpenRouter and Claude product journeys outstanding |
| Rollout/process ownership | Ordinary module 1.0.8 only on compatible new guests, legacy 1.0.5 reconstruction; specialized versions retained; shared reap/release/capacity and normal agent-service exceptions | API database freeze-and-redispatch case, pure historical-snapshot case, catalog tests and full Linux dispatcher suite |
| Claude-specific behavior | Background/report/End, pending native answers, buffered input after EOF, no consumption-ack hold, report settling, Stop, tool/permission confinement | Retained unit, dispatcher, API suites and native restricted formal-review test; authenticated product reference journeys outstanding |
| VM/API/browser | Existing Codex queue/restart and tool-only persistence/recovery journeys now assert frozen module 1.0.8 + app_server_stdio_v1 for original and continuation attempts | Test source is coverage, not execution evidence; supported Cloud Build VM/browser run pending |

Executed commands and outcomes:

- `env -u G5D_WORKSPACE_ROOT cargo test -p g5d-agent -p fake-provider-cli -p g5-contracts --lib --profile g5-dev`:
  **167 agent, 40 fake-provider, 387 contracts passed** in the final rerun.
  Unsetting the platform checkout variable keeps fixture filesystem writes in their
  temporary workspace. An initial fake-workflow failure was traced to that inherited
  variable and resolved without changing production code.
- `env -u G5D_WORKSPACE_ROOT cargo test -p g5d --test dispatch_modules --profile g5-dev --no-default-features -- --test-threads=2`:
  **95 passed, 2 explicitly native-only tests ignored**. Includes all six App Server
  functions, shared image materialization, Linux reap/release/sibling isolation,
  harvest, replay and Claude background/EOF/native-request regressions. A stale
  catalog-list expectation failed the first full run; updated for the retained four
  module declarations and the complete rerun passed.
- `cargo test -p g5d-dispatch --lib --profile g5-dev`: **146 passed**.
- `cargo test -p fake-provider-cli --profile g5-dev --test cli`: **47 passed**,
  including actual App Server frames, strict resume, EOF/exit and fake rollout files.
- `TEST_DATABASE_URL=<isolated-loopback-db> target/g5-dev/deps/integration-48ad6f4208dcc0ea api_task_lifecycle:: --test-threads=2`:
  **76 passed**, including new catalog/transport reservation and redispatch coverage.
- `cargo test -p g5-api-core --lib --profile g5-dev historical_reservations_never_infer_a_new_transport`:
  **passed**, including absent and inconsistent historical snapshot fields.
- `G5_NATIVE_PROVIDER_TEST_BIN_DIR=<exact-pins> cargo test -p g5d --test dispatch_modules --profile g5-dev --no-default-features formal_review::formal_review_real_native_clients_enforce_tools_and_report_transport -- --ignored --exact --test-threads=1`:
  **passed** for both pinned providers, using synthetic endpoints and real restricted
  native tools/report transport.
- `env -u G5D_WORKSPACE_ROOT G5_NATIVE_CLAUDE_TEST_PATH=<claude-2.1.285> TMPDIR=/tmp cargo test -p g5d --test dispatch_modules --profile g5-dev --no-default-features shared_memory_native_dispatch_capture_and_fresh_session_recall -- --ignored --exact --test-threads=1`:
  **passed** native memory save and fresh-session recall through the production dispatcher.
  The older opt-in fixture omitted the now-required auth mode/selected endpoint and
  staged native binary; corrected that setup and added bounded failure logs. Together
  these commands explicitly execute both default-ignored native tests.
- `scripts/check-codex-cli-flags.sh <codex-0.164.0> 0.164.0`: **passed**, including
  App Server stdio and retained exec/formal-review confinement configuration.
- `python3 scripts/test-codex-app-server.py --codex <codex-0.164.0>`: **passed**
  stable schema equality, hostile inherited features, native validation refusal,
  fresh completion/EOF/exit, same-home and rollout-only replacement-home resume.
- The same native probe with `--subscription-auth-home <existing-private-login-home>`:
  **passed authenticated ChatGPT** fresh/same-home/replacement-home turns with retained
  history. Only private temporary auth copies were used. Subscription model/list has
  seven account-filtered models; custom/API catalogs retain eight, including gpt-5.5.
  No user model/effort selections were removed.
- `python3 scripts/test-native-provider-transport.py --g5d target/g5-dev/g5d --codex <codex-0.164.0>`:
  all **six synthetic-endpoint auth routes passed** through the production supervisor
  again on the final source, including frozen effort overriding inherited low. Native
  two-model-call tool journeys passed for Codex OpenAI and OpenRouter App Server and
  retained OpenAI exec; each reports exactly 22 input / 6 output tokens. The OpenRouter
  filter also exercises its corresponding native Claude route. These remain synthetic
  upstreams, not authenticated external-provider passes.
- Final `cargo clippy -p g5d-agent -p fake-provider-cli --all-targets --profile g5-dev -- -D warnings`: **passed**.
- Browser journey formatting and `playwright test ...chat-restart-continuity.spec.ts --list`:
  **passed discovery** (64 configured-browser cases including dependency setup); no
  browser execution is claimed locally.

## Codex launch inventory

| Caller | Preserved/migrated contract |
| --- | --- |
| Ordinary runtime | New compatible reservations use App Server module 1.0.8; historical/older-guest reservations retain exec 1.0.5. Both share the sole runtime supervisor and pin-wide feature opt-outs. |
| Restricted formal review | Ephemeral exec modules 1.0.6/1.0.7 retain their existing tool/report restrictions; binary upgrade receives the same feature opt-outs. |
| Device login/status | Native login/status commands and account binding remain before prompt delivery; device-auth cancellation regressions retained. |
| Summary titles | Codex remains explicitly unsupported; existing Claude implementation retained. |
| Developer scripts/codex | Native developer passthrough, outside managed attempt lifecycle; no new runtime or transport imposed on arbitrary developer invocations. |
| codex-doctor/installers/readiness | Doctor reads config; installers/package staging read the single version pin; readiness probes App Server help as well as restricted exec flags. |
| Fakes/probes/recordings | Native fake now implements App Server wire frames. Synthetic endpoints use real binaries. Historical exec recordings stay labeled historical evidence. |

## Outstanding acceptance

The implementation is reviewable but issue acceptance is **not complete** until required
external validation is supplied. This VM has no `/dev/kvm`; the published PR's supported
Cloud Build VM/API/browser lane must establish the core queue, Stop/Cancel, native
save/continuation/reconstruction flows and relevant Claude product regressions.
Authenticated upstream OpenAI API-key, compatible/OpenRouter, and relevant Claude
journeys have no configured test account/environment here. Synthetic credentials and
raw authenticated ChatGPT protocol probing do not substitute for those product journeys.
Do not count pending CI, compilation or test-source inventory as an executed pass.

Final peer regression rerun: `env -u G5D_WORKSPACE_ROOT cargo test -p fake-provider-cli --profile g5-dev`
passed all unit/integration/CLI targets, including **47 CLI cases**. The App Server
roundtrip now also asserts correct total-token sums, discards unsupported typed legacy
frames, and proves requested clean/nonzero exits and replay errors terminate while
stdin remains open. This closes three verified peer findings from PR review; normal
native completion continues to wait for client EOF. Production lifecycle code is unchanged.

The API/guest catalog pin guard passed on the final source. Exact matching deliberately
keeps unknown catalogs on legacy transport; this independent bundled-catalog assertion
requires future catalog updates to revise the API capability pin instead of silently
falling back. No ordered-capability assumption is made about arbitrary catalog labels.

An extra standalone authenticated-supervisor attempt could not enter subscription
authorization because it had no workspace incarnation API client. The production gate
requires that control-plane context even for a cached native login. This setup attempt
is not an authenticated product pass or evidence of native incompatibility; the real
VM/API ChatGPT journey remains part of the external product validation obligation.

The final peer roundtrip additionally requires completion and usage for its exit-7
scenario, while explicitly expecting no completion for the immediate-exit/error
scenarios. The targeted CLI test and strict fake-provider Clippy passed after this
assertion was tightened; a missing terminal can no longer satisfy that scenario.

## Supported CI checkpoint and resume-refusal correction

At `0c7cad352867d499d028e7b214a6242630a79116`, required Rust CI passed.
The supported provider-agent browser lane passed Codex restored tool-result recall,
same-VM recall and Stop/native-save continuation, and the Claude reference recovery
cases. Its Codex refused-resume case failed consistently: the fake App Server had
already acknowledged the initial turn before evaluating its refusal directive, so
the supervisor correctly treated the session as confirmed and eligible for harvest.
That peer behavior did not model the intended native refusal boundary.

The peer now returns a validation refusal for the resumed initial turn before its
success acknowledgement. No production harvest condition was weakened. New CLI
assertions cover both prompt- and environment-selected refusal, no acceptance or
turn-started frame, and unchanged native bytes. The existing real-dispatcher
refusal/harvest regression now covers App Server alongside Claude and legacy exec:
App Server must have no acceptance/session event, no unknown delivery, persist_failed
recovery state and no bundle publication. All fake-provider targets (including 47 CLI
cases), strict fake Clippy and this dispatcher regression passed locally after the fix.
The full supported VM/browser run must be repeated on the corrected published head;
the failed first run is not an acceptance pass.

## Claude stable upgrade (2026-10-25)

The owner explicitly selected the `stable` channel. The saved design evaluates
2.1.285 → **2.1.308** before the stable pin change, using npm metadata and the
official release notes. The offered catalog remains unchanged. Native 2.1.308
passed all **412 Claude** combinations; unchanged Codex 0.164.0 passed all **352**,
followed by all eight actual production-supervisor transport/authentication
journeys against synthetic endpoints. The shared projection carries the new pin.

The required native gate now runs ten Claude control/background schedules too.
New real-binary loopback recordings live in
`tests/fixtures/agent-turn-native/claude-2.1.308/`; their manifest records version,
provenance, hashes and process observations. Native EOF now waits for the background
Bash command to finish, and idle/active interrupts retain their 0/1 exit behavior.
The current-frame dispatcher/supervisor replay passed; historical recordings remain
labeled with their original version. Current control-envelope and interrupted-exit
unit tests require the new pin's recordings.

The native formal gate exposed a new custom-endpoint tool-search placeholder.
Formal launches now force tool search off to preserve the existing two-tool
contract. Its strict native confinement test passed after the fix, with no extra
tools allowed. The responder advances its schedule only after assertions succeed,
so retries cannot skip a failed assertion. Codex's hidden-file view is accepted as
unreadable only after verifying the host fixture still exists with its dirty bytes.
Native memory capture/flush/fresh-home recall passed. Local checks passed 391
contract tests, 170 agent tests, seven frontend catalog tests and the flag gate.
The exact-head required CI result belongs in the PR description; pending CI is not
an executed pass. Existing authenticated upstream product gaps remain outstanding.

The combined root gate exposed an overly strong cancellation-probe assumption:
native SIGINT can cancel the pending question and exit 0 without a terminal result.
The probe now requires that matching cancellation and bounded exit; any result it
does emit must be aborted. Normal schedules still require successful completion.
Repeated pending-question cancellations passed, and the current recordings capture
the no-result interrupted shape. The recorder drains stdout after exit before
declaring frames missing. G5's forced Stop/Cancel and bounded reap are unchanged.

At `b0c1c584b7a387bafff18499193aba0f39772319`, required
[Rust CI](https://console.cloud.google.com/cloud-build/builds;region=us-central1/34ab289d-77da-4f79-8b6e-b6e85fe17977?project=89784110315)
passed. The [E2E build](https://console.cloud.google.com/cloud-build/builds;region=us-central1/df844c9e-2fc7-41dc-831c-c90a34c0d8bd?project=89784110315)
passed both staged native gates (764 combinations, eight production transports,
ten Claude schedules and confined formal clients), Rust E2E, API smoke/profiles and
both browser shards. One unrelated review fixture spent 178 of its 180-second VM
readiness wait queued for capacity, then passed on retry. The strict browser
receipt rejected that retry and failed the aggregate build. This is not a clean
overall E2E pass. Preserve the receipt requirement and repeat required CI on the
unchanged implementation after this evidence checkpoint.

## Rebase and review follow-up (2026-10-25)

Before rebasing, required Rust and all seven VM/API/browser stages passed at
`1b190c48904cab733644ae476e50a7c95b2b7528`, including both native gates. The
assigned branch is now rebased onto `origin/main` at
`24cc289ba7e55e48e3b20d7ec4e6f9555f57ea1b`; main's chat presentation and launcher
changes are retained. Fresh exact-head CI is required for the rebased result.

CodeRabbit's malformed frozen-field finding was valid: only absent fields may
select the historical exec contract. Present null, numeric, boolean, array or
object module/transport fields now conflict. All 22 dispatch unit tests passed,
including 25 malformed-field combinations and retained historical/current valid
contracts. All 336 focused chat UI tests and formatting/diff checks passed after
rebasing. The bundled-catalog regression already rejects a catalog-version bump
without its matching capability pin; unknown catalogs remain on the established
contract. Authenticated upstream product acceptance remains incomplete.

At `86bdbdc5f92a1104333e2d584979b350cf470c5c`, required
[Rust CI](https://console.cloud.google.com/cloud-build/builds;region=us-central1/a337aba7-7ff1-4041-b490-45d96df85f84?project=89784110315)
and [VM/API/browser CI](https://console.cloud.google.com/cloud-build/builds;region=us-central1/f3ccbae0-02d1-4082-8eee-ec960c78eab6?project=89784110315)
passed all required stages and both native gates. Browser logs reported no failed
tests. The API Docker builder now supplies the CLI pin files embedded by contracts
and fakes; all 18 Docker CI contract checks passed. The running full-stack preview's
API and frontend image IDs matched rebuilds from that checkout; both were healthy.

Main advanced to `d1ae472f59c7e5760f239c12628b8bf3bd6c4b35` during final
verification. Rebase retains its shared assistant-turn renderer, view-state and
native permission-action commitments; transport and catalog behavior remains
compatible. All 17 existing commits retained their changes without conflicts.
All 409 focused shared-renderer, chat-message, provider-output and catalog UI
tests passed after rebasing, including saved Haiku effort handling.
Required exact-head CI and preview rebuild verification must be repeated for this
rebased result; the passes above establish only the earlier head.

Rust CI on `33486c588a2fa1c217432728d18641781d4275d2` exposed a setup-test
scheduling assumption: the 750 ms budget expired before the blocked-write case
began its turn write, correctly reporting no possible input delivery. The test
now allows five seconds for startup, sandbox preparation and handshake, requires
the peer's intended stall marker, and bounds phase readiness plus settlement
together at twelve seconds. Timeout, no acceptance and exact delivery-uncertainty
assertions remain. Production deadlines and transport behavior are unchanged.
The focused test passed twice; all six App Server dispatcher tests then passed.
The same head's E2E build passed all seven required stages and both native gates.
Its noncatalogued file-editor case missed a comment glyph on its first attempt
and passed on retry; no root cause was established. Strict catalogued receipts
passed unchanged. Repeat exact-head CI and preview image verification after
publishing the deadline-test fix.

Main advanced to `22e8fbbd839ea71085b2193bb7da50cda13f03dc` after the clean
required Rust and VM/API/browser runs on
`dff2f96c146a154e2300c45d6ff274311f943a36`. Rebase preserves main's workspace-tab
reordering and latest-stored-list semantics alongside the shared chat renderer
and this migration. All 19 existing commits reapplied without conflicts.
All 460 focused tab, chat-renderer and catalog UI tests and all 18 Docker build
contract checks passed locally. No preview was running at publication preparation.
Fresh required CI belongs to the new published head; prior results do not prove
this rebased head. Authenticated upstream product acceptance remains outstanding.

CodeRabbit's fresh review of `b57f3f32920700a6e01e840d5046f49a740b7735` found
that saved Codex `never/read-only` and `never/workspace-write` modes displayed
through the historical “Auto” fallback. Their labels now identify the actual
mode; unknown saved values display verbatim. Existing outgoing mode values and
historical labels are preserved. Three added display/send cases reproduced the
bug before the fix; all 412 focused chat-renderer/catalog tests and TypeScript
validation passed afterward. The provider contract document now labels the
October 18 flag evidence as historical and points separately to current-pin
native confinement evidence, including the forced tool-search opt-out.

The proposed normalization of retired stored permission modes was declined:
the saved design requires explicit refusal for new resolutions and historical
validation for frozen receipts. Silent normalization would change configured
permission semantics. CodeRabbit acknowledged that contract and withdrew and
resolved its finding. The authenticated upstream acceptance thread remains open.

Before the review-fix batch, `b57f3f32920700a6e01e840d5046f49a740b7735` passed
[required Rust CI](https://console.cloud.google.com/cloud-build/builds;region=us-central1/e7be7170-55ee-4eb9-920d-eb05247e451e?project=89784110315)
and [VM/API/browser CI](https://console.cloud.google.com/cloud-build/builds;region=us-central1/c7168eaa-cc98-4f88-9685-e7b31570119c?project=89784110315),
including all seven required stages, strict browser receipts and both native gates.
A fresh remote check still found main at `22e8fbbd839ea71085b2193bb7da50cda13f03dc`.
The owner started a full-stack preview during that run; its source images built
successfully with the review fixes. Verify running images after publishing the
batch and require fresh exact-head CI for the resulting commit.

## Scoped follow-up to review 6403166501 (2026-10-25)

Evaluated the [review of `98dba2adf`](https://github.com/G5Labs/G5-Lean/pull/813#issuecomment-6403166501)
against the saved migration contract. This follow-up keeps that design and the PR's scope.

- Process-mode staging now refuses a real provider whose `--version` differs from
  the repository pin before staging the advertised catalog. Explicit fake-provider
  lanes retain their fake runtime. Tests cover both old binaries and matching pins,
  including the existing launcher/stdio/idempotency guarantees.
- A strengthened split-secret assertion reproduced an actual chat-delta leak:
  generic JSON leaf order compared each text fragment with the next event's
  metadata instead of its displayed text. The shared dispatcher redactor now
  follows `text_delta` for chat deltas while retaining whole-value metadata
  redaction. Three-fragment tests cover both adjacent and expired holds.
- Adapter/dispatcher regressions cover all three validation refusals and internal
  errors, the guest transport/module mismatch, stale or missing delta/item/usage/error
  identities, thread-scoped warnings, native interruption versus owner cancellation,
  string request ids, cache-write usage, NUL output, secrets split across deltas,
  oversized acceptance/completion and the upper as well as lower drain bound.
  Acceptance-deadline testing now allows the same five-second setup budget as its
  sibling phase test and explicitly outlasts it after acceptance.
- Actual dispatcher/fake-App-Server turns save a tool-only fact and recall it through
  native resume in both the same home and a replacement home containing only the
  published rollout bytes. The executable rejects an App-Server-only E2E directive
  on exec; Rust M5/M6 and the browser restart journey now carry that assertion.
- The required native-catalog test now invokes the existing App Server stable-schema,
  hostile-feature and same/replacement-home probe. It also exercises retained exec
  with real tool turns on both Codex API routes. Matrix launches include the production
  `memories`, `daemon_auto_start` and memory-generation/use opt-outs; the App Server
  inventory comes from the generated direct-route catalog. Root CI runs the newly
  required protocol probe under an unprivileged identity.
- Added Rust coverage for the current Claude cancellation/no-result and EOF/background
  recordings, formal Codex feature overrides, formal Claude model overrides and absent
  effort, subscription file credential storage and inherited retired configured modes.
  Restored the editable Sonnet effort/source assertion while retaining the Haiku test.
- The fake CLI subprocess helper removes inherited `G5D_WORKSPACE_ROOT`; the ambient-
  environment-sensitive workflow unit test moved to that isolated CLI boundary with
  all its existing assertions. DTO fixtures read the canonical CLI pin. Unknown Claude
  permission labels now display their saved value, preserving the outgoing policy.

No permission migration is introduced: retired configured modes must still fail new
admission, and historical receipts remain readable. Native warnings have an optional
thread id and no turn id in the pinned schema, so they remain thread-scoped diagnostics.
Historical Claude recordings retain their own tests alongside current-pin coverage.
The native formal filesystem-denial assertion retains its existing host-file/content
invariant; a hidden path can legitimately report ENOENT under the pinned confinement.
Historical picker fixtures/labels remain useful compatibility coverage.

The proposed supervisor enum, acceptance/launch/catalog helper consolidation, Python
framework extraction, wholesale fake-test restructuring and duplicate-error-notice
change are broader refactors or pre-existing behavior, so are deferred. Full fake-frame
schema validation and the expanded browser/manual parity matrix remain coverage work;
this change requires the independent real-binary protocol and production-launch gates.
Authenticated upstream/product acceptance remains outstanding, so keep the PR WIP.

Local validation: 391 contract tests, 173 agent tests, all fake-provider targets
(including 49 CLI cases), 147 dispatcher unit tests, 337 focused UI tests, TypeScript
validation, Rust E2E compilation, 64 browser cases discovered, process-host shell
regression and strict agent/dispatcher/fake Clippy passed. The pinned native matrix
passed all 412 Claude + 352 Codex combinations. Separate production-supervisor probes
passed all eight App Server/Claude routes and both retained Codex exec routes with
real tool turns. The exact-pin App Server protocol/configuration/continuation probe
passed both as the owner and from a root CI driver dropping to an unprivileged user.
The PR records the full dispatcher integration regression and exact-head publication
CI outcomes separately from these completed local checks.

## Refactoring follow-up to review 6403166501 (2026-10-25)

The owner's follow-up explicitly asks to reconsider the refactor suggestions in
[the review](https://github.com/G5Labs/G5-Lean/pull/813#issuecomment-6403166501).
The earlier deferral above is superseded for the bounded changes below. Existing
transport, permission, acceptance, process-custody and settlement contracts remain
in force; no new supervisor or provider-state owner is introduced.

| Review suggestion | Evaluation and disposition |
| --- | --- |
| Transport enum | Accepted. `Transport` owns the selected Claude stream, Codex exec or App Server adapter and its acceptance, EOF, line handling, grace and interruption differences. Launch planning validates the frozen transport once and returns that adapter; App Server no longer constructs an unused exec mapper. |
| Acceptance consolidation | Accepted for shared data and write tracking. One payload builder serves ordinary acceptance and the restored-answer receipt; one frame-purpose predicate marks potentially delivered initial writes. The acceptance moments must remain distinct: App Server requires matching native acknowledgement, while a restored Claude answer uses the completed transport-write receipt. Its embedded acceptance remains atomic with `provider_request.replied`. |
| Input-write failure classification | Accepted. `provider_input_failed` assigns the App Server error once and reserves `reply_undeliverable` for Claude. The shutdown path no longer needs an App Server exclusion around the Claude Continue outcome. |
| Claude launch helpers | Accepted. Ordinary and confined formal launches share capability-settings merge and native model/effort arguments, including omission of `none`. Each caller still supplies its own workspace/memory or isolated formal settings. |
| Catalog walks | Accepted. Contracts expose `offered_entries()` for API/native projections and `model_efforts()` for admission and effort clamping. Historical models and aliases stay known without being reoffered; custom pins retain the existing fallback. The frontend continues to consume its generated catalog; changing it to fetch the HTTP endpoint adds no value here. |
| Constants | Accepted for the wire transport, CLI pin and retired mode inventory. The fault peer receives the canonical pin from its Rust staging helper. A global cross-language `none` substitution is deferred: it would add generator/import churn without removing the model capability contract already represented in the catalog. |
| Python framework extraction | Deferred. The substantive catalog-inventory and production feature-configuration discrepancies were already fixed in the preceding batch. A shared fixture framework and JSON-RPC client rewrite is optional maintenance, not needed for this runtime refactor; independent native/peer checks continue to protect the wire boundary. |
| Frontend labels | Unknown Claude modes were already fixed and tested. Saved Codex `on-request/read-only` now displays `Read Only (ask)`, distinct from `never/read-only`, while preserving the outgoing native value. Historical `minimal` and retired mode display metadata remain useful for saved-session compatibility; current offered choices still come from the catalog. |
| Duplicate error notices | Deferred. This is existing exec/App Server behavior and a separate user-visible error-projection change, not required to remove the identified duplication. |
| Test helper repetition | Deferred beyond the canonical pin injection and using the launch plan's actual App Server adapter in its test. Keeping independent fault-peer frames avoids making both sides agree through a shared fixture builder. Splitting the fake CLI scenario test is optional maintenance, while the real dispatcher and native gates cover the modified paths. |

New focused regressions cover offered routes and labels, historical aliases,
unknown/provider-mismatched model pins, and the distinct Claude/App Server
input-failure outcomes. Existing tests cover native questions, restored answers,
background/End/EOF, exec EOF, App Server acceptance/refusal, output limits,
interruption, session persistence, formal confinement and shared settlement.
Local checkpoint on source `695dc5e138ca677191ce396ef1ac84cc7571e80c`:

- `env -u G5D_WORKSPACE_ROOT cargo test -p g5d-agent -p g5-contracts --lib --profile g5-dev`:
  **174 agent + 393 contract tests passed**, including generated-catalog freshness.
- `npm test -- --run src/components/chat/ChatPanel.test.tsx src/components/agent/AgentLaunchConfigControls.test.tsx`:
  **337 tests passed**, including display/send coverage for both providers.
  TypeScript, Prettier, Rust formatting, peer syntax and diff checks passed.
- `cargo clippy -p g5d-agent -p g5-contracts -p g5-api-core --all-targets --profile g5-dev -- -D warnings`:
  **passed**.
- `TEST_DATABASE_URL=<isolated-test-database-bootstrap> cargo test -p g5-api --test integration --profile g5-dev api_setup_e2e::test_get_available_models -- --exact`:
  **passed**; the actual authenticated HTTP response matches every offered Claude
  and Codex route/model/effort/mode/pin in the generated native catalog.
- `python3 scripts/test-native-provider-transport.py --g5d <rebuilt-g5d> --claude <2.1.308> --codex <0.164.0> --codex-tool-turn`:
  **all eight production launch/auth routes passed**. A second run with
  `--codex-transport exec --codex-tool-turn` also passed, including both retained
  Codex routes. These use real pinned binaries and synthetic loopback upstreams.
- The opt-in `formal_review::formal_review_real_native_clients_enforce_tools_and_report_transport`
  dispatcher test **passed** for both native providers and both verdicts.
- The initial full dispatcher run passed 97 cases and exposed one staging omission
  in the image test's separate copy of the pin-injected peer. Both consumers now
  use one staging helper. A fresh full run of
  `env -u G5D_WORKSPACE_ROOT cargo test -p g5d --test dispatch_modules --profile g5-dev --no-default-features -- --test-threads=2`
  passed **98 tests**, with three opt-in native tests skipped, on
  `4de18d31d491951ccb227828f7c6efc0f7ee2379`. The PR description records the
  required exact-head CI outcomes separately.

Fresh automated review of that checkpoint exposed two bounded follow-ups. The
shared Claude settings helper now merges `modelOverrides` per key, retaining
unrelated workspace/memory aliases while host-owned catalog entries win. This
corrects a pre-existing loss of unrelated aliases, consistent with the saved
settings-preservation intent. The runtime regression covers both memory settings
and the workspace settings file, host precedence, custom aliases, malformed
settings and unchanged custom-model pins. The image peer injection now requires
exactly one complete top-level anchor line, so future peer edits cannot silently
remove its image assertions or produce an incorrectly indented injection.

After these follow-ups: **393 contract + 175 agent tests**, the real dispatcher
image test through Claude/exec/App Server, strict Clippy, all eight real pinned
native launch/auth routes and both providers' native confined formal reviews
passed again. UI/API and retained-exec source paths are unchanged since their
passes above. Fresh required CI for the resulting published head remains the
final regression evidence; see the PR description for its outcome.

## Current-main integration (2026-10-26)

Rebased onto `0ae45a05ffde1bc3077845156cefe8d815894efa`. All 26 provider
commits replayed without conflicts; range comparison confirms their patches are
unchanged. Main's Source Control Save workflow, live-tab restoration and CI VM
capacity changes remain intact alongside the migration. Earlier executed results
above remain historical evidence; required Rust and VM/API/browser CI must pass
on the newly published head. The PR description records that outcome and the
current-head preview rebuild. Authenticated upstream acceptance remains open.
