---
name: rust-and-integration-test-setup
description: "How to get cargo and the Rust integration tests running in a fresh G5-Lean MicroVM (rustup home, writable CARGO_HOME, Postgres container)."
metadata:
  node_type: memory
  type: reference
  originSessionId: 87feb4f9-6265-4bf1-8b05-8d1079fbea5d
  modified: 2026-10-11T23:19:37.174Z
---

A fresh G5-Lean workspace VM has the Rust toolchain installed but not wired up:

- `cargo`/`rustc` on PATH are rustup shims with no default toolchain. Export
  `RUSTUP_HOME=/usr/local/rustup` and they resolve (1.97.1 as of 2026-10).
- `/usr/local/cargo/registry` (cache, index, src) is world-writable since the template
  v3 hosts shipped (verified 2026-10-26: `cargo update` and new crate downloads work with
  the default `CARGO_HOME`); only `/usr/local/cargo/bin` stays read-only, so install extra
  cargo tools elsewhere (e.g. `install -m 0755 cargo-deny /home/agent/.local/bin/` and run
  it as `cargo-deny`; CI pins cargo-deny 0.19.0, musl tarball from GitHub releases).
  If a guest still refuses with `Permission denied` on the registry, fall back to the old
  copy: `cp -a /usr/local/cargo /home/agent/.cargo` and export `CARGO_HOME=/home/agent/.cargo`
  (root cause, found 2026-10-14 on `bake-permission`: `oci/extract.rs::apply_files` dropped a
  later layer's directory mode, so the Dockerfile's `chmod -R a+rwX` never reached the guest).
- `RUSTC_WRAPPER=sccache` with `SCCACHE_DIR=/opt/g5-build-cache/sccache` is preset and
  writable; a fresh VM starts with an empty cache.
- `frontend/node_modules` is absent; `npm ci` works (~2 min). The full `vitest run` suite
  takes ~13 min, so run targeted files while iterating.
- Rust integration tests need Postgres. Load the stack secrets first, then start it:
  `source scripts/lib/local-stack-secrets.sh && g5_load_local_stack_secrets && docker compose -p g5-dev -f infra/docker/docker-compose.dev.yml up -d postgres`.
  A bare `compose up postgres` creates the `g5-dev` container and the `g5-lean_pgdata_dev`
  volume without `~/.config/g5/local-stack-secrets.json`, and every later `just dev-up` (and
  `scripts/stack_secrets.py`, run by `just dev-stack-up`, which the full-stack preview
  profiles use) then refuses with "Existing G5 dev data has no retained secret bundle"
  (seen 2026-10-19 and 2026-10-20, #664). If that happened, remove both when the tests finish
  (`docker compose -f infra/docker/docker-compose.dev.yml rm -s -f postgres` and
  `docker volume rm g5-lean_pgdata_dev`; if the volume only holds `g5_test_*` DBs and an
  empty `g5_dev`, `just dev-down` clears it), or use an unlabeled container that the check
  ignores (worked 2026-10-26, image digest from the compose file):
  `docker run -d --name g5-test-pg --shm-size=1g -e POSTGRES_USER=g5 -e POSTGRES_PASSWORD=g5_dev
  -e POSTGRES_DB=g5_dev -p 127.0.0.1:5432:5432 pgvector/pgvector:pg17@sha256:<digest in
  infra/docker/docker-compose.dev.yml>`; `docker rm -f g5-test-pg` when done.
  The compose default maps Postgres to **5432**, but `scripts/run-integration.sh`
  defaults `DATABASE_URL` to 5433 — set it explicitly:
  `export DATABASE_URL=postgres://g5:g5_dev@localhost:5432/g5_dev`,
  `export TEST_DATABASE_URL=postgres://g5:g5_dev@localhost:5432/g5_dev`,
  `export SQLX_OFFLINE=true`.
- Rust compilation does not require a VMM installation. Cloud Hypervisor and
  the pinned guest kernel/initramfs are runtime prerequisites for Linux/KVM boot tests.
- `g5-api-core`'s `git_conflict_resolution::native_rebase_command_replays_related_and_unrelated_commit_series`
  used to fail in this VM, NOT in CI. Root cause: the VM exports
  `GIT_AUTHOR_*`/`GIT_COMMITTER_*` (= `zhengchun.liu@g5labs.ai`), which outrank
  the fixture's repo-local `user.email` config, so the replayed commit was
  reauthored and the `author@example.com` assertion failed. Fixed on branch
  `vm-idle-timeout` (commit `ec470569`) by routing the test's git calls through
  a `git_command()` helper that `env_remove`s those four vars. Lesson: tests that
  shell out to `git commit` must clear the ambient identity env, not just set
  `git config user.email` — env vars win.
- A cold `cargo build -p g5-api --tests` takes ~16–26 min here (26 min on 2026-10-15) and exceeds the default
  10-minute background limit; start it with `run_in_background` and `timeout` of
  3600000 or more, and do not run two cargo commands at once (target-dir lock).
