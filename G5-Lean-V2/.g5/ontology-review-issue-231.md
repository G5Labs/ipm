# Issue 231 implementation review

## Initial self-review, before repair

Compared with `origin/main` at `7e50b7a0`, using the saved [design](ontology-plan-issue-231.md) and [issue](https://github.com/G5Labs/G5-Lean/issues/231). This pass is self-review. The three fresh discovery reviews and external Claude CLI review are recorded separately before any consolidated repair.

| Commitment | Actual path and counterexample/check | Initial result |
|---|---|---|
| `g5.conversation.inbox.reservation` and `g5.conversation.inbox.content` | Public send/start persists `session_inputs.artifacts` with `input_id`; dispatch reads the reserved row, freezes typed descriptors into `agent.turn`; runtime rejects wrong input ID and duplicate/unsupported descriptors. A guessed ID from another input is rejected by returned identity. | Source inspected; direct public API to candidate module still awaits stack E2E. |
| `g5.security.tenant` and `g5.security.admission` | Incarnation-scoped internal download checks current VM/owner and session workspace; runtime compares session/input identity from response. Wrong session fixture produces `attachment_identity_mismatch`. | Code and local materialization test support the boundary; live API RLS/VM behavior awaits stack E2E. |
| `g5.conversation.inbox.prompt`, `g5.conversation.recovery.snapshot`, `g5.conversation.recovery.carry` | Prompt hash stays frozen; attachment descriptors come from the reserved input, while prior delivered history renders references only. Fork copies history, not queued inbox. | Source inspected; a fork followed by a fresh image has no dedicated runtime test. |
| `g5.conversation.delivery` and `g5.data.outcome-evidence` | Validation and download/hash checks precede provider spawn; Claude frame or Codex image path is then written before input acceptance. Missing, corrupt and wrong-session fixtures refuse. | Active-launch and real-dispatcher synthetic-provider tests pass; live-provider behavior unexecuted. |
| `g5.architecture.provider_adapter` and `g5.quality.evidence.fakes` | Active Claude launch uses checked image blocks; active Codex launch passes verified private files with `-i`. A synthetic provider inspects exact prompt and bytes through the real dispatcher. | Tested. The public API E2E synthetic provider check compiles but has not run on a candidate stack. |

Initial findings: no confirmed code defect from this self-review. Evidence gap: candidate-stack public API E2E has not run locally. The affected worker proxy suite had one occupied-socket failure; its failing selector passed alone, showing a fixture collision rather than an attachment-path failure.

## Independent discovery and consolidation

Three independent read-only discovery reports and the requested Claude CLI review produced the findings preserved below. Claude was reminded of its `gh` and `gcloud` access, given the verification results above, and told that six rounds is a limit and that reasoned pushback is expected.

| Finding | Initial evidence | Disposition before repair |
|---|---|---|
| A1/B3: image bytes are hashed but not decoded; Codex can replace an unreadable image with text, and the new PNG fixture is corrupt | Active `materialize_attachments`; independent PNG CRC check and pinned Codex source | Accept. Decode against declared format before provider launch; replace success fixtures and add failure check. |
| B1: first-turn request-key replay ignores attachments | `WorkspaceMessenger::request_digest` hashes message fields but no attachment set; route skips persistence on replay | Accept. Include validated attachment identity in the claim and test changed/same bytes. |
| B2: malformed persisted descriptors stall a tenant dispatch loop | `frozen_attachment_descriptors` errors inside reservation and maintenance loop breaks | Accept. Carry an explicit preflight failure into the module's durable outcome without blocking reservation. |
| C1: uppercase MIME accepted by upload but rejected by turn | Upload preserves case; dispatch and turn use case-sensitive checks | Accept. Compare MIME case-insensitively and normalize only the native provider value. |
| A2/B/C: queued/retry/recovery/fork evidence incomplete | Only a single public happy-path E2E compiles; dispatcher test uses a mock byte endpoint | Accept as verification gap. Add discriminating active-path cases and run candidate public E2E; report remaining VM/provider limits. |
| Claude: non-image, missing/corrupt and descriptor-boundary tests are absent from real module/unit boundary | Read-only CLI report; no code defect asserted | Accept as focused test gap, subject to proportionate regression coverage. |

Claude's statement that the fake's marker scoping proves no historical image replay is too broad: the checker currently permits unexpected image bytes when the current prompt has no marker. The repair will make absence an explicit assertion. Its no-code-defect conclusion also missed the independently demonstrated image decoding, replay, poison-manifest and MIME paths; these are being repaired. This is round 1 of at most 6.

## Repair and verification

The coherent repair adds image format matching and bounded decode before handoff, includes the validated inline set in first-turn request-key identity, converts a malformed stored manifest to a per-turn failed outcome, accepts case-insensitive image MIME, and makes historical image non-replay an explicit fake assertion. The real dispatcher/module regression now covers successful Claude and Codex image transport plus non-image, invalid-image, missing-image and invalid-manifest failures with no `input.accepted` event. The public E2E case covers a current image, a later turn without an image, exact request-key replay, changed-image conflict and uppercase MIME; it compiled but has not yet completed against the candidate stack.

After repairs, affected crate suites passed: API core 1011, API routes 242, agent turn 36, worker 131 and fake CLI 42. The focused real dispatcher/module test passed (1), architecture guards passed (49), and `g5-e2e` test code compiled. `cargo check --workspace`, strict Clippy on all workspace targets, formatting and diff checks passed. Local Docker limits are recorded below. The earlier isolated occupied-port worker failure passed its single selector and did not recur in the serialized affected suite.

Claude's read-only round 2 directly checked the repaired code and independently validated the replacement PNG CRCs. It found no remaining significant code defect and identified the candidate-stack E2E and live-provider evidence as outstanding. The test outcomes above were supplied to Claude; it did not rerun them. Review stops at two rounds, below the six-round limit. The three independent discovery reports and their distinct findings remain preserved above.

| Commitment | Recheck at owner/consumer | Status |
|---|---|---|
| `g5.conversation.inbox.reservation`, `.content`, `.prompt` | Frozen `session_inputs.artifacts` reaches `agent.turn`; malformed manifest settles as failed; request-key replay binds validated bytes; no accepted event on refusal. | Focused API core and real module checks pass; public candidate E2E pending. |
| `g5.security.tenant`, `.admission` | Incarnation-scoped route checks org/workspace/session; module checks returned session/input/metadata/hash. | Source and wrong-session/missing materialization checks pass; live VM authority check pending candidate E2E. |
| `g5.conversation.delivery`, `g5.data.outcome-evidence` | Valid images decode under declared type before native Claude/Codex encoding; file, invalid image, missing bytes and invalid manifest fail before acceptance. | Real dispatcher/module checks pass; live provider receipt unverified. |
| `g5.conversation.recovery.snapshot`, `.recovery.carry` | Prior attachment history is text context; current bytes derive only from reserved input; fake rejects unexpected historical image bytes. | Source and fake unit test pass; public continuation/fork evidence remains to complete. |
| `g5.architecture.provider_adapter`, `g5.quality.evidence.fakes`, `.claims` | Shared materialization validates bytes, provider branches encode; fake inspects actual frame/file bytes. | Synthetic boundary passed; no live provider claim. |

The full local Docker run passed preflight, then stopped in Phase 0b on four unchanged infrastructure script fixtures. One seed fixture failure was reproduced with its single unittest selector (`CODE_REPO_ID` and `DOCS_REPO_ID` remain unsubstituted); `git diff origin/main` showed the failing fixtures, seed script and recipe are unchanged. This is an unrelated harness assumption, so no broader Phase 0b rerun was made. A focused Phase 6 run built candidate API and host images and seeded the stack, then stopped in Phase 4b because Docker on this Mac has no `/dev/kvm`; the recipe explicitly has no process fallback. The public API/VM E2E therefore remains unverified locally and is delegated to required Cloud E2E CI. No live-provider claim follows from the synthetic checks.

## Cloud acceptance and final disposition

On PR 251 at `4e018b12`, required [Rust CI](https://console.cloud.google.com/cloud-build/builds;region=us-central1/8064c69f-cf5d-47e3-a559-bf188464a4f5?project=89784110315) and [E2E CI](https://console.cloud.google.com/cloud-build/builds;region=us-central1/8a91a570-d5dc-4c03-b9a5-44a7352a9576?project=89784110315) passed. Rust CI's six nextest lanes reported 6,255 passed and 6 skipped. Cloud E2E's retained Rust scenarios reported 118 passed; `test_public_chat_image_reaches_synthetic_provider_with_exact_bytes` passed in the `shared_vm_chats` selector. Its listed Playwright runs reported 206 passed and 13 skipped. CodeRabbit completed with no actionable inline comments. These reports validate the candidate VM/public path with synthetic providers, not live Claude/Codex receipt.

The reviewed implementation meets the changed commitment for current reserved image delivery and explicit refusal. Source inspection covers unconfirmed retry, continuation and fork separation; the Cloud public scenario directly covers first-turn replay and a follow-up without image bytes. A dedicated attachment-bearing fork runtime scenario and live-provider receipt remain unexecuted evidence. No significant unresolved code disagreement remained after two review rounds.

CodeRabbit's documentation-only follow-up on `f0c35cfb` had no actionable inline finding, but its linked-issue check highlighted the fork evidence gap against issue 231's acceptance text. The public E2E image scenario now forks after the image turn, verifies copied attachment metadata in historical messages, and sends an attachment-free turn through the real dispatcher and synthetic provider, which refuses historical image-byte redelivery. This addition compiles locally; its candidate VM run awaits the next Cloud E2E gate. Live-provider receipt remains outside the synthetic evidence.

Claude CLI read-only review round 3 inspected this follow-up after the focused fake-provider regression, E2E test executable build, workspace check, strict Clippy, formatting and diff checks passed. It traced the copied message order, stored digest, fork recovery prompt and provider guard and reported no significant defect at that head.

The final-head CodeRabbit review identified two valid acceptance-test weaknesses: the changed-image replay used invalid PNG bytes and accepted any failed status, and the three-column recheck table had a two-column delimiter. The test now uses a second valid PNG, checks the request-key error detail, and sends that image as fresh current content in the fork after the historical-only turn. The provider fixture checks its exact hash and rejects extra historical bytes. The table delimiter is corrected. The new PNG's CRCs and decoded pixel were checked independently; the focused fake-provider regression passed and the E2E executable built. Cloud execution remains pending.

Claude CLI read-only review round 4 traced the repaired replay and fork paths through the public API, recovery prompt and fake provider, independently checked both PNGs and the exact 422 response shape, and found no significant defect. The review loop stops at four rounds. Cloud E2E on this final scenario remains the required runtime evidence.
