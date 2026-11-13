---
name: infra-docker-single-base-scan
description: "Any new Dockerfile under infra/docker must pass the single-external-base static scan; build on pinned ubuntu + rustup, never a new distro base."
metadata:
  node_type: memory
  type: project
  originSessionId: 876a9c4b-0eaa-4a0d-8d1b-713d5f9668be
  modified: 2026-10-08T17:42:59.810Z
---

Every `Dockerfile*` under `infra/docker/` is enumerated by `scripts/verify-base-digest-identity.sh --static-scan infra/docker`, which runs in `just ci-docker` Phase 0b (test `scripts/test_verify_base_digest_identity.py::test_ci_scans_the_relocated_fleet`). Each `FROM` must classify as a `g5/base*` tier, an internal stage, or an allowlisted image (the `allowed` regex: pinned `ubuntu:26.04@sha256`, playwright, pinned maven, caddy binary-source). An unclassified external base (e.g. `rust:1.97.1-slim`) fails the scan and breaks docker-ci.

**Why:** the single-external-base property (`base_image_consolidation` [D2], `docker_images` [D18]) keeps the whole fleet derived from one reviewed root. `Dockerfile.base-rust` deliberately installs Rust via rustup over `ubuntu:26.04` precisely because pulling a second distribution (`rust:*-slim`, Debian) is disallowed.

**How to apply:** a new builder Dockerfile should `FROM ubuntu:26.04@sha256:<same digest as Dockerfile.base's UBUNTU_BASE>` and install toolchains via pinned, checksum-verified installers (mirror `Dockerfile.base-rust`'s rustup-init block). Only derive `FROM g5/base*` if the image is built after those tiers exist — a Phase 0 preflight tool must use ubuntu directly. See [[runtime-bundle-not-on-path]] for the related guest-tooling path note.
