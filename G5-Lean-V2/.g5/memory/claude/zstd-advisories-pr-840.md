---
name: zstd-advisories-pr-840
description: "PR #840 (branch fix-dependency-security-scan-for-zstd) upgrades zstd 0.13.3→0.14.3 for RUSTSEC-2026-0391/0398/0403/0410; scope, evidence, and what is left."
metadata:
  node_type: memory
  type: project
  originSessionId: 9db7081f-69d2-436e-a463-b036417b26cf
  modified: 2026-10-26T17:44:45.232Z
---

PR #840 (`fix-dependency-security-scan-for-zstd`, created 2026-10-26, no linked issue)
fixes the four zstd advisories published 2026-10-16 that fail `cargo deny check` on
main: zstd 0.13.3 → 0.14.3, zstd-safe 7.2.4 → 8.2.0, zstd-sys 2.0.16 → 2.1.2 (still
libzstd 1.5.7, vendored `cc` build). Only `backend/g5-internal-api/src/session_files.rs`
uses the crate (`encode_all` + `read::Decoder::with_buffer`); `backend/g5d/Cargo.toml`
declares `zstd` but has no direct use (left as a flagged follow-up, not removed).

**Decisions:** no `deny.toml` ignores; the lockfile changes only the three zstd entries
(`cargo update -p zstd` also re-resolved `tempfile → getrandom 0.3.4 → 0.4.3`, reverted
by hand because tempfile allows `>=0.3,<0.5`). Mixed-version interop was proven with
two scratch binaries in `/tmp/scratch-old` (pinned to main's exact chain; needs
`zstd-sys = { default-features = false }` or bindgen wants libclang) and
`/tmp/scratch-new`: identical frames, cross-decode both ways, identical malformed-input
errors. Those fixtures became `PREVIOUS_CODEC_FRAMES` in the codec's unit tests.

**Coverage added:** codec tests (previous-codec frames decode; truncated/trailing/
empty/legacy-magic bodies refused; file at the 128 MiB cap accepted) and the DB-backed
`backend/g5-api/tests/api_chat_session_file_bodies.rs` (413 past the cap, 422 for a
non-stream body, nothing stored, round trip through the content route). The old
`api_chat_session_files.rs` route tests had been deleted in the VM cutover (be37f45df).

**How to apply:** if CI on #840 fails, triage per [[g5-ci-cloudbuild-triage]]; local
reruns use the unlabeled `g5-test-pg` container from [[rust-and-integration-test-setup]].
Do not merge or deploy; the owner decides. Worktree `/work/worktrees/zstd-advisories`
(branch `wt/zstd-advisories`) is temporary and already integrated.
