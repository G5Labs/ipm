---
name: sqlx-prepare-after-schema-change
description: "How to regenerate .sqlx after a migration plus query! changes in the VM; the migrate binary is circular, a failed prepare clears .sqlx, and cargo install needs --root."
metadata:
  node_type: memory
  type: reference
  originSessionId: a5249e53-7356-4b88-a7f7-a5c0aae6489a
  modified: 2026-10-22T02:39:10.375Z
---

Regenerating SQLx offline data after adding a migration **and** changing `query!`
macros (seen on #660, 2026-10-22):

- `just db-migrate` / `cargo run --bin g5-db-migrate` is circular: it compiles
  `g5-api-core` with `SQLX_OFFLINE=true`, and the changed `query!` macros fail
  against the stale `.sqlx`. Apply the catalog with the CLI instead; migrations
  self-provision their roles, so a plain `pgvector/pgvector:pg17` container works:
  `sqlx migrate run --source backend/g5-api/src/db/migrations` with
  `DATABASE_URL=postgres://g5:g5_dev@localhost:5432/g5_dev`.
- Then run exactly CI's command: `SQLX_OFFLINE=false cargo sqlx prepare --workspace
  -- --package g5-api --all-targets` (the `sqlx-check` stage in
  `infra/ci/cloudbuild-ci-rust.yaml`). It compiles the workspace against the live
  schema, so it doubles as a compile check.
- A prepare that fails to compile **deletes the tracked `.sqlx` files first**;
  restore with `git checkout -- .sqlx` before rerunning, else the next run leaves
  orphans/missing files.
- `cargo install sqlx-cli` fails on `/usr/local/cargo/.crates.toml` (not writable);
  use `--root /home/agent/.local` (registry under `/usr/local/cargo` is writable) and
  put `/home/agent/.local/bin` on PATH. `--no-default-features --features
  postgres,rustls` builds in ~3 min.

Related: [[sqlx-migrate-embed-incremental-cache]], [[rust-and-integration-test-setup]].
