# Standard features: deployment gate retirement

Scope: the seven features approved by the user on September 13, 2026. G5 Lean starts in a new environment. Other feature-removal proposals in `deployment-gate-audit.md` remain proposals.

## Behavior

| Feature | Standard behavior | Checks retained |
| --- | --- | --- |
| Fork/chat carry | Carry is available for eligible non-Base sources. Scoped `fork_chat_enabled` values no longer control it; the compatibility response member is always true. | Workspace authorization, supported session/provider checks, explicit session selection, authoritative session-list readiness. |
| Codex UI | Codex and its full offered model catalog are available without instance-info feature gating. | Provider-account visibility and permissions, model catalog exclusions, launch resolution, runtime/provider readiness. |
| Make Remote | Adopt Remote, create-and-bind for absent refs, and merge routing for divergent refs are standard. | Org-admin authorization for shared repo re-pointing, URL validation, local-remote policy, compare-and-swap finalization, create-only publication, merge workflow. |
| Durable task CRDT | Task documents always use durable, task-scoped sessions. The ephemeral task fallback is removed. | Task/repo/tenant scope, concrete materialization head, stale reconnect and generation checks, persistence barriers and retention. |
| Zero-slot polling | Compatible hosts can negotiate and use the typed zero-slot lane without an operator switch. | Requested transport capability, authenticated host identity, zero-slot-only task filtering, ordinary capacity limits. |
| Strict lifecycle identity | Missing, stale, or foreign phase identity is rejected independently of fleet composition. | Locked/server-derived authority, non-mutating liveness validation before heartbeat writes, independent signal validation. |
| Discard fencing | V2 negotiation is available by default; deployment drain and gate-disabled acknowledgement settlement are retired. | Two-phase host challenge/proof, registration generation, provider composition, grant receipts, exact teardown operation/evidence and valid idempotent replay. |

The separate `G5_CRDT_PERSISTENCE_BARRIER_EXACT_V2_ENABLED` protocol setting remains unchanged, including its explicit-opt-in regression test. Normal host activation/registration and conservative wire-protocol compatibility handling remain; these do not provide a deployment toggle.

## Configuration and test coverage

Removed the selected switches from Helm overlays, Docker Compose, self-hosted installation templates, and Cloud Build. Removed the Codex browser-test mirror switch so the relevant tests cannot silently skip because a deployment flag was unset.

Gate-off-only tests for retired behavior were removed or rewritten. Their retained behavior is covered by default-path tests:

- Make Remote HTTP integration tests retain non-admin rejection, authorized re-pointing, URL conflicts, absent-ref publication, and single merge-run creation.
- Strict identity HTTP/Postgres tests now exercise empty-fleet rejection and unchanged runtime posture by default, alongside stale/foreign identity and real heartbeat ingress.
- Host registration tests retain challenge/proof, generation and capability tests. Rollout-drain and gate-disabled receipt-cleanup tests were retired with their exclusive paths; normal teardown authorization and acknowledgement tests remain.
- Fork carry E2E explicitly tests that an old scoped false value cannot disable carry. Base sources and session-list failures remain covered.
- Codex provider, model, settings and account tests now assert standard availability. Other feature switches are unchanged.
- A new real HTTP instance-info test checks Codex and Make Remote capability publication even when retired environment settings contain `false`.
- Legacy heartbeat fixtures now report explicit runtime generations; missing/foreign/stale identity rejection remains tested. Provider-acceptance fixtures use the real assignment ACK to create their execution attempts. Unknown runtime batches reject before renewing valid siblings, and receipt-loss decline replays reject without changing runtime state.

## Validation

Validation ran through `just docker-ci`, with `G5_CI_NO_INTERPHASE_PRUNE=1` to retain build caches. This does not skip tests. The initial full runs exposed stale fixtures; those were corrected without relaxing the retained safety assertions. The second full run passed through provider replay, then stopped on an E2E heartbeat fixture. After correcting that fixture, phases 6, 6b, and 7 were run separately. This is combined phase evidence, not a claim that one uninterrupted full invocation passed.

| Check | Result |
| --- | --- |
| CI script regressions, formatting, Linux strict all-target Clippy and guards | Passed; 36 script regression suites |
| Main Rust unit groups | 1,521 + 1,990 + 3,065 + 1,261 passed |
| Isolation groups | 17 + 195 passed |
| Frontend unit tests | 228 files / 4,182 passed |
| API integration | 2,419 passed, 11 skipped |
| Worker integration | 643 passed, 12 skipped |
| Provider replay | Claude 230 and Codex 165 passed |
| Phase 6 E2E | Main 497 passed / 5 ignored; additional groups 8 + 3 + 3 + 10 passed |
| Phase 6b API schema smoke | 38 passed |
| Phase 7 Chromium browser suite | 463 passed, 68 skipped, one test passed on retry; see below |
| TypeScript and frontend ESLint | Typecheck passed; zero lint errors, 616 warnings matching baseline |
| Just command audit | 103 recipes parse; 42 script entry points exist; named Rust targets resolve |
| Diff whitespace | `git diff --check` passed |

Logs: `/tmp/g5-seven-docker-ci-2.log`, `/tmp/g5-seven-docker-ci-phase6.log`, `/tmp/g5-seven-docker-ci-phase6b.log`, `/tmp/g5-seven-docker-ci-phase7.log`. The final E2E fixture also passed strict native Clippy (`/tmp/g5-seven-e2e-clippy-final.log`).

The browser exception was D42 remote auto-push: a saved commit converged in tracking state, but its history contained only `noop`, while the test required `pushed`. Repeating the four-test remote-tracking group with retries disabled reproduced D42 once in three repetitions. The trace is preserved at `/tmp/g5-seven-remote-repeat-artifacts/remote-tracking-Remote-tra-e314b-d-integration-status-pushed-chromium-repeat1/trace.zip`.

D42's polling callback submitted manual `/sync` requests every second. The queue repository refreshes `requested_at` on each such enqueue; the integration worker rejects a changed queue version at finalization, after remote Git I/O. A successful remote push followed by such a deferral can therefore retry as `noop`. The trace supports this sequence but does not contain server-side lease diagnostics that prove the exact interleaving. D42 now passively observes its commit-triggered automatic push, retaining the `pushed` history assertion, exact remote SHA check, and healthy UI/push-label assertions. Manual sync and divergence remain exercised by sibling tests. The subsequent five-repetition run passed all 20 tests with retries disabled (`G5_CI_NO_INTERPHASE_PRUNE=1 just docker-ci 7 false "" "e2e/flows/remote-tracking.spec.ts --retries=0 --repeat-each=5 --trace=on"`; log `/tmp/g5-seven-remote-tracking-passive-repeat.log`). The entire browser suite was not repeated after this test-only polling change.

Limitations: existing conditional KVM, live-provider, and unsupported-feature tests remain skipped; live LLM testing was not enabled. The worker/isolation suites reported the same process-leak warnings seen in the pre-change baseline. Numerical line/branch coverage was not measured. No deployment was performed.
