---
name: rust-toolchain-split-in-vm
description: In a fresh G5 MicroVM /usr/local/bin holds empty rustup shims while /opt/g5/bin ships the CI-pinned Rust; mixing them breaks doctests with E0514.
metadata:
  node_type: memory
  type: reference
  originSessionId: 696c299f-b4d3-466f-9422-ad8ea5f52629
  modified: 2026-10-08T15:05:52.986Z
---

A fresh G5 MicroVM has two Rust installs and the wrong one wins:

- `/usr/local/bin/cargo|rustc` are **rustup shims with no toolchain installed** —
  bare `cargo` fails with "rustup could not choose a version of cargo to run".
- `/opt/g5/bin` ships the **CI-pinned toolchain** (`cargo` 1.97.1 as of
  2026-10-08, plus `rustc`, `rustdoc`, `cargo-nextest`), but `/usr/local/bin`
  precedes it on PATH, so its `cargo`/`rustc` stay shadowed.

**Best fix: `export RUSTUP_HOME=/usr/local/rustup`** — the PATH shims then
resolve to the pinned 1.97.1 toolchain *including* `rustfmt` 1.9.0, matching
what CI's `RUST_FMT` gate checks. Pair it with a writable `CARGO_HOME`; see
[[rust-and-integration-test-setup]] for that and the Postgres port.

Two dead ends, so as not to retry them:

- `/opt/g5/bin/cargo` builds but **cannot `cargo fmt`**: its `RUSTUP_HOME` is
  in the read-only platform runtime ("could not write settings file … Read-only
  file system"), and redirecting `RUSTUP_HOME` to a writable copy then fails
  with "'cargo-fmt' is not installed for the toolchain" — that bundle ships no
  `rustfmt` component.
- `rustup default stable` works but installs a *newer* compiler (1.99.0 on
  2026-10-08) than the 1.97.1 pin, which breaks doctests (below). Prefer
  `RUSTUP_HOME=/usr/local/rustup`.

`rustup default stable` works but installs a *newer* compiler (1.99.0 on
2026-10-08). That is the trap: rustup provides no `rustdoc` shim in
`/usr/local/bin`, so `rustdoc` still resolves to `/opt/g5/bin/rustdoc` (1.97.1)
and every doctest dies with `error[E0514]: found crate ... compiled by an
incompatible version of rustc`. Unit and integration tests pass; only
`cargo test --doc` fails. Workaround if already on rustup stable:
`RUSTDOC=$(rustup which rustdoc) cargo test --doc`.

A stable rustfmt newer than the pin still agreed with the committed formatting
of every existing file in G5-Lean on 2026-10-08, so `cargo fmt --all --check`
remains usable as a gate; check that the only diffs are in files you wrote.

Related: [[runtime-bundle-not-on-path]], [[g5-vm-resets-lose-toolchains]].
