---
name: g5-scm-native-tests-flaky-under-load
description: "g5-scm native::tests (process deadlines, push outcomes) fail intermittently when the whole crate runs in the VM; they pass alone and fail on main too."
metadata:
  node_type: memory
  type: reference
  originSessionId: dd481282-b65d-403a-9ed5-24bff8953bf9
  modified: 2026-10-15T15:30:40.878Z
---

`cargo test -p g5-scm` in a G5 MicroVM can fail one or two `native::tests` cases, such as `deadline_includes_pipes_held_by_descendants_after_leader_exit`, `push_outcomes_preserve_confirmed_refs_and_separate_stale_proxy_from_auth_failure` or `api_observation_deadline_and_cancellation_reap_child_without_retry`. These tests depend on process timing. On 2026-10-15 a different one failed on `origin/main` too, and the failing ones passed when run alone.

**How to apply:** before blaming your change, rerun the failing names alone (`cargo test -p g5-scm -- native::tests::<name>`). Treat them as VM load flakes only if they pass alone and your diff does not touch `g5-scm/src/native`. Related: [[rust-and-integration-test-setup]].

**Also flaky on CI (2026-10-24):** `g5d-foundation terminal::session::tests::
streaming_output_is_retained_only_within_the_bounded_windows` (session.rs:2142, a
bounded-window timing assertion) failed `rust-xfs-orchestrator` in four unrelated
builds within 24 h (3290f83b, 6b4a6bf2, a24d4c53, ac7d8282). Treat a lone failure of
it as environmental only when the diff touches neither
`backend/g5d-foundation/src/terminal/session.rs` nor bounded-retention behavior;
then retrigger CI rather than chase it in the diff. Otherwise investigate the change.
The evidence for this test is CI-only; it has not been seen failing in the VM.

2026-10-27 (PR #853, build da592a42, a diff that touched no dispatcher code):
`g5d::dispatch_modules::agent_turn_owner_end_is_bounded_behind_slowly_read_replies`
failed its 13 s "End did not take effect within its bound" wait under the loaded
`rust-xfs-orchestrator` and passed in the previous run of the same branch; treat it as
the same timing-flake class when the diff is unrelated.
