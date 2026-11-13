---
name: ws-hints-and-revocation
description: "Workspace-channel events published on_commit of a transaction that can revoke a subscriber's read access (ownership transfer) break api_ws_revocation; even content-free hints count as application data."
metadata:
  node_type: memory
  type: feedback
  originSessionId: b76220f4-9843-40b2-9869-72aea99e8155
  modified: 2026-10-26T05:01:49.084Z
---

Do not publish workspace-channel WebSocket events from a commit that can revoke a
subscriber's access (e.g. `hold_tasks_for_transfer` during an ownership transfer).

**Why:** `api_ws_revocation::transfer_revokes_previous_owner_only_when_read_access_is_lost`
requires the revoked socket to close with no application data; a `chat.activity` hint
published via `ScopedDb::on_commit` reached it first (found 2026-10-26, #823).

**How to apply:** when adding post-commit hints, skip revoking paths and rely on the
client's refresh/poll fallback; run `api_ws_revocation` with any new workspace event.
Also: `cargo sqlx prepare -- --package g5-api --all-targets` does not compile
`g5-api-routes` lib tests — run `cargo clippy -p g5-api-routes --all-targets` too.

Related: [[sqlx-prepare-after-schema-change]].
