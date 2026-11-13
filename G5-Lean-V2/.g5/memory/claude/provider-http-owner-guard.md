---
name: provider-http-owner-guard
description: "Any reqwest/hyper reference in a backend crate outside a listed owner function fails g5-api's provider-ownership guard test, which runs only in CI's integration lane."
metadata:
  node_type: memory
  type: feedback
  originSessionId: 637a40ef-3da0-42c4-9e79-c0c26043f5ab
  modified: 2026-10-27T04:52:43.276Z
---

`authoritative_workspace_reader_guard::all_backend_provider_protocol_uses_its_owner` (in `backend/g5-api/tests/`) fails for any `reqwest::`/`hyper` client reference outside the reviewed owner functions in `permitted_non_git_http`. `reqwest::Url` is exempt; `reqwest::Method` and `reqwest::header::*` are not. It surfaced on PR #712 (2026-10-27) only in g5-ci-rust's `rust-xfs-orchestrator` lane: `cargo check`, clippy and g5d's own tests all passed.

**Why:** raw HTTP must stay with one reviewed owner per destination, so callers can't reach source-control providers directly.

**How to apply:** when new code sends HTTP, validate methods and headers with the `http` crate (it's the same type that reqwest re-exports) and add only the sending function to the owner list, with a comment naming its ontology ID. Run `cargo test -p g5-api --test integration authoritative_workspace_reader_guard::` before pushing. Related: [[route-auth-static-audit-gotchas]].
