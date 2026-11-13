---
name: route-auth-static-audit-gotchas
description: "The route_auth_static audit strips `//` naively, so handlers containing `https://` strings may be silently skipped until the text changes; new findings need a guard token or a service-level recognizer, not a manifest entry."
metadata:
  node_type: memory
  type: project
  originSessionId: faf3649a-619e-46f0-aefc-dad0b731b8f8
  modified: 2026-10-13T20:53:19.664Z
---

`backend/g5-api/tests/route_auth_static.rs` (`route_secondary_identifiers_have_auth_manifest_entries`) splits every line on `//` before parsing, so a handler body with an `https://...` string literal can fail brace extraction and be skipped entirely. Rewriting such a handler can make the audit "discover" it for the first time and fail CI (`rust-xfs-orchestrator` and the value-loop Rust lane both run it).

**Why:** Hit on 2026-10-13 when `preview_session_mint` and `preview_reauth` were rewritten for issue 488: the audit had never parsed their old bodies.

**How to apply:** Run `cargo test -p g5-api --test route_auth_static` locally after touching route handlers. To satisfy a finding, either name the real guard as a token in `auth_contract_for_argument` (e.g. `resolve_workspace_preview_target` for `task_id`) or add a `service_level_*_guard_exists` recognizer with ordering checks; `tests/fixtures/security/route_auth_manifest.json` entries are documentation only and do not suppress findings. Shadow the route param (`let task_id = task_id.into_uuid();`) so the guard's statement window contains the identifier.
