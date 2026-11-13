# Codex App Server and execution settlement

Work: [#809](https://github.com/G5Labs/G5-Lean/issues/809). The owner also requested
upgrading Codex to the latest stable binary on 2026-10-24. Authorized branch:
`issue-809-migrate-codex-to-app-server-with-explicit-turn`.
Comparison baseline: `origin/main` / `0ae45a05ffde1bc3077845156cefe8d815894efa`.

The 2026-10-25 rebase retains main's prompt-history, device-code clipboard and
durable error-notice commitments alongside this transport and catalog contract.
Those presentation changes preserve the existing attempt identity, delivery and
settlement owners; no governing intent conflicts were found.
Main's [shared assistant-turn presentation](../../ontology/features/chat.g5.md#g5.chat.presentation-owner)
also retains actual provider interaction capabilities and execution identities.
It is compatible with this transport boundary and per-model launch controls.
Main's [workspace-tab reordering](../../ontology/features/ui.g5.md#g5.workbench.tabs.reorder)
keeps selected workspace, URL and return state while moving entries in the latest
stored list. It changes neither chat attempt ownership nor provider launch controls;
retain its implementation and browser coverage alongside this migration.

The 2026-10-26 rebase also retains main's repository browsing, explicit Save
checkpoint, restored authorized routes after live-tab expiry and reduced browser
concurrency for source/fork VM occupancy. Those owners do not change provider
acceptance, process custody or settlement. Keep main's Save workflow and receipt
checks alongside both providers' recovery and native compatibility gates; no
governing intent conflict was found.

## Owners and contract

Retain the production `g5d-agent` supervisor, dispatcher custody, journal,
TaskService settlement, queue allocation and session-file bundle owners. A Codex
adapter owns only JSON-RPC initialization, thread/turn correlation, native frame
translation and status evidence. Adding a second supervisor or a resident provider
would duplicate custody and introduce recovery coupling without supplying a missing
guarantee. No database migration or new persistent lifecycle is needed.

One attempt launches `codex app-server --listen stdio://`. It sends `initialize`
with `capabilities.experimentalApi: false`, waits for its response, sends
`initialized`, starts/resumes the native thread, then sends exactly one frozen
`turn/start`. The matching successful turn/start response with a valid native turn
id and inProgress status is initial acceptance; initialization, thread creation,
a partial write and a flushed request are not acceptance. Reconstruction notices
publish with confirmed initial acceptance too; initialization alone cannot claim
that delivered history was reconstructed in a native session. Set possible delivery
only when the initial turn request starts writing. The pin's validation/method refusal
codes (-32600/-32601/-32602) establish rejection; internal errors and malformed or lost
acceptance responses keep existing uncertain-delivery recovery; it never creates delivered history speculatively.
The thread response must confirm the frozen model, never-approval policy and sandbox
mode before the initial turn is sent. Malformed known native requests and scoped turn
notifications fail the protocol even when an identity is missing, rather than creating
an unbounded wait. Thread and turn ids stay local to the already incarnation-bound attempt and its
journal; unrelated or stale identities cannot publish output or completion.

A matching `turn/completed` terminal status closes input and starts the existing
15-second exit grace. Continue bounded stdout/stderr reading through exit, including
the existing five-second post-exit drain. Successful native completion AND successful
process exit are necessary for success. Failed or interrupted status, missing native
completion, process failure and forced outcomes cannot become success. A matching native
interruption with clean process exit uses G5's existing interrupted outcome/continuation
policy; an additional process failure remains failure, and owner Stop/Cancel takes priority. Normal release,
forced cleanup, bounded harvest and finalization precede the authoritative outcome.
Native completion alone neither settles the task nor allocates its queued successor.

Keep Claude's current multiple-native-turn and background semantics, its three-second
report-turn settling window, pending request/write holds and EOF handling. Buffered
input may produce a later turn after EOF; drain it without a consumption acknowledgement.
Codex keeps queued follow-up input and explicit unsupported interactive outcomes.
Formal review's restricted ephemeral exec transport remains specialized and unchanged;
summary titles remain unsupported for Codex. Login retains the native device flow.

## Pin, approved configuration and rollout

The npm `latest` stable tag and official changelog select **0.164.0** (2026-10-24);
install that exact package and regenerate schemas without `--experimental`. The
command itself is upstream experimental: disabling experimental APIs is a separate
contract, not a maturity claim. Audit all effective runtime features at this pin,
including inherited owner/project config, and override every experimental or under
 development feature to false; disable daemon auto-start to keep process custody local.
Do not allow arbitrary configuration, transport, feature or handshake overrides from
provider options. Preserve frozen model, effort, approval=never, sandbox, credentials,
attachments, native MCP/harness and usage/redaction owners.

Use a new ordinary module contract, retaining old declarations. Freeze the module and
transport selection in the existing attempt policy snapshot at reservation. Rebuilding
an existing reservation without the new fields retains exec/1.0.5. New supported guests
advertise the new contract; older guests retain their supported contract during rollout.
Update the guest catalog and digest, API selection and host-derived pins together.
Specialized formal-review module versions continue to use exec.

## Verification and unresolved evidence

### Claude upgrade evaluation, before changing the pin (2026-10-25)

The owner authorizes the most recent **stable** Claude CLI after evaluating its
changes. Baseline is **2.1.285** (published 2026-09-29); npm's current `stable`
channel is **2.1.308** (2026-10-22). Select 2.1.308 and pin it exactly, following
the owner's explicit stable-channel clarification. Both packages require Node
22 or later; the installed production native executable and supported staging
platforms remain the relevant runtime closure. Sources: [npm package metadata](https://www.npmjs.com/package/@anthropic-ai/claude-code)
and [Anthropic's release changelog](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md).

| Area | Evaluated delta from 2.1.285 through 2.1.308 | G5 implication and verification |
|---|---|---|
| Native completion and custody | 2.1.292 waits for one-shot background commands after the result; 2.1.299/304 stop plugins extending headless runs beyond limits, interrupt or ended input; 2.1.300 force-kills commands still alive 1.5 seconds after shutdown begins. | Probe result-before-EOF, background report turns, EOF and idle/active interrupt on the new binary. Retain the supervisor's bounded settling, custody and exit requirements; never infer completion from the changelog. |
| Frozen controls and history | 2.1.290/287 preserve effort across fallback; 2.1.305 fixes modelOverrides case/context matching; 2.1.299/300 fix queued stream-json prompts and compacted headless resume. No documented removal of G5's stream-json/control surface. | Run flag gate and every offered model/mode/effort, including dotted OpenRouter ids, explicit absence of Haiku 4.5 effort and `[1m]`. Check native control/result shapes and retained session behavior. |
| Defaults and model inventory | 2.1.293 adds Haiku 5.5 and changes the `haiku` alias; 2.1.302 enables fork subagents in headless sessions and changes unspecified permission defaults. | G5 passes explicit catalog ids and permission modes. Preserve current offered catalog and frozen controls; check background subagent accounting and restricted formal-review tool confinement against the expanded native inventory. Offering a new model is separate from binary compatibility. |
| Direct and routed providers | Gateway fallbacks remove rejected betas/context fields (2.1.290/295/304); tool search is enabled for custom endpoints with rejection fallback (2.1.304/305); 2.1.307 stops sending prompt-caching-scope behind custom endpoints. | Exercise actual direct API-key/Bearer/OAuth and OpenRouter launch requests, frozen effort, cache markers, endpoint and authentication. Test capability support with accepting synthetic endpoints; fallback support does not prove a gateway implements a capability. |
| Permission and credential isolation | Fixes cover custom-gateway keys reaching Anthropic (2.1.299), late tools escaping `--tools` (2.1.295), hook actions failing open (2.1.299), and MCP connections made before policy checks (2.1.299). | Keep existing explicit permission policy, managed settings and tool allowlist. Run native formal confinement plus supervisor and memory regressions. No permission expansion is authorized by the upgrade. |
| Packaging and integration | Node requirement stays >=22; npm updater now detects a missing native download. MCP defaults move to protocol 2026-07-28 and server descriptions grow. Full request parameters now appear in native transcripts (2.1.300). | Install in isolation and check the actual executable. Exercise native MCP formal tools and memory/session bundle ownership; retain existing secret custody and transcript redaction. Historical 2.1.285 recordings remain labeled historical unless re-recorded with valid new-pin evidence. |

This evaluation precedes installation/pin changes. Proceed with the exact-pin
upgrade and proportionate native/production regression checks. Authenticated
upstream product acceptance remains outstanding as already recorded below.

The 2.1.308 native formal gate found a concrete default-change incompatibility:
custom-endpoint tool search adds `DeferredToolPlaceholder` despite the restricted
`--tools` list. Preserve formal review's existing two-tool surface by forcing
`ENABLE_TOOL_SEARCH=false` in its sanitized environment, overriding inherited
values. Ordinary chat keeps its existing tool policy. The [native option contract](https://code.claude.com/docs/en/mcp#tool-search-through-an-llm-gateway)
documents that leaving ToolSearch out of `--tools` alone can retain the placeholder.
Retain the strict native assertion; do not expand the expected tool set.

Native 2.1.308 SIGINT cancellation of a pending question always cancels the request,
but can exit 0 without emitting a final result. The native cancellation probe must
require the matching cancellation and bounded process exit, with any emitted result
classified as aborted. Normal turns still require native completion and successful
exit. G5's already forced Stop/Cancel outcome and bounded reap remain authoritative;
an interrupted CLI's zero exit never establishes normal completion. The recorder
also drains stdout through EOF after process exit before declaring a frame missing.

The owner's catalog-parity follow-up adds an exhaustive native compatibility gate
for **both** provider pins. The API/backend catalog owns model ids, offered flags,
route forms, native mode inventory and per-model effort sets. Generate the native
test matrix with the UI catalog and require freshness of both. Resolve every matrix
entry through the API contract and execute completed native turns against local
synthetic providers, checking the effective native mode and actual outbound model
and effort. Offer only modes the production dispatcher executes through UI/API choices
and new admission. Test the broader native mode inventory separately, preserve frozen
historical validation and retain the read-only UI mode policy. Every offered mode must
map to a production dispatch policy; this grants no new interaction capability.
Account-specific upstream availability remains live acceptance.
Native probes execute under an unprivileged identity even when their CI driver is
root, matching the guest owner and Claude's refusal of bypass mode under root.

The first matrix run found the pinned Claude CLI silently drops effort for older
OpenRouter dotted ids. Reuse the catalog's route mappings in native `modelOverrides`
settings so Claude recognizes the selected model's capabilities while sending the
frozen slug unchanged. Merge this host-owned mapping into the existing launch settings
for known OpenRouter ids; do not invent capabilities for custom model pins. Verify
the same settings through the native matrix and the production supervisor launch.

Haiku 4.5 has no native effort control. Keep the model offered and represent that
absence as G5's `none` effort marker: display “Not supported” read-only and omit
the native effort argument. A provider-wide inherited effort cannot manufacture
this capability. Existing Haiku sessions display that absence and send it on new
follow-ups even if their saved settings contain a former effort; keep saved history
and already frozen attempts unchanged. New API resolutions reject out-of-set efforts for known models;
historical frozen resolutions keep their existing validation. Unknown custom pins
retain their existing provider-level validation. This is not a new native effort.
Codex Ultra is a native orchestration selection: verify its selected value in native
turn metadata and its ordinary-request encoding using the pin's model metadata,
rather than requiring an invalid literal `reasoning.effort=ultra` on Responses.
Native 0.164.0 rejects the retired `on-failure` approval value. Remove that preset
from current catalog choices and use `never/workspace-write` as the catalog default,
consistent with existing G5 noninteractive execution. New resolutions reject retired
configured/explicit modes with a validation error; do not silently reinterpret them.
Frozen historical resolutions retain their old mode validation and transport.
Replace the instance model endpoint's obsolete hard-coded inventory with a projection
of the same catalog, exposing provider, route, modes, per-model efforts and runtime
pin. Unknown capacity and price metadata is nullable rather than invented. Exercise
the HTTP response against the generated native matrix; preserve endpoint authentication.

The [coverage matrix](coverage.md) is an inventory of exercised transports, not provider
labels. Controlled peers must drive the actual adapter, supervisor and dispatcher.
Run native pin schema/configuration probes, adapter tests, real process/dispatcher
failure schedules, API/database queue/settlement tests, Linux custody tests and VM/browser
core flows, plus authenticated provider/auth-route journeys and Claude regressions.
Record tested commits and commands with outcomes; unexecuted checks remain outstanding.

Native 0.164.0 synthetic-provider probing confirmed completed turns can resume in a
replacement home containing only the saved rollout JSONL, with earlier input present
in the next Responses request. Resume uses the pin's stable excludeTurns=true
metadata response so old transcript pages do not exceed G5's output-frame ceiling;
the native model still receives the saved history. The [reproducible probe](../../../scripts/test-codex-app-server.py)
also compares generated stable schemas byte-for-byte and overrides hostile inherited
experimental features. It is native compatibility evidence, not authenticated-provider
coverage. No new native-file owner or database persistence is required.

The runtime-feature inventory found `instant_interrupt` enabled by default and marked
under development at 0.164.0. Override all 78 experimental/under-development flags,
including that default, as well as daemon auto-start and the previously disabled memories.
Apply these pin-wide opt-outs to retained exec and restricted formal-review launches too:
the binary upgrade would otherwise introduce instant_interrupt there. Their frozen
transport and existing tool/permission confinement remain unchanged.
The bundled/custom-provider model list remains unchanged; subscription accounts may
filter it. Preserve model and effort selections. Native usage has session-cumulative
`total` and latest model-call `last`: the first matching current-turn update derives
the pre-turn baseline from `total - last`; subsequent total deltas count every model
call once, including identical-sized calls, without importing earlier session usage.

Self-review: the design separates native completion, process exit and G5 settlement;
reuses existing state/custody/finalization, preserves frozen historical transports and
Claude exceptions, and makes native compatibility and authentication evidence explicit.
Implementation is in reviewable form. Local adapter, dispatcher/process, API/database
and authenticated ChatGPT protocol evidence is recorded in the coverage matrix; required
external product/authentication acceptance remains outstanding until executed.
