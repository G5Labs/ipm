---
name: sqlx-migrate-embed-incremental-cache
description: "After adding/renaming a migration file, rebuild the migrator crate with SCCACHE_RECACHE=1 (the VM sets RUSTC_WRAPPER=sccache); sccache serves the stale sqlx::migrate! embed because the directory a proc macro reads is not in its cache key."
metadata:
  type: reference
---

# sccache serves a stale sqlx::migrate! embed after a migration file rename

`g5_api::db::run_migrations` embeds the catalog with
`sqlx::migrate!("../g5-api/src/db/migrations")` in `backend/g5-api-data/src/db.rs`.
The VM exports `RUSTC_WRAPPER=sccache` (`SCCACHE_DIR=/opt/g5-build-cache/sccache`) and
`CARGO_INCREMENTAL=0`. sccache keys a rustc invocation on its source inputs and
arguments; the directory listing a proc macro reads at expansion is not an input. So
after renaming or adding a migration file, `touch`ing `db.rs`, `cargo clean -p
g5-api-data`, and deleting incremental dirs all still produced an rlib embedding the old
file name (2026-10-19: the embedded migrator applied version 96 after the file was
renamed to `097_…`, failing `migration_097_start_command_sent_at` while the catalog on
disk was correct). The tell: the "rebuilt" rlib keeps the same hash and its
`include_str!` paths (visible with `strings`) name the old file.

**How to apply:** after changing the set of files in `backend/g5-api/src/db/migrations`,
run `cargo clean -p g5-api-data` and rebuild with `SCCACHE_RECACHE=1` (or
`RUSTC_WRAPPER=`) before trusting migration tests locally. Verify with
`strings target/debug/deps/libg5_api_data-*.rlib | grep -oE '[0-9]{3}_[a-z_]+\.sql'`.
Related: [[g5-api-test-binaries-fill-disk]].
